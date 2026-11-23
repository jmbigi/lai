use crate::policy::{PolicyConfig, PolicyEngine};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as SyncMutex};
use walkdir::WalkDir;
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)] pub struct ToolSchema { pub name: String, pub description: String, pub parameters: Value }
type ToolFunc = Arc<dyn Fn(HashMap<String, Value>) -> Pin<Box<dyn Future<Output = Result<String>> + Send>> + Send + Sync>;
#[derive(Clone)] pub struct ToolRegistry { pub tools: HashMap<String, ToolFunc>, pub require_approval: bool, pub sensitive_tools: Vec<String>, pub policy_engine: Arc<SyncMutex<PolicyEngine>> }
impl ToolRegistry {
    pub fn new() -> Self {
        let policy_engine = Arc::new(SyncMutex::new(PolicyEngine::new(PolicyConfig::default())));
        let mut registry = Self { tools: HashMap::new(), require_approval: false, sensitive_tools: vec!["write_file".into(), "run_command".into(), "apply_patch".into(), "run_python".into(), "git_add".into(), "git_commit".into(), "git_rollback".into()], policy_engine };
        registry.register_core_tools(); registry
    }
    fn register_tool<F, Fut>(&mut self, name: &str, func: F) where F: Fn(HashMap<String, Value>) -> Fut + Send + Sync + 'static, Fut: std::future::Future<Output = Result<String>> + Send + 'static {
        let func = Arc::new(func);
        let wrapped: Arc<dyn Fn(HashMap<String, Value>) -> Pin<Box<dyn Future<Output = Result<String>> + Send>> + Send + Sync> = Arc::new(move |args: HashMap<String, Value>| { let func = Arc::clone(&func); Box::pin(async move { func(args).await }) });
        self.tools.insert(name.to_string(), wrapped);
    }
    fn register_core_tools(&mut self) {
        self.register_tool("read_file", |args| async move { let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(""); Ok(fs::read_to_string(path)?) });
        self.register_tool("write_file", |args| async move { let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(""); let content = args.get("content").and_then(|v| v.as_str()).unwrap_or(""); fs::create_dir_all(Path::new(path).parent().unwrap_or(Path::new(".")))?; fs::write(path, content)?; Ok(format!("Written to {} ({} chars)", path, content.len())) });
        self.register_tool("list_dir", |args| async move { let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("."); let entries: Vec<String> = fs::read_dir(path)?.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect(); Ok(serde_json::to_string(&entries)?) });
        self.register_tool("run_command", |args| async move { let command = args.get("command").and_then(|v| v.as_str()).unwrap_or(""); let cwd = args.get("cwd").and_then(|v| v.as_str()); let mut cmd = Command::new("sh"); cmd.arg("-c").arg(command); if let Some(cwd) = cwd { cmd.current_dir(cwd); } let output = cmd.output()?; let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr); let mut result = stdout.to_string(); if !stderr.is_empty() { result.push_str("\nSTDERR:\n"); result.push_str(&stderr); } if result.trim().is_empty() { result = format!("(no output, exit code {})", output.status.code().unwrap_or(-1)); } Ok(result) });
        self.register_tool("repo_map", |args| async move { let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("."); let max_chars = args.get("max_chars").and_then(|v| v.as_u64()).unwrap_or(8000) as usize; let mut blocks = Vec::new(); let mut used = 0; for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) { let p = entry.path(); if !p.is_file() { continue; } let ext = p.extension().and_then(|e| e.to_str()).unwrap_or(""); if ext != "py" && ext != "rs" { continue; } if let Ok(content) = fs::read_to_string(p) { let mut symbols = Vec::new(); for line in content.lines() { let trimmed = line.trim(); if trimmed.starts_with("def ") || trimmed.starts_with("fn ") || trimmed.starts_with("async fn ") { if let Some(ne) = trimmed.find('(') { symbols.push(format!("  fn {}", trimmed[4..ne].trim())); } } else if trimmed.starts_with("class ") || trimmed.starts_with("struct ") { if let Some(ne) = trimmed.find([':', '(']) { let kind = if trimmed.starts_with("class ") { "class" } else { "struct" }; symbols.push(format!("  {}", trimmed[6..ne].trim())); } } } if !symbols.is_empty() { let rel = p.strip_prefix(path).unwrap_or(p); let block = format!("{}:\n{}", rel.display(), symbols.join("\n")); let bl = block.len(); if used + bl > max_chars { blocks.push("... (truncated)".into()); return Ok(blocks.join("\n\n")); } blocks.push(block); used += bl; } } } Ok(if blocks.is_empty() { "(no symbols found)".into() } else { blocks.join("\n\n") }) });
        self.register_tool("run_python", |args| async move { let code = args.get("code").and_then(|v| v.as_str()).unwrap_or(""); let temp_path = tempfile::NamedTempFile::new()?.path().to_path_buf(); fs::write(&temp_path, code)?; let output = Command::new("python3").arg(&temp_path).output()?; let stdout = String::from_utf8_lossy(&output.stdout); let stderr = String::from_utf8_lossy(&output.stderr); let mut result = stdout.to_string(); if !stderr.is_empty() { result.push_str("\nSTDERR:\n"); result.push_str(&stderr); } let _ = fs::remove_file(&temp_path); Ok(result) });
    }
    pub async fn call_tool(&self, name: &str, args: HashMap<String, Value>) -> Result<String> {
        let mut policy = self.policy_engine.lock().unwrap();
        let decision = policy.check_tool(name, &args);
        drop(policy);
        if !decision.allowed { return Err(anyhow::anyhow!("PolicyEngine blocked tool '{}': {}", name, decision.reason)); }
        if let Some(func) = self.tools.get(name) { func(args).await } else { Err(anyhow::anyhow!("Tool not found: {}", name)) }
    }
    pub fn get_tools_schema(&self) -> Vec<ToolSchema> {
        vec![
            ToolSchema { name: "read_file".into(), description: "Read a file".into(), parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}) },
            ToolSchema { name: "write_file".into(), description: "Write a file".into(), parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}) },
            ToolSchema { name: "list_dir".into(), description: "List directory".into(), parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}) },
            ToolSchema { name: "run_command".into(), description: "Run shell command".into(), parameters: serde_json::json!({"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"},"timeout":{"type":"integer"}},"required":["command"]}) },
            ToolSchema { name: "repo_map".into(), description: "Repo symbol map".into(), parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}) },
            ToolSchema { name: "run_python".into(), description: "Run Python".into(), parameters: serde_json::json!({"type":"object","properties":{"code":{"type":"string"},"timeout":{"type":"integer"}},"required":["code"]}) },
        ]
    }
}
impl Default for ToolRegistry { fn default() -> Self { Self::new() } }
