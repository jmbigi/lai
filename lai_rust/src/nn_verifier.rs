//! Verificación de redes neuronales (v5.0, herramienta 2: α,β-CROWN /
//! Marabou / ERAN).
//!
//! `lai` no incorpora hoy componentes ML propios; este adaptador deja el
//! punto de anclaje preparado y **fail-closed**: cuando un `SPEC.md` declara
//! `nn_robustness` en su `verification_scope`, se ejecuta el verificador
//! externo configurado (α,β-CROWN vía `python3 -m auto_LiRPA`, Marabou,
//! ERAN o cualquier binario compatible) mediante un protocolo JSON:
//!
//! ```text
//! entrada (stdin):  {"model": "...", "property": "robustness",
//!                    "input_bounds": [[lo, hi], ...], "epsilon": 0.1}
//! salida (stdout):  {"status": "safe"|"unsafe"|"timeout"|"unknown",
//!                    "detail": "..."}
//! ```
//!
//! Reglas de veredicto (v5.0, vía `contracts.rs`):
//! - Verificador no disponible o protocolo inválido → `Unknown` → si la
//!   propiedad era requerida, `BLOCKED` (no se ejecutó lo que se debía).
//! - `safe` → `Safe`; `unsafe` → `Unsafe` (counterexample).
//! - `timeout` → `Timeout`.
//!
//! α,β-CROWN alcanza ~100 % de coincidencia con la referencia en benchmarks
//! comparables; Marabou destaca en detección rápida en redes pequeñas pero
//! tiene un muro de escalabilidad: el adaptador no promete lo que no mide.

use crate::contracts::ExternalVerdict;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Especificación de una propiedad de robustez neuronal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NnPropertySpec {
    pub model: String,
    /// "robustness" (l_inf) | "monotonicity".
    pub property: String,
    /// Cotas de entrada por dimensión: [[lo, hi], ...].
    pub input_bounds: Vec<[f64; 2]>,
    #[serde(default = "default_epsilon")]
    pub epsilon: f64,
    /// Tiempo máximo para el verificador externo.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Comando del verificador externo (ej: "python3 -m auto_LiRPA.verify").
    #[serde(default = "default_verifier_cmd")]
    pub verifier_cmd: String,
}

fn default_epsilon() -> f64 {
    0.1
}
fn default_timeout_secs() -> u64 {
    120
}
fn default_verifier_cmd() -> String {
    "auto_LiRPA".to_string()
}

/// Resultado de la verificación neuronal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NnVerificationResult {
    pub verdict: ExternalVerdict,
    pub detail: String,
    pub duration_ms: u64,
    /// true si el verificador externo estuvo disponible y respondió.
    pub verifier_available: bool,
}

impl NnVerificationResult {
    pub fn unavailable(detail: String) -> Self {
        Self {
            verdict: ExternalVerdict::Unknown,
            detail,
            duration_ms: 0,
            verifier_available: false,
        }
    }
}

/// Ejecuta el verificador externo vía stdin/stdout JSON con timeout.
/// Fail-closed: cualquier fallo produce `Unknown` (nunca `safe` por defecto).
pub fn verify_nn(spec: &NnPropertySpec) -> NnVerificationResult {
    let input = serde_json::json!({
        "model": spec.model,
        "property": spec.property,
        "input_bounds": spec.input_bounds,
        "epsilon": spec.epsilon,
    });
    let input_str = input.to_string();

    let mut parts = spec.verifier_cmd.split_whitespace();
    let Some(program) = parts.next() else {
        return NnVerificationResult::unavailable("verifier_cmd vacío".to_string());
    };
    let args: Vec<&str> = parts.collect();

    let mut child = match Command::new(program)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return NnVerificationResult::unavailable(format!(
                "verificador neuronal no disponible ({}): {}",
                spec.verifier_cmd, e
            ))
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input_str.as_bytes());
    }

    let start = std::time::Instant::now();
    let deadline = start + Duration::from_secs(spec.timeout_secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    return NnVerificationResult {
                        verdict: ExternalVerdict::Timeout,
                        detail: format!("timeout tras {}s", spec.timeout_secs),
                        duration_ms: start.elapsed().as_millis() as u64,
                        verifier_available: true,
                    };
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                return NnVerificationResult::unavailable(format!("error esperando verificador: {}", e))
            }
        }
    };

    let duration_ms = start.elapsed().as_millis() as u64;
    let stdout = match child.wait_with_output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
        Err(e) => {
            return NnVerificationResult::unavailable(format!("error leyendo salida: {}", e))
        }
    };
    let _ = status;
    parse_verifier_output(&stdout, duration_ms)
}

