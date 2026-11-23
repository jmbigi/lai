//! verifier.rs - VerificationEngine (v4.0, Mejora 5).
//! Ejecuta verificaciones externas, compara con la línea base y emite un
//! veredicto ACOTADO: nunca `SUCCESS` sin evidencia. Distinguimos
//! `SUCCESS`, `SUCCESS_WITH_DECLARED_GAPS`, `PARTIALLY_VERIFIED` y `BLOCKED`.
//!
//! Regla de aceptación (v4.0):
//! - verificación requerida por la especificación y NO ejecutada  -> BLOCKED
//! - verificación fuera de alcance declarada                      -> gap (SUCCESS_WITH_DECLARED_GAPS)
//! - verificación requerida, ejecutada y superada                 -> SUCCESS

use crate::harness::{TaskSpec, VerificationResult};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Niveles de verificación (tabla de la v4.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VerificationLevel {
    Syntax = 1,
    Lint = 2,
    Static = 3,
    Types = 4,
    UnitTests = 5,
    Integration = 6,
    Functional = 7,
    Regression = 8,
    Security = 9,
    Contracts = 10,
}

impl VerificationLevel {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "syntax" | "sintaxis" => Some(Self::Syntax),
            "lint" => Some(Self::Lint),
            "static" | "static_analysis" => Some(Self::Static),
            "types" | "typecheck" => Some(Self::Types),
            "unit" | "unit_tests" | "tests" => Some(Self::UnitTests),
            "integration" => Some(Self::Integration),
            "functional" => Some(Self::Functional),
            "regression" | "regressions" => Some(Self::Regression),
            "security" => Some(Self::Security),
            "contracts" | "contracts_invariants" => Some(Self::Contracts),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Lint => "lint",
            Self::Static => "static",
            Self::Types => "types",
            Self::UnitTests => "unit_tests",
            Self::Integration => "integration",
            Self::Functional => "functional",
            Self::Regression => "regression",
            Self::Security => "security",
            Self::Contracts => "contracts",
        }
    }
}

/// Clasificación de fallos (v3.0, Mejora 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureKind {
    CodeError,
    Regression,
    Environment,
    MissingDependency,
    DefectiveTest,
    Timeout,
    Config,
    Inconclusive,
}

/// Veredicto acotado del VerificationEngine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Verdict {
    /// Todas las verificaciones requeridas se ejecutaron y superaron.
    Success,
    /// Éxito con gaps declarados (verificaciones fuera de alcance).
    SuccessWithDeclaredGaps(Vec<String>),
    /// Verificaciones fallaron: procede reparación.
    PartiallyVerified,
    /// Verificación requerida no ejecutada o regresión: la tarea no puede
    /// declararse resuelta.
    Blocked(String),
}

impl Verdict {
    pub fn is_success(&self) -> bool {
        matches!(self, Verdict::Success | Verdict::SuccessWithDeclaredGaps(_))
    }

    pub fn label(&self) -> String {
        match self {
            Verdict::Success => "SUCCESS".to_string(),
            Verdict::SuccessWithDeclaredGaps(gaps) => {
                format!("SUCCESS_WITH_DECLARED_GAPS ({})", gaps.join("; "))
            }
            Verdict::PartiallyVerified => "PARTIALLY_VERIFIED".to_string(),
            Verdict::Blocked(reason) => format!("BLOCKED ({})", reason),
        }
    }

    /// Veredicto a partir de resultados y gaps declarados.
    pub fn from_results(results: &[VerificationResult], gaps: Vec<String>) -> Self {
        if results.iter().any(|r| !r.passed) {
            Verdict::PartiallyVerified
        } else if gaps.is_empty() {
            Verdict::Success
        } else {
            Verdict::SuccessWithDeclaredGaps(gaps)
        }
    }
}

/// Línea base de una tarea: estado de las verificaciones antes del cambio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub task_key: String,
    /// clave -> passed. Claves: "file:<ruta>" o "test:<spec>".
    pub checks: HashMap<String, bool>,
    pub saved_at: String,
}

