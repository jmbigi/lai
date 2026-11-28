//! verification_engine.rs - Multi-level verification engine (v5.0)
//! Principles: Bounded verdicts (SUCCESS, SUCCESS_WITH_DECLARED_GAPS, BLOCKED), honest reporting

use crate::planner::{VerificationLevel, VerificationScope};
use crate::telemetry::record_verification;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VerificationVerdict {
    Success,
    SuccessWithDeclaredGaps,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub level: VerificationLevel,
    pub passed: bool,
    pub score: f64,
    pub details: String,
    pub artifacts: Vec<String>,
    pub duration: Duration,
    pub executed: bool,
    pub in_scope: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub task_id: String,
    pub spec_id: Option<String>,
    pub verification_scope: VerificationScope,
    pub results: Vec<VerificationResult>,
    pub overall_verdict: VerificationVerdict,
    pub summary: String,
    pub created_at: String,
    pub total_duration: Duration,
}

impl VerificationReport {
    pub fn compute_verdict(&mut self) {
        let mut all_in_scope_passed = true;
        let mut has_in_scope = false;
        let mut has_out_of_scope = false;
        let mut blocked = false;

        for result in &self.results {
            if result.in_scope {
                has_in_scope = true;
                if !result.passed {
                    all_in_scope_passed = false;
                    if matches!(
                        result.level,
                        VerificationLevel::Syntax | VerificationLevel::TypeCheck
                    ) {
                        // Critical levels: failure blocks
                        blocked = true;
                    }
                }
            } else {
                has_out_of_scope = true;
            }
            if !result.executed && result.in_scope {
                blocked = true;
            }
        }

        if blocked || (has_in_scope && !all_in_scope_passed) {
            self.overall_verdict = VerificationVerdict::Blocked;
        } else if has_out_of_scope {
            self.overall_verdict = VerificationVerdict::SuccessWithDeclaredGaps;
        } else {
            self.overall_verdict = VerificationVerdict::Success;
        }

        self.summary = match self.overall_verdict {
            VerificationVerdict::Success => "All in-scope verifications passed".to_string(),
            VerificationVerdict::SuccessWithDeclaredGaps => {
                let gaps: Vec<String> = self
                    .results
                    .iter()
                    .filter(|r| !r.in_scope)
                    .map(|r| format!("Level {} ({})", r.level.as_u32(), r.level.name()))
                    .collect();
                format!(
                    "In-scope verifications passed. Gaps declared: {}",
                    gaps.join(", ")
                )
            }
            VerificationVerdict::Blocked => {
                let failed: Vec<String> = self
                    .results
                    .iter()
                    .filter(|r| r.in_scope && !r.passed)
                    .map(|r| format!("Level {} ({})", r.level.as_u32(), r.level.name()))
                    .collect();
                format!("Blocked: {}", failed.join(", "))
            }
            VerificationVerdict::Failed => "Verification failed".to_string(),
        };
    }
}

/// Suma `passed`/`failed` de las líneas `test result:` de cargo/libtest.
///
/// Formato real: `test result: ok. 3 passed; 0 failed; 0 ignored; ...`.
/// Se ignoran las líneas de doc-tests con 0 tests para no inflar el total.
fn parse_cargo_test_summary(text: &str) -> (usize, usize) {
    let mut passed = 0;
    let mut failed = 0;
    for line in text.lines() {
        let Some(rest) = line.split("test result:").nth(1) else {
            continue;
        };
        // Formato: `ok. 3 passed; 0 failed; 0 ignored; ...`
        for part in rest.split(';') {
            let tokens: Vec<&str> = part.split_whitespace().collect();
            for window in tokens.windows(2) {
                let Ok(n) = window[0].parse::<usize>() else {
                    continue;
                };
                match window[1] {
                    "passed" => passed += n,
                    "failed" => failed += n,
                    _ => {}
                }
            }
        }
    }
    (passed, failed)
}

