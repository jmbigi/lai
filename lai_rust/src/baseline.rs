//! baseline.rs - Baseline tracking with regression detection (v5.0)
//! Principles: Measurable quality gates, regression alerts, SQLite persistence

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use uuid::Uuid;

#[derive(clap::Subcommand, Debug, Clone, Serialize, Deserialize)]
pub enum BaselineCommands {
    /// List all baselines
    List,
    /// Show baseline details
    Show {
        #[arg(help = "Baseline ID")]
        id: String,
    },
    /// Compare against baseline
    Compare {
        #[arg(help = "Baseline ID")]
        id: String,
        #[arg(help = "File or directory to compare")]
        path: String,
    },
    /// Create new baseline
    Create {
        #[arg(help = "Name for baseline")]
        name: String,
        #[arg(help = "Path to baseline")]
        path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BaselineMetric {
    TestPassRate,
    TestCount,
    Coverage,
    CompileTime,
    BinarySize,
    LintWarnings,
    TypeErrors,
    SecurityFindings,
    PerformanceScore,
    Custom(String),
}

impl BaselineMetric {
    pub fn as_str(&self) -> String {
        match self {
            BaselineMetric::TestPassRate => "test_pass_rate".to_string(),
            BaselineMetric::TestCount => "test_count".to_string(),
            BaselineMetric::Coverage => "coverage".to_string(),
            BaselineMetric::CompileTime => "compile_time".to_string(),
            BaselineMetric::BinarySize => "binary_size".to_string(),
            BaselineMetric::LintWarnings => "lint_warnings".to_string(),
            BaselineMetric::TypeErrors => "type_errors".to_string(),
            BaselineMetric::SecurityFindings => "security_findings".to_string(),
            BaselineMetric::PerformanceScore => "performance_score".to_string(),
            BaselineMetric::Custom(s) => format!("custom_{}", s),
        }
    }

    pub fn from_string(s: &str) -> Self {
        match s {
            "test_pass_rate" => BaselineMetric::TestPassRate,
            "test_count" => BaselineMetric::TestCount,
            "coverage" => BaselineMetric::Coverage,
            "compile_time" => BaselineMetric::CompileTime,
            "binary_size" => BaselineMetric::BinarySize,
            "lint_warnings" => BaselineMetric::LintWarnings,
            "type_errors" => BaselineMetric::TypeErrors,
            "security_findings" => BaselineMetric::SecurityFindings,
            "performance_score" => BaselineMetric::PerformanceScore,
            s if s.starts_with("custom_") => BaselineMetric::Custom(s[7..].to_string()),
            _ => BaselineMetric::Custom(s.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub id: String,
    pub name: String,
    pub description: String,
    pub project_path: String,
    pub git_commit: String,
    pub git_branch: String,
    pub created_at: String,
    pub metrics: HashMap<BaselineMetric, f64>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Baseline {
    pub fn new(name: &str, description: &str, project_path: &Path) -> Result<Self, String> {
        let git_commit = get_git_commit(project_path)?;
        let git_branch = get_git_branch(project_path)?;

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            project_path: project_path.display().to_string(),
            git_commit,
            git_branch,
            created_at: chrono::Utc::now().to_rfc3339(),
            metrics: HashMap::new(),
            metadata: HashMap::new(),
        })
    }

    pub fn add_metric(&mut self, metric: BaselineMetric, value: f64) {
        self.metrics.insert(metric, value);
    }

    pub fn get_metric(&self, metric: &BaselineMetric) -> Option<f64> {
        self.metrics.get(metric).copied()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionAlert {
    pub id: String,
    pub baseline_id: String,
    pub metric: BaselineMetric,
    pub baseline_value: f64,
    pub current_value: f64,
    pub delta: f64,
    pub delta_percent: f64,
    pub severity: RegressionSeverity,
    pub detected_at: String,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RegressionSeverity {
    Info,     // < 1% change
    Warning,  // 1-5% change
    Critical, // > 5% change
}

impl RegressionSeverity {
    pub fn from_delta_percent(percent: f64) -> Self {
        let abs = percent.abs();
        if abs > 5.0 {
            RegressionSeverity::Critical
        } else if abs > 1.0 {
            RegressionSeverity::Warning
        } else {
            RegressionSeverity::Info
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RegressionSeverity::Info => "info",
            RegressionSeverity::Warning => "warning",
            RegressionSeverity::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub baseline_id: String,
    pub baseline_name: String,
    pub current_commit: String,
    pub current_branch: String,
    pub compared_at: String,
    pub metrics_compared: usize,
    pub regressions: Vec<RegressionAlert>,
    pub improvements: Vec<RegressionAlert>,
    pub unchanged: usize,
    pub summary: String,
}

pub struct BaselineStore {
    conn: Connection,
}

impl BaselineStore {
    pub fn new(working_dir: &Path) -> Result<Self, String> {
        let db_path = working_dir.join(".lai").join("baselines.db");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS baselines (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                project_path TEXT NOT NULL,
                git_commit TEXT NOT NULL,
                git_branch TEXT NOT NULL,
                created_at TEXT NOT NULL,
                metrics TEXT NOT NULL,  -- JSON: metric -> value
                metadata TEXT NOT NULL  -- JSON
            );

            CREATE INDEX IF NOT EXISTS idx_baselines_name ON baselines(name);
            CREATE INDEX IF NOT EXISTS idx_baselines_commit ON baselines(git_commit);
            CREATE INDEX IF NOT EXISTS idx_baselines_created ON baselines(created_at);

            CREATE TABLE IF NOT EXISTS regression_alerts (
                id TEXT PRIMARY KEY,
                baseline_id TEXT NOT NULL,
                metric TEXT NOT NULL,
                baseline_value REAL NOT NULL,
                current_value REAL NOT NULL,
                delta REAL NOT NULL,
                delta_percent REAL NOT NULL,
                severity TEXT NOT NULL,
                detected_at TEXT NOT NULL,
                acknowledged INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (baseline_id) REFERENCES baselines(id)
            );

            CREATE INDEX IF NOT EXISTS idx_alerts_baseline ON regression_alerts(baseline_id);
            CREATE INDEX IF NOT EXISTS idx_alerts_metric ON regression_alerts(metric);
            CREATE INDEX IF NOT EXISTS idx_alerts_severity ON regression_alerts(severity);
            "#,
            )
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn create(&self, baseline: &Baseline) -> Result<(), String> {
        let metrics_json = serde_json::to_string(&baseline.metrics).map_err(|e| e.to_string())?;
        let metadata_json = serde_json::to_string(&baseline.metadata).map_err(|e| e.to_string())?;

        self.conn.execute(
            r#"
            INSERT INTO baselines (id, name, description, project_path, git_commit, git_branch, created_at, metrics, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                baseline.id,
                baseline.name,
                baseline.description,
                baseline.project_path,
                baseline.git_commit,
                baseline.git_branch,
                baseline.created_at,
                metrics_json,
                metadata_json,
            ],
        ).map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<Baseline>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, project_path, git_commit, git_branch, created_at, metrics, metadata
             FROM baselines WHERE id = ?"
        ).map_err(|e| e.to_string())?;

        let baseline = stmt
            .query_row([id], |row| self.row_to_baseline(row))
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(baseline)
    }

    pub fn get_by_name(&self, name: &str) -> Result<Option<Baseline>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, project_path, git_commit, git_branch, created_at, metrics, metadata
             FROM baselines WHERE name = ? ORDER BY created_at DESC LIMIT 1"
        ).map_err(|e| e.to_string())?;

        let baseline = stmt
            .query_row([name], |row| self.row_to_baseline(row))
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(baseline)
    }

    pub fn list(&self, limit: usize) -> Result<Vec<Baseline>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, project_path, git_commit, git_branch, created_at, metrics, metadata
             FROM baselines ORDER BY created_at DESC LIMIT ?"
        ).map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([limit], |row| self.row_to_baseline(row))
            .map_err(|e| e.to_string())?;

        let mut baselines = Vec::new();
        for row in rows {
            baselines.push(row.map_err(|e| e.to_string())?);
        }

        Ok(baselines)
    }

    pub fn compare(
        &self,
        baseline_id: &str,
        current_metrics: &HashMap<BaselineMetric, f64>,
    ) -> Result<ComparisonReport, String> {
        let baseline = self
            .get(baseline_id)?
            .ok_or_else(|| format!("Baseline not found: {}", baseline_id))?;

        let current_commit = get_git_commit(Path::new(&baseline.project_path))?;
        let current_branch = get_git_branch(Path::new(&baseline.project_path))?;

        let mut regressions = Vec::new();
        let mut improvements = Vec::new();
        let mut unchanged = 0;
        let mut metrics_compared = 0;

        for (metric, baseline_value) in &baseline.metrics {
            if let Some(current_value) = current_metrics.get(metric) {
                metrics_compared += 1;
                let delta = current_value - baseline_value;
                let delta_percent = if *baseline_value != 0.0 {
                    (delta / baseline_value) * 100.0
                } else {
                    0.0
                };

                let severity = RegressionSeverity::from_delta_percent(delta_percent);

                let alert = RegressionAlert {
                    id: Uuid::new_v4().to_string(),
                    baseline_id: baseline_id.to_string(),
                    metric: metric.clone(),
                    baseline_value: *baseline_value,
                    current_value: *current_value,
                    delta,
                    delta_percent,
                    severity: severity.clone(),
                    detected_at: chrono::Utc::now().to_rfc3339(),
                    acknowledged: false,
                };

                // Determine if regression or improvement
                // For most metrics: higher is better (pass rate, coverage, performance)
                // For some: lower is better (compile time, binary size, warnings, errors)
                let is_regression = match metric {
                    BaselineMetric::TestPassRate
                    | BaselineMetric::Coverage
                    | BaselineMetric::PerformanceScore => delta < 0.0,
                    BaselineMetric::TestCount => delta < 0.0,
                    BaselineMetric::CompileTime
                    | BaselineMetric::BinarySize
                    | BaselineMetric::LintWarnings
                    | BaselineMetric::TypeErrors
                    | BaselineMetric::SecurityFindings => delta > 0.0,
                    BaselineMetric::Custom(_) => delta < 0.0, // Default: higher is better
                };

                if is_regression && delta_percent.abs() > 0.1 {
                    self.store_alert(&alert)?;
                    regressions.push(alert);
                } else if !is_regression && delta_percent.abs() > 0.1 {
                    improvements.push(alert);
                } else {
                    unchanged += 1;
                }
            }
        }

        let summary = format!(
            "Compared {} metrics: {} regressions, {} improvements, {} unchanged",
            metrics_compared,
            regressions.len(),
            improvements.len(),
            unchanged
        );

        Ok(ComparisonReport {
            baseline_id: baseline.id,
            baseline_name: baseline.name,
            current_commit,
            current_branch,
            compared_at: chrono::Utc::now().to_rfc3339(),
            metrics_compared,
            regressions,
            improvements,
            unchanged,
            summary,
        })
    }

    fn store_alert(&self, alert: &RegressionAlert) -> Result<(), String> {
        self.conn.execute(
            r#"
            INSERT INTO regression_alerts (id, baseline_id, metric, baseline_value, current_value, delta, delta_percent, severity, detected_at, acknowledged)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)
            "#,
            params![
                alert.id,
                alert.baseline_id,
                alert.metric.as_str(),
                alert.baseline_value,
                alert.current_value,
                alert.delta,
                alert.delta_percent,
                alert.severity.as_str(),
                alert.detected_at,
            ],
        ).map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn list_alerts(
        &self,
        baseline_id: Option<&str>,
        severity: Option<RegressionSeverity>,
        limit: usize,
    ) -> Result<Vec<RegressionAlert>, String> {
        let mut sql = String::from(
            "SELECT id, baseline_id, metric, baseline_value, current_value, delta, delta_percent, severity, detected_at, acknowledged
             FROM regression_alerts WHERE 1=1"
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(bid) = baseline_id {
            sql.push_str(" AND baseline_id = ?");
            params_vec.push(Box::new(bid.to_string()));
        }

        if let Some(sev) = severity {
            sql.push_str(" AND severity = ?");
            params_vec.push(Box::new(sev.as_str().to_string()));
        }

        sql.push_str(" ORDER BY detected_at DESC LIMIT ?");
        params_vec.push(Box::new(limit as i64));

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|b| b.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(&params_refs[..], |row| self.row_to_alert(row))
            .map_err(|e| e.to_string())?;

        let mut alerts = Vec::new();
        for row in rows {
            alerts.push(row.map_err(|e| e.to_string())?);
        }

        Ok(alerts)
    }

    pub fn acknowledge_alert(&self, alert_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE regression_alerts SET acknowledged = 1 WHERE id = ?",
                [alert_id],
            )
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn row_to_baseline(&self, row: &Row) -> rusqlite::Result<Baseline> {
        let metrics_json: String = row.get(7)?;
        let metadata_json: String = row.get(8)?;

        let metrics_map: HashMap<String, f64> =
            serde_json::from_str(&metrics_json).unwrap_or_default();
        let mut metrics = HashMap::new();
        for (k, v) in metrics_map {
            metrics.insert(BaselineMetric::from_string(&k), v);
        }

        let metadata: HashMap<String, serde_json::Value> =
            serde_json::from_str(&metadata_json).unwrap_or_default();

        Ok(Baseline {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            project_path: row.get(3)?,
            git_commit: row.get(4)?,
            git_branch: row.get(5)?,
            created_at: row.get(6)?,
            metrics,
            metadata,
        })
    }

    fn row_to_alert(&self, row: &Row) -> rusqlite::Result<RegressionAlert> {
        let metric_str: String = row.get(2)?;
        let severity_str: String = row.get(7)?;

        Ok(RegressionAlert {
            id: row.get(0)?,
            baseline_id: row.get(1)?,
            metric: BaselineMetric::from_string(&metric_str),
            baseline_value: row.get(3)?,
            current_value: row.get(4)?,
            delta: row.get(5)?,
            delta_percent: row.get(6)?,
            severity: match severity_str.as_str() {
                "info" => RegressionSeverity::Info,
                "warning" => RegressionSeverity::Warning,
                "critical" => RegressionSeverity::Critical,
                _ => RegressionSeverity::Info,
            },
            detected_at: row.get(8)?,
            acknowledged: row.get(9)?,
        })
    }
}

pub async fn collect_current_metrics(
    project_path: &Path,
) -> Result<HashMap<BaselineMetric, f64>, String> {
    let mut metrics = HashMap::new();

    // Run tests and get pass rate
    if let Ok((passed, total)) = run_tests(project_path).await {
        if total > 0 {
            metrics.insert(
                BaselineMetric::TestPassRate,
                passed as f64 / total as f64 * 100.0,
            );
            metrics.insert(BaselineMetric::TestCount, total as f64);
        }
    }

    // Get coverage (if available)
    if let Ok(coverage) = get_coverage(project_path).await {
        metrics.insert(BaselineMetric::Coverage, coverage);
    }

    // Get lint warnings
    if let Ok(warnings) = get_lint_warnings(project_path).await {
        metrics.insert(BaselineMetric::LintWarnings, warnings as f64);
    }

    // Get type errors
    if let Ok(errors) = get_type_errors(project_path).await {
        metrics.insert(BaselineMetric::TypeErrors, errors as f64);
    }

    // Get security findings
    if let Ok(findings) = get_security_findings(project_path).await {
        metrics.insert(BaselineMetric::SecurityFindings, findings as f64);
    }

    // Get compile time
    if let Ok(compile_time) = measure_compile_time(project_path).await {
        metrics.insert(BaselineMetric::CompileTime, compile_time);
    }

    // Get binary size
    if let Ok(size) = get_binary_size(project_path).await {
        metrics.insert(BaselineMetric::BinarySize, size as f64);
    }

    Ok(metrics)
}

async fn run_tests(project_path: &Path) -> Result<(u32, u32), String> {
    // Try cargo test for Rust
    let cargo_toml = project_path.join("Cargo.toml");
    if cargo_toml.exists() {
        let output = tokio::process::Command::new("cargo")
            .arg("test")
            .arg("--no-fail-fast")
            .arg("--message-format=json")
            .current_dir(project_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut passed = 0;
        let mut total = 0;

        for line in stdout.lines() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if json["reason"] == "test" {
                    total += 1;
                    if json["status"] == "passed" {
                        passed += 1;
                    }
                }
            }
        }

        if total > 0 {
            return Ok((passed, total));
        }
    }

    // Try pytest for Python
    let pyproject = project_path.join("pyproject.toml");
    if pyproject.exists() {
        let _output = tokio::process::Command::new("python3")
            .arg("-m")
            .arg("pytest")
            .arg("--tb=no")
            .arg("-q")
            .arg("--json-report")
            .arg("--json-report-file=/tmp/pytest_report.json")
            .current_dir(project_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        if let Ok(report) = std::fs::read_to_string("/tmp/pytest_report.json") {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&report) {
                let passed = json["summary"]["passed"].as_u64().unwrap_or(0) as u32;
                let total = json["summary"]["total"].as_u64().unwrap_or(0) as u32;
                if total > 0 {
                    return Ok((passed, total));
                }
            }
        }
    }

    Ok((0, 0))
}

async fn get_coverage(project_path: &Path) -> Result<f64, String> {
    // Try cargo llvm-cov for Rust
    let cargo_toml = project_path.join("Cargo.toml");
    if cargo_toml.exists() {
        let output = tokio::process::Command::new("cargo")
            .arg("llvm-cov")
            .arg("--summary-only")
            .current_dir(project_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("Lines:") {
                if let Some(percent_str) = line.split('%').next() {
                    if let Some(num_str) = percent_str.split_whitespace().last() {
                        return Ok(num_str.parse().unwrap_or(0.0));
                    }
                }
            }
        }
    }

    // Try coverage.py for Python
    let pyproject = project_path.join("pyproject.toml");
    if pyproject.exists() {
        let output = tokio::process::Command::new("python3")
            .arg("-m")
            .arg("coverage")
            .arg("report")
            .arg("--format=total")
            .current_dir(project_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(val) = stdout.trim().parse::<f64>() {
            return Ok(val);
        }
    }

    Err("Coverage not available".to_string())
}

async fn get_lint_warnings(project_path: &Path) -> Result<u32, String> {
    // Try cargo clippy for Rust
    let cargo_toml = project_path.join("Cargo.toml");
    if cargo_toml.exists() {
        let output = tokio::process::Command::new("cargo")
            .arg("clippy")
            .arg("--message-format=json")
            .current_dir(project_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut warnings = 0;
        for line in stdout.lines() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if json["reason"] == "compiler-message" && json["message"]["level"] == "warning" {
                    warnings += 1;
                }
            }
        }
        return Ok(warnings);
    }

    // Try ruff for Python
    let pyproject = project_path.join("pyproject.toml");
    if pyproject.exists() {
        let output = tokio::process::Command::new("ruff")
            .arg("check")
            .arg("--output-format=json")
            .current_dir(project_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(arr) = json.as_array() {
                return Ok(arr.len() as u32);
            }
        }
    }

    Ok(0)
}

async fn get_type_errors(project_path: &Path) -> Result<u32, String> {
    // Try mypy for Python
    let pyproject = project_path.join("pyproject.toml");
    if pyproject.exists() {
        let output = tokio::process::Command::new("mypy")
            .arg("--strict")
            .arg(".")
            .current_dir(project_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut errors = 0;
        for line in stderr.lines() {
            if line.contains("error:") {
                errors += 1;
            }
        }
        return Ok(errors);
    }

    // Rust type errors are caught by cargo build
    Ok(0)
}

async fn get_security_findings(project_path: &Path) -> Result<u32, String> {
    // Try cargo audit for Rust
    let cargo_toml = project_path.join("Cargo.toml");
    if cargo_toml.exists() {
        let output = tokio::process::Command::new("cargo")
            .arg("audit")
            .arg("--json")
            .current_dir(project_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(vulns) = json["vulnerabilities"].as_array() {
                return Ok(vulns.len() as u32);
            }
        }
    }

    // Try bandit for Python
    let pyproject = project_path.join("pyproject.toml");
    if pyproject.exists() {
        let output = tokio::process::Command::new("bandit")
            .arg("-r")
            .arg(".")
            .arg("-f")
            .arg("json")
            .current_dir(project_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(results) = json["results"].as_array() {
                return Ok(results.len() as u32);
            }
        }
    }

    Ok(0)
}

async fn measure_compile_time(project_path: &Path) -> Result<f64, String> {
    let cargo_toml = project_path.join("Cargo.toml");
    if cargo_toml.exists() {
        let start = Instant::now();
        let output = tokio::process::Command::new("cargo")
            .arg("build")
            .current_dir(project_path)
            .output()
            .await
            .map_err(|e| e.to_string())?;

        let elapsed = start.elapsed().as_secs_f64();
        if output.status.success() {
            return Ok(elapsed);
        }
    }
    Err("Compile time measurement failed".to_string())
}

async fn get_binary_size(project_path: &Path) -> Result<u64, String> {
    let target_dir = project_path.join("target").join("debug");
    if let Ok(entries) = std::fs::read_dir(&target_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() && entry.file_name().to_string_lossy().contains("lai") {
                    return Ok(metadata.len());
                }
            }
        }
    }
    Ok(0)
}

fn get_git_commit(project_path: &Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .current_dir(project_path)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Ok("unknown".to_string())
    }
}

fn get_git_branch(project_path: &Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .current_dir(project_path)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Ok("unknown".to_string())
    }
}