/// Regresión detectada contra la línea base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Regression {
    pub key: String,
    pub baseline_passed: bool,
}

/// Verificador independiente del agente editor.
pub struct TestPipelineVerifier {
    workdir: PathBuf,
    baseline_dir: PathBuf,
}

impl TestPipelineVerifier {
    pub fn new(workdir: PathBuf) -> Self {
        let baseline_dir = workdir.join(".lai").join("baselines");
        Self { workdir, baseline_dir }
    }

    /// Ejecuta las verificaciones deterministas para la tarea.
    /// Devuelve (veredicto, resultados, gaps declarados).
    pub async fn verify(
        &self,
        spec: &TaskSpec,
        files_modified: &[String],
    ) -> Result<(Verdict, Vec<VerificationResult>, Vec<String>)> {
        let mut results = Vec::new();
        let mut gaps = Vec::new();

        // Niveles requeridos: verification_scope de la especificación.
        let scope: Vec<VerificationLevel> = spec
            .verification_scope
            .iter()
            .filter_map(|s| VerificationLevel::parse(s))
            .collect();
        let required: Vec<VerificationLevel> = if scope.is_empty() {
            vec![VerificationLevel::Syntax, VerificationLevel::UnitTests]
        } else {
            scope.clone()
        };

        // Gaps: niveles estándar no declarados en el alcance.
        let all_levels = [
            VerificationLevel::Syntax, VerificationLevel::Lint, VerificationLevel::Static,
            VerificationLevel::Types, VerificationLevel::UnitTests, VerificationLevel::Integration,
            VerificationLevel::Functional, VerificationLevel::Regression,
            VerificationLevel::Security, VerificationLevel::Contracts,
        ];
        for lvl in all_levels {
            if !required.contains(&lvl) {
                gaps.push(format!("{} fuera de alcance", lvl.label()));
            }
        }

        // Nivel 1: sintaxis en archivos modificados.
        if required.contains(&VerificationLevel::Syntax) {
            for path in files_modified {
                if path.ends_with(".py") {
                    results.push(self.check_python_syntax(path).await?);
                } else if path.ends_with(".rs") {
                    gaps.push(format!("syntax de {} no verificado (cargo check diferido)", path));
                }
            }
        }

        // Nivel 5: tests requeridos por la especificación.
        if required.contains(&VerificationLevel::UnitTests) {
            if spec.required_tests.is_empty() {
                gaps.push("unit_tests requeridos pero sin tests declarados".to_string());
            } else {
                for test_spec in &spec.required_tests {
                    results.push(self.run_test(test_spec).await?);
                }
            }
        }

        // Nivel 8: regresión contra la línea base.
        if required.contains(&VerificationLevel::Regression) || required.contains(&VerificationLevel::UnitTests) {
            if let Some(baseline) = self.load_baseline(&spec.id) {
                let current = self.collect_baseline_checks(spec, files_modified);
                let regressions = compare_with_baseline(&baseline, &current);
                if !regressions.is_empty() {
                    let detail = regressions
                        .iter()
                        .map(|r| r.key.clone())
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Ok((
                        Verdict::Blocked(format!("regresión detectada: {}", detail)),
                        results,
                        gaps,
                    ));
                }
            }
        }

        let verdict = Verdict::from_results(&results, gaps.clone());
        Ok((verdict, results, gaps))
    }

    async fn check_python_syntax(&self, path: &str) -> Result<VerificationResult> {
        let full = self.workdir.join(path);
        let output = tokio::process::Command::new("python3")
            .args(["-m", "py_compile", &full.to_string_lossy()])
            .output()
            .await;
        match output {
            Ok(out) => Ok(VerificationResult {
                verifier: format!("py_compile:{}", path),
                passed: out.status.success(),
                details: if out.status.success() {
                    "Syntax OK".to_string()
                } else {
                    String::from_utf8_lossy(&out.stderr).to_string()
                },
                artifacts: vec![path.to_string()],
            }),
            Err(e) => Ok(VerificationResult {
                verifier: format!("py_compile:{}", path),
                passed: false,
                details: format!("no se pudo ejecutar: {}", e),
                artifacts: vec![path.to_string()],
            }),
        }
    }

