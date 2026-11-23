//! Agent Execution Harness - Core runtime for autonomous software engineering tasks.
//! Provides deterministic execution loop, policy enforcement, checkpoints, and recovery.
//!
//! Fase 0/1 (DOCS/ARQUITECTURA_TECNICA_LAI.md):
//! - Deny del PolicyEngine es incondicional (ni YOLO ni auto lo saltan).
//! - Ningún SUCCESS sin verificaciones superadas.
//! - Verificación limitada a archivos modificados + tests requeridos + criterios.

use crate::telemetry::{
    end_task_span, record_checkpoint, record_llm_call, record_policy_decision,
    record_repair_cycle, record_tool_call, task_span,
};
use crate::policy::{Operation, PolicyEngine};
use crate::context::ContextEngine;
use crate::tools::ToolRegistry;
use crate::llm::LlmClient;
use anyhow::{Context as AnyhowContext, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Maximum iterations for a task before forcing completion
pub const DEFAULT_MAX_ITERATIONS: usize = 10;
/// Maximum tool turns per iteration
pub const DEFAULT_MAX_TOOL_TURNS: usize = 20;
/// Default timeout for tool execution (seconds)
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 120;
/// Default timeout for LLM calls (seconds)
pub const DEFAULT_LLM_TIMEOUT_SECS: u64 = 300;

/// Execution state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionState {
    /// Initializing the task
    Intake,
    /// Inspecting repository and gathering context
    Inspect,
    /// Planning the execution
    Plan,
    /// Locating relevant files and symbols
    Locate,
    /// Applying edits/modifications
    Edit,
    /// Running verification (tests, lint, type-check)
    Verify,
    /// Repairing issues found during verification
    Repair,
    /// Final reporting
    Report,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task cancelled
    Cancelled,
}

/// A single step in the execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub phase: ExecutionState,
    pub description: String,
    pub tool: Option<String>,
    pub arguments: Option<serde_json::Value>,
    pub expected_outcome: String,
    pub dependencies: Vec<String>,
    pub completed: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Task specification (Spec-Driven Development)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub id: String,
    pub task_type: String,
    pub objective: String,
    pub current_state: String,
    pub problem: Option<String>,
    pub scope: Vec<String>,
    pub constraints: Vec<String>,
    pub affected_files: Vec<String>,
    pub dependencies: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub required_tests: Vec<String>,
    pub risks: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Execution context for a task
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub task_id: String,
    pub task_spec: TaskSpec,
    pub state: ExecutionState,
    pub plan: Vec<PlanStep>,
    pub current_step: usize,
    pub repo_map: Option<crate::repomap::RepoMap>,
    pub context_engine: ContextEngine,
    pub checkpoints: Vec<Checkpoint>,
    pub iteration: usize,
    pub max_iterations: usize,
    pub tool_turns: usize,
    pub max_tool_turns: usize,
    pub start_time: Instant,
    pub last_checkpoint: Option<Instant>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// A checkpoint for recovery
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
}

/// Result of a task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub task_id: String,
    pub success: bool,
    pub final_state: ExecutionState,
    pub iterations: usize,
    pub total_tool_turns: usize,
    pub duration_ms: u64,
    pub files_modified: Vec<String>,
    pub verification_results: Vec<VerificationResult>,
    pub summary: String,
    pub error: Option<String>,
}

/// Verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub verifier: String,
    pub passed: bool,
    pub details: String,
    pub artifacts: Vec<String>,
}

/// Harness configuration
pub struct HarnessConfig {
    pub max_iterations: usize,
    pub max_tool_turns: usize,
    pub tool_timeout: Duration,
    pub llm_timeout: Duration,
    pub enable_checkpoints: bool,
    pub checkpoint_interval: Duration,
    pub checkpoint_dir: PathBuf,
    pub policy_engine: PolicyEngine,
    /// Modo YOLO: auto-aprobar. NUNCA convierte un Deny en ejecución.
    pub auto_approve: bool,
    pub working_dir: PathBuf,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        let checkpoint_dir = std::env::var("LAI_CHECKPOINT_DIR")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/lai-checkpoints"));

