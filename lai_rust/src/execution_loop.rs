//! ExecutionLoop - Estados: PLAN -> LOCATE -> EDIT -> VERIFY -> REPORT (Regla 1).

use crate::context::{ContextEngine, TaskContext};
use crate::llm::LlmClient;
use crate::policy::PolicyEngine;
use crate::quality::{Language, Verifier, VerificationReport};
use crate::repomap::RepoMap;
use crate::tools::ToolRegistry;
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex as SyncMutex};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecState { Plan, Locate, Edit, Verify, Report }

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
        Self { max_turns: 20, max_steps: 10, token_budget: 50_000, yolo: false }
    }
}

pub struct ExecutionLoop {
    pub goal: String,
    pub config: ExecConfig,
    pub tools: Arc<ToolRegistry>,
    pub llm: Arc<LlmClient>,
    pub policy: Arc<SyncMutex<PolicyEngine>>,
    pub context_engine: Arc<SyncMutex<ContextEngine>>,
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
        policy: Arc<SyncMutex<PolicyEngine>>,
        context_engine: Arc<SyncMutex<ContextEngine>>,
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
            context_engine,
            verifier: Verifier::new(language, &goal),
            context: TaskContext {
                task_id,
                objective: goal.clone(),
                items: Vec::new(),
                total_tokens: 0,
                token_budget: config.token_budget as u32,
                files_referenced: Vec::new(),
                symbols_referenced: Vec::new(),
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
        println!("\nExecutionLoop - Deterministic Agent Harness\nMeta: {}\n", self.goal);
        let base_url = self.llm.base_url.clone();
        let host = base_url
            .strip_prefix("http://")
            .or_else(|| base_url.strip_prefix("https://"))
            .unwrap_or(&base_url)
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .to_lowercase();
        let local_hosts = ["localhost", "127.0.0.1", "::1", "0.0.0.0"];
        let is_local = local_hosts.contains(&host.as_str())
            || host.ends_with(".local")
            || match host.parse::<IpAddr>() {
                Ok(ip) => match ip {
                    IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
                    IpAddr::V6(v6) => v6.is_loopback() || v6.is_unicast_link_local(),
                },
                Err(_) => false,
            };
        if !is_local {
            self.error = Some("Rule 4: local endpoint required".into());
            return Err(anyhow::anyhow!("{}", self.error.as_ref().unwrap()));
        }
        while self.turn < self.config.max_turns && self.step < self.config.max_steps {
            if !self.completed {
                self.turn += 1;
                println!("\nTurno {} | Estado: {:?}", self.turn, self.state);
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
                self.error = Some(format!("Token budget: {} >=", self.tokens_used, self.config.token_budget));
                break;
            }
        }
        if !self.completed && self.error.is_none() {
            self.error = Some(format!("Max turns/steps reached"));
        }
        self.finalize()
    }

    async fn step_plan(&mut self) -> Result<()> {
        let prompt = format!(
            "/no_think\nPlanifica esta meta:\nMeta: {}\n1. <desc> | Criterios: <c1>; <c2>",
            self.goal
        );
        let response = self.llm.chat(&prompt).await?;
        self.track_usage(&response);
        let steps = self.parse_plan(&response);
        self.step = steps.len();
        self.steps.push(StepResult {
            state: ExecState::Plan,
            ok: !steps.is_empty(),
            output: response.clone(),
            evidence: vec![format!("{} pasos", steps.len())],
            next_state: Some(ExecState::Locate),
        });
        self.state = ExecState::Locate;
        Ok(())
    }

    async fn step_locate(&mut self) -> Result<()> {
        let mut ce = self.context_engine.lock().unwrap();
        ce.load_repo_map(std::path::Path::new("."));
        let mut context = ce.build_context(&self.goal, self.config.token_budget);
        ce.truncate_to_budget(&mut context);
        drop(ce);
        self.context = context;
        self.steps.push(StepResult {
            state: ExecState::Locate,
            ok: true,
            output: "Contexto construido".into(),
            evidence: vec![format!("{} tokens/{}", self.context.total_tokens, self.context.token_budget)],
            next_state: Some(ExecState::Edit),
        });
        self.state = ExecState::Edit;
        Ok(())
    }

    async fn step_edit(&mut self) -> Result<()> {
        let ctx_str = self.context.items.iter()
            .map(|i| format!("[{}] {}", i.source, i.content.chars().take(2000).collect::<String>()))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "/no_think\nTarea: {}\nContexto:\n{}\nAplica cambios con herramientas.\nMeta: {}",
            self.goal, ctx_str, self.goal
        );
        let response = self.llm.chat(&prompt).await?;
        self.track_usage(&response);
        self.steps.push(StepResult {
            state: ExecState::Edit,
            ok: true,
            output: response.clone(),
            evidence: vec!["Herramientas ejecutadas".into()],
            next_state: Some(ExecState::Verify),
        });
        self.state = ExecState::Verify;
        Ok(())
    }

    async fn step_verify(&mut self) -> Result<()> {
        let report = self.verifier.verify_static("", true).await;
        let ok = self.verifier.is_verified(&report, 85.0);
        self.steps.push(StepResult {
            state: ExecState::Verify,
            ok: ok,
            output: report.render(),
            evidence: report.checks.iter().map(|c| format!("{}: {}", c.id, c.detail)).collect(),
            next_state: if ok { Some(ExecState::Report) } else { Some(ExecState::Edit) },
        });
        if ok {
            self.state = ExecState::Report;
        } else {
            self.state = ExecState::Edit;
        }
        Ok(())
    }

    async fn step_report(&mut self) -> Result<()> {
        self.completed = true;
        println!(
            "\n=== REPORTE ===\nMeta: {}\nEstado: COMPLETADA\nTurnos: {}/{}\nPasos: {}\nTokens: {}",
            self.goal, self.turn, self.config.max_turns, self.steps.len(), self.tokens_used
        );
        for s in &self.steps {
            println!("[{:?}] {} (ok={})", s.state, s.output, s.ok);
        }
        Ok(())
    }

    fn parse_plan(&self, content: &str) -> Vec<String> {
        let mut s = Vec::new();
        for l in content.lines() {
            let t = l.trim();
            let r = t.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');
            if !r.is_empty() {
                s.push(r.to_string());
            }
        }
        s
    }

    fn track_usage(&mut self, response: &str) {
        let e = (response.len() / 4) as u64;
        self.tokens_used += e;
        if let Ok(mut p) = self.policy.lock() {
            p.add_usage(e, 0);
        }
    }

    fn finalize(&mut self) -> Result<String> {
        let mut r = String::new();
        r.push_str(&format!(
            "Resultado: {}\nEstado: {}\nTurnos: {}/{}\nTokens: {}\n",
            self.goal,
            if self.completed { "Completada" } else { "Incompleta" },
            self.turn,
            self.config.max_turns,
            self.tokens_used
        ));
        if let Some(e) = &self.error {
            r.push_str(&format!("Error: {}\n", e));
        }
        Ok(r)
    }
}
