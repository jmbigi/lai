//! Contratos v5.0 para herramientas externas (capa de integración).
//!
//! Las herramientas externas de verificación (α,β-CROWN/Marabou/ERAN para
//! redes neuronales, Dafny/TLA+ para métodos formales, Hypothesis/fast-check
//! para testing basado en propiedades) **no declaran SUCCESS**: producen
//! resultados parciales (safe / unsafe / timeout / unknown) que esta capa
//! mapea al veredicto acotado del `VerificationEngine` (v4.0):
//!
//! - `SUCCESS` — verificado plenamente y dentro del alcance.
//! - `SUCCESS_WITH_DECLARED_GAPS` — verificado parcialmente; lo no verificado
//!   queda declarado como gap explícito.
//! - `PARTIALLY_VERIFIED` — evidencia insuficiente pero sin violación.
//! - `BLOCKED` — el alcance exigía verificar y no se pudo (timeout/unknown/
//!   unsafe sobre un contrato requerido).
//!
//! Regla de admisión (v5.0): ninguna herramienta externa se adopta de forma
//! permanente sin medición contra línea base en `lai bench`; ninguna sustituye
//! un control determinista por una afirmación del modelo.

use crate::verifier::Verdict;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Resultado parcial de una herramienta externa de verificación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalVerdict {
    /// Propiedad verificada (safe / proved / ok).
    Safe,
    /// Propiedad violada (unsafe / counterexample encontrado).
    Unsafe,
    /// Límite de tiempo/profundidad agotado sin conclusión.
    Timeout,
    /// Herramienta no disponible, no concluyente o error.
    Unknown,
}

impl ExternalVerdict {
    /// Parsea la etiqueta textual que emiten las herramientas externas.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "safe" | "proved" | "proven" | "ok" | "pass" | "passed" | "valid" => ExternalVerdict::Safe,
            "unsafe" | "violated" | "counterexample" | "fail" | "failed" | "invalid" => ExternalVerdict::Unsafe,
            "timeout" | "unknown-timeout" | "inconclusive" => ExternalVerdict::Timeout,
            _ => ExternalVerdict::Unknown,
        }
    }

    /// Mapea el resultado externo al veredicto acotado, considerando si la
    /// propiedad era **requerida** por el `verification_scope` de la tarea.
    ///
    /// - Requerida + Safe → `SUCCESS`.
    /// - Requerida + Unsafe → `BLOCKED` (violación detectada: no se declara éxito).
    /// - Requerida + Timeout/Unknown → `BLOCKED` (no se ejecutó lo que se debía).
    /// - Opcional + Safe → `SUCCESS_WITH_DECLARED_GAPS` (lo demás queda declarado).
    /// - Opcional + Unsafe → `PARTIALLY_VERIFIED` (se registra la violación).
    /// - Opcional + Timeout/Unknown → `SUCCESS_WITH_DECLARED_GAPS`.
    pub fn to_verdict(&self, required: bool) -> Verdict {
        match (self, required) {
            (ExternalVerdict::Safe, true) => Verdict::Success,
            (ExternalVerdict::Safe, false) => Verdict::SuccessWithDeclaredGaps(vec![
                "verificación externa ejecutada; resto del alcance no cubierto por esta herramienta".to_string(),
            ]),
            (ExternalVerdict::Unsafe, true) => Verdict::Blocked(format!(
                "propiedad requerida VIOLADA (counterexample externo)"
            )),
            (ExternalVerdict::Unsafe, false) => {
                Verdict::PartiallyVerified
            }
            (ExternalVerdict::Timeout, true) => Verdict::Blocked(
                "verificación externa requerida agotó timeout sin conclusión".to_string(),
            ),
            (ExternalVerdict::Timeout, false) => Verdict::SuccessWithDeclaredGaps(vec![
                "verificación externa opcional agotó timeout (declarado)".to_string(),
            ]),
            (ExternalVerdict::Unknown, true) => Verdict::Blocked(
                "verificación externa requerida no concluyente (unknown)".to_string(),
            ),
            (ExternalVerdict::Unknown, false) => Verdict::SuccessWithDeclaredGaps(vec![
                "verificación externa opcional no concluyente (declarado)".to_string(),
            ]),
        }
    }
}

