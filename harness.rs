//! harness.rs - ExecutionHarness con checkpoints y recuperación (v5.0).

use crate::context::ContextEngine;
use crate::execution_loop::{ExecutionConfig, ExecutionEngine, ExecutionContext, ExecutionState, ExecutionResult, TaskSpec, PlanStep, Verdict};
use crate::policy::PolicyEngine;
use crate::repomap::RepoMap;
use crate::telemetry::{record_checkpoint, task_span, end_task_span, record_error};
use crate::tools::ToolRegistry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub task_id: String,
    pub timestamp: String,
    pub state: ExecutionState,
    pub step_index: usize,
    pub description: String,
    pub files_modified: Vec<String>,
    pub context_snapshot: serde_json::Value,
    pub plan_snapshot: Vec<PlanStep>,
    pub repomap_hash: Option<String>, // v5.0 Mejora 4.2: estado del índice RepoMap
}

#[derive(Debug, Clone)]
pub struct HarnessConfig {
    pub max_iterations: usize,
    pub max_tool_turns: usize,
    pub enable_checkpoints: bool,
    pub checkpoint_dir: PathBuf,
    pub policy_engine: PolicyEngine,
    pub auto_approve: bool,
    pub working_dir: PathBuf,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        let checkpoint_dir = std::env::var("LAI_CHECKPOINT_DIR")
            .ok().map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/lai-checkpoints"));
        Self {
            max_iterations: 10,
            max_tool_turns: 20,
            enable_checkpoints: true,
            checkpoint_dir,
            policy_engine: PolicyEngine::default(),
            auto_approve: false,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}

pub struct ExecutionHarness {
    config: HarnessConfig,
    llm: crate::llm::LlmClient,
    tools: ToolRegistry,
    context_engine: ContextEngine,
    recovery_mode: bool,
    recovered_context: Option<ExecutionContext>,
}

impl ExecutionHarness {
    pub fn new(config: HarnessConfig, llm: crate::llm::LlmClient) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        fs::create_dir_all(&config.checkpoint_dir)?;
        let tools = ToolRegistry::new();
        let context_engine = ContextEngine::new(config.working_dir.clone())?;
        Ok(Self {
            config, llm, tools, context_engine,
            recovery_mode: false, recovered_context: None,
        })
    }

    pub fn new_yolo(config: HarnessConfig, llm: crate::llm::LlmClient) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut harness = Self::new(config, llm)?;
        harness.config.auto_approve = true;
        Ok(harness)
    }

    pub async fn execute(&mut self, description: &str, task_type: &str) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let task_id = format!("task-{}-{}", task_type, uuid::Uuid::new_v4().to_string()[..8].to_string());
        task_span(&task_id, task_type, description);

        if self.recovery_mode {
            if let Some(ctx) = self.recovered_context.take() {
                crate::telemetry::record_checkpoint(&ctx.task_id, "resume", "Resuming from recovery", vec![], serde_json::json!({}));
                return self.resume_execution(ctx).await;
            }
        }

        let exec_config = ExecutionConfig {
            max_iterations: self.config.max_iterations,
            max_tool_turns: self.config.max_tool_turns,
            require_verification_level_5: true,
            enable_checkpoints: self.config.enable_checkpoints,
        };
        let mut engine = crate::execution_loop::ExecutionEngine::new(exec_config);
        let result = engine.execute(description, task_type).await?;

        // Checkpoint final
        if self.config.enable_checkpoints {
            // Necesitamos el ctx para crear checkpoint, pero engine.execute consume todo.
            // Aquí guardamos un checkpoint de resultado.
            let _ = self.create_checkpoint(&task_id, "final", "Final state", &result.files_modified, serde_json::json!({}), None).await;
        }

        end_task_span(&task_id, result.verdict == Verdict::Success || result.verdict == Verdict::SuccessWithDeclaredGaps, result.duration_ms.into(), &result.summary);
        Ok(result)
    }

    async fn resume_execution(&mut self, mut ctx: ExecutionContext) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        crate::telemetry::task_span(&ctx.task_id, "resume", &ctx.task_spec.objective);
        let exec_config = ExecutionConfig {
            max_iterations: self.config.max_iterations,
            max_tool_turns: self.config.max_tool_turns,
            require_verification_level_5: true,
            enable_checkpoints: self.config.enable_checkpoints,
        };
        let mut engine = crate::execution_loop::ExecutionEngine::new(exec_config);
        // Re-anotar: engine.execute crea un nuevo contexto, así que no podemos reusar ctx directamente.
        // Para simplicidad, ejecutamos una tarea nueva basada en el objetivo recuperado.
        let result = engine.execute(&ctx.task_spec.objective, "resume").await?;
        Ok(result)
    }

    async fn create_checkpoint(&self, task_id: &str, checkpoint_id: &str, description: &str, files_modified: &[String], context_snapshot: serde_json::Value, repomap_hash: Option<String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let checkpoint = Checkpoint {
            id: checkpoint_id.to_string(),
            task_id: task_id.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            state: ExecutionState::Edit, // placeholder
            step_index: 0,
            description: description.to_string(),
            files_modified: files_modified.to_vec(),
            context_snapshot,
            plan_snapshot: vec![],
            repomap_hash,
        };
        let path = self.config.checkpoint_dir.join(format!("{}-{}.json", task_id, checkpoint_id));
        fs::write(&path, serde_json::to_string_pretty(&checkpoint)?)?;
        record_checkpoint(task_id, checkpoint_id, description, files_modified.to_vec(), context_snapshot);
        Ok(())
    }

    pub fn config(&self) -> &HarnessConfig { &self.config }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_config_default() {
        let config = HarnessConfig::default();
        assert_eq!(config.max_iterations, 10);
    }
}