/// Parsea la salida JSON del verificador externo.
pub fn parse_verifier_output(stdout: &str, duration_ms: u64) -> NnVerificationResult {
    let value: serde_json::Value = match serde_json::from_str(stdout.trim()) {
        Ok(v) => v,
        Err(e) => {
            return NnVerificationResult {
                verdict: ExternalVerdict::Unknown,
                detail: format!("salida no JSON del verificador: {}", e),
                duration_ms,
                verifier_available: true,
            }
        }
    };
    let status = value
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let detail = value
        .get("detail")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    NnVerificationResult {
        verdict: ExternalVerdict::parse(status),
        detail,
        duration_ms,
        verifier_available: true,
    }
}

/// Evalúa la propiedad neuronal y la mapea al veredicto acotado.
pub fn verify_nn_to_verdict(spec: &NnPropertySpec, required: bool) -> crate::verifier::Verdict {
    let result = verify_nn(spec);
    let verdict = result.verdict.to_verdict(required);
    if !result.verifier_available {
        tracing::warn!(
            "NN verification unavailable (required={}): {}",
            required,
            result.detail
        );
    }
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_safe() {
        let r = parse_verifier_output(r#"{"status": "safe", "detail": "proved"}"#, 100);
        assert_eq!(r.verdict, ExternalVerdict::Safe);
        assert!(r.verifier_available);
    }

    #[test]
    fn test_parse_unsafe_with_counterexample() {
        let r = parse_verifier_output(r#"{"status": "unsafe", "detail": "x=[0.5]"}"#, 50);
        assert_eq!(r.verdict, ExternalVerdict::Unsafe);
    }

    #[test]
    fn test_parse_timeout() {
        let r = parse_verifier_output(r#"{"status": "timeout"}"#, 120000);
        assert_eq!(r.verdict, ExternalVerdict::Timeout);
    }

    #[test]
    fn test_parse_garbage_is_unknown() {
        let r = parse_verifier_output("not json at all", 10);
        assert_eq!(r.verdict, ExternalVerdict::Unknown);
        assert!(r.verifier_available);
    }

    #[test]
    fn test_unavailable_is_fail_closed() {
        let r = NnVerificationResult::unavailable("no existe".to_string());
        assert_eq!(r.verdict, ExternalVerdict::Unknown);
        assert!(!r.verifier_available);
        // Requerido + no disponible -> BLOCKED.
        let v = r.verdict.to_verdict(true);
        assert!(matches!(v, crate::verifier::Verdict::Blocked(_)));
    }

    #[test]
    fn test_verify_nn_unavailable_command_is_unknown() {
        let spec = NnPropertySpec {
            model: "m.onnx".to_string(),
            property: "robustness".to_string(),
            input_bounds: vec![[0.0, 1.0]],
            epsilon: 0.1,
            timeout_secs: 5,
            verifier_cmd: "/nonexistent/verifier-xyz".to_string(),
        };
        let r = verify_nn(&spec);
        assert_eq!(r.verdict, ExternalVerdict::Unknown);
        assert!(!r.verifier_available);
    }

    #[test]
    fn test_required_unavailable_maps_blocked() {
        let spec = NnPropertySpec {
            model: "m.onnx".to_string(),
            property: "robustness".to_string(),
            input_bounds: vec![],
            epsilon: 0.1,
            timeout_secs: 5,
            verifier_cmd: "/nonexistent/verifier-xyz".to_string(),
        };
        let v = verify_nn_to_verdict(&spec, true);
        assert!(matches!(v, crate::verifier::Verdict::Blocked(_)));
    }
}