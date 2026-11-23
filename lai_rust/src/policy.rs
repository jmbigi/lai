//! PolicyEngine - Deterministic guardrails for all tool calls.
//! Rule 8: Non-bypassable barriers that the model cannot ignore or forget.

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Configuration for the policy engine.
#[derive(Debug, Clone)]
pub struct PolicyConfig {
    /// Maximum tokens per session.
    pub max_session_tokens: u64,
    /// Maximum cost per session (USD cents).
    pub max_session_cost_cents: u64,
    /// Maximum consecutive identical tool calls before blocking.
    pub max_consecutive_calls: usize,
    /// Maximum writes to the same file before requiring confirmation.
    pub max_writes_per_file: usize,
    /// Allowed commands (beyond the built-in allowlist).
    pub extra_allowed_commands: Vec<String>,
    /// Additional protected paths.
    pub extra_protected_paths: Vec<String>,
    /// Whether to enable network egress blocking.
    pub block_network_egress: bool,
    /// Whether to enable secret detection.
    pub detect_secrets: bool,
    /// Workspace root (all paths must stay within).
    pub workspace_root: PathBuf,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            max_session_tokens: 100_000,
            max_session_cost_cents: 500, // $5.00
            max_consecutive_calls: 3,
            max_writes_per_file: 3,
            extra_allowed_commands: vec![],
            extra_protected_paths: vec![],
            block_network_egress: true,
            detect_secrets: true,
            workspace_root: cwd,
        }
    }
}

/// Result of a policy check (compatible with harness.rs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub reason: String,
    pub policy: String,
    pub risk_level: RiskLevel,
}

/// Risk level of an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    None,
    Low,
    Medium,
    High,
    Critical,
}

/// Operation that the agent desires to perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    /// Read a file.
    FileRead { path: String },
    /// Write a file.
    FileWrite { path: String },
    /// Delete a file.
    FileDelete { path: String },
    /// Search in files.
    FileSearch { pattern: String, path: String },
    /// Execute a shell command.
    CommandExec { command: String },
    /// Run tests.
    TestRun { command: String },
    /// Git operation.
    GitOperation { operation: String },
    /// Install a package.
    PackageInstall { package: String },
    /// Network access.
    NetworkAccess { url: String },
    /// Tool call from the registry.
    ToolCall { tool: String, args: Value },
    /// Create path/directory.
    PathCreate { path: String },
}

/// Permission level for an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    Allow,
    Deny,
    RequireApproval,
}

/// PolicyEngine - intercepts ALL tool calls and applies deterministic rules.
pub struct PolicyEngine {
    config: PolicyConfig,
    call_history: Arc<Mutex<Vec<String>>>,
    file_write_counts: Arc<Mutex<HashMap<String, usize>>>,
    session_tokens: Arc<Mutex<u64>>,
    session_cost: Arc<Mutex<u64>>,
    secret_patterns: Vec<Regex>,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new(PolicyConfig::default())
    }
}

