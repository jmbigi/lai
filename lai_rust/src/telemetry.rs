//! telemetry.rs - Observabilidad y telemetría (v5.0).
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub level: String,
    pub log_dir: Option<std::path::PathBuf>,
    pub json_output: bool,
    pub console_output: bool,
    pub debug_artifacts: bool,
    pub debug_artifacts_dir: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Telemetry {
    pub config: TelemetryConfig,
    pub request_counter: u64,
    pub tool_call_counter: u64,
    pub llm_call_counter: u64,
    pub error_counter: u64,
    pub session_id: String,
    pub start_time: Instant,
    pub events: Vec<TelemetryEvent>,
}

impl Telemetry {
    pub fn init(config: TelemetryConfig) -> Result<(), String> {
        let t = Telemetry {
            config,
            request_counter: 0,
            tool_call_counter: 0,
            llm_call_counter: 0,
            error_counter: 0,
            session_id: uuid::Uuid::new_v4().to_string(),
            start_time: Instant::now(),
            events: Vec::new(),
        };
        *TELEMETRY.lock().unwrap() = Some(t);
        Ok(())
    }

    pub fn instance() -> &'static Mutex<Option<Telemetry>> {
        &TELEMETRY
    }

    pub fn metrics(&self) -> TelemetryMetrics {
        TelemetryMetrics {
            requests: self.request_counter,
            tool_calls: self.tool_call_counter,
            llm_calls: self.llm_call_counter,
            errors: self.error_counter,
            uptime: self.start_time.elapsed(),
        }
    }

    pub fn inc_requests(&mut self) {
        self.request_counter += 1;
    }
    pub fn inc_tool_calls(&mut self) {
        self.tool_call_counter += 1;
    }
    pub fn inc_llm_calls(&mut self) {
        self.llm_call_counter += 1;
    }
    pub fn inc_errors(&mut self) {
        self.error_counter += 1;
    }
    pub fn record_event(&mut self, event: TelemetryEvent) {
        self.events.push(event);
        if self.events.len() > 10_000 {
            self.events.drain(0..self.events.len() - 10_000);
        }
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            level: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            log_dir: std::env::var("LAI_LOG_DIR")
                .ok()
                .map(std::path::PathBuf::from),
            json_output: std::env::var("LAI_LOG_JSON").is_ok(),
            console_output: true,
            debug_artifacts: std::env::var("LAI_DEBUG_ARTIFACTS").is_ok(),
            debug_artifacts_dir: std::env::var("LAI_DEBUG_TESTS")
                .ok()
                .map(std::path::PathBuf::from),
        }
    }
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            config: TelemetryConfig::default(),
            request_counter: 0,
            tool_call_counter: 0,
            llm_call_counter: 0,
            error_counter: 0,
            session_id: uuid::Uuid::new_v4().to_string(),
            start_time: Instant::now(),
            events: Vec::new(),
        }
    }
}

