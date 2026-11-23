//! Observabilidad v5.0 (herramienta 4: Langfuse).
//!
//! Exporta los eventos de telemetría (TASK/LLM/TOOL/VERIFY/POLICY) al
//! formato batch de Langfuse (compatible con su API `/api/public/ingestion`),
//! auto-alojable y **sin dependencia de cloud** (Regla 0):
//!
//! - `LANGFUSE_HOST` (default `http://localhost:3000`): si hay una instancia
//!   local, se hace POST del batch; si no, el batch queda en
//!   `.lai/langfuse/batch-*.json` para ingestión posterior.
//! - **Fail-open**: la observabilidad NO es un control; un fallo de export
//!   nunca bloquea la ejecución (se registra en telemetry como ERROR).
//!   Los controles (integridad, policy, verificación) siguen siendo
//!   fail-closed.
//!
//! La adopción formal de Langfuse como backend se consolida en la Fase 5
//! del roadmap; desde la Fase 1 se registran las métricas obligatorias
//! (tasa de verificación efectiva, tool_definition_hash, policy_decision).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Evento en formato Langfuse (subset de ingestion).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangfuseEvent {
    pub id: String,
    pub timestamp: String,
    pub name: String,
    pub user_id: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
}

/// Batch de eventos Langfuse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LangfuseBatch {
    pub batch: Vec<LangfuseEvent>,
}

/// Exportador de observabilidad.
pub struct LangfuseExporter {
    dir: PathBuf,
    host: Option<String>,
    api_key: Option<String>,
}

impl LangfuseExporter {
    /// `dir`: directorio local de batches (default `.lai/langfuse`).
    /// `host`: si `Some`, se intenta POST tras escribir el batch local.
    pub fn new(dir: &Path, host: Option<String>, api_key: Option<String>) -> Self {
        Self {
            dir: dir.to_path_buf(),
            host,
            api_key,
        }
    }

    /// Exporta un batch: siempre lo persiste localmente; si hay host
    /// configurado, intenta el POST (fail-open).
    pub fn export(&self, batch: &LangfuseBatch) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let path = self.dir.join(format!("batch-{}.json", ts));
        std::fs::write(&path, serde_json::to_string_pretty(batch)?)?;

        if let Some(host) = &self.host {
            self.try_post(batch, host);
        }
        Ok(path)
    }

    /// POST fail-open: cualquier error se registra y no se propaga.
    fn try_post(&self, batch: &LangfuseBatch, host: &str) {
        let url = format!("{}/api/public/ingestion", host.trim_end_matches('/'));
        let body = serde_json::to_string(batch).unwrap_or_default();
        let auth: Option<String> = self
            .api_key
            .as_ref()
            .map(|k| format!("Authorization: Basic {}", k));
        let mut cmd = std::process::Command::new("curl");
        cmd.args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--max-time",
            "5",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
        ]);
        if let Some(auth) = &auth {
            cmd.args(["-H", auth]);
        }
        let result = cmd.arg("-d").arg(&body).arg(&url).output();

        match result {
            Ok(out) if out.status.success() => {
                let code = String::from_utf8_lossy(&out.stdout);
                if code.trim() != "200" && code.trim() != "207" {
                    crate::telemetry::record_error(
                        "langfuse",
                        "export",
                        &format!("Langfuse respondió HTTP {}", code.trim()),
                    );
                }
            }
            Ok(out) => {
                crate::telemetry::record_error(
                    "langfuse",
                    "export",
                    &format!("curl falló: {}", String::from_utf8_lossy(&out.stderr).trim()),
                );
            }
            Err(e) => {
                crate::telemetry::record_error("langfuse", "export", &format!("curl no disponible: {}", e));
            }
        }
    }

    /// Directorio donde quedan los batches.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Construye un batch Langfuse desde eventos de trayectoria.
