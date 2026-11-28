//! contracts.rs — capa de contratos para herramientas externas de verificación.
//!
//! Portado de la rama `feat/fase-0-1-harness` (ver
//! `DOCS/ADR-0001-rama-fase-0-1-harness.md`) y adaptado al
//! `VerificationVerdict` de `master` (el `Verdict` original tenía payload y no
//! existe en esta rama).
//!
//! Las herramientas externas (α,β-CROWN/Marabou/ERAN para redes neuronales,
//! Dafny/TLA+ para métodos formales, Hypothesis/fast-check para testing basado
//! en propiedades) **no declaran SUCCESS**: producen resultados parciales
//! (safe / unsafe / timeout / unknown) que esta capa mapea al veredicto acotado:
//!
//! - `Success` — verificado plenamente y dentro del alcance.
//! - `SuccessWithDeclaredGaps` — verificado parcialmente; lo no verificado
//!   queda declarado como gap explícito.
//! - `Failed` — violación detectada fuera del alcance requerido.
//! - `Blocked` — el alcance exigía verificar y no se pudo (timeout/unknown/
//!   unsafe sobre un contrato requerido).
//!
//! Regla de admisión (v5.0): ninguna herramienta externa se adopta de forma
//! permanente sin medición contra línea base; ninguna sustituye un control
//! determinista por una afirmación del modelo.
//!
//! **Estado:** módulo disponible pero todavía no conectado a un verificador
//! externo real. Se porta para no perder la semántica de admisión y para que el
//! futuro adaptador no invente sus propias reglas.

use crate::verification_engine::VerificationVerdict;
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
            "safe" | "proved" | "proven" | "ok" | "pass" | "passed" | "valid" => {
                ExternalVerdict::Safe
            }
            "unsafe" | "violated" | "counterexample" | "fail" | "failed" | "invalid" => {
                ExternalVerdict::Unsafe
            }
            "timeout" | "unknown-timeout" | "inconclusive" => ExternalVerdict::Timeout,
            _ => ExternalVerdict::Unknown,
        }
    }

    /// Mapea el resultado externo al veredicto acotado, considerando si la
    /// propiedad era **requerida** por el alcance de verificación de la tarea.
    ///
    /// - Requerida + Safe → `Success`.
    /// - Requerida + Unsafe/Timeout/Unknown → `Blocked` (no se declara éxito).
    /// - Opcional + Safe/Timeout/Unknown → `SuccessWithDeclaredGaps`.
    /// - Opcional + Unsafe → `Failed` (se registra la violación).
    pub fn to_verdict(&self, required: bool) -> VerificationVerdict {
        match (self, required) {
            (ExternalVerdict::Safe, true) => VerificationVerdict::Success,
            (ExternalVerdict::Safe, false) => VerificationVerdict::SuccessWithDeclaredGaps,
            (ExternalVerdict::Unsafe, true) => VerificationVerdict::Blocked,
            (ExternalVerdict::Unsafe, false) => VerificationVerdict::Failed,
            (ExternalVerdict::Timeout, true) => VerificationVerdict::Blocked,
            (ExternalVerdict::Timeout, false) => VerificationVerdict::SuccessWithDeclaredGaps,
            (ExternalVerdict::Unknown, true) => VerificationVerdict::Blocked,
            (ExternalVerdict::Unknown, false) => VerificationVerdict::SuccessWithDeclaredGaps,
        }
    }

    /// ¿El veredicto permite declarar la tarea resuelta?
    pub fn is_success(&self, required: bool) -> bool {
        matches!(
            self.to_verdict(required),
            VerificationVerdict::Success | VerificationVerdict::SuccessWithDeclaredGaps
        )
    }
}

/// Una medición de admisión: compara la herramienta candidata contra la línea
/// base en una métrica concreta.
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
    fn parse_external_labels() {
        assert_eq!(ExternalVerdict::parse("safe"), ExternalVerdict::Safe);
        assert_eq!(ExternalVerdict::parse("PROVED"), ExternalVerdict::Safe);
        assert_eq!(ExternalVerdict::parse("unsafe"), ExternalVerdict::Unsafe);
        assert_eq!(
            ExternalVerdict::parse("counterexample"),
            ExternalVerdict::Unsafe
        );
        assert_eq!(ExternalVerdict::parse("timeout"), ExternalVerdict::Timeout);
        assert_eq!(ExternalVerdict::parse("whatever"), ExternalVerdict::Unknown);
    }

    #[test]
    fn required_safe_is_success() {
        assert!(matches!(
            ExternalVerdict::Safe.to_verdict(true),
            VerificationVerdict::Success
        ));
        assert!(ExternalVerdict::Safe.is_success(true));
    }

    #[test]
    fn required_unsafe_is_blocked() {
        let v = ExternalVerdict::Unsafe.to_verdict(true);
        assert!(matches!(v, VerificationVerdict::Blocked));
        assert!(!ExternalVerdict::Unsafe.is_success(true));
    }

    #[test]
    fn required_timeout_is_blocked() {
        // Una prueba que no cierra NO es un gap declarado si era requerida.
        assert!(matches!(
            ExternalVerdict::Timeout.to_verdict(true),
            VerificationVerdict::Blocked
        ));
    }

    #[test]
    fn optional_timeout_is_declared_gap() {
        assert!(matches!(
            ExternalVerdict::Timeout.to_verdict(false),
            VerificationVerdict::SuccessWithDeclaredGaps
        ));
    }

    #[test]
    fn optional_unsafe_is_failed_not_success() {
        assert!(matches!(
            ExternalVerdict::Unsafe.to_verdict(false),
            VerificationVerdict::Failed
        ));
        assert!(!ExternalVerdict::Unsafe.is_success(false));
    }

    #[test]
    fn admission_gate_requires_measurement() {
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
    fn admission_gate_rejects_regression() {
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
