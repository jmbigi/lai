//! Registro de trayectorias (v4.0, Fase 6 - Auto-mejora).
//!
//! Cada tarea deja una trayectoria completa y auditable:
//! Task -> Plan -> Actions -> Observations -> Verification -> Outcome.
//!
//! La auto-mejora (largo plazo) consume estas trayectorias para analizar
//! rendimiento y comparar estrategias. El registro es local (JSONL) y
//! nunca contiene secretos: solo IDs, nombres de herramientas, veredictos
//! y métricas.

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// Acción individual dentro de una trayectoria.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryAction {
    pub phase: String,
    pub tool: Option<String>,
    pub tool_definition_hash: Option<String>,
    pub success: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
}

/// Registro completo de una tarea.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryRecord {
    pub schema_version: u32,
    pub task_id: String,
    pub timestamp: String,
    pub model: String,
    pub harness_version: String,
    pub task_type: String,
    pub objective: String,
    pub plan_steps: usize,
    pub actions: Vec<TrajectoryAction>,
    pub verification_levels_attempted: Vec<String>,
    pub verification_levels_passed: Vec<String>,
    pub verdict: String,
    pub files_modified: Vec<String>,
    pub iterations: u32,
    pub duration_ms: u64,
    pub outcome: String,
}

impl TrajectoryRecord {
    pub fn new(task_id: &str, model: &str, task_type: &str, objective: &str) -> Self {
        Self {
            schema_version: 1,
            task_id: task_id.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            model: model.to_string(),
            harness_version: env!("CARGO_PKG_VERSION").to_string(),
            task_type: task_type.to_string(),
            objective: objective.to_string(),
            plan_steps: 0,
            actions: vec![],
            verification_levels_attempted: vec![],
            verification_levels_passed: vec![],
            verdict: String::new(),
            files_modified: vec![],
            iterations: 0,
            duration_ms: 0,
            outcome: String::new(),
        }
    }
}

/// Directorio de trayectorias: `.lai/trajectories/`.
fn trajectory_dir() -> PathBuf {
    std::env::var("LAI_TRAJECTORY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".lai/trajectories"))
}

/// Registra una trayectoria completa (append JSONL).
pub fn record_trajectory(record: &TrajectoryRecord) -> std::io::Result<PathBuf> {
    let dir = trajectory_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("trajectories.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    let line = serde_json::to_string(record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    writeln!(file, "{}", line)?;
    Ok(path)
}

/// Lee todas las trayectorias registradas (para análisis offline).
pub fn load_trajectories() -> Vec<TrajectoryRecord> {
    let path = trajectory_dir().join("trajectories.jsonl");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<TrajectoryRecord>(l).ok())
        .collect()
}

/// Resumen agregado de trayectorias para comparar estrategias.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrajectoryStats {
    pub tasks: usize,
    pub success_rate: f64,
    pub avg_duration_ms: f64,
    pub avg_iterations: f64,
    pub verification_effective_rate: f64,
    pub tool_failure_rate: f64,
}

impl TrajectoryStats {
    pub fn compute(records: &[TrajectoryRecord]) -> Self {
        if records.is_empty() {
            return Self::default();
        }
        let n = records.len() as f64;
        let successes = records
            .iter()
            .filter(|r| r.verdict == "SUCCESS" || r.verdict == "SUCCESS_WITH_DECLARED_GAPS")
            .count();
        let verified = records
            .iter()
            .filter(|r| {
                r.verdict == "SUCCESS" || r.verdict == "SUCCESS_WITH_DECLARED_GAPS"
            })
            .count();
        let total_actions: usize = records.iter().map(|r| r.actions.len()).sum();
        let failed_actions: usize = records
            .iter()
            .flat_map(|r| r.actions.iter())
            .filter(|a| !a.success)
            .count();
        Self {
            tasks: records.len(),
            success_rate: successes as f64 / n,
            avg_duration_ms: records.iter().map(|r| r.duration_ms).sum::<u64>() as f64 / n,
            avg_iterations: records.iter().map(|r| r.iterations).sum::<u32>() as f64 / n,
            verification_effective_rate: verified as f64 / n,
            tool_failure_rate: if total_actions == 0 {
                0.0
            } else {
                failed_actions as f64 / total_actions as f64
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record(id: &str, verdict: &str) -> TrajectoryRecord {
        let mut r = TrajectoryRecord::new(id, "test-model", "test", "objective");
        r.verdict = verdict.to_string();
        r.duration_ms = 100;
        r.iterations = 2;
        r.actions.push(TrajectoryAction {
            phase: "EDIT".to_string(),
            tool: Some("read_file".to_string()),
            tool_definition_hash: Some("abc123".to_string()),
            success: true,
            duration_ms: 5,
            error: None,
        });
        r
    }

    #[test]
    fn test_stats_computation() {
        let records = vec![sample_record("a", "SUCCESS"), sample_record("b", "PARTIALLY_VERIFIED")];
        let stats = TrajectoryStats::compute(&records);
        assert_eq!(stats.tasks, 2);
        assert!((stats.success_rate - 0.5).abs() < 1e-9);
        assert!((stats.avg_duration_ms - 100.0).abs() < 1e-9);
        assert!((stats.avg_iterations - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_stats_empty() {
        let stats = TrajectoryStats::compute(&[]);
        assert_eq!(stats.tasks, 0);
        assert!((stats.success_rate - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_record_roundtrip_jsonl() {
        let dir = std::env::temp_dir().join(format!("lai-traj-test-{}", uuid::Uuid::new_v4()));
        std::env::set_var("LAI_TRAJECTORY_DIR", &dir);
        let record = sample_record("t1", "SUCCESS");
        let path = record_trajectory(&record).unwrap();
        assert!(path.exists());
        let loaded = load_trajectories();
        std::env::remove_var("LAI_TRAJECTORY_DIR");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].task_id, "t1");
        assert_eq!(loaded[0].verdict, "SUCCESS");
    }
}