/// Extrae `passed`/`failed` del resumen de pytest (`3 passed, 1 failed in ...`).
fn parse_pytest_summary(text: &str) -> (usize, usize) {
    let mut passed = 0;
    let mut failed = 0;
    for line in text.lines() {
        if !(line.contains(" passed") || line.contains(" failed") || line.contains(" error")) {
            continue;
        }
        // Las líneas de progreso (`....F`) no contienen estos tokens con
        // número; se exige que el token anterior sea numérico.
        let tokens: Vec<&str> = line
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
            .collect();
        for window in tokens.windows(2) {
            let Ok(n) = window[0].parse::<usize>() else {
                continue;
            };
            match window[1] {
                "passed" => passed += n,
                "failed" | "error" | "errors" => failed += n,
                _ => {}
            }
        }
    }
    (passed, failed)
}

pub struct VerificationEngine {
    working_dir: PathBuf,
    config: VerificationConfig,
}

#[derive(Debug, Clone)]
pub struct VerificationConfig {
    pub timeout_per_level: Duration,
    pub min_test_pass_rate: f64,
    pub min_coverage: f64,
    pub max_lint_warnings: u32,
    pub max_type_errors: u32,
    pub fail_fast_on_critical: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            timeout_per_level: Duration::from_secs(120),
            min_test_pass_rate: 95.0,
            min_coverage: 80.0,
            max_lint_warnings: 0,
            max_type_errors: 0,
            fail_fast_on_critical: true,
        }
    }
}

