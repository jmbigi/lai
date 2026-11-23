//! Property-based testing (v5.0, herramienta 9: Hypothesis / fast-check).
//!
//! Patrón "goalie": un LLM propone invariantes (generadores + propiedad), se
//! ejecutan como tests deterministas con semilla fija, y la propia suite se
//! valida **inyectando bugs** (`<=` → `<`, `==` → `!=`) para comprobar que
//! los tests fallan cuando deben fallar. Si un test no detecta el bug
//! inyectado, la propiedad es débil y no puede declararse `SUCCESS`.
//!
//! Contrato v5.0: el veredicto se mapea vía `contracts.rs`:
//! - Todos los casos pasan + mutation testing detecta los bugs → `Safe`.
//! - Algún caso falla → `Unsafe` (counterexample).
//! - Suite no valida mutation → `Unknown` (propiedad débil, no concluyente).

use crate::contracts::ExternalVerdict;
use serde::{Deserialize, Serialize};

/// Especificación de una propiedad.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySpec {
    pub id: String,
    /// Invariante en lenguaje natural (para el LLM y la documentación).
    pub invariant: String,
    /// Expresión Python que define el generador de casos (ej: `random.randint(-100, 100)`).
    pub generator: String,
    /// Expresión Python de la propiedad sobre las variables generadas.
    /// Debe evaluar a bool. Ej: `add(a, b) == a + b`.
    pub property: String,
    /// Variables que el generador produce (ej: `["a", "b"]`).
    #[serde(default)]
    pub variables: Vec<String>,
    /// Nº de casos aleatorios (default 100).
    #[serde(default = "default_cases")]
    pub cases: u32,
}

fn default_cases() -> u32 {
    100
}

/// Resultado de un caso individual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub case: usize,
    pub passed: bool,
    pub inputs: Vec<String>,
}

/// Resultado de la suite de propiedades.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySuiteResult {
    pub property_id: String,
    pub cases: Vec<CaseResult>,
    pub counterexample: Option<Vec<String>>,
    /// true si la inyección de bugs fue detectada por la propiedad.
    pub mutation_detected: bool,
    pub mutation_bug: Option<String>,
}

impl PropertySuiteResult {
    /// Mapea el resultado al veredicto externo (para `contracts.rs`).
    pub fn to_external_verdict(&self) -> ExternalVerdict {
        if self.counterexample.is_some() {
            ExternalVerdict::Unsafe
        } else if self.mutation_detected {
            ExternalVerdict::Safe
        } else {
            ExternalVerdict::Unknown
        }
    }
}

/// Modo de ejecución: `Llm` usa el LLM para generar la propiedad (no
/// determinista); `Scripted` ejecuta la propiedad propuesta directamente.
pub enum PropertyMode {
    /// Ejecuta las propiedades tal cual (determinista).
    Scripted,
}

/// Ejecuta una propiedad con casos pseudoaleatorios de semilla fija.
/// `evaluate` recibe (variable, valor) y devuelve el resultado de la propiedad.
pub fn run_property<F>(
    spec: &PropertySpec,
    mut evaluate: F,
) -> PropertySuiteResult
where
    F: FnMut(&str, &str) -> bool,
{
    let mut cases = Vec::new();
    let mut counterexample = None;
    // PRNG determinista (LCG) para reproducibilidad.
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    for i in 0..spec.cases {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut inputs = Vec::new();
        let mut all_pass = true;
        for var in &spec.variables {
            // El valor concreto lo decide `evaluate` vía el generador; aquí
            // se registra la variable y el evaluador produce el valor.
            let value = format!("{:x}", seed ^ (i as u64));
            let ok = evaluate(var, &value);
            if !ok {
                all_pass = false;
            }
            inputs.push(format!("{}={}", var, value));
        }
        if !all_pass {
            counterexample = Some(inputs.clone());
        }
        cases.push(CaseResult { case: i as usize, passed: all_pass, inputs });
        if counterexample.is_some() {
            break; // primer counterexample: suficiente
        }
    }
    PropertySuiteResult {
        property_id: spec.id.clone(),
        cases,
        counterexample,
        mutation_detected: false,
        mutation_bug: None,
    }
}

/// Bugs de inyección para validar la suite (patrón goalie):
/// sustituciones que deben ser detectadas por una propiedad fuerte.
pub fn mutation_bugs() -> Vec<(&'static str, &'static str)> {
    vec![
        ("<=", "<"),
        ("==", "!="),
        ("+", "-"),
        ("and", "or"),
        ("len(", "len(0) or len("),
    ]
}

/// Valida la suite: aplica cada bug de inyección a la propiedad y comprueba
/// que la propiedad **falla** (detecta el bug). Devuelve el primer bug
/// detectado y si la suite es fuerte.
pub fn validate_suite<F>(spec: &PropertySpec, mut evaluate: F) -> (bool, Option<String>)
where
    F: FnMut(&str, &str, &str) -> bool,
{
    for (from, to) in mutation_bugs() {
        if spec.property.contains(from) {
            // Ejecuta con la propiedad mutada; debe fallar.
            let mutated_failed = !evaluate(from, to, &spec.property);
            if mutated_failed {
                return (true, Some(format!("{} -> {}", from, to)));
            }
        }
    }
    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> PropertySpec {
        PropertySpec {
            id: "add-commutative".to_string(),
            invariant: "add(a,b) == add(b,a)".to_string(),
            generator: "random.randint(-100,100)".to_string(),
            property: "add(a, b) <= add(b, a) + 1".to_string(),
            variables: vec!["a".to_string(), "b".to_string()],
            cases: 50,
        }
    }

    #[test]
    fn test_run_property_all_pass() {
        let r = run_property(&spec(), |_, _| true);
        assert!(r.counterexample.is_none());
        assert_eq!(r.cases.len(), 50);
        assert!(r.cases.iter().all(|c| c.passed));
    }

    #[test]
    fn test_run_property_finds_counterexample() {
        let r = run_property(&spec(), |_, _| false);
        assert!(r.counterexample.is_some());
        assert!(!r.cases[0].passed);
    }

    #[test]
    fn test_verdict_mapping() {
        let mut r = run_property(&spec(), |_, _| true);
        assert_eq!(r.to_external_verdict(), ExternalVerdict::Unknown); // sin mutation
        r.mutation_detected = true;
        assert_eq!(r.to_external_verdict(), ExternalVerdict::Safe);
        r.counterexample = Some(vec!["a=1".into()]);
        assert_eq!(r.to_external_verdict(), ExternalVerdict::Unsafe);
    }

    #[test]
    fn test_mutation_bugs_nonempty() {
        assert!(!mutation_bugs().is_empty());
        assert!(mutation_bugs().iter().any(|(a, _)| *a == "<="));
    }

    #[test]
    fn test_validate_suite_detects_mutation() {
        let s = spec();
        // El evaluador simula: con la propiedad original pasa (true), con la
        // mutada falla (false) -> suite fuerte.
        let (detected, bug) = validate_suite(&s, |_from, _to, _prop| false);
        assert!(detected);
        assert!(bug.is_some());
    }

    #[test]
    fn test_validate_suite_no_mutation_if_property_lacks_patterns() {
        let s = PropertySpec {
            id: "x".into(),
            invariant: "x".into(),
            generator: "g".into(),
            property: "checker(x)".into(),
            variables: vec![],
            cases: 10,
        };
        let (detected, _) = validate_suite(&s, |_, _, _| true);
        assert!(!detected);
    }
}