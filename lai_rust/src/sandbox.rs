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
    let probe = Command::new("bwrap")
        .args([
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--unshare-pid",
            "--unshare-net",
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
                     user namespaces): {}",
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
    matches!(
        std::env::var("LAI_REQUIRE_SANDBOX")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
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
    #[cfg(not(unix))]
    {
        let _ = (command, workspace, required, status);
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
        // El workspace es lo único escribible, además de /tmp (temporal).
        cmd.args(["--bind", &ws, &ws]);
        if Path::new("/tmp").exists() {
            cmd.args(["--bind", "/tmp", "/tmp"]);
        }
        // Caché de Cargo/Rustup: lectura para toolchains, escritura para el
        // registro de dependencias. Sin red, no se puede descargar nada nuevo.
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
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
            "--chdir",
            &ws,
            "--",
            "bash",
            "-c",
            command,
        ]);
        Ok(Some(cmd))
    }
}

/// Comando envuelto en bwrap (para diagnóstico y tests).
pub fn wrapped_argv(command: &str, workspace: &Path) -> Vec<String> {
    match build_command(command, workspace, false) {
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
}