        Self {
            max_iterations: DEFAULT_MAX_ITERATIONS,
            max_tool_turns: DEFAULT_MAX_TOOL_TURNS,
            tool_timeout: Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS),
            llm_timeout: Duration::from_secs(DEFAULT_LLM_TIMEOUT_SECS),
            enable_checkpoints: true,
            checkpoint_interval: Duration::from_secs(60),
            checkpoint_dir,
            policy_engine: PolicyEngine::default(),
            auto_approve: false,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}

/// Main Execution Harness
pub struct ExecutionHarness {
    config: HarnessConfig,
    llm: LlmClient,
    tools: ToolRegistry,
    context_engine: ContextEngine,
    // Recovery state
    recovery_mode: bool,
    recovered_context: Option<ExecutionContext>,
}

impl ExecutionHarness {
    /// Create a new execution harness
    pub fn new(config: HarnessConfig, llm: LlmClient) -> Result<Self> {
        fs::create_dir_all(&config.checkpoint_dir)?;
        let tools = ToolRegistry::new();
        let context_engine = ContextEngine::new(config.working_dir.clone())?;

        Ok(Self {
            config,
            llm,
            tools,
            context_engine,
            recovery_mode: false,
            recovered_context: None,
        })
    }

    /// Create harness with YOLO mode (auto-approve confirmations).
    /// Deny del PolicyEngine sigue siendo incondicional.
    pub fn new_yolo(config: HarnessConfig, llm: LlmClient) -> Result<Self> {
        let mut harness = Self::new(config, llm)?;
        harness.config.auto_approve = true;
        Ok(harness)
    }

    /// Execute a task from a natural language description
    pub async fn execute(&mut self, description: &str, task_type: &str) -> Result<ExecutionResult> {
        let task_id = format!("task-{}-{}", task_type, &Uuid::new_v4().to_string()[..8]);
        let span = task_span(&task_id, task_type, description);
        let _enter = span.enter();

        info!("Starting task: {} ({})", task_id, description);

        // Check for recovery
        if self.recovery_mode {
            if let Some(ctx) = self.recovered_context.take() {
                info!("Resuming from recovery: {}", ctx.task_id);
                return self.resume_execution(ctx).await;
            }
        }

        // Phase 1: INTAKE - Create task specification
        let task_spec = self.intake_phase(&task_id, description, task_type).await?;

        // Phase 2: INSPECT - Analyze repository
        let repo_map = self.inspect_phase(&task_id, &task_spec).await?;

        // Phase 3: PLAN - Create execution plan
        let plan = self.plan_phase(&task_id, &task_spec, &repo_map).await?;

        // Initialize execution context
        let mut ctx = ExecutionContext {
            task_id: task_id.clone(),
            task_spec,
            state: ExecutionState::Locate,
            plan,
            current_step: 0,
            repo_map: Some(repo_map),
            context_engine: self.context_engine.clone(),
            checkpoints: Vec::new(),
            iteration: 0,
            max_iterations: self.config.max_iterations,
            tool_turns: 0,
            max_tool_turns: self.config.max_tool_turns,
            start_time: Instant::now(),
            last_checkpoint: Some(Instant::now()),
            metadata: HashMap::new(),
        };

        // Main execution loop
        let result = self.run_execution_loop(&mut ctx).await?;

        // Final checkpoint
        if self.config.enable_checkpoints {
            self.create_checkpoint(&ctx, "final", "Final state before completion").await?;
        }

        end_task_span(&task_id, result.success, ctx.start_time.elapsed(), &result.summary);
        Ok(result)
    }

    /// Resume execution from a recovered context
    async fn resume_execution(&mut self, mut ctx: ExecutionContext) -> Result<ExecutionResult> {
        info!("Resuming task: {} from iteration {}", ctx.task_id, ctx.iteration);
        ctx.state = ExecutionState::Edit; // Resume at edit phase
        let result = self.run_execution_loop(&mut ctx).await?;
        end_task_span(&ctx.task_id, result.success, ctx.start_time.elapsed(), &result.summary);
        Ok(result)
    }

