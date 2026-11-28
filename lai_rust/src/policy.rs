use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub reason: String,
    pub policy: String,
    pub risk_level: RiskLevel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}
#[derive(Debug, Clone)]
pub struct PolicyConfig {
    pub max_session_tokens: u64,
    pub max_session_cost_cents: u64,
    pub extra_allowed_commands: Vec<String>,
    pub extra_protected_paths: Vec<String>,
    pub block_network_egress: bool,
    pub max_consecutive_calls: usize,
    pub max_writes_per_file: usize,
    pub detect_secrets: bool,
    pub workspace_root: PathBuf,
}
impl Default for PolicyConfig {
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            max_session_tokens: 100_000,
            max_session_cost_cents: 500,
            extra_allowed_commands: vec![],
            extra_protected_paths: vec![],
            block_network_egress: true,
            max_consecutive_calls: 3,
            max_writes_per_file: 3,
            detect_secrets: true,
            workspace_root: cwd,
        }
    }
}

impl PolicyConfig {
    /// Configuración con extensiones desde el entorno.
    ///
    /// - `LAI_ALLOW_COMMANDS=cmd1,cmd2` añade comandos a la allowlist.
    /// - `LAI_PROTECTED_PATHS=path1,path2` añade rutas protegidas.
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(list) = std::env::var("LAI_ALLOW_COMMANDS") {
            config.extra_allowed_commands = list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
        if let Ok(list) = std::env::var("LAI_PROTECTED_PATHS") {
            config.extra_protected_paths = list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
        config
    }
}

/// ¿Se debe aplicar la allowlist de comandos en `auto`?
///
/// Es **opt-in** (`LAI_COMMAND_ALLOWLIST=1`): por defecto la allowlist no se
/// aplica porque un agente de código necesita `cd`, `sed`, `make`, etc. Con la
/// variable activa, `run_command`/`bash`/`run_python` pasan por el
/// `PolicyEngine` y se rechazan los comandos fuera de la lista.
pub fn command_allowlist_from_env() -> bool {
    matches!(
        std::env::var("LAI_COMMAND_ALLOWLIST")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}
#[derive(Debug, Clone)]
pub enum Operation {
    FileRead { path: String },
    FileWrite { path: String },
    FileDelete { path: String },
    CommandExec { command: String },
    ToolCall { tool: String, args: Value },
    GitOperation { operation: String },
    PackageInstall { package: String },
    NetworkAccess { url: String },
}
pub struct PolicyEngine {
    config: PolicyConfig,
    file_write_counts: HashMap<String, usize>,
    session_tokens: u64,
    session_cost: u64,
}
impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new(PolicyConfig::default())
    }
}
impl PolicyEngine {
    pub fn new(config: PolicyConfig) -> Self {
        Self {
            config,
            file_write_counts: HashMap::new(),
            session_tokens: 0,
            session_cost: 0,
        }
    }
    pub fn check(&mut self, operation: &Operation) -> PolicyDecision {
        let decision = match operation {
            Operation::FileWrite { path } | Operation::FileDelete { path } => {
                self.check_write(path)
            }
            Operation::CommandExec { command } => self.check_command(command),
            Operation::ToolCall { tool, args } => self.check_tool_value(tool, args),
            Operation::NetworkAccess { url } => self.check_network(url),
            Operation::GitOperation { operation } => self.check_git(operation),
            Operation::PackageInstall { package } => self.check_package(package),
            Operation::FileRead { path } => self.check_read(path),
        };
        if decision.allowed {
            self.record_call(operation);
        }
        decision
    }
    pub fn check_tool(&mut self, tool_name: &str, args: &HashMap<String, Value>) -> PolicyDecision {
        let op = Operation::ToolCall {
            tool: tool_name.to_string(),
            args: Value::Object(args.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        };
        self.check(&op)
    }
    fn check_tool_value(&self, tool: &str, args: &Value) -> PolicyDecision {
        match tool {
            "write_file" | "apply_patch" | "edit" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                self.check_write(path)
            }
            "run_command" | "bash" => {
                let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                self.check_command(cmd)
            }
            "run_python" => {
                let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
                if self.config.detect_secrets && self.has_secrets(code) {
                    return PolicyDecision {
                        allowed: false,
                        reason: "Secrets in code".into(),
                        policy: "SecretDetection".into(),
                        risk_level: RiskLevel::Critical,
                    };
                }
                PolicyDecision {
                    allowed: true,
                    reason: "Python allowed".into(),
                    policy: "RunPython".into(),
                    risk_level: RiskLevel::Medium,
                }
            }
            "read_file" | "list_dir" | "grep" | "find_files" | "repo_map" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                self.check_read(path)
            }
            _ => PolicyDecision {
                allowed: true,
                reason: "Tool allowed".into(),
                policy: "ToolCall".into(),
                risk_level: RiskLevel::Low,
            },
        }
    }
    fn check_write(&self, path: &str) -> PolicyDecision {
        if !self.is_within_workspace(path) {
            return PolicyDecision {
                allowed: false,
                reason: format!("Outside workspace: {}", path),
                policy: "WorkspaceBoundary".into(),
                risk_level: RiskLevel::High,
            };
        }
        if self.is_protected(path) {
            return PolicyDecision {
                allowed: false,
                reason: format!("Protected path: {}", path),
                policy: "ProtectedPath".into(),
                risk_level: RiskLevel::Critical,
            };
        }
        let writes = self.file_write_counts.get(path).copied().unwrap_or(0);
        if writes >= self.config.max_writes_per_file {
            return PolicyDecision {
                allowed: false,
                reason: format!("Too many writes to {}", path),
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
        let base = command.split_whitespace().next().unwrap_or("");
        let base_name = Path::new(base)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(base);
        if !self.is_allowed(base_name) {
            return PolicyDecision {
                allowed: false,
                reason: format!("Not allowed: {}", base_name),
                policy: "CommandAllowlist".into(),
                risk_level: RiskLevel::High,
            };
        }
        let dangerous = [
            "rm -rf",
            "rm -fr",
            "mkfs",
            "fdisk",
            "chmod 777",
            "passwd",
            "iptables",
            "systemctl",
            "mount",
        ];
        for p in dangerous {
            if command.contains(p) {
                return PolicyDecision {
                    allowed: false,
                    reason: format!("Dangerous: {}", p),
                    policy: "DangerousCommand".into(),
                    risk_level: RiskLevel::Critical,
                };
            }
        }
        if self.config.block_network_egress {
            for nc in ["curl", "wget", "ssh", "nc ", "telnet"] {
                if command.starts_with(nc) {
                    return PolicyDecision {
                        allowed: false,
                        reason: format!("Network blocked: {}", nc),
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
    fn check_read(&self, path: &str) -> PolicyDecision {
        if !self.is_within_workspace(path) {
            return PolicyDecision {
                allowed: false,
                reason: format!("Outside workspace: {}", path),
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
    fn check_network(&self, url: &str) -> PolicyDecision {
        if self.config.block_network_egress {
            return PolicyDecision {
                allowed: false,
                reason: format!("Network blocked: {}", url),
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
    fn check_git(&self, op: &str) -> PolicyDecision {
        if op == "push --force" {
            return PolicyDecision {
                allowed: false,
                reason: "Force push blocked".into(),
                policy: "GitForcePush".into(),
                risk_level: RiskLevel::Critical,
            };
        }
        PolicyDecision {
            allowed: true,
            reason: "Git allowed".into(),
            policy: "GitOp".into(),
            risk_level: RiskLevel::Medium,
        }
    }
    fn check_package(&self, _pkg: &str) -> PolicyDecision {
        PolicyDecision {
            allowed: true,
            reason: "Package allowed".into(),
            policy: "PackageInstall".into(),
            risk_level: RiskLevel::Medium,
        }
    }
    fn record_call(&mut self, _op: &Operation) {}
    pub fn add_usage(&mut self, tokens: u64, cost: u64) {
        self.session_tokens += tokens;
        self.session_cost += cost;
    }
    fn is_within_workspace(&self, path: &str) -> bool {
        let p = Path::new(path);
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.config.workspace_root.join(p)
        };

        // Para rutas existentes, canonicalize resuelve `..` y symlinks. Para
        // las que aún no existen (write_file crea archivos nuevos), hay que
        // normalizar `..` lexicamente: si no, `../fuera.txt` pasaba el
        // `starts_with` porque el prefijo del workspace coincidía.
        let canon = resolve_for_policy(&abs);
        let ws = self
            .config
            .workspace_root
            .canonicalize()
            .map(|p| normalize_path(&p))
            .unwrap_or_else(|_| normalize_path(&self.config.workspace_root));

        // On Windows, compare case-insensitively
        #[cfg(target_os = "windows")]
        let starts_with = canon
            .to_string_lossy()
            .to_lowercase()
            .starts_with(&ws.to_string_lossy().to_lowercase());
        #[cfg(not(target_os = "windows"))]
        let starts_with = canon.starts_with(&ws);

        starts_with
    }
    fn is_protected(&self, path: &str) -> bool {
        // Comparar por componentes, no por subcadena: `notsrc/main.rs` no es
        // `src/` y `Cargo.toml.bak` no es `Cargo.toml`.
        let p = Path::new(path);
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.config.workspace_root.join(p)
        };
        let ws = self
            .config
            .workspace_root
            .canonicalize()
            .map(|p| normalize_path(&p))
            .unwrap_or_else(|_| normalize_path(&self.config.workspace_root));
        let rel = resolve_for_policy(&abs)
            .strip_prefix(&ws)
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|_| PathBuf::from(path));
        let rel = rel.to_string_lossy().replace('\\', "/");
        let rel = rel.trim_start_matches("./").trim_end_matches('/');

        // Credenciales y metadatos de VCS: nunca deben escribirse, ni siquiera
        // en un proyecto ajeno.
        const ALWAYS_PROTECTED: &[&str] = &[
            ".git",
            ".env",
            ".ssh",
            ".netrc",
            ".npmrc",
            ".pypirc",
            "id_rsa",
            "id_ed25519",
        ];
        for protected in ALWAYS_PROTECTED {
            if path_has_component(rel, protected) {
                return true;
            }
        }

        // El código fuente de lai solo se protege cuando el workspace es el
        // propio repositorio de lai. En un proyecto de usuario, `src/` es
        // precisamente lo que el asistente debe poder editar.
        if self.is_lai_workspace() {
            const LAI_PROTECTED: &[&str] = &["src", "Cargo.toml", "Cargo.lock"];
            for protected in LAI_PROTECTED {
                if path_has_component(rel, protected) {
                    return true;
                }
            }
        }

        for extra in &self.config.extra_protected_paths {
            let extra = extra.replace('\\', "/");
            let extra = extra.trim_start_matches("./").trim_end_matches('/');
            if !extra.is_empty() && (rel == extra || rel.starts_with(&format!("{extra}/"))) {
                return true;
            }
        }
        false
    }

    /// ¿El workspace es el repositorio de lai (crate `lai` o raíz con
    /// `lai_rust/Cargo.toml`)? Solo entonces se protege `src/` y los Cargo.*.
    fn is_lai_workspace(&self) -> bool {
        let candidates = [
            self.config.workspace_root.clone(),
            self.config.workspace_root.join("lai_rust"),
        ];
        candidates.iter().any(|root| {
            std::fs::read_to_string(root.join("Cargo.toml"))
                .map(|text| {
                    text.lines().any(|line| {
                        let line = line.trim();
                        line.starts_with("name") && line.contains("\"lai\"")
                    })
                })
                .unwrap_or(false)
        })
    }
    fn is_allowed(&self, cmd: &str) -> bool {
        const ALLOWED: &[&str] = &[
            "cargo", "rustc", "python3", "pytest", "git", "ls", "cat", "grep", "find", "head",
            "tail", "wc", "echo", "mkdir", "touch",
        ];
        ALLOWED
            .iter()
            .copied()
            .chain(
                self.config
                    .extra_allowed_commands
                    .iter()
                    .map(String::as_str),
            )
            .any(|a| cmd == a || cmd.ends_with(&format!(".{a}")))
    }
    fn has_secrets(&self, text: &str) -> bool {
        text.contains("api_key") || text.contains("password") || text.contains("secret")
    }
}

/// Resuelve una ruta para compararla con el workspace.
///
/// Si existe, `canonicalize` resuelve `..` y symlinks. Si no existe (por
/// ejemplo, un archivo nuevo), se canonicaliza el directorio padre y se
/// conserva el nombre; si el padre tampoco existe, se normaliza `..` de forma
/// léxica para no aceptar `../fuera.txt` por accidente.
fn resolve_for_policy(path: &Path) -> PathBuf {
    if let Ok(canon) = path.canonicalize() {
        return normalize_path(&canon);
    }
    if let Some(parent) = path.parent() {
        if let Ok(parent) = parent.canonicalize() {
            if let Some(name) = path.file_name() {
                return normalize_path(&parent.join(name));
            }
        }
    }
    normalize_lexically(path)
}

/// Colapsa `.` y `..` sin tocar el sistema de archivos.
fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() && !out.has_root() {
                    out.push("..");
                }
            }
            Component::RootDir | Component::Prefix(_) => out.push(component.as_os_str()),
            Component::Normal(part) => out.push(part),
        }
    }
    normalize_path(&out)
}

/// ¿La ruta relativa contiene `name` como componente completo?
fn path_has_component(path: &str, name: &str) -> bool {
    path.split('/').any(|component| component == name)
}

/// Strip the Windows verbatim prefix (`\\?\`) so canonicalized paths compare
/// correctly with regular absolute paths.
fn normalize_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{}", rest))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(extra: Vec<&str>) -> PolicyConfig {
        PolicyConfig {
            extra_allowed_commands: extra.into_iter().map(str::to_string).collect(),
            ..PolicyConfig::default()
        }
    }

    #[test]
    fn default_allowlist_accepts_cargo_and_rejects_shell_abuse() {
        let mut engine = PolicyEngine::new(PolicyConfig::default());
        assert!(
            engine
                .check(&Operation::CommandExec {
                    command: "cargo test".into()
                })
                .allowed
        );
        assert!(
            !engine
                .check(&Operation::CommandExec {
                    command: "curl http://example.com".into()
                })
                .allowed
        );
    }

    #[test]
    fn extra_allowed_commands_extend_the_allowlist() {
        let mut engine = PolicyEngine::new(config_with(vec!["make", "sed"]));
        assert!(
            engine
                .check(&Operation::CommandExec {
                    command: "make check".into()
                })
                .allowed
        );
        assert!(
            engine
                .check(&Operation::CommandExec {
                    command: "sed -n 1p file".into()
                })
                .allowed
        );
        assert!(
            !engine
                .check(&Operation::CommandExec {
                    command: "curl http://example.com".into()
                })
                .allowed
        );
    }

    #[test]
    fn bash_tool_is_checked_like_run_command() {
        let mut engine = PolicyEngine::new(PolicyConfig::default());
        let allowed = engine.check_tool(
            "bash",
            &HashMap::from([("command".to_string(), Value::String("ls".into()))]),
        );
        assert!(allowed.allowed, "{}", allowed.reason);
        let denied = engine.check_tool(
            "bash",
            &HashMap::from([(
                "command".to_string(),
                Value::String("curl http://example.com".into()),
            )]),
        );
        assert!(!denied.allowed);
    }

    #[test]
    fn command_allowlist_env_parsing_is_pure() {
        for v in ["1", "true", "YES", "on"] {
            assert!(matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ));
        }
        for v in ["0", "false", "", "off"] {
            assert!(!matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ));
        }
    }
}
