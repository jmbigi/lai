//! execution_loop.rs - ExecutionEngine FSM determinista (v5.0, Mejora 1).
use serde::{Deserialize, Serialize};
use std::time::Instant;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionState { Intake, Inspect, Plan, Locate, Edit, Verify, Repair, Report, Success, SuccessWithDeclaredGaps, Blocked, Failed, Cancelled }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict { Success, SuccessWithDeclaredGaps, Blocked }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep { pub id: String, pub phase: ExecutionState, pub description: String, pub tool: Option<String>, pub arguments: Option<serde_json::Value>, pub expected_outcome: String, pub dependencies: Vec<String>, pub completed: bool, pub result: Option<serde_json::Value>, pub error: Option<String>, }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec { pub id: String, pub task_type: String, pub objective: String, pub current_state: String, pub problem: Option<String>, pub scope: Vec<String>, pub constraints: Vec<String>, pub affected_files: Vec<String>, pub dependencies: Vec<String>, pub acceptance_criteria: Vec<String>, pub required_tests: Vec<String>, pub risks: Vec<String>, pub verification_scope: Vec<String>, pub created_at: String, pub updated_at: String, }
#[derive(Debug, Clone)]
pub struct ExecutionContext { pub task_id: String, pub task_spec: TaskSpec, pub state: ExecutionState, pub plan: Vec<PlanStep>, pub current_step: usize, pub repo_map: Option<crate::repomap::RepoMap>, pub context_engine: Option<crate::context::ContextEngine>, pub checkpoints: Vec<crate::harness::Checkpoint>, pub iteration: usize, pub max_iterations: usize, pub tool_turns: usize, pub max_tool_turns: usize, pub start_time: Instant, pub last_checkpoint: Option<Instant>, pub metadata: std::collections::HashMap<String, serde_json::Value>, }
impl ExecutionContext { pub fn new(task_id: String, task_spec: TaskSpec, max_iterations: usize, max_tool_turns: usize) -> Self { Self { task_id, task_spec, state: ExecutionState::Intake, plan: Vec::new(), current_step: 0, repo_map: None, context_engine: None, checkpoints: Vec::new(), iteration: 0, max_iterations, tool_turns: 0, max_tool_turns, start_time: Instant::now(), last_checkpoint: Some(Instant::now()), metadata: std::collections::HashMap::new(), } } pub fn can_complete(&self) -> bool { matches!(self.state, ExecutionState::Success | ExecutionState::SuccessWithDeclaredGaps) } }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult { pub task_id: String, pub verdict: Verdict, pub final_state: ExecutionState, pub iterations: usize, pub total_tool_turns: usize, pub duration_ms: u64, pub files_modified: Vec<String>, pub verification_results: Vec<crate::quality::VerificationResult>, pub summary: String, pub error: Option<String>, }
#[derive(Debug, Clone)]
pub struct ExecutionEngine { pub config: ExecutionConfig }
#[derive(Debug, Clone)]
pub struct ExecutionConfig { pub max_iterations: usize, pub max_tool_turns: usize, pub require_verification_level_5: bool, pub enable_checkpoints: bool }
impl Default for ExecutionConfig { fn default() -> Self { Self { max_iterations: 10, max_tool_turns: 20, require_verification_level_5: true, enable_checkpoints: true } } }
impl ExecutionEngine {
    pub fn new(config: ExecutionConfig) -> Self { Self { config } }
    pub async fn execute(&mut self, description: &str, task_type: &str) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let task_id = format!("task-{}-{}", task_type, uuid::Uuid::new_v4().to_string()[..8].to_string());
        crate::telemetry::task_span(&task_id, task_type, description);
        let task_spec = self.intake_phase(&task_id, description, task_type).await?;
        let repo_map = self.inspect_phase(&task_id, &task_spec).await?;
        let plan = self.plan_phase(&task_id, &task_spec, &repo_map).await?;
        let mut ctx = ExecutionContext::new(task_id.clone(), task_spec, self.config.max_iterations, self.config.max_tool_turns);
        ctx.plan = plan; ctx.repo_map = Some(repo_map); ctx.state = ExecutionState::Locate;
        let mut verification_results = Vec::new(); let mut files_modified = Vec::new();
        while ctx.iteration < ctx.max_iterations {
            ctx.iteration += 1; ctx.state = ExecutionState::Edit;
            let edit = self.edit_phase(&mut ctx).await?; files_modified.extend(edit.files_modified);
            ctx.state = ExecutionState::Verify;
            let verify_results = self.verify_phase(&mut ctx).await?;
            verification_results.extend(verify_results.clone());
            let all_passed = verify_results.iter().all(|v| v.passed);
            let has_level_5 = verify_results.iter().any(|v| v.level >= 5 && v.passed);
            if all_passed {
                if self.config.require_verification_level_5 && !ctx.task_spec.required_tests.is_empty() && !has_level_5 {
                    ctx.state = ExecutionState::SuccessWithDeclaredGaps;
                    return Ok(self.finalize(&ctx, Verdict::SuccessWithDeclaredGaps, "All verifications passed but no level-5", verification_results, files_modified));
                }
                ctx.state = ExecutionState::Success;
                return Ok(self.finalize(&ctx, Verdict::Success, "All verifications passed", verification_results, files_modified));
            }
            ctx.state = ExecutionState::Repair;
            let repair = self.repair_phase(&mut ctx, &verify_results).await?;
            if !repair.progress_made { ctx.state = ExecutionState::Blocked; return Ok(self.finalize(&ctx, Verdict::Blocked, "No progress in repair", verification_results, files_modified)); }
        }
        ctx.state = ExecutionState::Failed;
        Ok(self.finalize(&ctx, Verdict::Blocked, "Max iterations reached", verification_results, files_modified))
    }
    async fn intake_phase(&self, task_id: &str, description: &str, task_type: &str) -> Result<TaskSpec, Box<dyn std::error::Error + Send + Sync>> {
        Ok(TaskSpec { id: task_id.to_string(), task_type: task_type.to_string(), objective: description.to_string(), current_state: "unknown".to_string(), problem: None, scope: vec![], constraints: vec![], affected_files: vec![], dependencies: vec![], acceptance_criteria: vec![], required_tests: vec![], risks: vec![], verification_scope: vec!["level_5".to_string()], created_at: chrono::Utc::now().to_rfc3339(), updated_at: chrono::Utc::now().to_rfc3339(), })
    }
    async fn inspect_phase(&self, _task_id: &str, _spec: &TaskSpec) -> Result<crate::repomap::RepoMap, Box<dyn std::error::Error + Send + Sync>> {
        let builder = crate::repomap::RepoMapBuilder::new(".");
        let map = builder.build()?;
        Ok(map)
    }
    async fn plan_phase(&self, _task_id: &str, _spec: &TaskSpec, _repo_map: &crate::repomap::RepoMap) -> Result<Vec<PlanStep>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![PlanStep { id: "step-1".to_string(), phase: ExecutionState::Edit, description: "Analyze code".to_string(), tool: None, arguments: None, expected_outcome: "Code understood".to_string(), dependencies: vec![], completed: false, result: None, error: None }])
    }
    async fn edit_phase(&mut self, _ctx: &mut ExecutionContext) -> Result<EditResult, Box<dyn std::error::Error + Send + Sync>> { Ok(EditResult { files_modified: vec![] }) }
    async fn verify_phase(&mut self, ctx: &mut ExecutionContext) -> Result<Vec<crate::quality::VerificationResult>, Box<dyn std::error::Error + Send + Sync>> {
        let mut results = Vec::new();
        if let Some(repo_map) = &ctx.repo_map {
            for file in &repo_map.files { if file.path.ends_with(".py") { let passed = std::process::Command::new("python3").args(["-m", "py_compile", &file.path]).output().map(|o| o.status.success()).unwrap_or(false); crate::telemetry::record_verification(&ctx.task_id, "py_compile", 5, passed, format!("Syntax check {}", file.path), vec![file.path.clone()]); results.push(crate::quality::VerificationResult { verifier: "py_compile".to_string(), level: 5, passed, details: if passed { "Syntax OK".into() } else { "Syntax error".into() }, artifacts: vec![file.path.clone()], }); }
        }
        Ok(results)
    }
    async fn repair_phase(&mut self, ctx: &mut ExecutionContext, results: &[crate::quality::VerificationResult]) -> Result<RepairResult, Box<dyn std::error::Error + Send + Sync>> { let failed = results.iter().filter(|v| !v.passed).count(); Ok(RepairResult { progress_made: failed == 0, issues_fixed: 0 }) }
    fn finalize(&self, ctx: &ExecutionContext, verdict: Verdict, summary: &str, verification_results: Vec<crate::quality::VerificationResult>, files_modified: Vec<String>) -> ExecutionResult { ExecutionResult { task_id: ctx.task_id.clone(), verdict, final_state: ctx.state, iterations: ctx.iteration, total_tool_turns: ctx.tool_turns, duration_ms: ctx.start_time.elapsed().as_millis() as u64, files_modified, verification_results, summary: summary.to_string(), error: None } }
}
#[derive(Debug)]
struct EditResult { files_modified: Vec<String> }
#[derive(Debug)]
struct RepairResult { progress_made: bool, issues_fixed: u32 }