    /// Main execution loop: EDIT -> VERIFY -> REPAIR
    async fn run_execution_loop(&mut self, ctx: &mut ExecutionContext) -> Result<ExecutionResult> {
        let mut verification_results = Vec::new();
        let mut files_modified = Vec::new();

        while ctx.iteration < ctx.max_iterations {
            ctx.iteration += 1;
            info!("Iteration {}/{}", ctx.iteration, ctx.max_iterations);

            // Check for timeout
            if ctx.start_time.elapsed() > Duration::from_secs(3600) {
                return Ok(self.finalize_result(ctx, false, "Task timeout".to_string(), verification_results, files_modified));
            }

            // EDIT phase
            ctx.state = ExecutionState::Edit;
            let edit_result = self.edit_phase(ctx).await?;
            files_modified.extend(edit_result.files_modified);
            files_modified.sort();
            files_modified.dedup();

            // Checkpoint after edit
            if self.config.enable_checkpoints {
                self.create_checkpoint(ctx, &format!("edit-iter-{}", ctx.iteration), "After edit phase").await?;
            }

            // VERIFY phase (solo archivos modificados + tests requeridos + criterios)
            ctx.state = ExecutionState::Verify;
            let verify_results = self.verify_phase(ctx, &files_modified).await?;
            verification_results.extend(verify_results.clone());

            // Check if all verifications passed
            let all_passed = verify_results.iter().all(|v| v.passed);
            if all_passed {
                info!("All verifications passed!");
                ctx.state = ExecutionState::Completed;
                return Ok(self.finalize_result(ctx, true, "All verifications passed".to_string(), verification_results, files_modified));
            }

            // REPAIR phase
            ctx.state = ExecutionState::Repair;
            let repair_result = self.repair_phase(ctx, &verify_results).await?;

            if !repair_result.progress_made {
                warn!("No progress in repair phase, stopping");
                return Ok(self.finalize_result(ctx, false, "No progress in repair".to_string(), verification_results, files_modified));
            }

            // Checkpoint after repair
            if self.config.enable_checkpoints {
                self.create_checkpoint(ctx, &format!("repair-iter-{}", ctx.iteration), "After repair phase").await?;
            }
        }

        // Max iterations reached
        Ok(self.finalize_result(ctx, false, "Max iterations reached".to_string(), verification_results, files_modified))
    }

    /// Phase 1: INTAKE - Create task specification from description
    async fn intake_phase(&self, task_id: &str, description: &str, task_type: &str) -> Result<TaskSpec> {
        let prompt = format!(
            r#"You are lai, an autonomous software engineering agent. Analyze this task and create a structured specification.

Task Type: {}
Description: {}

Generate a JSON specification with these fields:
- objective: Clear, concise objective
- current_state: What you know about the current state
- problem: The problem to solve (if any)
- scope: List of areas/files likely affected
- constraints: Technical/business constraints
- affected_files: Specific files likely to be modified
- dependencies: External dependencies
- acceptance_criteria: Measurable criteria for success
- required_tests: Tests that should pass
- risks: Potential risks and side effects

Return ONLY valid JSON."#,
            task_type, description
        );

        let result = timeout(self.config.llm_timeout, self.llm.chat(&prompt, false, true)).await?;
        if let Some(err) = result.error {
            return Err(anyhow::anyhow!("LLM error in intake: {}", err));
        }

        let spec: TaskSpec = self.parse_json_response(&result.content, task_id, task_type, description)?;
        Ok(spec)
    }

    /// Phase 2: INSPECT - Analyze repository structure
    async fn inspect_phase(&mut self, task_id: &str, spec: &TaskSpec) -> Result<crate::repomap::RepoMap> {
        info!("Inspecting repository for task: {}", task_id);

        // Build or update repo map
        let repo_map = self.context_engine.build_repo_map(&self.config.working_dir).await?;

        // Enhance context with relevant files
        let relevant_files = self.find_relevant_files(&repo_map, spec);
        self.context_engine.add_files(&relevant_files).await?;

        Ok(repo_map)
    }

