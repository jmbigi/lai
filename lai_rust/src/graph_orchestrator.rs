//! Orquestación por grafo explícito (v5.0, herramienta 1: LangGraph).
//!
//! Capa de orquestación alternativa al `ExecutionEngine` propio: un grafo
//! dirigido explícito (nodos LLM/tool/verify con transiciones condicionales)
//! y **presupuesto de tokens** que bloquea la ejecución cuando se agota
//! (fail-closed). LangGraph es el único orquestador externalizável que ofrece
//! un grafo explícito y medible en tokens; esta implementación interna
//! adopta el mismo contrato sin depender de Python.
//!
//! Contrato v5.0: la ejecución termina en un `Verdict` acotado. Si el
//! presupuesto se agota sin alcanzar un nodo terminal, el veredicto es
//! `BLOCKED` (no se ejecutó lo que se debía).

use crate::contracts::ExternalVerdict;
use crate::verifier::Verdict;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tipo de nodo del grafo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: String, // "llm" | "tool" | "verify" | "terminal"
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    /// Siguientes nodos con condición opcional ("always" | "on_success" |
    /// "on_failure" | "budget_exceeded").
    #[serde(default)]
    pub next: Vec<GraphTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphTransition {
    pub target: String,
    #[serde(default = "default_always")]
    pub when: String,
}

fn default_always() -> String {
    "always".to_string()
}

/// Grafo de orquestación completo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSpec {
    pub entry: String,
    pub nodes: Vec<GraphNode>,
    /// Presupuesto total de tokens para toda la ejecución.
    pub token_budget: u64,
}

impl GraphSpec {
    pub fn node(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    #[cfg(test)]
    pub fn node_mut(&mut self, id: &str) -> Option<&mut GraphNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn validate(&self) -> Result<(), String> {
        let ids: Vec<&str> = self.nodes.iter().map(|n| n.id.as_str()).collect();
        if !ids.contains(&self.entry.as_str()) {
            return Err(format!("entrypoint '{}' inexistente", self.entry));
        }
        for n in &self.nodes {
            for t in &n.next {
                if !ids.contains(&t.target.as_str()) {
                    return Err(format!("nodo '{}' transiciona a '{}' inexistente", n.id, t.target));
                }
            }
        }
        Ok(())
    }

    /// Nodos terminales alcanzables.
    pub fn terminals(&self) -> Vec<&GraphNode> {
        self.nodes.iter().filter(|n| n.kind == "terminal").collect()
    }
}

/// Estado de ejecución del grafo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphState {
    pub current: String,
    pub tokens_used: u64,
    pub steps: Vec<String>,
    pub last_result: Option<String>,
    pub halted_reason: Option<String>,
}

/// Ejecuta un paso del grafo y devuelve el siguiente estado.
/// `step_fn` ejecuta el nodo y devuelve (resultado, tokens_consumidos, success).
pub fn step_graph<F>(
    spec: &GraphSpec,
    state: &mut GraphState,
    step_fn: &mut F,
) -> Result<bool, String>
where
    F: FnMut(&GraphNode) -> Result<(String, u64, bool), String>,
{
    let node = spec
        .node(&state.current)
        .ok_or_else(|| format!("nodo '{}' inexistente", state.current))?;

    if node.kind == "terminal" {
        state.halted_reason = Some("terminal alcanzado".to_string());
        return Ok(true);
    }

    let (result, tokens, success) = step_fn(node)?;
    state.tokens_used += tokens;
    state.last_result = Some(result);
    state.steps.push(node.id.clone());

    if state.tokens_used > spec.token_budget {
        state.halted_reason = Some("budget de tokens agotado".to_string());
        return Ok(true);
    }

    // Selección de transición según condición.
    let next_id = node
        .next
        .iter()
        .find(|t| match t.when.as_str() {
            "on_success" => success,
            "on_failure" => !success,
            _ => true, // always / budget_exceeded (manejado arriba)
        })
        .map(|t| t.target.clone());

    match next_id {
        Some(id) => {
            state.current = id;
            Ok(false)
        }
        None => {
            state.halted_reason = Some("sin transición aplicable".to_string());
            Ok(true)
        }
    }
}