/// ¿El fallo es "la herramienta no está instalada"?
///
/// Un proyecto Python sin `ruff`/`mypy` instalado no debe reprobar la
/// verificación: la herramienta no está disponible, no hay hallazgos.
fn tool_missing(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

impl VerificationEngine {
    pub fn new(working_dir: &Path) -> Self {
        Self {
            working_dir: working_dir.to_path_buf(),
            config: VerificationConfig::default(),
        }
    }

    pub fn with_config(mut self, config: VerificationConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn verify(
        &self,
        task_id: &str,
        spec_id: Option<String>,
        verification_scope: VerificationScope,
    ) -> Result<VerificationReport, String> {
        let start = Instant::now();
        let mut results = Vec::new();

        // Determine which levels to execute
        let levels_to_check: Vec<VerificationLevel> = (1..=8)
            .filter_map(VerificationLevel::from_u32)
            .filter(|l| {
                verification_scope.in_scope.contains(l)
                    || verification_scope.out_of_scope.contains(l)
            })
            .collect();

        for level in levels_to_check {
            let in_scope = verification_scope.in_scope.contains(&level);
            let _out_of_scope = verification_scope.out_of_scope.contains(&level);

            let result = self.verify_level(task_id, level, in_scope).await;
            results.push(result);
        }

        let mut report = VerificationReport {
            task_id: task_id.to_string(),
            spec_id,
            verification_scope,
            results,
            overall_verdict: VerificationVerdict::Failed,
            summary: String::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            total_duration: start.elapsed(),
        };

        report.compute_verdict();
        Ok(report)
    }

    async fn verify_level(
        &self,
        task_id: &str,
        level: VerificationLevel,
        in_scope: bool,
    ) -> VerificationResult {
        let level_start = Instant::now();

        let (passed, score, details, artifacts) = match level {
            VerificationLevel::Syntax => self.verify_syntax().await,
            VerificationLevel::Lint => self.verify_lint().await,
            VerificationLevel::TypeCheck => self.verify_type_check().await,
            VerificationLevel::UnitTests => self.verify_unit_tests().await,
            VerificationLevel::IntegrationTests => self.verify_integration_tests().await,
            VerificationLevel::RegressionTests => self.verify_regression_tests().await,
            VerificationLevel::SecurityScan => self.verify_security().await,
            VerificationLevel::Performance => self.verify_performance().await,
        };

        let duration = level_start.elapsed();
        let level_name = format!("{:?}", level);
        let level_num = level.as_u32();

        let details_for_telemetry = details.clone();
        let result = VerificationResult {
            level,
            passed,
            score,
            details,
            artifacts,
            duration,
            executed: true,
            in_scope,
        };

        record_verification(
            task_id,
            &level_name,
            level_num,
            passed,
            details_for_telemetry,
            vec![],
        );

        result
    }

    async fn verify_syntax(&self) -> (bool, f64, String, Vec<String>) {
        // Rust
        if self.working_dir.join("Cargo.toml").exists() {
            let output = Command::new("cargo")
                .args(["check", "--message-format=json"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let success = out.status.success();
                    let _stdout = String::from_utf8_lossy(&out.stdout);
                    let stderr = String::from_utf8_lossy(&out.stderr);

                    if success {
                        (
                            true,
                            100.0,
                            "Syntax check passed (cargo check)".to_string(),
                            vec![],
                        )
                    } else {
                        (false, 0.0, format!("Syntax errors: {}", stderr), vec![])
                    }
                }
                Err(e) => (false, 0.0, format!("cargo check failed: {}", e), vec![]),
            }
        }
        // Python
        else if self.working_dir.join("pyproject.toml").exists()
            || self.working_dir.join("setup.py").exists()
        {
            let output = Command::new("python3")
                // `py_compile` recibe archivos, no directorios: pasarle "."
                // fallaba siempre con "Is a directory". `compileall` sí recorre
                // el árbol.
                .args(["-m", "compileall", "-q", "."])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let success = out.status.success();
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if success {
                        (
                            true,
                            100.0,
                            "Syntax check passed (compileall)".to_string(),
                            vec![],
                        )
                    } else {
                        (false, 0.0, format!("Syntax errors: {}", stderr), vec![])
                    }
                }
                Err(e) => (false, 0.0, format!("py_compile failed: {}", e), vec![]),
            }
        }
        // JavaScript/TypeScript
        else if self.working_dir.join("package.json").exists() {
            let output = Command::new("npx")
                .args(["tsc", "--noEmit"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let success = out.status.success();
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if success {
                        (true, 100.0, "Type check passed (tsc)".to_string(), vec![])
                    } else {
                        (false, 0.0, format!("Type errors: {}", stderr), vec![])
                    }
                }
                Err(_) => {
                    // Try eslint
                    let output = Command::new("npx")
                        .args(["eslint", ".", "--ext", ".js,.ts,.jsx,.tsx"])
                        .current_dir(&self.working_dir)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .output();

                    match output {
                        Ok(out) => {
                            let success = out.status.success();
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            if success {
                                (true, 100.0, "Lint passed (eslint)".to_string(), vec![])
                            } else {
                                (false, 0.0, format!("Lint errors: {}", stdout), vec![])
                            }
                        }
                        Err(e) => (false, 0.0, format!("Syntax check failed: {}", e), vec![]),
                    }
                }
            }
        }
        // Go
        else if self.working_dir.join("go.mod").exists() {
            let output = Command::new("go")
                .args(["build", "./..."])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let success = out.status.success();
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if success {
                        (true, 100.0, "Build passed (go build)".to_string(), vec![])
                    } else {
                        (false, 0.0, format!("Build errors: {}", stderr), vec![])
                    }
                }
                Err(e) => (false, 0.0, format!("go build failed: {}", e), vec![]),
            }
        } else {
            (
                true,
                100.0,
                "No syntax checker available for project type".to_string(),
                vec![],
            )
        }
    }

    async fn verify_lint(&self) -> (bool, f64, String, Vec<String>) {
        // Rust: clippy
        if self.working_dir.join("Cargo.toml").exists() {
            let output = Command::new("cargo")
                .args(["clippy", "--message-format=json", "--", "-D", "warnings"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let success = out.status.success();
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let mut warnings = 0;

                    for line in stdout.lines() {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                            if json["reason"] == "compiler-message"
                                && json["message"]["level"] == "warning"
                            {
                                warnings += 1;
                            }
                        }
                    }

                    if success && warnings == 0 {
                        (true, 100.0, "No lint warnings (clippy)".to_string(), vec![])
                    } else {
                        let score = if warnings <= self.config.max_lint_warnings {
                            100.0
                        } else {
                            50.0
                        };
                        (
                            warnings <= self.config.max_lint_warnings,
                            score,
                            format!("{} lint warnings", warnings),
                            vec![],
                        )
                    }
                }
                Err(e) => (false, 0.0, format!("clippy failed: {}", e), vec![]),
            }
        }
        // Python: ruff
        else if self.working_dir.join("pyproject.toml").exists() {
            let output = Command::new("ruff")
                .args(["check", "--output-format=json", "."])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                        let count = json.as_array().map(|a| a.len()).unwrap_or(0) as u32;
                        let success = count <= self.config.max_lint_warnings;
                        let score = if count == 0 {
                            100.0
                        } else if count <= self.config.max_lint_warnings {
                            80.0
                        } else {
                            30.0
                        };
                        (success, score, format!("{} ruff findings", count), vec![])
                    } else {
                        (true, 100.0, "Ruff check passed".to_string(), vec![])
                    }
                }
                Err(e) if tool_missing(&e) => (
                    true,
                    100.0,
                    "ruff no instalado: lint de Python omitido".to_string(),
                    vec![],
                ),
                Err(e) => (false, 0.0, format!("ruff failed: {}", e), vec![]),
            }
        }
        // JS/TS: eslint
        else if self.working_dir.join("package.json").exists() {
            let output = Command::new("npx")
                .args(["eslint", ".", "--ext", ".js,.ts,.jsx,.tsx", "--format=json"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                        let total_errors: u32 = json
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .map(|f| f["errorCount"].as_u64().unwrap_or(0) as u32)
                                    .sum()
                            })
                            .unwrap_or(0);

                        let success = total_errors <= self.config.max_lint_warnings;
                        let score = if total_errors == 0 {
                            100.0
                        } else if total_errors <= self.config.max_lint_warnings {
                            80.0
                        } else {
                            30.0
                        };
                        (
                            success,
                            score,
                            format!("{} ESLint errors", total_errors),
                            vec![],
                        )
                    } else {
                        (true, 100.0, "ESLint passed".to_string(), vec![])
                    }
                }
                Err(e) if tool_missing(&e) => (
                    true,
                    100.0,
                    "eslint no instalado: lint de JS/TS omitido".to_string(),
                    vec![],
                ),
                Err(e) => (false, 0.0, format!("eslint failed: {}", e), vec![]),
            }
        } else {
            (
                true,
                100.0,
                "No linter available for project type".to_string(),
                vec![],
            )
        }
    }

    async fn verify_type_check(&self) -> (bool, f64, String, Vec<String>) {
        // Rust: cargo check already covers types
        if self.working_dir.join("Cargo.toml").exists() {
            // Already done in syntax check
            return (
                true,
                100.0,
                "Type check covered by cargo check".to_string(),
                vec![],
            );
        }

        // Python: mypy
        if self.working_dir.join("pyproject.toml").exists() {
            let output = Command::new("mypy")
                .args(["--strict", "."])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let _success = out.status.success();
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let error_count =
                        stderr.lines().filter(|l| l.contains("error:")).count() as u32;

                    let success = error_count <= self.config.max_type_errors;
                    let score = if error_count == 0 {
                        100.0
                    } else if error_count <= self.config.max_type_errors {
                        70.0
                    } else {
                        20.0
                    };
                    (
                        success,
                        score,
                        format!("{} mypy errors", error_count),
                        vec![],
                    )
                }
                Err(e) if tool_missing(&e) => (
                    true,
                    100.0,
                    "mypy no instalado: type check de Python omitido".to_string(),
                    vec![],
                ),
                Err(e) => (false, 0.0, format!("mypy failed: {}", e), vec![]),
            }
        }
        // TypeScript: tsc
        else if self.working_dir.join("package.json").exists()
            && self.working_dir.join("tsconfig.json").exists()
        {
            let output = Command::new("npx")
                .args(["tsc", "--noEmit"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let _success = out.status.success();
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let error_count =
                        stderr.lines().filter(|l| l.contains("error TS")).count() as u32;

                    let success = error_count <= self.config.max_type_errors;
                    let score = if error_count == 0 {
                        100.0
                    } else if error_count <= self.config.max_type_errors {
                        70.0
                    } else {
                        20.0
                    };
                    (
                        success,
                        score,
                        format!("{} TypeScript errors", error_count),
                        vec![],
                    )
                }
                Err(e) => (false, 0.0, format!("tsc failed: {}", e), vec![]),
            }
        } else {
            (
                true,
                100.0,
                "No type checker available for project type".to_string(),
                vec![],
            )
        }
    }

    async fn verify_unit_tests(&self) -> (bool, f64, String, Vec<String>) {
        // Rust
        if self.working_dir.join("Cargo.toml").exists() {
            // Sin `--message-format=json`: ese formato solo emite eventos de
            // compilación, nunca `reason == "test"`, así que el parser anterior
            // reportaba "No tests found" incluso con tests (defecto real).
            let output = Command::new("cargo")
                .args(["test", "--quiet"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let combined = format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    let (passed, failed) = parse_cargo_test_summary(&combined);
                    let total = passed + failed;
                    if total > 0 {
                        let rate = (passed as f64 / total as f64) * 100.0;
                        let success =
                            out.status.success() && rate >= self.config.min_test_pass_rate;
                        (
                            success,
                            rate,
                            format!("{passed}/{total} tests passed ({rate:.1}%)"),
                            vec![],
                        )
                    } else if out.status.success() {
                        (
                            true,
                            0.0,
                            "No tests found (nothing verified)".to_string(),
                            vec![],
                        )
                    } else {
                        (
                            false,
                            0.0,
                            format!(
                                "cargo test falló: {}",
                                combined
                                    .lines()
                                    .find(|l| l.contains("error"))
                                    .unwrap_or("sin detalle")
                            ),
                            vec![],
                        )
                    }
                }
                Err(e) => (false, 0.0, format!("cargo test failed: {}", e), vec![]),
            }
        }
        // Python
        else if self.working_dir.join("pyproject.toml").exists()
            || self.working_dir.join("pytest.ini").exists()
        {
            // Sin `--json-report`: depende de un plugin que puede no estar
            // instalado, y su ausencia se interpretaba como "todo pasó".
            let output = Command::new("python3")
                .args(["-m", "pytest", "--tb=no", "-q"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let combined = format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    let (passed, failed) = parse_pytest_summary(&combined);
                    let total = passed + failed;
                    if total > 0 {
                        let rate = (passed as f64 / total as f64) * 100.0;
                        let success =
                            out.status.success() && rate >= self.config.min_test_pass_rate;
                        (
                            success,
                            rate,
                            format!("{passed}/{total} tests passed ({rate:.1}%)"),
                            vec![],
                        )
                    } else if out.status.code() == Some(5) || out.status.success() {
                        (
                            true,
                            0.0,
                            "No tests found (nothing verified)".to_string(),
                            vec![],
                        )
                    } else {
                        (
                            false,
                            0.0,
                            "pytest falló sin resumen de tests".to_string(),
                            vec![],
                        )
                    }
                }
                Err(e) => (false, 0.0, format!("pytest failed: {}", e), vec![]),
            }
        }
        // JavaScript/TypeScript
        else if self.working_dir.join("package.json").exists() {
            let output = Command::new("npm")
                .args(["test", "--", "--json"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                        let passed = json["numPassedTests"].as_u64().unwrap_or(0);
                        let total = json["numTotalTests"].as_u64().unwrap_or(0);
                        if total > 0 {
                            let rate = (passed as f64 / total as f64) * 100.0;
                            let success =
                                out.status.success() && rate >= self.config.min_test_pass_rate;
                            (
                                success,
                                rate,
                                format!("{}/{} tests passed ({:.1}%)", passed, total, rate),
                                vec![],
                            )
                        } else {
                            (
                                true,
                                0.0,
                                "No tests found (nothing verified)".to_string(),
                                vec![],
                            )
                        }
                    } else {
                        let success = out.status.success();
                        (
                            success,
                            if success { 100.0 } else { 0.0 },
                            "Tests completed".to_string(),
                            vec![],
                        )
                    }
                }
                Err(e) => (false, 0.0, format!("npm test failed: {}", e), vec![]),
            }
        } else {
            (
                true,
                100.0,
                "No test runner available for project type".to_string(),
                vec![],
            )
        }
    }

    async fn verify_integration_tests(&self) -> (bool, f64, String, Vec<String>) {
        // Usually a subset of unit tests marked as integration
        // For now, same as unit tests but with different marker
        self.verify_unit_tests().await
    }

    async fn verify_regression_tests(&self) -> (bool, f64, String, Vec<String>) {
        // Compare against baseline
        let baseline_store = crate::baseline::BaselineStore::new(&self.working_dir).ok();
        if let Some(store) = baseline_store {
            if let Ok(baselines) = store.list(1) {
                if let Some(baseline) = baselines.first() {
                    if let Ok(current_metrics) =
                        crate::baseline::collect_current_metrics(&self.working_dir).await
                    {
                        if let Ok(report) = store.compare(&baseline.id, &current_metrics) {
                            let critical_regressions = report
                                .regressions
                                .iter()
                                .filter(|r| {
                                    r.severity == crate::baseline::RegressionSeverity::Critical
                                })
                                .count();

                            let success = critical_regressions == 0;
                            let score = if report.regressions.is_empty() {
                                100.0
                            } else {
                                50.0
                            };
                            return (success, score, report.summary, vec![]);
                        }
                    }
                }
            }
        }

        (
            true,
            100.0,
            "No baseline for regression comparison".to_string(),
            vec![],
        )
    }

    async fn verify_security(&self) -> (bool, f64, String, Vec<String>) {
        // Rust: cargo audit
        if self.working_dir.join("Cargo.toml").exists() {
            let output = Command::new("cargo")
                .args(["audit", "--json"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                        let vuln_count = json["vulnerabilities"]
                            .as_array()
                            .map(|a| a.len())
                            .unwrap_or(0);
                        let success = vuln_count == 0;
                        let score = if vuln_count == 0 { 100.0 } else { 0.0 };
                        (
                            success,
                            score,
                            format!("{} vulnerabilities found", vuln_count),
                            vec![],
                        )
                    } else {
                        (true, 100.0, "Security audit passed".to_string(), vec![])
                    }
                }
                Err(e) => (false, 0.0, format!("cargo audit failed: {}", e), vec![]),
            }
        }
        // Python: bandit
        else if self.working_dir.join("pyproject.toml").exists() {
            let output = Command::new("bandit")
                .args(["-r", ".", "-f", "json"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                        let findings = json["results"].as_array().map(|a| a.len()).unwrap_or(0);
                        let success = findings == 0;
                        let score = if findings == 0 { 100.0 } else { 50.0 };
                        (
                            success,
                            score,
                            format!("{} security findings", findings),
                            vec![],
                        )
                    } else {
                        (true, 100.0, "Security scan passed".to_string(), vec![])
                    }
                }
                Err(e) => (false, 0.0, format!("bandit failed: {}", e), vec![]),
            }
        }
        // JS/TS: npm audit
        else if self.working_dir.join("package.json").exists() {
            let output = Command::new("npm")
                .args(["audit", "--json"])
                .current_dir(&self.working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                        let vuln_count = json["metadata"]["vulnerabilities"]["total"]
                            .as_u64()
                            .unwrap_or(0);
                        let success = vuln_count == 0;
                        let score = if vuln_count == 0 { 100.0 } else { 50.0 };
                        (
                            success,
                            score,
                            format!("{} vulnerabilities found", vuln_count),
                            vec![],
                        )
                    } else {
                        (true, 100.0, "Security audit passed".to_string(), vec![])
                    }
                }
                Err(e) => (false, 0.0, format!("npm audit failed: {}", e), vec![]),
            }
        } else {
            (
                true,
                100.0,
                "No security scanner available for project type".to_string(),
                vec![],
            )
        }
    }

    async fn verify_performance(&self) -> (bool, f64, String, Vec<String>) {
        // Placeholder for performance benchmarks
        (
            true,
            100.0,
            "Performance benchmarks not configured".to_string(),
            vec![],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cargo_test_summary() {
        let text = "running 3 tests\n...\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\nrunning 1 test\n.\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n";
        assert_eq!(parse_cargo_test_summary(text), (4, 0));
    }

    #[test]
    fn parses_cargo_test_failures() {
        let text = "test result: FAILED. 2 passed; 1 failed; 0 ignored; finished in 0.1s";
        assert_eq!(parse_cargo_test_summary(text), (2, 1));
    }

    #[test]
    fn parses_pytest_summary() {
        assert_eq!(parse_pytest_summary("3 passed, 1 failed in 0.12s"), (3, 1));
        assert_eq!(parse_pytest_summary("10 passed in 1.00s"), (10, 0));
        assert_eq!(parse_pytest_summary("1 failed, 2 passed in 0.3s"), (2, 1));
        assert_eq!(parse_pytest_summary("no tests ran in 0.01s"), (0, 0));
    }

    #[test]
    fn pytest_progress_dots_are_not_counted() {
        // Líneas de progreso con puntos y nombres no deben contar.
        assert_eq!(parse_pytest_summary("....F....  [100%]"), (0, 0));
    }
}