    /// Phase 3: PLAN - Create execution plan
    async fn plan_phase(&self, task_id: &str, spec: &TaskSpec, repo_map: &crate::repomap::RepoMap) -> Result<Vec<PlanStep>> {
        let repo_summary = repo_map.summary();
        let context_summary = self.context_engine.summary();

        let prompt = format!(
            r#"You are lai, creating an execution plan for a software engineering task.

Task Specification:
{}

Repository Summary:
{}

Current Context:
{}

Create a step-by-step execution plan as JSON array of steps. Each step must have:
- id: Unique step ID
- phase: One of [Locate, Edit, Verify, Repair]
- description: What this step does
- tool: Tool name to use (if any)
- arguments: Tool arguments as JSON (if any)
- expected_outcome: Expected result
- dependencies: List of step IDs this depends on
- completed: false
- result: null
- error: null

Focus on: locate relevant code -> make edits -> verify -> repair if needed.
Return ONLY valid JSON array."#,
            serde_json::to_string_pretty(spec)?,
            repo_summary,
            context_summary
        );

        let result = timeout(self.config.llm_timeout, self.llm.chat(&prompt, false, true)).await?;
        if let Some(err) = result.error {
            return Err(anyhow::anyhow!("LLM error in planning: {}", err));
        }

        let plan: Vec<PlanStep> = self.parse_json_response(&result.content, task_id, "plan", "")?;
        Ok(plan)
    }

    /// EDIT phase - Execute plan steps
    async fn edit_phase(&mut self, ctx: &mut ExecutionContext) -> Result<EditResult> {
        let mut files_modified = Vec::new();
        let mut steps_executed = 0;

        while ctx.current_step < ctx.plan.len() && ctx.tool_turns < ctx.max_tool_turns {
            let step = ctx.plan[ctx.current_step].clone();

            if step.completed {
                ctx.current_step += 1;
                continue;
            }

            // Check dependencies
            let deps_met = step.dependencies.iter().all(|dep_id| {
                ctx.plan.iter().any(|s| s.id == *dep_id && s.completed)
            });
            if !deps_met {
                ctx.current_step += 1;
                continue;
            }

            info!("Executing step: {} - {}", step.id, step.description);

            // Policy check for tool execution.
            // Deny es INCONDICIONAL: auto_approve nunca lo convierte en ejecución.
            if let Some(tool_name) = &step.tool {
                let operation = Operation::ToolCall {
                    tool: tool_name.clone(),
                    args: step.arguments.clone().unwrap_or_default(),
                };

                let decision = self.config.policy_engine.check(&operation, &ctx.task_id);
                record_policy_decision(&ctx.task_id, tool_name, decision.allowed, &decision.reason, &decision.policy);

                if !decision.allowed {
                    warn!("Policy denied tool call: {} - {}", tool_name, decision.reason);
                    ctx.plan[ctx.current_step].error = Some(format!("Policy denied: {}", decision.reason));
                    ctx.current_step += 1;
                    continue;
                }
            }

            // Execute step
            let result = if let Some(tool_name) = &step.tool {
                self.execute_tool_step(ctx, &step).await?
            } else {
                // LLM-driven step (e.g., analysis, planning)
                self.execute_llm_step(ctx, &step).await?
            };

            // Update step
            ctx.plan[ctx.current_step].completed = true;
            ctx.plan[ctx.current_step].result = Some(result.clone());
            steps_executed += 1;
            ctx.tool_turns += 1;

            // Track modified files
            if let Some(files) = result.get("files_modified").and_then(|v| v.as_array()) {
                for f in files {
                    if let Some(file_str) = f.as_str() {
                        files_modified.push(file_str.to_string());
                    }
                }
            }

            // Check for early completion
            if steps_executed >= 5 {
                break;
            }

            ctx.current_step += 1;
        }

        Ok(EditResult { files_modified })
    }

