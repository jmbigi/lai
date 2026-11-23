//! execution_loop.rs - Máquina de estados determinista para el agente.
//! Estados: PLAN → LOCATE → EDIT → VERIFY → REPORT
//! El LLM propone, el runtime decide y verifica. Regla 1.

use crate::llm::{ChatResult, LlmClient, Message};
use crate::policy::PolicyEngine;
use crate::quality::{Language, Verifier, VerificationReport};
use crate::repomap::RepoMap;
use crate::tools::ToolRegistry;
use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecState {
    Plan,
    Locate,
    Edit,
    Verify,
    Report,
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub state: ExecState,
    pub ok: bool,
    pub output: String,
    pub evidence: Vec<String>,
    pub next_state: Option<ExecState>,
}

#[derive(Debug, Clone)]
pub struct ExecConfig {
    pub max_turns: usize,
    pub max_steps: usize,
    pub token_budget: u64,
    pub yolo: bool,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            max_turns: 20,
            max_steps: 10,
            token_budget: 50_000,
            yolo: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskContext {
    pub task_id: String,
    pub objective: String,
    pub items: Vec<ContextItem>,
    pub total_tokens: u32,
    pub token_budget: u32,
    pub generated_at: String,
}

#[derive(Debug, Clone)]
pub struct ContextItem {
    pub id: String,
    pub priority: u8,
    pub content: String,
    pub source: String,
    pub token_estimate: u32,
}

pub struct ExecutionLoop {
    pub goal: String,
    pub config: ExecConfig,
    pub tools: Arc<ToolRegistry>,
    pub llm: Arc<LlmClient>,
    pub policy: Arc<std::sync::Mutex<PolicyEngine>>,
    pub verifier: Verifier,
    pub context: TaskContext,
    pub state: ExecState,
    pub turn: usize,
    pub step: usize,
    pub tokens_used: u64,
    pub steps: Vec<StepResult>,
    pub completed: bool,
    pub error: Option<String>,
    pub start_time: Instant,
}

impl ExecutionLoop {
    pub fn new(
        goal: String,
        tools: Arc<ToolRegistry>,
        llm: Arc<LlmClient>,
        policy: Arc<std::sync::Mutex<PolicyEngine>>,
        language: Language,
        config: ExecConfig,
    ) -> Self {
        let task_id = format!("task-{}", Utc::now().timestamp());
        Self {
            goal,
            config,
            tools,
            llm,
            policy,
            verifier: Verifier::new(language, &goal),
            context: TaskContext {
                task_id,
                objective: goal.clone(),
                items: Vec::new(),
                total_tokens: 0,
                token_budget: config.token_budget as u32,
                generated_at: Utc::now().to_rfc3339(),
            },
            state: ExecState::Plan,
            turn: 0,
            step: 0,
            tokens_used: 0,
            steps: Vec::new(),
            completed: false,
            error: None,
            start_time: Instant::now(),
        }
    }

    pub async fn run(&mut self) -> Result<String> {
        println!("\n╔════════════════════════════════════════════════════════════╗");
        println!("║  lai ExecutionLoop - Deterministic Agent Harness          ║");
        println!("╚════════════════════════════════════════════════════════════╝");
        println!("Meta: {}\n", self.goal);

        if !LlmClient::is_local_url(&self.llm.base_url) {
            self.error = Some("ExecutionLoop requires a local model endpoint (Rule 4)".into());
            return Err(anyhow::anyhow!("{}", self.error.as_ref().unwrap()));
        }

        while self.turn < self.config.max_turns && self.step < self.config.max_steps {
            if !self.completed {
                self.turn += 1;
                println!("\n--- Turno {} | Estado: {:?} ---", self.turn, self.state);
            }

            match self.state {
                ExecState::Plan => self.step_plan().await?,
                ExecState::Locate => self.step_locate().await?,
                ExecState::Edit => self.step_edit().await?,
                ExecState::Verify => self.step_verify().await?,
                ExecState::Report => {
                    self.step_report().await?;
                    break;
                }
            }

            if self.tokens_used >= self.config.token_budget {
                self.error = Some(format!(
                    "Token budget exceeded: {} >= {}",
                    self.tokens_used, self.config.token_budget
                ));
                break;
            }
        }

        if !self.completed && self.error.is_none() {
            self.error = Some(format!(
                "Max turns ({}) or steps ({}) reached without completion",
                self.config.max_turns, self.config.max_steps
            ));
        }

        self.finalize()
    }

