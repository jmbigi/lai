//! telemetry.rs - Observabilidad y trazabilidad para lai.
//! Registro estructurado de eventos (tareas, LLM, herramientas, políticas,
//! checkpoints, reparaciones) en consola y JSONL opcional.

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{info, Level};

/// Configuración de telemetría.
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub level: String,
    pub log_dir: Option<PathBuf>,
    pub json_output: bool,
    pub console_output: bool,
    pub debug_artifacts: bool,
    pub debug_artifacts_dir: Option<PathBuf>,
    pub max_file_size_mb: usize,
    pub max_files: usize,
    pub span_events: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            log_dir: None,
            json_output: false,
            console_output: true,
            debug_artifacts: false,
            debug_artifacts_dir: None,
            max_file_size_mb: 100,
            max_files: 10,
            span_events: true,
        }
    }
}

/// Error de inicialización de telemetría.
#[derive(Debug)]
pub struct TelemetryError(pub String);

impl std::fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TelemetryError {}

struct TelemetryState {
    config: TelemetryConfig,
    file: Option<Mutex<std::fs::File>>,
}

static STATE: std::sync::OnceLock<TelemetryState> = std::sync::OnceLock::new();

/// Inicializa la telemetría global. Debe llamarse una vez al arrancar.
pub fn init_telemetry(config: TelemetryConfig) -> Result<(), TelemetryError> {
    let file = match &config.log_dir {
        Some(dir) => {
            std::fs::create_dir_all(dir).map_err(|e| TelemetryError(e.to_string()))?;
            let path = dir.join(format!("lai-{}.jsonl", chrono::Utc::now().format("%Y%m%d")));
            let f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| TelemetryError(e.to_string()))?;
            Some(Mutex::new(f))
        }
        None => None,
    };

    let level = match config.level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .with_target(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let _ = STATE.set(TelemetryState { config, file });
    Ok(())
}

/// Tipos de evento registrables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TelemetryEventKind {
    Task,
    LlmCall,
    ToolCall,
    Error,
    Checkpoint,
    PolicyDecision,
    ContextChange,
    RepairCycle,
}

/// Evento genérico de telemetría.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub timestamp: String,
    pub kind: String,
    pub task_id: Option<String>,
    pub message: String,
    pub success: Option<bool>,
    pub duration_ms: Option<u64>,
    pub data: serde_json::Value,
}

/// Registra un evento genérico.
pub fn record_event(kind: TelemetryEventKind, event: TelemetryEvent) {
    let json = serde_json::to_string(&event).unwrap_or_default();
    if let Some(state) = STATE.get() {
        if let Some(file) = &state.file {
            if let Ok(mut f) = file.lock() {
                let _ = writeln!(f, "{}", json);
            }
        }
        if state.config.console_output {
            info!("[{}] {}", kind_str(kind), event.message);
        }
    }
}

