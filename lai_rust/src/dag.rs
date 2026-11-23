//! Planificación DAG (v5.0, herramienta 10: DAGent).
//!
//! Sustituye el plan lineal del `ExecutionEngine` por un **grafo de tareas**
//! con dependencias explícitas: planificación incremental *evaluate-then-grow*,
//! ejecución por capas topológicas y aislamiento de sub-tareas. Cada nodo es
//! una unidad verificable; las capas independientes pueden ejecutarse en
//! paralelo (coordinadas por los locks de `concurrency.rs` y los worktrees
//! de la transversal 4.1).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Nodo de un plan DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    pub id: String,
    pub description: String,
    /// Herramienta o "llm" para pasos generativos.
    pub tool: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub completed: bool,
    /// Criticidad: si true, el fallo del nodo bloquea la tarea (fail-closed).
    #[serde(default)]
    pub critical: bool,
}

/// Plan DAG completo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagPlan {
    pub nodes: Vec<DagNode>,
}

impl DagPlan {
    pub fn node(&self, id: &str) -> Option<&DagNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut DagNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// IDs de los nodos sin dependencias (capa inicial).
    pub fn roots(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|n| n.depends_on.is_empty())
            .map(|n| n.id.clone())
            .collect()
    }

    /// Capas topológicas: `layers[i]` contiene los nodos ejecutables tras
    /// completar `layers[0..i]`. Preserva el orden de `nodes`.
    pub fn layers(&self) -> Result<Vec<Vec<String>>, String> {
        let mut remaining: HashSet<String> = self.nodes.iter().map(|n| n.id.clone()).collect();
        let mut completed: HashSet<String> = HashSet::new();
        let mut layers: Vec<Vec<String>> = Vec::new();

        while !remaining.is_empty() {
            let layer: Vec<String> = self
                .nodes
                .iter()
                .filter(|n| {
                    remaining.contains(&n.id)
                        && n.depends_on.iter().all(|d| completed.contains(d))
                })
                .map(|n| n.id.clone())
                .collect();
            if layer.is_empty() {
                return Err(format!(
                    "ciclo o dependencia inexistente en el DAG: {:?}",
                    remaining
                ));
            }
            for id in &layer {
                remaining.remove(id);
                completed.insert(id.clone());
            }
            layers.push(layer);
        }
        Ok(layers)
    }

    /// Nodos cuya dependencia referenciada no existe (validación estructural).
    pub fn validate(&self) -> Result<(), String> {
        let ids: HashSet<String> = self.nodes.iter().map(|n| n.id.clone()).collect();
        for n in &self.nodes {
            for d in &n.depends_on {
                if !ids.contains(d) {
                    return Err(format!("nodo '{}' depende de '{}' inexistente", n.id, d));
                }
            }
        }
        self.layers().map(|_| ())
    }

    /// Plan incremental *evaluate-then-grow*: añade un nodo solo si sus
    /// dependencias están completadas. Devuelve true si se añadió.
    pub fn grow(&mut self, node: DagNode) -> Result<bool, String> {
        let done: HashSet<String> = self
            .nodes
            .iter()
            .filter(|n| n.completed)
            .map(|n| n.id.clone())
            .collect();
        if node.depends_on.iter().any(|d| !done.contains(d)) {
            return Ok(false);
        }
        if self.node(&node.id).is_some() {
            return Err(format!("nodo '{}' ya existe", node.id));
        }
        self.nodes.push(node);
        Ok(true)
    }

    /// Progreso: fracción de nodos completados.
    pub fn progress(&self) -> f64 {
        if self.nodes.is_empty() {
            return 0.0;
        }
        self.nodes.iter().filter(|n| n.completed).count() as f64 / self.nodes.len() as f64
    }
}

/// Programa la ejecución del DAG por capas. `on_layer` se invoca con cada
/// capa (ids de la capa) y debe devolver los ids completados con éxito.
/// Falla cerrado: si una capa no completa el 100 %, la ejecución se detiene
/// y los nodos críticos pendientes se marcan como bloqueados.
pub fn execute_layers<F>(plan: &DagPlan, mut on_layer: F) -> Result<Vec<String>, String>
where
    F: FnMut(&[String]) -> Result<Vec<String>, String>,
{
    let layers = plan.layers()?;
    let mut executed = Vec::new();
    for layer in &layers {
        let done = on_layer(layer)?;
        let layer_set: HashSet<&String> = layer.iter().collect();
        let done_set: HashSet<&String> = done.iter().collect();
        let missing: Vec<&String> = layer_set.difference(&done_set).cloned().collect();
        if !missing.is_empty() {
            let critical_blocked: Vec<&String> = missing
                .iter()
                .filter(|id| plan.node(id).map(|n| n.critical).unwrap_or(false))
                .cloned()
                .collect();
            if !critical_blocked.is_empty() {
                return Err(format!(
                    "capa con nodos críticos sin completar (fail-closed): {:?}",
                    critical_blocked
                ));
            }
        }
        executed.extend(done);
    }
    Ok(executed)
}

/// Orden de ejecución plano (capas concatenadas) — para depuración y
/// comparación con el plan lineal en `lai bench`.
pub fn flatten_layers(plan: &DagPlan) -> Result<Vec<String>, String> {
    Ok(plan.layers()?.into_iter().flatten().collect())
}

/// Serializa un plan DAG desde el JSON que produce el LLM (validando).
pub fn parse_plan(json: &str) -> Result<DagPlan, String> {
    let plan: DagPlan = serde_json::from_str(json)
        .map_err(|e| format!("JSON de plan DAG inválido: {}", e))?;
    plan.validate()?;
    Ok(plan)
}