    async fn step_plan(&mut self) -> Result<()> {
        let prompt = format!(
            "/no_think\nEres un planificador de tareas. Descompón esta meta en pasos pequeños y verificables:\n\nMeta: {}\n\nDevuelve SOLO una lista numerada:\n1. <descripción> | Criterios: <c1>; <c2>\n\nNo escribas código, solo planifica.",
            self.goal
        );

        let response = self
            .llm
            .chat_messages(
                vec![
                    Message { role: "system".into(), content: "Eres un planificador experto.".into() },
                    Message { role: "user".into(), content: prompt },
                ],
                None,
                false,
                false,
            )
            .await;

        if let Some(err) = &response.error {
            self.error = Some(format!("Plan error: {}", err));
            return Err(anyhow::anyhow!("{}", err));
        }

        self.track_usage(&response);

        let steps = self.parse_plan(&response.content);
        self.step = steps.len();

        self.steps.push(StepResult {
            state: ExecState::Plan,
            ok: !steps.is_empty(),
            output: response.content.clone(),
            evidence: vec![format!("Plan generado con {} pasos", steps.len())],
            next_state: Some(ExecState::Locate),
        });

        self.state = ExecState::Locate;
        Ok(())
    }

    async fn step_locate(&mut self) -> Result<()> {
        let repo_map_result = self
            .tools
            .call_tool(
                "repo_map",
                serde_json::json!({ "path": ".", "max_chars": 8000 })
                    .as_object()
                    .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<HashMap<String, Value>>())
                    .unwrap_or_default(),
            )
            .await;

        match repo_map_result {
            Ok(map_content) => {
                let len = map_content.len();
                self.context.items.push(ContextItem {
                    id: "repo_map".into(),
                    priority: 1,
                    content: map_content,
                    source: "repo_map".into(),
                    token_estimate: len as u32 / 4,
                });
                self.context.total_tokens += len as u32 / 4;

                self.steps.push(StepResult {
                    state: ExecState::Locate,
                    ok: true,
                    output: "Contexto del repositorio construido".into(),
                    evidence: vec![format!("RepoMap: {} chars", len)],
                    next_state: Some(ExecState::Edit),
                });
            }
            Err(e) => {
                eprintln!("Warning: repo_map failed: {}", e);
                self.steps.push(StepResult {
                    state: ExecState::Locate,
                    ok: false,
                    output: format!("repo_map failed: {}", e),
                    evidence: vec![],
                    next_state: Some(ExecState::Edit),
                });
            }
        }

        self.state = ExecState::Edit;
        Ok(())
    }

    async fn step_edit(&mut self) -> Result<()> {
        let context_str = self
            .context
            .items
            .iter()
            .map(|i| format!("[{}] {}", i.source, i.content.chars().take(2000).collect::<String>()))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "/no_think\nEres un programador autónomo. Tarea: {}\n\nContexto disponible:\n{}\n\nAplica los cambios necesarios usando las herramientas disponibles.\nMeta: {}\n\nCuando termines, responde sin llamar a herramientas.",
            self.goal, context_str, self.goal
        );

        let tools_schema = self.tools.get_tools_schema();
        let tools_value: Vec<Value> = tools_schema
            .iter()
            .map(|t| serde_json::to_value(t).unwrap())
            .collect();

