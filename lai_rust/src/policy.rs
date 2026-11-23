use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision { pub allowed: bool, pub reason: String, pub policy: String, pub risk_level: RiskLevel }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel { Low, Medium, High, Critical }
#[derive(Debug, Clone)]
pub struct PolicyConfig {
    pub max_session_tokens: u64, pub max_session_cost_cents: u64,
    pub extra_allowed_commands: Vec<String>, pub extra_protected_paths: Vec<String>,
    pub block_network_egress: bool, pub max_consecutive_calls: usize,
    pub max_writes_per_file: usize, pub detect_secrets: bool,
    pub workspace_root: PathBuf,
}
impl Default for PolicyConfig {
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self { max_session_tokens: 100_000, max_session_cost_cents: 500, extra_allowed_commands: vec![], extra_protected_paths: vec![], block_network_egress: true, max_consecutive_calls: 3, max_writes_per_file: 3, detect_secrets: true, workspace_root: cwd }
    }
    pub fn extra_protected_paths(&self) -> &[String] { &self.extra_protected_paths }
}
#[derive(Debug, Clone)]
pub enum Operation {
    FileRead { path: String }, FileWrite { path: String }, FileDelete { path: String },
    FileSearch { pattern: String, path: String }, CommandExec { command: String },
    TestRun { command: String }, GitOperation { operation: String },
    PackageInstall { package: String }, NetworkAccess { url: String },
    ToolCall { tool: String, args: Value }, PathCreate { path: String },
}
pub struct PolicyEngine {
    config: PolicyConfig, call_history: Vec<String>,
    file_write_counts: HashMap<String, usize>, session_tokens: u64,
    session_cost: u64, secret_patterns: Vec<Regex>,
}
impl Default for PolicyEngine { fn default() -> Self { Self::new(PolicyConfig::default()) } }
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
        Self { config, call_history: Vec::new(), file_write_counts: HashMap::new(), session_tokens: 0, session_cost: 0, secret_patterns }
    }
    pub fn check(&mut self, operation: &Operation) -> PolicyDecision {
        let decision = match operation {
            Operation::FileWrite { path } | Operation::FileDelete { path } => self.check_write_path(path),
            Operation::CommandExec { command } => self.check_command(command),
            Operation::TestRun { command } => self.check_command(command),
            Operation::NetworkAccess { url } => self.check_network(url),
            Operation::ToolCall { tool, args } => self.check_tool_call(tool, args),
            Operation::FileRead { path } | Operation::FileSearch { path, .. } | Operation::PathCreate { path } => self.check_read_path(path),
            Operation::GitOperation { operation } => self.check_git_op(operation),
            Operation::PackageInstall { package } => self.check_package(package),
        };
        if decision.allowed { self.record_call(operation); }
        decision
    }
    pub fn check_tool(&mut self, tool_name: &str, args: &HashMap<String, Value>) -> PolicyDecision {
        let operation = Operation::ToolCall { tool: tool_name.to_string(), args: Value::Object(args.iter().map(|(k,v)|(k.clone(),v.clone())).collect()) };
        self.check(&operation)
    }
    fn check_write_path(&self, path: &str) -> PolicyDecision {
        if !self.is_within_workspace(path) { return PolicyDecision { allowed: false, reason: format!("Path outside workspace: {}", path), policy: "WorkspaceBoundary".into(), risk_level: RiskLevel::High }; }
        if self.is_protected_path(path) { return PolicyDecision { allowed: false, reason: format!("Protected path, cannot write: {}", path), policy: "ProtectedPath".into(), risk_level: RiskLevel::Critical }; }
        let writes = self.file_write_counts.get(path).copied().unwrap_or(0);
        if writes >= self.config.max_writes_per_file { return PolicyDecision { allowed: false, reason: format!("Too many writes to {} ({}), possible loop", path, writes), policy: "DuplicateWrite".into(), risk_level: RiskLevel::High }; }
        PolicyDecision { allowed: true, reason: "Write allowed".into(), policy: "WriteFile".into(), risk_level: RiskLevel::Medium }
    }
    fn check_command(&self, command: &str) -> PolicyDecision {
        let base_cmd = command.split_whitespace().next().unwrap_or("");
        let base_name = Path::new(base_cmd).file_name().and_then(|s| s.to_str()).unwrap_or(base_cmd);
        if !self.is_allowed_command(base_name) { return PolicyDecision { allowed: false, reason: format!("Command not in allowlist: {}", base_name), policy: "CommandAllowlist".into(), risk_level: RiskLevel::High }; }
        let dangerous = ["rm -rf","rm -fr","dd if=","mkfs","fdisk","parted","curl.*|.*sh","wget.*|.*sh","nc -l","netcat -l","chmod 777","chown root","passwd","useradd","usermod","iptables","ufw","systemctl","service ","mount ","umount ","cryptsetup","openssl enc"];
        for pattern in dangerous { if Regex::new(pattern).unwrap().is_match(command) { return PolicyDecision { allowed: false, reason: format!("Dangerous command pattern: {}", pattern), policy: "DangerousCommand".into(), risk_level: RiskLevel::Critical }; } }
        if self.config.block_network_egress {
            let net = ["curl","wget","nc ","netcat","telnet","ssh","scp","rsync","ftp","sftp","dig","nslookup","host ","ping","traceroute","nmap","masscan"];
            for nc in net { if command.starts_with(nc) || command.contains(&format!(" {}", nc)) { return PolicyDecision { allowed: false, reason: format!("Network egress blocked: {}", nc), policy: "NetworkEgress".into(), risk_level: RiskLevel::Critical }; } }
        }
        PolicyDecision { allowed: true, reason: "Command allowed".into(), policy: "CommandExec".into(), risk_level: RiskLevel::Medium }
    }
    fn check_network(&self, url: &str) -> PolicyDecision {
        if self.config.block_network_egress { return PolicyDecision { allowed: false, reason: format!("Network access blocked: {}", url), policy: "NetworkAccess".into(), risk_level: RiskLevel::Critical }; }
        PolicyDecision { allowed: true, reason: "Network allowed".into(), policy: "NetworkAccess".into(), risk_level: RiskLevel::High }
    }
    fn check_tool_call(&self, tool: &str, args: &Value) -> PolicyDecision {
        if self.config.detect_secrets {
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let patch = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");
            if self.contains_secrets(content) || self.contains_secrets(patch) {
                return PolicyDecision { allowed: false, reason: "Content appears to contain secrets".into(), policy: "SecretDetection".into(), risk_level: RiskLevel::Critical };
            }
        }
        match tool {
            "write_file" | "apply_patch" => { let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(""); self.check_write_path(path) }
            "run_command" => { let command = args.get("command").and_then(|v| v.as_str()).unwrap_or(""); self.check_command(command) }
            "run_python" => { let code = args.get("code").and_then(|v| v.as_str()).unwrap_or(""); if self.config.detect_secrets && self.contains_secrets(code) { return PolicyDecision { allowed: false, reason: "Python code contains secrets".into(), policy: "SecretDetection".into(), risk_level: RiskLevel::Critical }; } PolicyDecision { allowed: true, reason: "Python execution allowed".into(), policy: "RunPython".into(), risk_level: RiskLevel::Medium } }
            "read_file" | "list_dir" | "grep" | "find_files" | "repo_map" => { let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("."); self.check_read_path(path) }
            "git_push" => { let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false); if force { return PolicyDecision { allowed: false, reason: "git push --force blocked".into(), policy: "GitForcePush".into(), risk_level: RiskLevel::Critical }; } PolicyDecision { allowed: true, reason: "Git operation allowed".into(), policy: "GitOp".into(), risk_level: RiskLevel::Medium } }
            _ => PolicyDecision { allowed: true, reason: "Tool call allowed".into(), policy: "ToolCall".into(), risk_level: RiskLevel::Low },
        }
    }
    fn check_read_path(&self, path: &str) -> PolicyDecision {
        if !self.is_within_workspace(path) { return PolicyDecision { allowed: false, reason: format!("Path outside workspace: {}", path), policy: "WorkspaceBoundary".into(), risk_level: RiskLevel::High }; }
        PolicyDecision { allowed: true, reason: "Read allowed".into(), policy: "FileRead".into(), risk_level: RiskLevel::Low }
    }
    fn check_git_op(&self, _op: &str) -> PolicyDecision { PolicyDecision { allowed: true, reason: "Git operation allowed".into(), policy: "GitOp".into(), risk_level: RiskLevel::Medium } }
    fn check_package(&self, _pkg: &str) -> PolicyDecision { PolicyDecision { allowed: true, reason: "Package install allowed".into(), policy: "PackageInstall".into(), risk_level: RiskLevel::Medium } }
    fn record_call(&mut self, op: &Operation) {
        let op_str = format!("{:?}", op);
        self.call_history.push(op_str);
        if self.call_history.len() > 100 { self.call_history.drain(0..self.call_history.len()-100); }
        if self.call_history.len() >= self.config.max_consecutive_calls {
            let last = &self.call_history[self.call_history.len()-1];
            let mut consecutive = 1;
            for i in (0..self.call_history.len()-1).rev() { if self.call_history[i] == *last { consecutive += 1; } else { break; } }
            if consecutive >= self.config.max_consecutive_calls { eprintln!("⚠️  PolicyEngine: Loop detected - {} repetitions", consecutive); }
        }
        if let Operation::FileWrite { path } | Operation::FileDelete { path } = op { *self.file_write_counts.entry(path.clone()).or_insert(0) += 1; }
    }
    pub fn add_usage(&mut self, tokens: u64, cost_cents: u64) { self.session_tokens += tokens; self.session_cost += cost_cents; }
    pub fn reset_session(&mut self) { self.session_tokens = 0; self.session_cost = 0; self.call_history.clear(); self.file_write_counts.clear(); }
    fn is_within_workspace(&self, path: &str) -> bool {
        let path = Path::new(path);
        let abs = if path.is_absolute() { path.to_path_buf() } else { self.config.workspace_root.join(path) };
        let canonical = abs.canonicalize().unwrap_or(abs);
        let ws = self.config.workspace_root.canonicalize().unwrap_or_else(|_| self.config.workspace_root.clone());
        canonical.starts_with(&ws)
    }
    fn is_protected_path(&self, path: &str) -> bool {
        const PROTECTED: &[&str] = &["lai_rust/src/","lai_rust/Cargo.toml","lai_rust/Cargo.lock","lai.py","lai_tools.py","lai_automode.py","lai_agents.py","gateway-go/","DOCUMENTACION_REGLAS.md","AGENTS.md","README.md","CHANGELOG.md",".git/"];
        let ps = path.to_string();
        for p in PROTECTED { if ps.contains(p) { return true; } }
        for extra in &self.config.extra_protected_paths { if ps.contains(extra) { return true; } }
        false
    }
    fn is_allowed_command(&self, cmd: &str) -> bool {
        const ALLOWED: &[&str] = &["cargo","rustc","python3","pytest","git","ls","cat","grep","find","head","tail","wc","echo","mkdir","touch","cp","mv","chmod","chown","ruff","black","isort","mypy","pyflakes","vulture","bandit","radon","pydeps","tesseract","patch","diff","make","npm","go","node","deno","bwrap","bash","sh","zsh","fish"];
        for a in ALLOWED { if cmd == *a || cmd.ends_with(&format!(".{}", a)) { return true; } }
        for extra in &self.config.extra_allowed_commands { if cmd == extra.as_str() { return true; } }
        false
    }
    fn contains_secrets(&self, text: &str) -> bool {
        for p in &self.secret_patterns { if p.is_match(text) { return true; } }
        false
    }
}