fn kind_str(kind: TelemetryEventKind) -> &'static str {
    match kind {
        TelemetryEventKind::Task => "TASK",
        TelemetryEventKind::LlmCall => "LLM",
        TelemetryEventKind::ToolCall => "TOOL",
        TelemetryEventKind::Error => "ERROR",
        TelemetryEventKind::Checkpoint => "CKPT",
        TelemetryEventKind::PolicyDecision => "POLICY",
        TelemetryEventKind::ContextChange => "CTX",
        TelemetryEventKind::RepairCycle => "REPAIR",
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn base_event(kind: TelemetryEventKind, task_id: &str, message: String) -> TelemetryEvent {
    TelemetryEvent {
        timestamp: now(),
        kind: kind_str(kind).to_string(),
        task_id: Some(task_id.to_string()),
        message,
        success: None,
        duration_ms: None,
        data: serde_json::Value::Null,
    }
}

/// Span de tarea para tracing.
pub fn task_span(task_id: &str, task_type: &str, description: &str) -> tracing::Span {
    tracing::span!(
        Level::INFO,
        "task",
        task_id = task_id,
        task_type = task_type,
        description = description
    )
}

/// Cierra el span de tarea y registra el resultado.
pub fn end_task_span(task_id: &str, success: bool, duration: std::time::Duration, summary: &str) {
    let mut event = base_event(
        TelemetryEventKind::Task,
        task_id,
        format!("Task {}: {} ({}ms)", if success { "completed" } else { "failed" }, summary, duration.as_millis()),
    );
    event.success = Some(success);
    event.duration_ms = Some(duration.as_millis() as u64);
    record_event(TelemetryEventKind::Task, event);
}

/// Registra una llamada a herramienta.
pub fn record_tool_call(
    task_id: &str,
    tool: &str,
    args: serde_json::Value,
    result: Option<serde_json::Value>,
    duration: std::time::Duration,
    success: bool,
    error: Option<String>,
) {
    let mut event = base_event(
        TelemetryEventKind::ToolCall,
        task_id,
        format!("tool={} ok={} err={:?}", tool, success, error),
    );
    event.success = Some(success);
    event.duration_ms = Some(duration.as_millis() as u64);
    event.data = serde_json::json!({
        "tool": tool,
        "args": args,
        "result": result,
        "error": error,
    });
    record_event(TelemetryEventKind::ToolCall, event);
}

/// Registra una llamada al LLM.
pub fn record_llm_call(
    task_id: &str,
    model: &str,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    duration: std::time::Duration,
    success: bool,
    error: Option<String>,
) {
    let mut event = base_event(
        TelemetryEventKind::LlmCall,
        task_id,
        format!("model={} ok={} ptok={:?} ctok={:?}", model, success, prompt_tokens, completion_tokens),
    );
    event.success = Some(success);
    event.duration_ms = Some(duration.as_millis() as u64);
    event.data = serde_json::json!({
        "model": model,
        "prompt_tokens": prompt_tokens,
        "completion_tokens": completion_tokens,
        "error": error,
    });
    record_event(TelemetryEventKind::LlmCall, event);
}

/// Registra un error.
pub fn record_error(task_id: &str, context: &str, error: &str) {
    let mut event = base_event(
        TelemetryEventKind::Error,
        task_id,
        format!("{}: {}", context, error),
    );
    event.success = Some(false);
    record_event(TelemetryEventKind::Error, event);
}

/// Registra un checkpoint.
pub fn record_checkpoint(
    task_id: &str,
    checkpoint_id: &str,
    description: &str,
    files_modified: Vec<String>,
    _context_snapshot: serde_json::Value,
) {
    let event = base_event(
        TelemetryEventKind::Checkpoint,
        task_id,
        format!("checkpoint={} files={:?} desc={}", checkpoint_id, files_modified, description),
    );
    record_event(TelemetryEventKind::Checkpoint, event);
}

/// Registra una decisión de política.
pub fn record_policy_decision(task_id: &str, tool: &str, allowed: bool, reason: &str, policy: &str) {
    let mut event = base_event(
        TelemetryEventKind::PolicyDecision,
        task_id,
        format!("tool={} allowed={} policy={} reason={}", tool, allowed, policy, reason),
    );
    event.success = Some(allowed);
    event.data = serde_json::json!({ "tool": tool, "policy": policy, "reason": reason });
    record_event(TelemetryEventKind::PolicyDecision, event);
}

/// Registra un cambio de contexto.
pub fn record_context_change(task_id: &str, change_type: &str, description: &str, files_affected: Vec<String>) {
    let event = base_event(
        TelemetryEventKind::ContextChange,
        task_id,
        format!("type={} files={:?} desc={}", change_type, files_affected, description),
    );
    record_event(TelemetryEventKind::ContextChange, event);
}

/// Registra un ciclo de reparación.
pub fn record_repair_cycle(
    task_id: &str,
    iteration: u32,
    phase: &str,
    failures: u32,
    fixes: u32,
    progress: bool,
) {
    let mut event = base_event(
        TelemetryEventKind::RepairCycle,
        task_id,
        format!("iter={} phase={} failures={} fixes={} progress={}", iteration, phase, failures, fixes, progress),
    );
    event.success = Some(progress);
    record_event(TelemetryEventKind::RepairCycle, event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = base_event(TelemetryEventKind::ToolCall, "t1", "hello".to_string());
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("hello"));
        assert!(json.contains("TOOL"));
    }
}