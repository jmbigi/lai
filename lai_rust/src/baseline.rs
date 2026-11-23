//! Baseline - Tracking de línea base para detección de regresiones.
//! Implementa captura, comparación y alerta de regresiones en métricas de calidad.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use chrono::{DateTime, Utc};

/// Métricas de calidad para baseline
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualityMetrics {
    pub syntax_score: f64,
    pub format_score: f64,
    pub lint_score: f64,
    pub type_score: f64,
    pub test_score: f64,
    pub security_score: f64,
    pub performance_score: f64,
    pub overall_score: f64,
    pub total_files: usize,
    pub total_symbols: usize,
    pub total_lines: usize,
    pub test_count: usize,
    pub test_passed: usize,
    pub test_failed: usize,
}

/// Snapshot de baseline con timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineSnapshot {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub commit_hash: Option<String>,
    pub branch: Option<String>,
    pub metrics: QualityMetrics,
    pub verification_report: Option<String>, // JSON serialized
    pub metadata: HashMap<String, String>,
}

/// Configuración del baseline tracker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineConfig {
    pub storage_dir: PathBuf,
    pub max_snapshots: usize,
    pub regression_threshold: f64, // % de caída para alertar
    pub auto_capture_on_verify: bool,
    pub retention_days: u32,
}

impl Default for BaselineConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            storage_dir: home.join(".lai").join("baselines"),
            max_snapshots: 100,
            regression_threshold: 5.0, // 5% caída
            auto_capture_on_verify: true,
            retention_days: 90,
        }
    }
}