static TELEMETRY: Mutex<Option<Telemetry>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub timestamp: String,
    pub session_id: String,
    pub task_id: Option<String>,
    #[serde(rename = "type")]
    pub kind: TelemetryEventKind,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TelemetryEventKind {
    TaskStart {
        task_type: String,
        description: String,
    },
    TaskEnd {
        success: bool,
        duration_ms: u64,
        summary: String,
    },
    ToolCall {
        tool: String,
        definition_hash: Option<String>,
        policy_decision: bool,
        args: serde_json::Value,
        result: Option<serde_json::Value>,
        duration_ms: u64,
        success: bool,
        error: Option<String>,
    },
    LlmCall {
        model: String,
        prompt_tokens: Option<u64>,
        completion_tokens: Option<u64>,
        duration_ms: u64,
        success: bool,
        error: Option<String>,
    },
    Error {
        error_type: String,
        message: String,
        context: HashMap<String, serde_json::Value>,
        recoverable: bool,
    },
    Checkpoint {
        checkpoint_id: String,
        description: String,
        files: Vec<String>,
        state_snapshot: serde_json::Value,
    },
    PolicyDecision {
        tool: String,
        allowed: bool,
        reason: String,
        policy: String,
    },
    ContextChange {
        change_type: String,
        files_added: Vec<String>,
        files_removed: Vec<String>,
        token_delta: i64,
    },
    RepairCycle {
        cycle: u32,
        phase: String,
        issues_found: u32,
        issues_fixed: u32,
        success: bool,
    },
    Verification {
        verifier: String,
        level: u32,
        passed: bool,
        details: String,
        artifacts: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct TelemetryMetrics {
    pub requests: u64,
    pub tool_calls: u64,
    pub llm_calls: u64,
    pub errors: u64,
    pub uptime: Duration,
}

pub fn record_policy_decision(
    task_id: &str,
    tool: &str,
    allowed: bool,
    reason: &str,
    policy: &str,
) {
    if let Some(mut t) = TELEMETRY.lock().unwrap().take() {
        let event = TelemetryEvent {
            timestamp: now(),
            session_id: t.session_id.clone(),
            task_id: Some(task_id.to_string()),
            kind: TelemetryEventKind::PolicyDecision {
                tool: tool.to_string(),
                allowed,
                reason: reason.to_string(),
                policy: policy.to_string(),
            },
            data: serde_json::json!({}),
        };
        t.record_event(event);
        *TELEMETRY.lock().unwrap() = Some(t);
    }
}

pub fn record_tool_call(
    task_id: &str,
    tool: &str,
    definition_hash: Option<String>,
    policy_decision: bool,
    args: serde_json::Value,
    result: Option<serde_json::Value>,
    duration: Duration,
    success: bool,
    error: Option<String>,
) {
    if let Some(mut t) = TELEMETRY.lock().unwrap().take() {
        t.inc_tool_calls();
        let event = TelemetryEvent {
            timestamp: now(),
            session_id: t.session_id.clone(),
            task_id: Some(task_id.to_string()),
            kind: TelemetryEventKind::ToolCall {
                tool: tool.to_string(),
                definition_hash,
                policy_decision,
                args,
                result,
                duration_ms: duration.as_millis() as u64,
                success,
                error,
            },
            data: serde_json::json!({}),
        };
        t.record_event(event);
        *TELEMETRY.lock().unwrap() = Some(t);
    }
}

pub fn record_llm_call(
    task_id: &str,
    model: &str,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    duration: Duration,
    success: bool,
    error: Option<String>,
) {
    if let Some(mut t) = TELEMETRY.lock().unwrap().take() {
        t.inc_llm_calls();
        let event = TelemetryEvent {
            timestamp: now(),
            session_id: t.session_id.clone(),
            task_id: Some(task_id.to_string()),
            kind: TelemetryEventKind::LlmCall {
                model: model.to_string(),
                prompt_tokens,
                completion_tokens,
                duration_ms: duration.as_millis() as u64,
                success,
                error,
            },
            data: serde_json::json!({}),
        };
        t.record_event(event);
        *TELEMETRY.lock().unwrap() = Some(t);
    }
}

pub fn record_error(
    task_id: Option<&str>,
    error_type: &str,
    message: &str,
    context: HashMap<String, serde_json::Value>,
    recoverable: bool,
) {
    if let Some(mut t) = TELEMETRY.lock().unwrap().take() {
        t.inc_errors();
        let event = TelemetryEvent {
            timestamp: now(),
            session_id: t.session_id.clone(),
            task_id: task_id.map(|s| s.to_string()),
            kind: TelemetryEventKind::Error {
                error_type: error_type.to_string(),
                message: message.to_string(),
                context,
                recoverable,
            },
            data: serde_json::json!({}),
        };
        t.record_event(event);
        *TELEMETRY.lock().unwrap() = Some(t);
    }
}

pub fn record_checkpoint(
    task_id: &str,
    checkpoint_id: &str,
    description: &str,
    files: Vec<String>,
    state_snapshot: serde_json::Value,
) {
    if let Some(mut t) = TELEMETRY.lock().unwrap().take() {
        let event = TelemetryEvent {
            timestamp: now(),
            session_id: t.session_id.clone(),
            task_id: Some(task_id.to_string()),
            kind: TelemetryEventKind::Checkpoint {
                checkpoint_id: checkpoint_id.to_string(),
                description: description.to_string(),
                files,
                state_snapshot,
            },
            data: serde_json::json!({}),
        };
        t.record_event(event);
        *TELEMETRY.lock().unwrap() = Some(t);
    }
}

pub fn record_repair_cycle(
    task_id: &str,
    cycle: u32,
    phase: &str,
    issues_found: u32,
    issues_fixed: u32,
    success: bool,
) {
    if let Some(mut t) = TELEMETRY.lock().unwrap().take() {
        let event = TelemetryEvent {
            timestamp: now(),
            session_id: t.session_id.clone(),
            task_id: Some(task_id.to_string()),
            kind: TelemetryEventKind::RepairCycle {
                cycle,
                phase: phase.to_string(),
                issues_found,
                issues_fixed,
                success,
            },
            data: serde_json::json!({}),
        };
        t.record_event(event);
        *TELEMETRY.lock().unwrap() = Some(t);
    }
}

pub fn record_verification(
    task_id: &str,
    verifier: &str,
    level: u32,
    passed: bool,
    details: String,
    artifacts: Vec<String>,
) {
    if let Some(mut t) = TELEMETRY.lock().unwrap().take() {
        let event = TelemetryEvent {
            timestamp: now(),
            session_id: t.session_id.clone(),
            task_id: Some(task_id.to_string()),
            kind: TelemetryEventKind::Verification {
                verifier: verifier.to_string(),
                level,
                passed,
                details,
                artifacts,
            },
            data: serde_json::json!({}),
        };
        t.record_event(event);
        *TELEMETRY.lock().unwrap() = Some(t);
    }
}

pub fn task_span(task_id: &str, task_type: &str, description: &str) {
    if let Some(mut t) = TELEMETRY.lock().unwrap().take() {
        t.inc_requests();
        let event = TelemetryEvent {
            timestamp: now(),
            session_id: t.session_id.clone(),
            task_id: Some(task_id.to_string()),
            kind: TelemetryEventKind::TaskStart {
                task_type: task_type.to_string(),
                description: description.to_string(),
            },
            data: serde_json::json!({}),
        };
        t.record_event(event);
        *TELEMETRY.lock().unwrap() = Some(t);
    }
}

pub fn end_task_span(task_id: &str, success: bool, duration: Duration, summary: &str) {
    if let Some(mut t) = TELEMETRY.lock().unwrap().take() {
        let event = TelemetryEvent {
            timestamp: now(),
            session_id: t.session_id.clone(),
            task_id: Some(task_id.to_string()),
            kind: TelemetryEventKind::TaskEnd {
                success,
                duration_ms: duration.as_millis() as u64,
                summary: summary.to_string(),
            },
            data: serde_json::json!({}),
        };
        t.record_event(event);
        *TELEMETRY.lock().unwrap() = Some(t);
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