/// Ejecuta el grafo completo hasta un terminal o hasta agotar un límite de
/// pasos (anti-bucle). Devuelve el estado final.
pub fn run_graph<F>(spec: &GraphSpec, mut state: GraphState, step_fn: F) -> Result<GraphState, String>
where
    F: FnMut(&GraphNode) -> Result<(String, u64, bool), String>,
{
    spec.validate()?;
    let mut step_fn = step_fn;
    let mut steps = 0;
    let max_steps = 1000;
    loop {
        if step_graph(spec, &mut state, &mut step_fn)? {
            break;
        }
        steps += 1;
        if steps > max_steps {
            state.halted_reason = Some("límite de pasos del grafo".to_string());
            break;
        }
    }
    Ok(state)
}

/// Variante async de [`run_graph`]: `step_fn` es un closure async (para
/// integrarse con el ToolRegistry y el LLM sin bloquear el runtime).
/// Recibe el nodo **por valor** (clonado) para evitar problemas de lifetimes
/// con el futuro devuelto.
pub async fn run_graph_async<F, Fut>(spec: &GraphSpec, mut state: GraphState, mut step_fn: F) -> Result<GraphState, String>
where
    F: FnMut(GraphNode) -> Fut,
    Fut: std::future::Future<Output = Result<(String, u64, bool), String>>,
{
    spec.validate()?;
    let mut steps = 0;
    let max_steps = 1000;
    loop {
        let node = spec
            .node(&state.current)
            .cloned()
            .ok_or_else(|| format!("nodo '{}' inexistente", state.current))?;
        if node.kind == "terminal" {
            state.halted_reason = Some("terminal alcanzado".to_string());
            break;
        }
        let (result, tokens, success) = step_fn(node).await?;
        state.tokens_used += tokens;
        state.last_result = Some(result);
        state.steps.push(spec.node(&state.current).map(|n| n.id.clone()).unwrap_or_default());
        if state.tokens_used > spec.token_budget {
            state.halted_reason = Some("budget de tokens agotado".to_string());
            break;
        }
        let next_id = spec
            .node(&state.current)
            .and_then(|n| {
                n.next
                    .iter()
                    .find(|t| match t.when.as_str() {
                        "on_success" => success,
                        "on_failure" => !success,
                        _ => true,
                    })
                    .map(|t| t.target.clone())
            });
        match next_id {
            Some(id) => state.current = id,
            None => {
                state.halted_reason = Some("sin transición aplicable".to_string());
                break;
            }
        }
        steps += 1;
        if steps > max_steps {
            state.halted_reason = Some("límite de pasos del grafo".to_string());
            break;
        }
    }
    Ok(state)
}

/// Mapea el estado final del grafo al veredicto acotado.
pub fn graph_to_verdict(spec: &GraphSpec, state: &GraphState, required: bool) -> Verdict {
    if let Some(reason) = &state.halted_reason {
        if reason == "terminal alcanzado" {
            // El terminal debe declarar su resultado; si no hay evidencia de
            // verificación, no se declara SUCCESS pleno.
            if state.last_result.as_deref() == Some("ok") {
                return ExternalVerdict::Safe.to_verdict(required);
            }
            return ExternalVerdict::Unknown.to_verdict(required);
        }
        // Budget agotado / sin transición / límite de pasos: no concluyente.
        return ExternalVerdict::Timeout.to_verdict(required);
    }
    ExternalVerdict::Unknown.to_verdict(required)
}