/// Genera un plan DAG de ejemplo a partir de una lista de tareas lineales
/// (fallback determinista cuando el LLM no produce un DAG válido).
pub fn linear_to_dag(steps: &[(String, String, serde_json::Value)]) -> DagPlan {
    let mut nodes = Vec::new();
    for (i, (desc, tool, args)) in steps.iter().enumerate() {
        nodes.push(DagNode {
            id: format!("n{}", i),
            description: desc.clone(),
            tool: tool.clone(),
            arguments: args.clone(),
            depends_on: if i == 0 { vec![] } else { vec![format!("n{}", i - 1)] },
            completed: false,
            critical: true,
        });
    }
    DagPlan { nodes }
}

/// Cola de trabajo para planificación incremental (evaluate-then-grow).
pub struct GrowQueue {
    pub plan: DagPlan,
    frontier: VecDeque<String>,
}

impl GrowQueue {
    pub fn new(plan: DagPlan) -> Self {
        let frontier: VecDeque<String> = plan.roots().into();
        Self { plan, frontier }
    }

    /// Siguiente nodo candidato (cuyas dependencias están completas).
    pub fn next_candidate(&mut self) -> Option<DagNode> {
        while let Some(id) = self.frontier.pop_front() {
            if let Some(node) = self.plan.node(&id) {
                if !node.completed && node.depends_on.iter().all(|d| {
                    self.plan.node(d).map(|n| n.completed).unwrap_or(false)
                }) {
                    // Los sucesores del nodo elegido entran en la frontera.
                    for n in &self.plan.nodes {
                        if n.depends_on.contains(&id) {
                            self.frontier.push_back(n.id.clone());
                        }
                    }
                    return Some(node.clone());
                }
            }
        }
        None
    }

    pub fn mark_completed(&mut self, id: &str) -> Result<(), String> {
        let node = self
            .plan
            .node_mut(id)
            .ok_or_else(|| format!("nodo '{}' inexistente", id))?;
        node.completed = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DagPlan {
        DagPlan {
            nodes: vec![
                DagNode { id: "a".into(), description: "raíz".into(), tool: "t".into(), arguments: Default::default(), depends_on: vec![], completed: false, critical: true },
                DagNode { id: "b".into(), description: "dep a".into(), tool: "t".into(), arguments: Default::default(), depends_on: vec!["a".into()], completed: false, critical: true },
                DagNode { id: "c".into(), description: "dep a".into(), tool: "t".into(), arguments: Default::default(), depends_on: vec!["a".into()], completed: false, critical: false },
                DagNode { id: "d".into(), description: "dep b,c".into(), tool: "t".into(), arguments: Default::default(), depends_on: vec!["b".into(), "c".into()], completed: false, critical: true },
            ],
        }
    }

    #[test]
    fn test_layers_topological() {
        let plan = sample();
        let layers = plan.layers().unwrap();
        assert_eq!(layers[0], vec!["a"]);
        assert_eq!(layers[1].len(), 2); // b, c
        assert_eq!(layers[2], vec!["d"]);
    }

    #[test]
    fn test_cycle_detected() {
        let mut plan = sample();
        plan.node_mut("a").unwrap().depends_on = vec!["d".into()];
        assert!(plan.layers().is_err());
    }

    #[test]
    fn test_missing_dependency_detected() {
        let mut plan = sample();
        plan.node_mut("a").unwrap().depends_on = vec!["ghost".into()];
        assert!(plan.validate().is_err());
    }

    #[test]
    fn test_grow_requires_completed_deps() {
        let mut plan = sample();
        let ok = plan.grow(DagNode { id: "e".into(), description: "nuevo".into(), tool: "t".into(), arguments: Default::default(), depends_on: vec!["a".into()], completed: false, critical: true });
        assert_eq!(ok.unwrap(), false); // a no completado
        plan.node_mut("a").unwrap().completed = true;
        let ok = plan.grow(DagNode { id: "e".into(), description: "nuevo".into(), tool: "t".into(), arguments: Default::default(), depends_on: vec!["a".into()], completed: false, critical: true });
        assert_eq!(ok.unwrap(), true);
    }

    #[test]
    fn test_execute_layers_fails_closed_on_critical() {
        let plan = sample();
        // Capa 2: b (crítico) no se completa -> error fail-closed.
        let result = execute_layers(&plan, |layer| {
            if layer.contains(&"b".to_string()) {
                Ok(vec!["c".to_string()]) // solo c
            } else {
                Ok(layer.to_vec())
            }
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_layers_ok() {
        let plan = sample();
        let executed = execute_layers(&plan, |layer| Ok(layer.to_vec())).unwrap();
        assert_eq!(executed.len(), 4);
        assert!(plan.progress() >= 0.0);
    }

    #[test]
    fn test_parse_plan_validates() {
        let json = r#"{"nodes":[{"id":"a","description":"x","tool":"t","depends_on":[],"completed":false,"critical":true}]}"#;
        assert!(parse_plan(json).is_ok());
        let bad = r#"{"nodes":[{"id":"a","description":"x","tool":"t","depends_on":["z"],"completed":false,"critical":true}]}"#;
        assert!(parse_plan(bad).is_err());
    }

    #[test]
    fn test_linear_to_dag() {
        let steps = vec![
            ("uno".to_string(), "t1".to_string(), serde_json::json!({})),
            ("dos".to_string(), "t2".to_string(), serde_json::json!({})),
        ];
        let plan = linear_to_dag(&steps);
        assert_eq!(plan.layers().unwrap(), vec![vec!["n0".to_string()], vec!["n1".to_string()]]);
    }
}