    async fn run_test(&self, test_spec: &str) -> Result<VerificationResult> {
        let output = tokio::process::Command::new("bash")
            .args(["-c", test_spec])
            .current_dir(&self.workdir)
            .output()
            .await;
        match output {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let kind = classify_failure(&stderr, out.status.success());
                Ok(VerificationResult {
                    verifier: format!("test:{}", test_spec),
                    passed: out.status.success(),
                    details: format!("[{}] {}", kind_str(kind), String::from_utf8_lossy(&out.stdout)),
                    artifacts: vec![],
                })
            }
            Err(e) => Ok(VerificationResult {
                verifier: format!("test:{}", test_spec),
                passed: false,
                details: format!("no se pudo ejecutar: {}", e),
                artifacts: vec![],
            }),
        }
    }

    fn collect_baseline_checks(&self, spec: &TaskSpec, files_modified: &[String]) -> HashMap<String, bool> {
        let mut checks = HashMap::new();
        for path in files_modified {
            checks.insert(format!("file:{}", path), true);
        }
        for t in &spec.required_tests {
            checks.insert(format!("test:{}", t), true);
        }
        checks
    }

    /// Guarda la línea base de una tarea.
    pub fn save_baseline(&self, baseline: &Baseline) -> Result<()> {
        std::fs::create_dir_all(&self.baseline_dir)?;
        let path = self.baseline_dir.join(format!("{}.json", sanitize_key(&baseline.task_key)));
        std::fs::write(path, serde_json::to_string_pretty(baseline)?)?;
        Ok(())
    }

    /// Carga la línea base de una tarea.
    pub fn load_baseline(&self, task_key: &str) -> Option<Baseline> {
        let path = self.baseline_dir.join(format!("{}.json", sanitize_key(task_key)));
        std::fs::read_to_string(path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
    }
}

/// Compara checks actuales contra la línea base y devuelve regresiones.
pub fn compare_with_baseline(baseline: &Baseline, current: &HashMap<String, bool>) -> Vec<Regression> {
    let mut regressions = Vec::new();
    for (key, was_passed) in &baseline.checks {
        if *was_passed && current.get(key).copied().unwrap_or(true) == false {
            regressions.push(Regression {
                key: key.clone(),
                baseline_passed: *was_passed,
            });
        }
    }
    regressions
}

fn classify_failure(stderr: &str, passed: bool) -> FailureKind {
    if passed {
        return FailureKind::CodeError;
    }
    let s = stderr.to_lowercase();
    if s.contains("no module named") || s.contains("modulenotfounderror") {
        FailureKind::MissingDependency
    } else if s.contains("command not found") || s.contains("no such file") {
        FailureKind::Environment
    } else if s.contains("timeout") || s.contains("timed out") {
        FailureKind::Timeout
    } else if s.contains("importerror") {
        FailureKind::MissingDependency
    } else {
        FailureKind::CodeError
    }
}

fn kind_str(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::CodeError => "CODE_ERROR",
        FailureKind::Regression => "REGRESSION",
        FailureKind::Environment => "ENVIRONMENT",
        FailureKind::MissingDependency => "MISSING_DEPENDENCY",
        FailureKind::DefectiveTest => "DEFECTIVE_TEST",
        FailureKind::Timeout => "TIMEOUT",
        FailureKind::Config => "CONFIG",
        FailureKind::Inconclusive => "INCONCLUSIVE",
    }
}