/// Una medición de admisión: compara la herramienta candidata contra la
/// línea base en una métrica concreta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub tool: String,
    pub metric: String,
    pub baseline_value: f64,
    pub candidate_value: f64,
    /// true si la métrica no se degrada (candidato >= baseline para métricas
    /// de "más es mejor"; el llamador invierte el signo para latencia).
    pub no_regression: bool,
    pub notes: String,
}

impl Measurement {
    pub fn improvement(&self) -> f64 {
        self.candidate_value - self.baseline_value
    }
}

/// Puerta de admisión v5.0: registra mediciones y decide si una herramienta
/// externa puede adoptarse de forma permanente.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdmissionGate {
    pub measurements: Vec<Measurement>,
}

impl AdmissionGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, m: Measurement) {
        self.measurements.push(m);
    }

    /// Una herramienta puede adoptarse si existe al menos una medición que
    /// muestre mejora (o no regresión) en su métrica objetivo.
    pub fn may_adopt(&self, tool: &str) -> bool {
        self.measurements
            .iter()
            .any(|m| m.tool == tool && m.no_regression)
    }

    /// Mejoras registradas por herramienta (para `lai bench`).
    pub fn summary(&self) -> HashMap<String, (f64, usize)> {
        let mut out = HashMap::new();
        for m in &self.measurements {
            let e = out.entry(m.tool.clone()).or_insert((0.0, 0));
            e.0 += m.improvement();
            e.1 += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_external_labels() {
        assert_eq!(ExternalVerdict::parse("safe"), ExternalVerdict::Safe);
        assert_eq!(ExternalVerdict::parse("PROVED"), ExternalVerdict::Safe);
        assert_eq!(ExternalVerdict::parse("unsafe"), ExternalVerdict::Unsafe);
        assert_eq!(ExternalVerdict::parse("counterexample"), ExternalVerdict::Unsafe);
        assert_eq!(ExternalVerdict::parse("timeout"), ExternalVerdict::Timeout);
        assert_eq!(ExternalVerdict::parse("whatever"), ExternalVerdict::Unknown);
    }

    #[test]
    fn test_required_safe_is_success() {
        let v = ExternalVerdict::Safe.to_verdict(true);
        assert!(matches!(v, Verdict::Success));
        assert!(v.is_success());
    }

    #[test]
    fn test_required_unsafe_is_blocked() {
        let v = ExternalVerdict::Unsafe.to_verdict(true);
        assert!(matches!(v, Verdict::Blocked(_)));
        assert!(!v.is_success());
    }

    #[test]
    fn test_required_timeout_is_blocked() {
        // Una prueba que no cierra NO es un gap declarado si era requerida.
        let v = ExternalVerdict::Timeout.to_verdict(true);
        assert!(matches!(v, Verdict::Blocked(_)));
    }

    #[test]
    fn test_optional_timeout_is_declared_gap() {
        let v = ExternalVerdict::Timeout.to_verdict(false);
        assert!(matches!(v, Verdict::SuccessWithDeclaredGaps(_)));
    }

    #[test]
    fn test_admission_gate_requires_measurement() {
        let mut gate = AdmissionGate::new();
        assert!(!gate.may_adopt("alpha-beta-crown"));
        gate.record(Measurement {
            tool: "alpha-beta-crown".to_string(),
            metric: "verification_effective_rate".to_string(),
            baseline_value: 0.5,
            candidate_value: 0.6,
            no_regression: true,
            notes: "test".to_string(),
        });
        assert!(gate.may_adopt("alpha-beta-crown"));
        assert!(!gate.may_adopt("other-tool"));
    }

    #[test]
    fn test_admission_gate_rejects_regression() {
        let mut gate = AdmissionGate::new();
        gate.record(Measurement {
            tool: "mem0".to_string(),
            metric: "tokens_per_task".to_string(),
            baseline_value: 1000.0,
            candidate_value: 1200.0,
            no_regression: false,
            notes: "más tokens".to_string(),
        });
        assert!(!gate.may_adopt("mem0"));
    }
}