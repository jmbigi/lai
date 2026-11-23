use crate::context::ContextEngine;
use crate::execution_loop::{ExecConfig, ExecutionLoop, Language};
use crate::llm::LlmClient;
use crate::policy::PolicyEngine;
use crate::tools::ToolRegistry;
use std::sync::{Arc, Mutex as SyncMutex};
pub async fn cmd_automode(goal: &str, max_turns: usize, yolo: bool, model: &str, base_url: &str, allow_remote: bool) {
    let tools = Arc::new(ToolRegistry::new());
    let llm = Arc::new(LlmClient::new(base_url.to_string(), model.to_string()));
    let policy = Arc::new(SyncMutex::new(PolicyEngine::new(Default::default())));
    let context_engine = Arc::new(SyncMutex::new(ContextEngine::new(std::env::current_dir().unwrap_or_default()).unwrap()));
    let config = ExecConfig { max_turns, max_steps: 10, token_budget: 50_000, yolo: false };
    let mut loop_exec = ExecutionLoop::new(goal.to_string(), tools, llm, policy, context_engine, Language::Python, config);
    match loop_exec.run().await { Ok(report) => { println!("\n{}", report); } Err(e) => { eprintln!("ExecutionLoop error: {}", e); } }
}