/// Detección de regresión
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionAlert {
    pub baseline_id: String,
    pub current_snapshot_id: String,
    pub metric_name: String,
    pub baseline_value: f64,
    pub current_value: f64,
    pub delta: f64,
    pub delta_percent: f64,
    pub severity: RegressionSeverity,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegressionSeverity {
    Minor,    // 5-10% caída
    Moderate, // 10-25% caída
    Major,    // 25-50% caída
    Critical, // >50% caída
}

impl RegressionSeverity {
    pub fn from_delta_percent(delta: f64) -> Self {
        match delta.abs() {
            d if d >= 50.0 => RegressionSeverity::Critical,
            d if d >= 25.0 => RegressionSeverity::Major,
            d if d >= 10.0 => RegressionSeverity::Moderate,
            _ => RegressionSeverity::Minor,
        }
    }
}

/// Tracker de baseline para detección de regresiones
pub struct BaselineTracker {
    config: BaselineConfig,
    snapshots: Arc<RwLock<HashMap<String, BaselineSnapshot>>>,
    alerts: Arc<RwLock<Vec<RegressionAlert>>>,
}

impl BaselineTracker {
    pub fn new(config: BaselineConfig) -> Result<Self> {
        fs::create_dir_all(&config.storage_dir)?;
        
        let tracker = Self {
            config,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            alerts: Arc::new(RwLock::new(Vec::new())),
        };
        
        tracker.load_snapshots()?;
        tracker.load_alerts()?;
        
        Ok(tracker)
    }

    /// Capturar snapshot actual como baseline
    pub fn capture_snapshot(
        &self,
        metrics: QualityMetrics,
        verification_report: Option<String>,
        commit_hash: Option<String>,
        branch: Option<String>,
    ) -> Result<String> {
        let snapshot_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        
        let mut metadata = HashMap::new();
        if let Some(commit) = commit_hash {
            metadata.insert("commit".to_string(), commit);
        }
        if let Some(branch) = branch {
            metadata.insert("branch".to_string(), branch);
        }

        let snapshot = BaselineSnapshot {
            id: snapshot_id.clone(),
            timestamp: now,
            commit_hash,
            branch,
            metrics,
            verification_report,
            metadata,
        };

        // Guardar en memoria
        self.snapshots.write().unwrap().insert(snapshot_id.clone(), snapshot.clone());
        
        // Persistir a disco
        self.save_snapshot(&snapshot)?;
        
        // Limpiar snapshots antiguos
        self.cleanup_old_snapshots()?;
        
        Ok(snapshot_id)
    }

    /// Capturar snapshot automáticamente desde reporte de verificación
    pub fn capture_from_verification(&self, report: &crate::verification_engine::VerificationReport) -> Result<String> {
        let metrics = QualityMetrics {
            syntax_score: report.results.iter()
                .find(|r| r.level == crate::verification_engine::VerificationLevel::Syntax)
                .map(|r| r.score)
                .unwrap_or(0.0),
            format_score: report.results.iter()
                .find(|r| r.level == crate::verification_engine::VerificationLevel::Format)
                .map(|r| r.score)
                .unwrap_or(0.0),
            lint_score: report.results.iter()
                .find(|r| r.level == crate::verification_engine::VerificationLevel::Lint)
                .map(|r| r.score)
                .unwrap_or(0.0),
            type_score: report.results.iter()
                .find(|r| r.level == crate::verification_engine::VerificationLevel::Types)
                .map(|r| r.score)
                .unwrap_or(0.0),
            test_score: report.results.iter()
                .find(|r| r.level == crate::verification_engine::VerificationLevel::UnitTests)
                .map(|r| r.score)
                .unwrap_or(0.0),
            security_score: report.results.iter()
                .find(|r| r.level == crate::verification_engine::VerificationLevel::Security)
                .map(|r| r.score)
                .unwrap_or(0.0),
            performance_score: report.results.iter()
                .find(|r| r.level == crate::verification_engine::VerificationLevel::Performance)
                .map(|r| r.score)
                .unwrap_or(0.0),
            overall_score: report.overall_score,
            total_files: 0,
            total_symbols: 0,
            total_lines: 0,
            test_count: 0,
            test_passed: 0,
            test_failed: 0,
        };

        // Obtener commit hash actual
        let commit_hash = self.get_current_commit_hash().ok();
        let branch = self.get_current_branch().ok();

        self.capture_snapshot(metrics, Some(serde_json::to_string(report)?), commit_hash, branch)
    }

    /// Comparar snapshot actual con baseline y detectar regresiones
    pub fn detect_regressions(&self, current_metrics: &QualityMetrics, baseline_id: &str) -> Result<Vec<RegressionAlert>> {
        let baseline = self.snapshots.read().unwrap().get(baseline_id).cloned()
            .ok_or_else(|| anyhow::anyhow!("Baseline not found: {}", baseline_id))?;

        let mut alerts = Vec::new();
        let now = Utc::now();

        // Comparar cada métrica
        let metrics_to_compare = vec![
            ("syntax_score", baseline.metrics.syntax_score, current_metrics.syntax_score),
            ("format_score", baseline.metrics.format_score, current_metrics.format_score),
            ("lint_score", baseline.metrics.lint_score, current_metrics.lint_score),
            ("type_score", baseline.metrics.type_score, current_metrics.type_score),
            ("test_score", baseline.metrics.test_score, current_metrics.test_score),
            ("security_score", baseline.metrics.security_score, current_metrics.security_score),
            ("performance_score", baseline.metrics.performance_score, current_metrics.performance_score),
            ("overall_score", baseline.metrics.overall_score, current_metrics.overall_score),
        ];

        for (name, baseline_val, current_val) in metrics_to_compare {
            let delta = current_val - baseline_val;
            let delta_percent = if baseline_val != 0.0 {
                ((current_val - baseline_val) / baseline_val) * 100.0
            } else {
                0.0
            };

            // Solo alertar si hay regresión significativa
            if delta_percent < -self.config.regression_threshold {
                let severity = RegressionSeverity::from_delta_percent(delta_percent);
                
                let alert = RegressionAlert {
                    baseline_id: baseline_id.to_string(),
                    current_snapshot_id: Utc::now().to_rfc3339(), // placeholder
                    metric_name: name.to_string(),
                    baseline_value: baseline_val,
                    current_value: current_val,
                    delta,
                    delta_percent,
                    severity,
                    timestamp: Utc::now(),
                };
                
                alerts.push(alert);
            }
        }

        // Guardar alertas
        if !alerts.is_empty() {
            self.alerts.write().unwrap().extend(alerts.clone());
            self.save_alerts()?;
        }

        Ok(alerts)
    }

    /// Obtener snapshot por ID
    pub fn get_snapshot(&self, id: &str) -> Option<BaselineSnapshot> {
        self.snapshots.read().unwrap().get(id).cloned()
    }

    /// Listar todos los snapshots
    pub fn list_snapshots(&self) -> Vec<BaselineSnapshot> {
        let mut snapshots: Vec<_> = self.snapshots.read().unwrap().values().cloned().collect();
        snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        snapshots
    }

    /// Obtener alertas de regresión
    pub fn get_alerts(&self, since: Option<DateTime<Utc>>) -> Vec<RegressionAlert> {
        let alerts = self.alerts.read().unwrap();
        if let Some(since) = since {
            alerts.iter().filter(|a| a.timestamp > since).cloned().collect()
        } else {
            alerts.clone()
        }
    }

    /// Obtener estadísticas de regresiones
    pub fn get_regression_stats(&self) -> BaselineStats {
        let alerts = self.alerts.read().unwrap();
        let snapshots = self.snapshots.read().unwrap();
        
        let total_alerts = alerts.len();
        let critical_alerts = alerts.iter().filter(|a| a.severity == RegressionSeverity::Critical).count();
        let major_alerts = alerts.iter().filter(|a| a.severity == RegressionSeverity::Major).count();
        let moderate_alerts = alerts.iter().filter(|a| a.severity == RegressionSeverity::Moderate).count();
        let minor_alerts = alerts.iter().filter(|a| a.severity == RegressionSeverity::Minor).count();

        BaselineStats {
            total_snapshots: snapshots.len(),
            total_alerts,
            critical_alerts,
            major_alerts,
            moderate_alerts,
            minor_alerts,
            latest_snapshot: snapshots.values().max_by_key(|s| s.timestamp).cloned(),
        }
    }

    // --- Persistencia ---

    fn save_snapshot(&self, snapshot: &BaselineSnapshot) -> Result<()> {
        let path = self.config.storage_dir.join(format!("{}.json", snapshot.id));
        let content = serde_json::to_string_pretty(snapshot)?;
        fs::write(path, content)?;
        Ok(())
    }

    fn load_snapshots(&self) -> Result<()> {
        if !self.config.storage_dir.exists() {
            return Ok(());
        }

        let mut snapshots = self.snapshots.write().unwrap();
        for entry in fs::read_dir(&self.config.storage_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let content = fs::read_to_string(&path)?;
                if let Ok(snapshot) = serde_json::from_str::<BaselineSnapshot>(&content) {
                    snapshots.insert(snapshot.id.clone(), snapshot);
                }
            }
        }
        Ok(())
    }

    fn save_alerts(&self) -> Result<()> {
        let path = self.config.storage_dir.join("alerts.json");
        let alerts = self.alerts.read().unwrap();
        let content = serde_json::to_string_pretty(&*alerts)?;
        fs::write(path, content)?;
        Ok(())
    }

    fn load_alerts(&self) -> Result<()> {
        let path = self.config.storage_dir.join("alerts.json");
        if !path.exists() {
            return Ok(());
        }
        let content = fs::read_to_string(&path)?;
        let alerts = serde_json::from_str(&content)?;
        *self.alerts.write().unwrap() = alerts;
        Ok(())
    }

    fn cleanup_old_snapshots(&self) -> Result<()> {
        let mut snapshots = self.snapshots.write().unwrap();
        if snapshots.len() > self.config.max_snapshots {
            let mut sorted: Vec<_> = snapshots.values().collect();
            sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            
            let to_remove = sorted.len() - self.config.max_snapshots;
            for snapshot in sorted.iter().take(to_remove) {
                snapshots.remove(&snapshot.id);
                // También borrar archivo
                let path = self.config.storage_dir.join(format!("{}.json", snapshot.id));
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }

    fn cleanup_old_alerts(&self) -> Result<()> {
        let mut alerts = self.alerts.write().unwrap();
        let cutoff = Utc::now() - chrono::Duration::days(self.config.retention_days as i64);
        alerts.retain(|a| a.timestamp > cutoff);
        Ok(())
    }

    fn get_current_commit_hash(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&std::env::current_dir()?)
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn get_current_branch(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&std::env::current_dir()?)
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

/// Estadísticas del baseline tracker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineStats {
    pub total_snapshots: usize,
    pub total_alerts: usize,
    pub critical_alerts: usize,
    pub major_alerts: usize,
    pub moderate_alerts: usize,
    pub minor_alerts: usize,
    pub latest_snapshot: Option<BaselineSnapshot>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_baseline_tracker_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config = BaselineConfig {
            storage_dir: temp_dir.path().to_path_buf(),
            max_snapshots: 10,
            regression_threshold: 5.0,
            auto_capture_on_verify: true,
            retention_days: 90,
        };
        
        let tracker = BaselineTracker::new(config).unwrap();
        
        let metrics = QualityMetrics {
            overall_score: 90.0,
            syntax_score: 100.0,
            format_score: 100.0,
            lint_score: 90.0,
            type_score: 85.0,
            test_score: 80.0,
            security_score: 95.0,
            performance_score: 85.0,
            ..Default::default()
        };

        let id = tracker.capture_snapshot(metrics, None, None, None).unwrap();
        let snapshot = tracker.get_snapshot(&id).unwrap();
        assert_eq!(snapshot.metrics.overall_score, 90.0);
    }

    #[test]
    fn test_regression_detection() {
        let temp_dir = TempDir::new().unwrap();
        let config = BaselineConfig {
            storage_dir: temp_dir.path().to_path_buf(),
            max_snapshots: 10,
            regression_threshold: 5.0,
            auto_capture_on_verify: true,
            retention_days: 90,
        };
        
        let tracker = BaselineTracker::new(config).unwrap();
        
        // Baseline con score alto
        let baseline_metrics = QualityMetrics {
            overall_score: 90.0,
            syntax_score: 100.0,
            format_score: 100.0,
            lint_score: 90.0,
            type_score: 85.0,
            test_score: 80.0,
            security_score: 95.0,
            performance_score: 85.0,
            ..Default::default()
        };
        
        let baseline_id = tracker.capture_snapshot(baseline_metrics, None, None, None).unwrap();
        
        // Current con regresión en overall_score
        let current_metrics = QualityMetrics {
            overall_score: 80.0, // 10% caída
            syntax_score: 100.0,
            format_score: 100.0,
            lint_score: 90.0,
            type_score: 85.0,
            test_score: 80.0,
            security_score: 95.0,
            performance_score: 85.0,
            ..Default::default()
        };
        
        let alerts = tracker.detect_regressions(&current_metrics, &baseline_id).unwrap();
        assert!(!alerts.is_empty());
        assert_eq!(alerts[0].metric_name, "overall_score");
        assert!(alerts[0].delta_percent < -5.0);
    }
}