    /// Execute a tool step
    async fn execute_tool_step(&mut self, ctx: &mut ExecutionContext, step: &PlanStep) -> Result<serde_json::Value> {
        let tool_name = step.tool.as_ref().unwrap();
        let args = step.arguments.clone().unwrap_or_default();
        let start = Instant::now();

        debug!("Calling tool: {} with args: {}", tool_name, args);

        // Execute tool through registry (ToolRegistry re-valida con su PolicyEngine)
        let result = timeout(self.config.tool_timeout, self.tools.call(tool_name, args.clone())).await??;

        let duration = start.elapsed();
        let success = result.get("error").is_none();

        record_tool_call(
            &ctx.task_id,
            tool_name,
            args,
            Some(result.clone()),
            duration,
            success,
            result.get("error").and_then(|e| e.as_str()).map(|s| s.to_string()),
        );

        Ok(result)
    }

    /// Execute an LLM-driven step
    async fn execute_llm_step(&mut self, ctx: &mut ExecutionContext, step: &PlanStep) -> Result<serde_json::Value> {
        let context = self.context_engine.build_context_for_step(ctx, step).await?;
        let start = Instant::now();

        let result = timeout(self.config.llm_timeout, self.llm.chat(&context, false, true)).await?;
        if let Some(err) = &result.error {
            return Err(anyhow::anyhow!("LLM error in step {}: {}", step.id, err));
        }

        let duration = start.elapsed();
        record_llm_call(&ctx.task_id, &self.llm.model, None, None, duration, true, None);

        Ok(serde_json::json!({
            "content": result.content,
            "tool_calls": result.tool_calls
        }))
    }

    /// VERIFY phase - Run verification checks on modified files and requirements
    async fn verify_phase(&mut self, ctx: &mut ExecutionContext, files_modified: &[String]) -> Result<Vec<VerificationResult>> {
        info!("Running verification for task: {}", ctx.task_id);
        let mut results = Vec::new();

        // 1. Syntax check on modified Python files only (no full-repo scans)
        for path in files_modified {
            if path.ends_with(".py") {
                let file = crate::repomap::FileInfo {
                    path: path.clone(),
                    language: "python".to_string(),
                    size_bytes: 0,
                    symbols: vec![],
                    imports: vec![],
                };
                results.push(self.verify_python_file(&file).await?);
            }
        }

        // 2. Run tests if specified
        for test_spec in &ctx.task_spec.required_tests {
            results.push(self.run_test_spec(test_spec).await?);
        }

        // 3. Custom verifiers from acceptance criteria
        for criteria in &ctx.task_spec.acceptance_criteria {
            results.push(self.verify_criteria(criteria).await?);
        }

        Ok(results)
    }

    /// Verify a Python file
    async fn verify_python_file(&self, file: &crate::repomap::FileInfo) -> Result<VerificationResult> {
        let output = tokio::process::Command::new("python3")
            .args(["-m", "py_compile", &file.path])
            .output()
            .await?;

        Ok(VerificationResult {
            verifier: "py_compile".to_string(),
            passed: output.status.success(),
            details: if output.status.success() {
                "Syntax OK".to_string()
            } else {
                String::from_utf8_lossy(&output.stderr).to_string()
            },
            artifacts: vec![file.path.clone()],
        })
    }

    /// Run a test specification
    async fn run_test_spec(&self, spec: &str) -> Result<VerificationResult> {
        let output = tokio::process::Command::new("bash")
            .args(["-c", spec])
            .output()
            .await?;

        Ok(VerificationResult {
            verifier: "test".to_string(),
            passed: output.status.success(),
            details: String::from_utf8_lossy(&output.stdout).to_string(),
            artifacts: vec![],
        })
    }

    /// Verify acceptance criteria
    async fn verify_criteria(&self, criteria: &str) -> Result<VerificationResult> {
        let context = self.context_engine.current_context().await?;
        let prompt = format!(
            "Verify this acceptance criteria against the current codebase state:\n\nCriteria: {}\n\nContext:\n{}\n\nReturn JSON: {{\"passed\": bool, \"details\": \"...\"}}",
            criteria, context
        );

        let result = timeout(self.config.llm_timeout, self.llm.chat(&prompt, false, true)).await?;
        if let Some(err) = result.error {
            return Err(anyhow::anyhow!("LLM error in criteria verification: {}", err));
        }
        let verification: serde_json::Value = self.parse_json_response(&result.content, "", "", "")?;

        Ok(VerificationResult {
            verifier: "criteria".to_string(),
            passed: verification.get("passed").and_then(|v| v.as_bool()).unwrap_or(false),
            details: verification.get("details").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            artifacts: vec![],
        })
    }

