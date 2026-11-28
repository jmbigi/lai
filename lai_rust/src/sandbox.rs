//! sandbox.rs — aislamiento de ejecución con bubblewrap (bwrap).
//!
//! Portado de la rama `feat/fase-0-1-harness` (ver
//! `DOCS/ADR-0001-rama-fase-0-1-harness.md`). El CLI Rust ejecutaba
//! `run_command` con `sh -c` y `run_python` con `python3` sin aislamiento; este
//! módulo añade una vía opcional y un modo **fail-closed**.
//!
//! Modos:
//!
//! - **Opt-in** (`sandbox: true` en la herramienta): se ejecuta en bwrap. Si el
//!   preflight falla, la llamada falla; nunca se cae silenciosamente a ejecución
//!   sin aislamiento.
//! - **Requerido** (`LAI_REQUIRE_SANDBOX=1` o `lai --sandbox …`): todas las
//!   ejecuciones de shell/Python van por bwrap y no se pueden desactivar con
//!   `sandbox: false`. Si bwrap no está disponible, la llamada falla.
//!
//! Advertencia operativa: en Ubuntu 24.04+ AppArmor puede restringir los user
//! namespaces no privilegiados e impedir que bwrap funcione. El preflight
//! ejecuta una sonda real (no solo `--version`) y falla cerrado.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Estado del sandbox tras el preflight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxStatus {
    /// true si el sandbox de namespaces puede aplicarse realmente.
    pub available: bool,
    /// Mecanismo detectado ("bubblewrap", "none").
    pub mechanism: String,
    /// Detalle diagnóstico (versión, error, recomendación).
    pub detail: String,
    /// true si se ejecutó la sonda real de namespaces (no solo `--version`).
    pub probe_executed: bool,
}

impl Default for SandboxStatus {
    fn default() -> Self {
        Self {
            available: false,
            mechanism: "none".to_string(),
            detail: "preflight no ejecutado".to_string(),
            probe_executed: false,
        }
    }
}

static STATUS: OnceLock<SandboxStatus> = OnceLock::new();

/// Ejecuta el preflight una única vez y lo cachea.
pub fn preflight() -> &'static SandboxStatus {
    STATUS.get_or_init(run_preflight)
}

fn run_preflight() -> SandboxStatus {
    // 1. ¿Existe bubblewrap?
    let version = Command::new("bwrap").arg("--version").output();
    let version_str = match version {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => {
            return SandboxStatus {
                available: false,
                mechanism: "none".to_string(),
                detail: "bubblewrap (bwrap) no está instalado o no es ejecutable".to_string(),
                probe_executed: false,
            };
        }
    };

    // 2. Sonda real: crear namespaces y ejecutar un comando trivial. En Ubuntu
    //    24.04+ con AppArmor restringido esto falla aunque bwrap exista.
    //
    //    Se usan los mismos flags críticos que en `build_command_full`
    //    (`--unshare-all`, `--clearenv`, `--setenv`, `--tmpfs`) para que el
    //    preflight detecte también un bwrap demasiado antiguo.
    let probe = Command::new("bwrap")
        .args([
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--tmpfs",
            "/run",
            "--unshare-all",
            "--clearenv",
            "--setenv",
            "PATH",
            "/usr/bin:/bin",
            "--die-with-parent",
            "true",
        ])
        .output();

    match probe {
        Ok(out) if out.status.success() => SandboxStatus {
            available: true,
            mechanism: "bubblewrap".to_string(),
            detail: format!("{version_str}; namespaces PID+net OK"),
            probe_executed: true,
        },
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            SandboxStatus {
                available: false,
                mechanism: "bubblewrap".to_string(),
                detail: format!(
                    "{}; la sonda de namespaces falló (posible restricción AppArmor de \
                     user namespaces, o bwrap sin --clearenv/--tmpfs): {}",
                    version_str,
                    stderr.trim().chars().take(200).collect::<String>()
                ),
                probe_executed: true,
            }
        }
        Err(e) => SandboxStatus {
            available: false,
            mechanism: "bubblewrap".to_string(),
            detail: format!("{version_str}; no se pudo ejecutar la sonda: {e}"),
            probe_executed: true,
        },
    }
}

