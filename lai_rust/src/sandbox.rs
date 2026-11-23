//! sandbox.rs - SandboxManager: preflight de aislamiento (v4.0, Mejora 7.2).
//! Implementación de referencia Linux: bubblewrap (namespaces PID/user/mount/
//! network/IPC) + cgroups v2 (límites de recursos).
//!
//! Advertencia operativa v4.0: en Ubuntu 24.04+ AppArmor restringe los user
//! namespaces no privilegiados y puede impedir que bubblewrap funcione. El
//! preflight detecta esta condición al arrancar y el PolicyEngine FALLA CERRADO
//! (deniega la ejecución con sandbox) en lugar de ejecutar sin aislamiento.

use serde::{Deserialize, Serialize};
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
    /// true si se ejecutó la sonda real de namespaces (no solo --version).
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
    let version = std::process::Command::new("bwrap").arg("--version").output();
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

    // 2. Sonda real: crear namespaces y ejecutar un comando trivial.
    //    En Ubuntu 24.04+ con AppArmor restringido esto falla aunque bwrap exista.
    let probe = std::process::Command::new("bwrap")
        .args([
            "--ro-bind", "/", "/",
            "--dev", "/dev",
            "--proc", "/proc",
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
            detail: format!("{}; namespaces PID+net OK", version_str),
            probe_executed: true,
        },
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            SandboxStatus {
                available: false,
                mechanism: "bubblewrap".to_string(),
                detail: format!(
                    "{}; la sonda de namespaces falló (posible restricción AppArmor de user namespaces): {}",
                    version_str,
                    stderr.trim().chars().take(200).collect::<String>()
                ),
                probe_executed: true,
            }
        }
        Err(e) => SandboxStatus {
            available: false,
            mechanism: "bubblewrap".to_string(),
            detail: format!("{}; no se pudo ejecutar la sonda: {}", version_str, e),
            probe_executed: true,
        },
    }
}

/// Construye el comando bwrap para ejecutar `cmd` dentro del sandbox.
/// Solo debe usarse si `preflight().available` es true.
pub fn wrap_command(cmd: &str, workspace: &std::path::Path) -> Vec<String> {
    vec![
        "bwrap".to_string(),
        "--ro-bind".to_string(), "/".to_string(), "/".to_string(),
        "--bind".to_string(), workspace.to_string_lossy().to_string(), workspace.to_string_lossy().to_string(),
        "--dev".to_string(), "/dev".to_string(),
        "--proc".to_string(), "/proc".to_string(),
        "--unshare-pid".to_string(),
        "--unshare-net".to_string(),
        "--die-with-parent".to_string(),
        "bash".to_string(),
        "-c".to_string(),
        cmd.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preflight_is_cached() {
        let a = preflight() as *const SandboxStatus;
        let b = preflight() as *const SandboxStatus;
        assert_eq!(a, b, "el preflight debe cachearse");
    }

    #[test]
    fn test_wrap_command_shape() {
        let cmd = wrap_command("ls -la", std::path::Path::new("/tmp"));
        assert_eq!(cmd[0], "bwrap");
        assert!(cmd.contains(&"--unshare-net".to_string()));
        assert!(cmd.contains(&"ls -la".to_string()));
    }

    #[test]
    fn test_status_default_fails_closed() {
        let st = SandboxStatus::default();
        assert!(!st.available);
    }
}