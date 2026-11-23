use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision { Allow, Deny(String) }

#[derive(Debug, Clone, Default)]
pub struct PolicyConfig {
    pub max_session_tokens: u64,
    pub max_session_cost_cents: u64,
    pub max_consecutive_calls: usize,
    pub max_writes_per_file: usize,
    pub workspace_root: PathBuf,
}

pub struct PolicyEngine { config: PolicyConfig }

impl Default for PolicyEngine {
    fn default() -> Self { Self::new(PolicyConfig::default()) }
}

impl PolicyConfig {
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self { max_session_tokens: 100_000, max_session_cost_cents: 500, max_consecutive_calls: 3, max_writes_per_file: 3, workspace_root: cwd }
    }
}

impl PolicyEngine {
    pub fn new(config: PolicyConfig) -> Self { Self { config } }
    pub fn check_tool(&self, tool_name: &str, _args: &HashMap<String, Value>) -> PolicyDecision {
        let allowed = matches!(tool_name, "read_file" | "write_file" | "list_dir" | "run_command" | "grep" | "find_files" | "repo_map" | "run_python" | "apply_patch" | "git_add" | "git_commit" | "git_status" | "git_diff");
        if allowed { PolicyDecision::Allow } else { PolicyDecision::Deny(format!("Tool not allowed: {}", tool_name)) }
    }
}