/// Cuenta tokens consumidos por un grafo ejecutado (para `lai bench`).
pub fn tokens_used(state: &GraphState) -> u64 {
    state.tokens_used
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_graph() -> GraphSpec {
        GraphSpec {
            entry: "start".to_string(),
            token_budget: 1000,
            nodes: vec![
                GraphNode {
                    id: "start".to_string(),
                    kind: "tool".to_string(),
                    prompt: None,
                    tool: Some("read_file".to_string()),
                    next: vec![GraphTransition { target: "verify".to_string(), when: "always".to_string() }],
                },
                GraphNode {
                    id: "verify".to_string(),
                    kind: "verify".to_string(),
                    prompt: None,
                    tool: None,
                    next: vec![
                        GraphTransition { target: "ok".to_string(), when: "on_success".to_string() },
                        GraphTransition { target: "fail".to_string(), when: "on_failure".to_string() },
                    ],
                },
                GraphNode { id: "ok".to_string(), kind: "terminal".to_string(), prompt: None, tool: None, next: vec![] },
                GraphNode { id: "fail".to_string(), kind: "terminal".to_string(), prompt: None, tool: None, next: vec![] },
            ],
        }
    }

    #[test]
    fn test_validate_ok() {
        assert!(simple_graph().validate().is_ok());
    }

    #[test]
    fn test_validate_missing_target() {
        let mut g = simple_graph();
        g.node_mut("start").unwrap().next[0].target = "ghost".to_string();
        assert!(g.validate().is_err());
    }

    #[test]
    fn test_run_to_terminal_success() {
        let spec = simple_graph();
        let state = run_graph(&spec, GraphState {
            current: "start".to_string(),
            tokens_used: 0,
            steps: vec![],
            last_result: None,
            halted_reason: None,
        }, |node| {
            if node.id == "verify" {
                Ok(("ok".to_string(), 10, true))
            } else {
                Ok(("done".to_string(), 5, true))
            }
        }).unwrap();
        assert_eq!(state.halted_reason.as_deref(), Some("terminal alcanzado"));
        assert_eq!(state.tokens_used, 15);
        assert!(matches!(graph_to_verdict(&spec, &state, true), Verdict::Success));
    }

    #[test]
    fn test_budget_exceeded_blocks() {
        let mut spec = simple_graph();
        spec.token_budget = 10; // el primer nodo consume 5, el segundo 10 -> agota
        let state = run_graph(&spec, GraphState {
            current: "start".to_string(),
            tokens_used: 0,
            steps: vec![],
            last_result: None,
            halted_reason: None,
        }, |node| {
            if node.id == "verify" {
                Ok(("ok".to_string(), 10, true))
            } else {
                Ok(("done".to_string(), 5, true))
            }
        }).unwrap();
        assert_eq!(state.halted_reason.as_deref(), Some("budget de tokens agotado"));
        // Requerido + budget agotado -> BLOCKED.
        assert!(matches!(graph_to_verdict(&spec, &state, true), Verdict::Blocked(_)));
    }

    #[test]
    fn test_failure_branch() {
        let spec = simple_graph();
        let state = run_graph(&spec, GraphState {
            current: "start".to_string(),
            tokens_used: 0,
            steps: vec![],
            last_result: None,
            halted_reason: None,
        }, |node| {
            if node.id == "verify" {
                Ok(("bad".to_string(), 10, false))
            } else {
                Ok(("done".to_string(), 5, true))
            }
        }).unwrap();
        assert_eq!(state.current, "fail");
        // Resultado no-ok en terminal -> Unknown -> BLOCKED si requerido.
        assert!(matches!(graph_to_verdict(&spec, &state, true), Verdict::Blocked(_)));
    }

    #[test]
    fn test_cycle_limit() {
        let spec = GraphSpec {
            entry: "a".to_string(),
            token_budget: 100000,
            nodes: vec![GraphNode {
                id: "a".to_string(),
                kind: "tool".to_string(),
                prompt: None,
                tool: None,
                next: vec![GraphTransition { target: "a".to_string(), when: "always".to_string() }],
            }],
        };
        let state = run_graph(&spec, GraphState {
            current: "a".to_string(),
            tokens_used: 0,
            steps: vec![],
            last_result: None,
            halted_reason: None,
        }, |_| Ok(("x".to_string(), 1, true))).unwrap();
        // Sin terminal, el grafo debe detenerse por límite de pasos.
        assert!(state.halted_reason.is_some());
    }
}