    /// REPAIR phase - Fix issues found during verification
    async fn repair_phase(&mut self, ctx: &mut ExecutionContext, verify_results: &[VerificationResult]) -> Result<RepairResult> {
        let failed: Vec<_> = verify_results.iter().filter(|v| !v.passed).collect();
        if failed.is_empty() {
            return Ok(RepairResult { progress_made: false, issues_fixed: 0 });
        }

        info!("Repairing {} failed verifications", failed.len());
        let mut issues_fixed = 0;

        for failure in &failed {
            let repair_prompt = format!(
                r#"Fix this verification failure:

Verifier: {}
Details: {}
Artifacts: {:?}

Current context:
{}

Provide a concrete fix as a tool call or code change. Return JSON with:
- tool: tool name
- arguments: tool arguments
- explanation: what this fixes"#,
                failure.verifier,
                failure.details,
                failure.artifacts,
                self.context_engine.current_context().await?
            );

            let result = timeout(self.config.llm_timeout, self.llm.chat(&repair_prompt, false, true)).await?;
            if let Some(err) = result.error {
                warn!("LLM error in repair: {}", err);
                continue;
            }

            if let Ok(fix) = self.parse_json_response::<serde_json::Value>(&result.content, &ctx.task_id, "repair", "") {
                if let Some(tool) = fix.get("tool").and_then(|v| v.as_str()) {
                    let args = fix.get("arguments").cloned().unwrap_or_default();
                    let _ = self.execute_tool_step(ctx, &PlanStep {
                        id: format!("repair-{}", Uuid::new_v4()),
                        phase: ExecutionState::Repair,
                        description: fix.get("explanation").and_then(|v| v.as_str()).unwrap_or("Repair").to_string(),
                        tool: Some(tool.to_string()),
                        arguments: Some(args),
                        expected_outcome: "Fix verification failure".to_string(),
                        dependencies: vec![],
                        completed: false,
                        result: None,
                        error: None,
                    }).await;
                    issues_fixed += 1;
                }
            }
        }

        record_repair_cycle(&ctx.task_id, ctx.iteration as u32, "repair", failed.len() as u32, issues_fixed, issues_fixed > 0);

        Ok(RepairResult { progress_made: issues_fixed > 0, issues_fixed })
    }

    /// Finalize execution result
    fn finalize_result(
        &self,
        ctx: &ExecutionContext,
        success: bool,
        summary: String,
        verification_results: Vec<VerificationResult>,
        files_modified: Vec<String>,
    ) -> ExecutionResult {
        ExecutionResult {
            task_id: ctx.task_id.clone(),
            success,
            final_state: ctx.state,
            iterations: ctx.iteration,
            total_tool_turns: ctx.tool_turns,
            duration_ms: ctx.start_time.elapsed().as_millis() as u64,
            files_modified,
            verification_results,
            summary: summary.clone(),
            error: if success { None } else { Some(summary) },
        }
    }

    /// Create a checkpoint
    async fn create_checkpoint(&self, ctx: &ExecutionContext, checkpoint_id: &str, description: &str) -> Result<()> {
        let checkpoint = Checkpoint {
            id: checkpoint_id.to_string(),
            task_id: ctx.task_id.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            state: ctx.state,
            step_index: ctx.current_step,
            description: description.to_string(),
            files_modified: ctx.task_spec.affected_files.clone(),
            context_snapshot: self.context_engine.snapshot().await?,
            plan_snapshot: ctx.plan.clone(),
        };

        let path = self.config.checkpoint_dir.join(format!("{}-{}.json", ctx.task_id, checkpoint_id));
        fs::write(&path, serde_json::to_string_pretty(&checkpoint)?)?;

        record_checkpoint(
            &ctx.task_id,
            checkpoint_id,
            description,
            checkpoint.files_modified.clone(),
            checkpoint.context_snapshot.clone(),
        );

        debug!("Checkpoint created: {:?}", path);
        Ok(())
    }