/// ¿El operador exige sandbox para todas las ejecuciones?
pub fn required_from_env() -> bool {
    env_flag("LAI_REQUIRE_SANDBOX")
}

/// ¿Se permite red dentro del sandbox? Por defecto no (aislamiento real).
pub fn allow_net_from_env() -> bool {
    env_flag("LAI_SANDBOX_ALLOW_NET")
}

/// Parsea un flag booleano de entorno ("1", "true", "yes", "on").
pub fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Resumen del estado del sandbox para `lai doctor` y `lai status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxReport {
    pub available: bool,
    pub mechanism: String,
    pub detail: String,
    pub required: bool,
    pub allow_net: bool,
}

impl SandboxReport {
    pub fn collect() -> Self {
        let status = preflight();
        Self {
            available: status.available,
            mechanism: status.mechanism.clone(),
            detail: status.detail.clone(),
            required: required_from_env(),
            allow_net: allow_net_from_env(),
        }
    }

    /// ¿Es un problema bloqueante? Solo si el operador exige sandbox y no hay.
    pub fn is_blocking(&self) -> bool {
        self.required && !self.available
    }

    /// Línea corta para `lai status`.
    pub fn short_line(&self) -> String {
        format!(
            "sandbox: {} ({}, {}, red {})",
            if self.available {
                "disponible"
            } else {
                "no disponible"
            },
            self.mechanism,
            if self.required {
                "obligatorio"
            } else {
                "opt-in"
            },
            if self.allow_net {
                "permitida"
            } else {
                "bloqueada"
            }
        )
    }

    /// Líneas detalladas para `lai doctor`.
    pub fn detail_lines(&self) -> Vec<String> {
        vec![
            format!(
                "sandbox: {} ({})",
                if self.available {
                    "disponible"
                } else {
                    "no disponible"
                },
                self.mechanism
            ),
            format!("  detalle: {}", self.detail),
            format!(
                "  modo: {}",
                if self.required {
                    "obligatorio (LAI_REQUIRE_SANDBOX/--sandbox, fail-closed)"
                } else {
                    "opt-in (\"sandbox\": true)"
                }
            ),
            format!(
                "  red dentro del sandbox: {}",
                if self.allow_net {
                    "permitida (LAI_SANDBOX_ALLOW_NET=1)"
                } else {
                    "bloqueada (por defecto)"
                }
            ),
        ]
    }
}

/// Error accionable cuando se pidió sandbox y no está disponible.
fn unavailable_error(detail: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "sandbox solicitado pero no disponible: {detail}. Instala bubblewrap \
         (`sudo apt install bubblewrap`) y comprueba que los user namespaces no \
         estén restringidos por AppArmor; o ejecuta con \"sandbox\": false si \
         aceptas ejecutar sin aislamiento."
    )
}

/// Construye el `Command` de bwrap para ejecutar `command` con el workspace
/// montado en lectura/escritura y sin red.
///
/// `required` controla si un sandbox no disponible es un error (true) o si se
/// devuelve `Ok(None)` para que el llamador ejecute sin aislamiento (false).
/// Con `required = true` **nunca** se ejecuta sin sandbox.
pub fn build_command(
    command: &str,
    workspace: &Path,
    required: bool,
) -> anyhow::Result<Option<Command>> {
    build_command_with_status(command, workspace, required, preflight())
}

/// Variante testeable: recibe el estado del preflight en lugar de consultarlo.
/// Con `required = true` y un estado no disponible devuelve error (fail-closed);
/// nunca devuelve `Ok(None)` en ese caso.
pub fn build_command_with_status(
    command: &str,
    workspace: &Path,
    required: bool,
    status: &SandboxStatus,
) -> anyhow::Result<Option<Command>> {
    build_command_full(command, workspace, required, allow_net_from_env(), status)
}

/// Construcción completa con red explícita (para tests y para `sandbox_network`).
pub fn build_command_full(
    command: &str,
    workspace: &Path,
    required: bool,
    allow_net: bool,
    status: &SandboxStatus,
) -> anyhow::Result<Option<Command>> {
    build_command_with_binds(command, workspace, required, allow_net, status, &[])
}

/// Igual que `build_command_full`, pero monta archivos adicionales en solo
/// lectura dentro del sandbox.
///
/// Se usa para `run_python`: el script vive en un temporal del host y se monta
/// como `/tmp/lai_script.py` sobre un `/tmp` privado (`--tmpfs`). Así el sandbox
/// deja de compartir el `/tmp` del host.
pub fn build_command_with_binds(
    command: &str,
    workspace: &Path,
    required: bool,
    allow_net: bool,
    status: &SandboxStatus,
    extra_ro_binds: &[(std::path::PathBuf, String)],
) -> anyhow::Result<Option<Command>> {
    #[cfg(not(unix))]
    {
        let _ = (
            command,
            workspace,
            required,
            allow_net,
            status,
            extra_ro_binds,
        );
        return Err(anyhow::anyhow!(
            "el sandbox bubblewrap solo está disponible en Linux/Unix; \
             usa \"sandbox\": false o no definas LAI_REQUIRE_SANDBOX"
        ));
    }

    #[cfg(unix)]
    {
        if !status.available {
            if required {
                return Err(unavailable_error(&status.detail));
            }
            return Ok(None);
        }

        let workspace = workspace.canonicalize().map_err(|e| {
            anyhow::anyhow!(
                "sandbox: no se pudo resolver el workspace '{}': {e}",
                workspace.display()
            )
        })?;
        let ws = workspace.to_string_lossy().to_string();

        let mut cmd = Command::new("bwrap");
        // Raíces de solo lectura imprescindibles para el shell y Python.
        for dir in ["/usr", "/lib", "/lib64", "/bin", "/sbin", "/etc"] {
            if Path::new(dir).exists() {
                cmd.args(["--ro-bind", dir, dir]);
            }
        }
        // /tmp privado: no se comparte el del host (otros procesos podrían leer
        // los temporales del agente). Va ANTES del bind del workspace: bwrap
        // aplica los montajes en orden, y un workspace dentro de /tmp quedaría
        // tapado por el tmpfs si se montara primero.
        cmd.args(["--tmpfs", "/tmp"]);
        // El workspace es lo único escribible.
        cmd.args(["--bind", &ws, &ws]);
        // Los scripts a ejecutar se montan como archivos de solo lectura
        // (después del tmpfs para que no los tape).
        for (source, target) in extra_ro_binds {
            cmd.args(["--ro-bind", &source.to_string_lossy(), target.as_str()]);
        }
        // /run contiene sockets de demonios del host (Docker, systemd). Un
        // montaje de solo lectura no impide conectarse, así que se oculta.
        if Path::new("/run").exists() {
            cmd.args(["--tmpfs", "/run"]);
        }
        // Caché de Cargo/Rustup: lectura para toolchains, escritura para el
        // registro de dependencias. Sin red, no se puede descargar nada nuevo.
        let mut home_path = None;
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            home_path = Some(home.clone());
            let cargo = home.join(".cargo");
            if cargo.is_dir() {
                let p = cargo.to_string_lossy().to_string();
                cmd.args(["--bind", &p, &p]);
            }
            let rustup = home.join(".rustup");
            if rustup.is_dir() {
                let p = rustup.to_string_lossy().to_string();
                cmd.args(["--ro-bind", &p, &p]);
            }
            let local = home.join(".local");
            if local.is_dir() {
                let p = local.to_string_lossy().to_string();
                cmd.args(["--ro-bind", &p, &p]);
            }
        }
        cmd.args([
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
        ]);
        // Entorno limpio: sin esto se heredan `AWS_SECRET_ACCESS_KEY`,
        // `GITHUB_TOKEN`, `SSH_AUTH_SOCK`, etc., y el comando puede leerlos o
        // exfiltrarlos. Solo se pasan las variables necesarias.
        cmd.arg("--clearenv");
        for (key, value) in sandbox_env(home_path.as_deref()) {
            cmd.args(["--setenv", &key, &value]);
        }
        // `--unshare-all` incluye red. Si el operador la permite explícitamente,
        // se vuelve a compartir la red del host (sigue aislado en PID/mount/IPC).
        if allow_net {
            cmd.arg("--share-net");
        }
        cmd.args(["--chdir", &ws, "--", "bash", "-c", command]);
        Ok(Some(cmd))
    }
}

/// Entorno mínimo que se pasa al proceso sandboxeado.
///
/// No se hereda nada del host: solo `PATH`, `HOME`, `TERM`, `LANG`/`LC_ALL`.
/// Así un `GITHUB_TOKEN` o `AWS_SECRET_ACCESS_KEY` presente en la sesión no
/// llega al comando que ejecuta el modelo.
pub fn sandbox_env(home: Option<&Path>) -> Vec<(String, String)> {
    let mut env = Vec::new();
    let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    env.push(("PATH".to_string(), path));
    if let Some(home) = home {
        env.push(("HOME".to_string(), home.to_string_lossy().to_string()));
    }
    if let Ok(term) = std::env::var("TERM") {
        env.push(("TERM".to_string(), term));
    }
    for key in ["LANG", "LC_ALL", "LC_CTYPE"] {
        if let Ok(value) = std::env::var(key) {
            env.push((key.to_string(), value));
        }
    }
    // Cargo/Rustup necesitan saber dónde están cuando HOME no es el real.
    if let Some(home) = home {
        env.push((
            "CARGO_HOME".to_string(),
            home.join(".cargo").to_string_lossy().to_string(),
        ));
        env.push((
            "RUSTUP_HOME".to_string(),
            home.join(".rustup").to_string_lossy().to_string(),
        ));
    }
    env
}

/// Comando envuelto en bwrap (para diagnóstico y tests).
pub fn wrapped_argv(command: &str, workspace: &Path) -> Vec<String> {
    wrapped_argv_with_net(command, workspace, allow_net_from_env())
}

/// Igual que `wrapped_argv` pero con red explícita.
pub fn wrapped_argv_with_net(command: &str, workspace: &Path, allow_net: bool) -> Vec<String> {
    match build_command_full(command, workspace, false, allow_net, preflight()) {
        Ok(Some(cmd)) => {
            let mut out = vec![cmd.get_program().to_string_lossy().to_string()];
            out.extend(
                cmd.get_args()
                    .map(|a| a.to_string_lossy().to_string())
                    .collect::<Vec<_>>(),
            );
            out
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_is_cached() {
        let a = preflight() as *const SandboxStatus;
        let b = preflight() as *const SandboxStatus;
        assert_eq!(a, b, "el preflight debe cachearse");
    }

    #[test]
    fn default_status_fails_closed() {
        let st = SandboxStatus::default();
        assert!(!st.available);
        assert_eq!(st.mechanism, "none");
    }

    #[test]
    fn required_env_parsing_is_explicit() {
        // No se puede mutar el entorno del proceso en paralelo con otros tests;
        // se comprueba la lógica pura replicada aquí.
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

    #[test]
    fn wrap_argv_has_isolation_flags() {
        let argv = wrapped_argv("echo hi", Path::new("/tmp"));
        if argv.is_empty() {
            // Sin bwrap disponible no hay nada que comprobar.
            return;
        }
        assert_eq!(argv[0], "bwrap");
        for flag in [
            "--unshare-all",
            "--die-with-parent",
            "--cap-drop",
            "--chdir",
        ] {
            assert!(argv.iter().any(|a| a == flag), "falta {flag}: {argv:?}");
        }
        assert!(argv.iter().any(|a| a == "echo hi"));
    }

    #[test]
    fn required_without_sandbox_is_an_error_not_a_fallback() {
        // Determinista: no depende de si bwrap está instalado en la máquina.
        let unavailable = SandboxStatus {
            available: false,
            mechanism: "none".to_string(),
            detail: "bwrap no instalado".to_string(),
            probe_executed: false,
        };
        let err = build_command_with_status("true", Path::new("."), true, &unavailable)
            .unwrap_err()
            .to_string();
        assert!(err.contains("sandbox solicitado"), "{err}");
    }

    #[test]
    fn optional_without_sandbox_returns_none_instead_of_failing() {
        let unavailable = SandboxStatus {
            available: false,
            mechanism: "none".to_string(),
            detail: "bwrap no instalado".to_string(),
            probe_executed: false,
        };
        let result = build_command_with_status("true", Path::new("."), false, &unavailable)
            .expect("opcional no debe fallar");
        assert!(result.is_none(), "debe indicar que no hay sandbox");
    }

    #[test]
    fn network_is_blocked_by_default() {
        let argv = wrapped_argv_with_net("echo hi", Path::new("/tmp"), false);
        if argv.is_empty() {
            return;
        }
        assert!(argv.iter().any(|a| a == "--unshare-all"), "{argv:?}");
        assert!(!argv.iter().any(|a| a == "--share-net"), "{argv:?}");
    }

    #[test]
    fn network_can_be_allowed_explicitly() {
        let argv = wrapped_argv_with_net("echo hi", Path::new("/tmp"), true);
        if argv.is_empty() {
            return;
        }
        assert!(argv.iter().any(|a| a == "--share-net"), "{argv:?}");
        // `--unshare-all` sigue presente, pero `--share-net` lo anula solo para
        // la red; el resto de namespaces permanece aislado.
        assert!(argv.iter().any(|a| a == "--unshare-all"), "{argv:?}");
    }

    #[test]
    fn report_blocks_only_when_required_and_unavailable() {
        let base = SandboxReport {
            available: false,
            mechanism: "none".to_string(),
            detail: "sin bwrap".to_string(),
            required: false,
            allow_net: false,
        };
        assert!(!base.is_blocking());
        let required = SandboxReport {
            required: true,
            ..base.clone()
        };
        assert!(required.is_blocking());
        let ok = SandboxReport {
            available: true,
            ..required
        };
        assert!(!ok.is_blocking());
    }

    #[test]
    fn report_lines_mention_modes() {
        let report = SandboxReport {
            available: true,
            mechanism: "bubblewrap".to_string(),
            detail: "ok".to_string(),
            required: true,
            allow_net: true,
        };
        let text = report.detail_lines().join("\n");
        assert!(text.contains("obligatorio"), "{text}");
        assert!(text.contains("permitida"), "{text}");
        assert!(report.short_line().contains("disponible"));
    }

    #[test]
    fn sandbox_env_does_not_leak_secrets() {
        let env = sandbox_env(Some(Path::new("/home/user")));
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"PATH"));
        assert!(keys.contains(&"HOME"));
        for secret in [
            "AWS_SECRET_ACCESS_KEY",
            "GITHUB_TOKEN",
            "SSH_AUTH_SOCK",
            "OPENAI_API_KEY",
            "DATABASE_URL",
        ] {
            assert!(!keys.contains(&secret), "no debe pasar {secret}");
        }
    }

    #[test]
    fn sandbox_env_sets_cargo_home() {
        let env = sandbox_env(Some(Path::new("/home/user")));
        let map: std::collections::HashMap<_, _> = env.into_iter().collect();
        assert_eq!(map.get("CARGO_HOME").unwrap(), "/home/user/.cargo");
        assert_eq!(map.get("RUSTUP_HOME").unwrap(), "/home/user/.rustup");
    }
}