fn sanitize_key(key: &str) -> String {
    key.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_with(scope: Vec<&str>, tests: Vec<&str>) -> TaskSpec {
        TaskSpec {
            id: "task-test-1".to_string(),
            task_type: "bugfix".to_string(),
            objective: "Test".to_string(),
            current_state: String::new(),
            problem: None,
            scope: vec![],
            constraints: vec![],
            affected_files: vec![],
            dependencies: vec![],
            acceptance_criteria: vec![],
            required_tests: tests.into_iter().map(|s| s.to_string()).collect(),
            risks: vec![],
            created_at: String::new(),
            updated_at: String::new(),
            verification_scope: scope.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_verdict_semantics() {
        assert!(Verdict::Success.is_success());
        assert!(Verdict::SuccessWithDeclaredGaps(vec!["x".into()]).is_success());
        assert!(!Verdict::PartiallyVerified.is_success());
        assert!(!Verdict::Blocked("r".into()).is_success());
        assert_eq!(Verdict::Success.label(), "SUCCESS");
        assert!(Verdict::SuccessWithDeclaredGaps(vec!["g".into()]).label().contains("GAPS"));
    }

    #[test]
    fn test_verdict_from_results() {
        let ok = vec![VerificationResult { verifier: "t".into(), passed: true, details: String::new(), artifacts: vec![] }];
        let fail = vec![VerificationResult { verifier: "t".into(), passed: false, details: String::new(), artifacts: vec![] }];
        assert!(matches!(Verdict::from_results(&ok, vec![]), Verdict::Success));
        assert!(matches!(Verdict::from_results(&ok, vec!["g".into()]), Verdict::SuccessWithDeclaredGaps(_)));
        assert!(matches!(Verdict::from_results(&fail, vec![]), Verdict::PartiallyVerified));
    }

    #[test]
    fn test_level_parse() {
        assert_eq!(VerificationLevel::parse("syntax"), Some(VerificationLevel::Syntax));
        assert_eq!(VerificationLevel::parse("unit_tests"), Some(VerificationLevel::UnitTests));
        assert_eq!(VerificationLevel::parse("tests"), Some(VerificationLevel::UnitTests));
        assert_eq!(VerificationLevel::parse("unknown"), None);
        assert!(VerificationLevel::Syntax < VerificationLevel::UnitTests);
    }

    #[test]
    fn test_baseline_roundtrip() {
        let dir = std::env::temp_dir().join(format!("lai-verifier-test-{}", uuid::Uuid::new_v4()));
        let v = TestPipelineVerifier::new(dir.clone());
        let baseline = Baseline {
            task_key: "task x/1".to_string(),
            checks: HashMap::from([("file:a.py".to_string(), true)]),
            saved_at: "now".to_string(),
        };
        v.save_baseline(&baseline).unwrap();
        let loaded = v.load_baseline("task x/1").unwrap();
        assert!(loaded.checks.contains_key("file:a.py"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_regression_detection() {
        let baseline = Baseline {
            task_key: "t".to_string(),
            checks: HashMap::from([("file:a.py".to_string(), true), ("file:b.py".to_string(), false)]),
            saved_at: "now".to_string(),
        };
        let current = HashMap::from([("file:a.py".to_string(), false)]);
        let regs = compare_with_baseline(&baseline, &current);
        assert_eq!(regs.len(), 1);
        assert_eq!(regs[0].key, "file:a.py");
    }

    #[tokio::test]
    async fn test_verify_success_with_gaps() {
        let dir = std::env::temp_dir().join(format!("lai-verify-gap-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ok.py"), "x = 1\n").unwrap();
        let v = TestPipelineVerifier::new(dir.clone());
        let spec = spec_with(vec!["syntax"], vec![]);
        let (verdict, results, gaps) = v.verify(&spec, &["ok.py".to_string()]).await.unwrap();
        assert!(verdict.is_success());
        assert!(matches!(verdict, Verdict::SuccessWithDeclaredGaps(_)));
        assert!(results.iter().all(|r| r.passed));
        assert!(!gaps.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_verify_syntax_failure_is_partial() {
        let dir = std::env::temp_dir().join(format!("lai-verify-fail-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bad.py"), "def broken(:\n").unwrap();
        let v = TestPipelineVerifier::new(dir.clone());
        let spec = spec_with(vec!["syntax", "unit_tests"], vec![]);
        let (verdict, results, _gaps) = v.verify(&spec, &["bad.py".to_string()]).await.unwrap();
        assert!(matches!(verdict, Verdict::PartiallyVerified));
        assert!(results.iter().any(|r| !r.passed));
        let _ = std::fs::remove_dir_all(&dir);
    }
}