    /// Find relevant files for a task
    fn find_relevant_files(&self, repo_map: &crate::repomap::RepoMap, spec: &TaskSpec) -> Vec<String> {
        let mut files = Vec::new();

        // Add explicitly mentioned files
        files.extend(spec.affected_files.clone());

        // Add files from scope
        for scope_item in &spec.scope {
            files.extend(repo_map.find_files(scope_item));
        }

        // Deduplicate
        files.sort();
        files.dedup();
        files
    }

    /// Parse JSON from LLM response (handles markdown code fences)
    fn parse_json_response<T: for<'de> Deserialize<'de>>(&self, response: &str, task_id: &str, context: &str, fallback: &str) -> Result<T> {
        let _ = fallback;
        // Try to extract JSON from markdown code fences
        let json_str = if let Some(start) = response.find("```json") {
            let after = &response[start + 7..];
            if let Some(end) = after.find("```") {
                after[..end].trim()
            } else {
                response
            }
        } else if let Some(start) = response.find("```") {
            let after = &response[start + 3..];
            if let Some(end) = after.find("```") {
                after[..end].trim()
            } else {
                response
            }
        } else {
            response
        };

        serde_json::from_str(json_str.trim())
            .with_context(|| format!("Failed to parse JSON for {} {}: {}", task_id, context, json_str[..json_str.len().min(200)].to_string()))
    }
}

/// Result of edit phase
#[derive(Debug)]
struct EditResult {
    files_modified: Vec<String>,
}

/// Result of repair phase
#[derive(Debug)]
struct RepairResult {
    progress_made: bool,
    issues_fixed: u32,
}

/// Recovery manager for task recovery
pub struct RecoveryManager {
    checkpoint_dir: PathBuf,
}

impl RecoveryManager {
    pub fn new(checkpoint_dir: PathBuf) -> Self {
        Self { checkpoint_dir }
    }

    /// List available checkpoints for a task
    pub fn list_checkpoints(&self, task_id: &str) -> Result<Vec<Checkpoint>> {
        let mut checkpoints = Vec::new();
        for entry in fs::read_dir(&self.checkpoint_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    if name.starts_with(task_id) {
                        let content = fs::read_to_string(&path)?;
                        let checkpoint: Checkpoint = serde_json::from_str(&content)?;
                        checkpoints.push(checkpoint);
                    }
                }
            }
        }
        checkpoints.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(checkpoints)
    }

    /// Load a specific checkpoint
    pub fn load_checkpoint(&self, task_id: &str, checkpoint_id: &str) -> Result<Checkpoint> {
        let path = self.checkpoint_dir.join(format!("{}-{}.json", task_id, checkpoint_id));
        let content = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Recover the latest checkpoint for a task
    pub fn recover_latest(&self, task_id: &str) -> Result<Option<Checkpoint>> {
        let checkpoints = self.list_checkpoints(task_id)?;
        Ok(checkpoints.into_iter().last())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_config_default() {
        let config = HarnessConfig::default();
        assert_eq!(config.max_iterations, DEFAULT_MAX_ITERATIONS);
        assert_eq!(config.max_tool_turns, DEFAULT_MAX_TOOL_TURNS);
        assert!(!config.auto_approve);
    }

    #[test]
    fn test_execution_state_serialization() {
        let state = ExecutionState::Edit;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"Edit\"");
    }

    #[test]
    fn test_plan_step_serialization() {
        let step = PlanStep {
            id: "step-1".to_string(),
            phase: ExecutionState::Edit,
            description: "Test step".to_string(),
            tool: Some("edit_file".to_string()),
            arguments: Some(serde_json::json!({"path": "test.py"})),
            expected_outcome: "File edited".to_string(),
            dependencies: vec![],
            completed: false,
            result: None,
            error: None,
        };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("step-1"));
    }

    #[test]
    fn test_recovery_manager_list_empty() {
        let dir = std::env::temp_dir().join("lai-test-checkpoints-empty");
        let _ = std::fs::create_dir_all(&dir);
        let rm = RecoveryManager::new(dir);
        let task_id = format!("task-x-{}", Uuid::new_v4());
        assert!(rm.list_checkpoints(&task_id).unwrap().is_empty());
    }
}