pub fn batch_from_trajectory(
    task_id: &str,
    model: &str,
    verdict: &str,
    tool_definition_hashes: Vec<String>,
    policy_decisions: Vec<(String, bool)>,
    verification_attempted: Vec<String>,
    verification_passed: Vec<String>,
    duration_ms: u64,
) -> LangfuseBatch {
    let now = chrono::Utc::now().to_rfc3339();
    let mut batch = Vec::new();

    batch.push(LangfuseEvent {
        id: format!("{}-task", task_id),
        timestamp: now.clone(),
        name: "lai.task".to_string(),
        user_id: None,
        input: Some(serde_json::json!({"task_id": task_id, "model": model})),
        output: Some(serde_json::json!({
            "verdict": verdict,
            "duration_ms": duration_ms,
        })),
        metadata: Some(serde_json::json!({
            "verification_levels_attempted": verification_attempted,
            "verification_levels_passed": verification_passed,
        })),
    });

    for (i, hash) in tool_definition_hashes.iter().enumerate() {
        batch.push(LangfuseEvent {
            id: format!("{}-tool-{}", task_id, i),
            timestamp: now.clone(),
            name: "lai.tool".to_string(),
            user_id: None,
            input: None,
            output: None,
            metadata: Some(serde_json::json!({"tool_definition_hash": hash})),
        });
    }

    for (i, (tool, allowed)) in policy_decisions.iter().enumerate() {
        batch.push(LangfuseEvent {
            id: format!("{}-policy-{}", task_id, i),
            timestamp: now.clone(),
            name: "lai.policy".to_string(),
            user_id: None,
            input: Some(serde_json::json!({"tool": tool})),
            output: Some(serde_json::json!({"allowed": allowed})),
            metadata: Some(serde_json::json!({"policy_decision": if *allowed { "ALLOW" } else { "DENY" }})),
        });
    }

    LangfuseBatch { batch }
}

/// Timeout por defecto del POST (segundos), usado en tests.
pub const DEFAULT_POST_TIMEOUT_SECS: u64 = 5;
#[allow(dead_code)]
fn _timeout_marker() -> Duration {
    Duration::from_secs(DEFAULT_POST_TIMEOUT_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_writes_local_batch() {
        let dir = std::env::temp_dir().join(format!("lai-lf-test-{}", uuid::Uuid::new_v4()));
        let exp = LangfuseExporter::new(&dir, None, None);
        let batch = batch_from_trajectory(
            "task-1",
            "m",
            "SUCCESS",
            vec!["abc".to_string()],
            vec![("write_file".to_string(), false)],
            vec!["syntax".to_string()],
            vec!["syntax".to_string()],
            100,
        );
        let path = exp.export(&batch).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("lai.task"));
        assert!(content.contains("abc"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_batch_contains_policy_decisions() {
        let batch = batch_from_trajectory(
            "t",
            "m",
            "BLOCKED",
            vec![],
            vec![("run_command".to_string(), false)],
            vec![],
            vec![],
            0,
        );
        let policy: Vec<_> = batch.batch.iter().filter(|e| e.name == "lai.policy").collect();
        assert_eq!(policy.len(), 1);
        assert_eq!(
            policy[0].metadata.as_ref().unwrap()["policy_decision"],
            "DENY"
        );
    }

    #[test]
    fn test_batch_contains_verification_levels() {
        let batch = batch_from_trajectory(
            "t",
            "m",
            "SUCCESS",
            vec![],
            vec![],
            vec!["syntax".to_string(), "unit_tests".to_string()],
            vec!["syntax".to_string()],
            0,
        );
        let task = batch.batch.iter().find(|e| e.name == "lai.task").unwrap();
        let meta = task.metadata.as_ref().unwrap();
        assert_eq!(meta["verification_levels_attempted"].as_array().unwrap().len(), 2);
        assert_eq!(meta["verification_levels_passed"].as_array().unwrap().len(), 1);
    }
}