impl PolicyEngine {
    pub fn new(config: PolicyConfig) -> Self {
        let secret_patterns: Vec<Regex> = vec![
            Regex::new(r#"(?i)(api[_-]?key|apikey|secret|password|token|auth)["']?\s*[:=]\s*["'][^"']+["']"#).unwrap(),
            Regex::new(r#"(?i)(aws[_-]?access[_-]?key|aws[_-]?secret[_-]?key)["']?\s*[:=]\s*["'][^"']+["']"#).unwrap(),
            Regex::new(r#"(?i)ssh[_-]?private[_-]?key"#).unwrap(),
            Regex::new(r#"-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----"#).unwrap(),
            Regex::new(r#"(?i)bearer\s+[a-zA-Z0-9._-]+"#).unwrap(),
            Regex::new(r#"(?i)authorization:\s*basic\s+[a-zA-Z0-9+/=]+"#).unwrap(),
        ];

        Self {
            config,
            call_history: Arc::new(Mutex::new(Vec::new())),
            file_write_counts: Arc::new(Mutex::new(HashMap::new())),
            session_tokens: Arc::new(Mutex::new(0)),
            session_cost: Arc::new(Mutex::new(0)),
            secret_patterns,
        }
    }

    /// Check an operation against all policies. Returns a PolicyDecision.
    pub fn check(&self, operation: &Operation, _task_id: &str) -> PolicyDecision {
        match operation {
            Operation::FileWrite { path } | Operation::FileDelete { path } => {
                self.check_write_path(path)
            }
            Operation::CommandExec { command } => self.check_command(command),
            Operation::TestRun { command } => self.check_command(command),
            Operation::NetworkAccess { url } => self.check_network(url),
            Operation::ToolCall { tool, args } => self.check_tool_call(tool, args),
            Operation::FileRead { path } | Operation::FileSearch { path, .. } | Operation::PathCreate { path } => {
                self.check_read_path(path)
            }
            Operation::GitOperation { operation } => self.check_git_op(operation),
            Operation::PackageInstall { package } => self.check_package(package),
        }
    }

    /// Check a tool call by name and args (for ToolRegistry integration).
    pub fn check_tool(&self, tool_name: &str, args: &HashMap<String, Value>) -> PolicyDecision {
        let operation = Operation::ToolCall {
            tool: tool_name.to_string(),
            args: Value::Object(args.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        };
        self.check(&operation, "")
    }

    fn check_write_path(&self, path: &str) -> PolicyDecision {
        if !self.is_within_workspace(path) {
            return PolicyDecision {
                allowed: false,
                reason: format!("Path outside workspace: {}", path),
                policy: "WorkspaceBoundary".into(),
                risk_level: RiskLevel::High,
            };
        }
        if self.is_protected_path(path) {
            return PolicyDecision {
                allowed: false,
                reason: format!("Protected path, cannot write: {}", path),
                policy: "ProtectedPath".into(),
                risk_level: RiskLevel::Critical,
            };
        }
        let writes = self.file_write_counts.lock().unwrap().get(path).copied().unwrap_or(0);
        if writes >= self.config.max_writes_per_file {
            return PolicyDecision {
                allowed: false,
                reason: format!("Too many writes to {} ({}), possible loop", path, writes),
                policy: "DuplicateWrite".into(),
                risk_level: RiskLevel::High,
            };
        }
        PolicyDecision {
            allowed: true,
            reason: "Write allowed".into(),
            policy: "WriteFile".into(),
            risk_level: RiskLevel::Medium,
        }
    }

    fn check_command(&self, command: &str) -> PolicyDecision {
        let base_cmd = command.split_whitespace().next().unwrap_or("");
        let base_name = Path::new(base_cmd)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(base_cmd);

        if !self.is_allowed_command(base_name) {
            return PolicyDecision {
                allowed: false,
                reason: format!("Command not in allowlist: {}", base_name),
                policy: "CommandAllowlist".into(),
                risk_level: RiskLevel::High,
            };
        }

        let dangerous_patterns = [
            "rm -rf", "rm -fr", "dd if=", "mkfs", "fdisk", "parted",
            "curl.*|.*sh", "wget.*|.*sh", "nc -l", "netcat -l",
            "chmod 777", "chown root", "passwd", "useradd", "usermod",
            "iptables", "ufw", "systemctl", "service ",
            "mount ", "umount ", "cryptsetup", "openssl enc",
        ];
        for pattern in dangerous_patterns {
            if Regex::new(pattern).unwrap().is_match(command) {
                return PolicyDecision {
                    allowed: false,
                    reason: format!("Dangerous command pattern: {}", pattern),
                    policy: "DangerousCommand".into(),
                    risk_level: RiskLevel::Critical,
                };
            }
        }

        if self.config.block_network_egress {
            let network_commands = [
                "curl", "wget", "nc ", "netcat", "telnet", "ssh", "scp",
                "rsync", "ftp", "sftp", "dig", "nslookup", "host ",
                "ping", "traceroute", "nmap", "masscan",
            ];
            for nc in network_commands {
                if command.starts_with(nc) || command.contains(&format!(" {}", nc)) {
                    return PolicyDecision {
                        allowed: false,
                        reason: format!("Network egress blocked: {}", nc),
                        policy: "NetworkEgress".into(),
                        risk_level: RiskLevel::Critical,
                    };
                }
            }
        }

        PolicyDecision {
            allowed: true,
            reason: "Command allowed".into(),
            policy: "CommandExec".into(),
            risk_level: RiskLevel::Medium,
        }
    }

    fn check_network(&self, url: &str) -> PolicyDecision {
        if self.config.block_network_egress {
            return PolicyDecision {
                allowed: false,
                reason: format!("Network access blocked: {}", url),
                policy: "NetworkAccess".into(),
                risk_level: RiskLevel::Critical,
            };
        }
        PolicyDecision {
            allowed: true,
            reason: "Network allowed".into(),
            policy: "NetworkAccess".into(),
            risk_level: RiskLevel::High,
        }
    }

    fn check_tool_call(&self, tool: &str, args: &Value) -> PolicyDecision {
        match tool {
            "write_file" | "apply_patch" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let write_decision = self.check_write_path(path);
                if !write_decision.allowed {
                    return write_decision;
                }
                // Secret detection in content/patch (Rule 8: no credentials in writes).
                if self.config.detect_secrets {
                    for field in ["content", "patch"] {
                        if let Some(text) = args.get(field).and_then(|v| v.as_str()) {
                            if self.contains_secrets(text) {
                                return PolicyDecision {
                                    allowed: false,
                                    reason: format!("{} appears to contain secrets/credentials", field),
                                    policy: "SecretDetection".into(),
                                    risk_level: RiskLevel::Critical,
                                };
                            }
                        }
                    }
                }
                write_decision
            }
            "run_command" => {
                let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                self.check_command(command)
            }
            "run_python" => {
                let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
                if self.config.detect_secrets && self.contains_secrets(code) {
                    return PolicyDecision {
                        allowed: false,
                        reason: "Python code contains secrets/credentials".into(),
                        policy: "SecretDetection".into(),
                        risk_level: RiskLevel::Critical,
                    };
                }
                PolicyDecision {
                    allowed: true,
                    reason: "Python execution allowed".into(),
                    policy: "RunPython".into(),
                    risk_level: RiskLevel::Medium,
                }
            }
            "read_file" | "list_dir" | "grep" | "find_files" | "repo_map" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                self.check_read_path(path)
            }
            "git_push" => {
                let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
                if force {
                    return PolicyDecision {
                        allowed: false,
                        reason: "git push --force is blocked".into(),
                        policy: "GitForcePush".into(),
                        risk_level: RiskLevel::Critical,
                    };
                }
                PolicyDecision {
                    allowed: true,
                    reason: "Git operation allowed".into(),
                    policy: "GitOp".into(),
                    risk_level: RiskLevel::Medium,
                }
            }
            _ => PolicyDecision {
                allowed: true,
                reason: "Tool call allowed".into(),
                policy: "ToolCall".into(),
                risk_level: RiskLevel::Low,
            },
        }
    }

    fn check_read_path(&self, path: &str) -> PolicyDecision {
        if !self.is_within_workspace(path) {
            return PolicyDecision {
                allowed: false,
                reason: format!("Path outside workspace: {}", path),
                policy: "WorkspaceBoundary".into(),
                risk_level: RiskLevel::High,
            };
        }
        PolicyDecision {
            allowed: true,
            reason: "Read allowed".into(),
            policy: "FileRead".into(),
            risk_level: RiskLevel::Low,
        }
    }

    fn check_git_op(&self, _operation: &str) -> PolicyDecision {
        PolicyDecision {
            allowed: true,
            reason: "Git operation allowed".into(),
            policy: "GitOp".into(),
            risk_level: RiskLevel::Medium,
        }
    }

    fn check_package(&self, _package: &str) -> PolicyDecision {
        PolicyDecision {
            allowed: true,
            reason: "Package install allowed".into(),
            policy: "PackageInstall".into(),
            risk_level: RiskLevel::Medium,
        }
    }

    /// Update session token/cost tracking.
    pub fn add_usage(&self, tokens: u64, cost_cents: u64) {
        *self.session_tokens.lock().unwrap() += tokens;
        *self.session_cost.lock().unwrap() += cost_cents;
    }

    /// Reset session counters (new task).
    pub fn reset_session(&self) {
        *self.session_tokens.lock().unwrap() = 0;
        *self.session_cost.lock().unwrap() = 0;
        self.call_history.lock().unwrap().clear();
        self.file_write_counts.lock().unwrap().clear();
    }

    fn is_within_workspace(&self, path: &str) -> bool {
        let path = Path::new(path);
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.config.workspace_root.join(path)
        };
        let canonical = abs_path.canonicalize().unwrap_or(abs_path);
        let workspace_canonical = self.config.workspace_root.canonicalize()
            .unwrap_or_else(|_| self.config.workspace_root.clone());
        canonical.starts_with(&workspace_canonical)
    }

    fn is_protected_path(&self, path: &str) -> bool {
        const PROTECTED: &[&str] = &[
            "lai_rust/src/", "lai_rust/Cargo.toml", "lai_rust/Cargo.lock",
            "lai.py", "lai_tools.py", "lai_automode.py", "lai_agents.py",
            "gateway-go/", "DOCUMENTACION_REGLAS.md", "AGENTS.md",
            "README.md", "CHANGELOG.md", ".git/",
        ];

        let path_str = path.to_string();
        for protected in PROTECTED {
            if path_str.contains(protected) {
                return true;
            }
        }
        for extra in &self.config.extra_protected_paths {
            if path_str.contains(extra) {
                return true;
            }
        }
        false
    }

    fn is_allowed_command(&self, cmd: &str) -> bool {
        const ALLOWED: &[&str] = &[
            "cargo", "rustc", "python3", "pytest", "git", "ls", "cat",
            "grep", "find", "head", "tail", "wc", "echo", "mkdir", "touch",
            "cp", "mv", "chmod", "chown", "ruff", "black", "isort", "mypy",
            "pyflakes", "vulture", "bandit", "radon", "pydeps", "tesseract",
            "patch", "diff", "make", "npm", "cargo", "go", "node", "deno",
            "bwrap", "bash", "sh", "zsh", "fish",
        ];
        for allowed in ALLOWED {
            if cmd == *allowed || cmd.ends_with(&format!(".{}", allowed)) {
                return true;
            }
        }
        for extra in &self.config.extra_allowed_commands {
            if cmd == extra.as_str() {
                return true;
            }
        }
        false
    }

    fn contains_secrets(&self, text: &str) -> bool {
        for pattern in &self.secret_patterns {
            if pattern.is_match(text) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_engine_basic() {
        let engine = PolicyEngine::new(PolicyConfig::default());

        let op = Operation::CommandExec { command: "ls -la".to_string() };
        let decision = engine.check(&op, "task1");
        assert!(decision.allowed);

        let op = Operation::CommandExec { command: "rm -rf /".to_string() };
        let decision = engine.check(&op, "task1");
        assert!(!decision.allowed);
    }

    #[test]
    fn test_write_protection() {
        let engine = PolicyEngine::new(PolicyConfig::default());

        let op = Operation::FileWrite {
            path: "lai_rust/src/main.rs".to_string(),
        };
        let decision = engine.check(&op, "task1");
        assert!(!decision.allowed);
    }

    #[test]
    fn test_secret_detection() {
        let engine = PolicyEngine::new(PolicyConfig::default());

        let op = Operation::ToolCall {
            tool: "write_file".to_string(),
            args: serde_json::json!({
                "path": "test.py",
                "content": "api_key = \"sk-1234567890abcdef\""
            }),
        };
        let decision = engine.check(&op, "task1");
        assert!(!decision.allowed);
    }
}