        let response = self
            .llm
            .chat_messages(
                vec![
                    Message { role: "system".into(), content: "Eres un ejecutor experto con acceso a herramientas.".into() },
                    Message { role: "user".into(), content: prompt },
                ],
                Some(tools_value),
                false,
                false,
            )
            .await;

        if let Some(err) = &response.error {
            self.error = Some(format!("Edit error: {}", err));
            return Err(anyhow::anyhow!("{}", err));
        }

        self.track_usage(&response);

        let had_tool_calls = !response.tool_calls.is_empty();

        for call in &response.tool_calls {
            let args: HashMap<String, Value> =
                serde_json::from_str(&call.function.arguments).unwrap_or_default();
            let _result = self.tools.call_tool(&call.function.name, args).await;
        }

        self.steps.push(StepResult {
            state: ExecState::Edit,
            ok: true,
            output: response.content.clone(),
            evidence: if had_tool_calls {
                vec!["Tool calls ejecutados".into()]
            } else {
                vec!["No se requirieron tool calls".into()]
            },
            next_state: Some(ExecState::Verify),
        });

        self.state = ExecState::Verify;
        Ok(())
    }

    async fn step_verify(&mut self) -> Result<()> {
        let report = self.verifier.verify_static("", true).await;
        let is_verified = self.verifier.is_verified(&report, 85.0);

        self.steps.push(StepResult {
            state: ExecState::Verify,
            ok: is_verified,
            output: report.render(),
            evidence: report.checks.iter().map(|c| format!("{}: {}", c.id, c.detail)).collect(),
            next_state: if is_verified { Some(ExecState::Report) } else { Some(ExecState::Edit) },
        });

        if is_verified {
            self.state = ExecState::Report;
        } else {
            self.state = ExecState::Edit;
        }

        Ok(())
    }

    async fn step_report(&mut self) -> Result<()> {
        self.completed = true;

        println!("\n=== REPORTE FINAL ===");
        println!("Meta: {}", self.goal);
        println!("Estado: COMPLETADA");
        println!("Turnos: {}/{}", self.turn, self.config.max_turns);
        println!("Pasos: {}", self.steps.len());
        println!("Tokens estimados: {}", self.tokens_used);
        println!("Tiempo: {:?}", self.start_time.elapsed());

        for step in &self.steps {
            println!(
                "\n[{:?}] {} (ok={})",
                step.state,
                if step.ok { "éxito" } else { "fallo" },
                step.ok
            );
            for ev in &step.evidence {
                println!("  - {}", ev);
            }
        }

        Ok(())
    }

    fn parse_plan(&self, content: &str) -> Vec<String> {
        let mut steps = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            let rest = trimmed.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
            if !rest.is_empty() {
                steps.push(rest.to_string());
            }
        }
        steps
    }

    fn track_usage(&mut self, response: &ChatResult) {
        let estimated = (response.content.len() / 4) as u64;
        self.tokens_used += estimated;
        if let Ok(mut policy) = self.policy.lock() {
            policy.add_usage(estimated, 0);
        }
    }

    fn finalize(&mut self) -> Result<String> {
        let mut report = String::new();
        report.push_str(&format!("Resultado de la ejecución: {}\n", self.goal));
        report.push_str(&format!(
            "Estado: {}\n",
            if self.completed { "Completada" } else { "Incompleta" }
        ));
        if let Some(ref err) = self.error {
            report.push_str(&format!("Error: {}\n", err));
        }
        report.push_str(&format!("Turnos: {}/{}\n", self.turn, self.config.max_turns));
        report.push_str(&format!("Tokens: {}\n", self.tokens_used));
        report.push_str("\nPasos ejecutados:\n");
        for step in &self.steps {
            report.push_str(&format!("  {:?}: {} (ok={})\n", step.state, step.output, step.ok));
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_config_default() {
        let config = ExecConfig::default();
        assert_eq!(config.max_turns, 20);
        assert_eq!(config.token_budget, 50_000);
    }
}
