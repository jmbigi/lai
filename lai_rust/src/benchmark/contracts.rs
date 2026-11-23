/// Mandatory Contracts for lai Tool Adapters
/// 
/// This module defines the mandatory contracts that all external tool
/// integrations must implement to ensure security, verifiability,
/// and measurable performance.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Execution contract - must be implemented by every adapter
#[async_trait::async_trait]
pub trait CapabilityAdapter: Send + Sync {
    /// Unique identifier for this capability
    fn capability_id(&self) -> String;
    
    /// Human-readable name
    fn capability_name(&self) -> String;
    
    /// Version of the adapter implementation
    fn adapter_version(&self) -> String;
    
    /// Version of the underlying tool
    fn tool_version(&self) -> String;
    
    /// Tool definition fingerprint (SHA-256)
    fn definition_hash(&self) -> String;
    
    /// Check if tool is available and healthy
    async fn health_check(&self) -> HealthCheckResult;
    
    /// Pre-flight checks before execution
    async fn preflight(&self, request: &ExecutionRequest) -> Result<PreflightResult, ContractError>;
    
    /// Execute the capability
    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult, ContractError>;
    
    /// Validate execution result
    async fn validate(&self, result: &ExecutionResult) -> ValidationResult;
    
    /// Get execution metrics
    async fn get_metrics(&self, execution_id: &str) -> Option<ExecutionMetrics>;
    
    /// Get supported features
    fn supported_features(&self) -> AdapterFeatures;
    
    /// Get resource requirements
    fn resource_requirements(&self) -> ResourceRequirements;
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub healthy: bool,
    pub tool_version: String,
    pub definition_hash: String,
    pub checks: Vec<HealthCheck>,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
    pub severity: HealthSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthSeverity {
    Info,
    Warning,
    Critical,
}

/// Pre-flight check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub warnings: Vec<String>,
    pub estimated_resources: ResourceEstimate,
    pub required_permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEstimate {
    pub estimated_cpu_seconds: f64,
    pub estimated_memory_mb: u64,
    pub estimated_network_mb: u64,
    pub estimated_tokens: u64,
}

/// Execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub execution_id: String,
    pub task_id: String,
    pub capability_id: String,
    pub input: serde_json::Value,
    pub parameters: HashMap<String, serde_json::Value>,
    pub budget: ExecutionBudget,
    pub context: ExecutionContext,
    pub security_context: SecurityContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionBudget {
    pub max_cpu_seconds: f64,
    pub max_memory_mb: u64,
    pub max_network_mb: u64,
    pub max_tokens: u64,
    pub max_wall_time_seconds: u64,
}

impl Default for ExecutionBudget {
    fn default() -> Self {
        Self {
            max_cpu_seconds: 60.0,
            max_memory_mb: 512,
            max_network_mb: 10,
            max_tokens: 4096,
            max_wall_time_seconds: 120,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub project_root: PathBuf,
    pub working_directory: PathBuf,
    pub environment: HashMap<String, String>,
    pub previous_results: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityContext {
    pub allowed_paths: Vec<PathBuf>,
    pub denied_paths: Vec<PathBuf>,
    pub allowed_commands: Vec<String>,
    pub network_policy: NetworkPolicy,
    pub sandbox_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkPolicy {
    None,
    LocalOnly,
    AllowList(Vec<String>),
    DenyList(Vec<String>),
}

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub execution_id: String,
    pub capability_id: String,
    pub status: ResultStatus,
    pub output: serde_json::Value,
    pub evidence: Vec<Evidence>,
    pub metrics: ExecutionMetrics,
    pub security_audit: SecurityAudit,
    pub started_at: SystemTime,
    pub completed_at: SystemTime,
}

/// Standardized result statuses (mandatory for all adapters)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultStatus {
    /// Formally verified within declared scope
    Verified,
    /// Test/check passed (experimental evidence)
    Passed,
    /// Reproducible failure found
    Failed,
    /// Valid counterexample found
    CounterexampleFound,
    /// Only partially completed
    Partial,
    /// Insufficient evidence
    Unknown,
    /// Budget exhausted
    Timeout,
    /// Resource limit reached
    ResourceLimit,
    /// Blocked by mandatory condition
    Blocked,
    /// Internal error
    Error,
}

impl ResultStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, ResultStatus::Verified | ResultStatus::Passed)
    }
    
    pub fn is_failure(&self) -> bool {
        matches!(self, ResultStatus::Failed | ResultStatus::CounterexampleFound)
    }
    
    pub fn is_inconclusive(&self) -> bool {
        matches!(self, ResultStatus::Partial | ResultStatus::Unknown | ResultStatus::Timeout | ResultStatus::ResourceLimit)
    }
}

/// Evidence for results (mandatory)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub evidence_id: String,
    pub evidence_type: EvidenceType,
    pub description: String,
    pub data: serde_json::Value,
    pub source: EvidenceSource,
    pub timestamp: SystemTime,
    pub verifiable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceType {
    /// Formal proof artifact
    Proof,
    /// Test execution log
    TestLog,
    /// Verifier output
    VerifierOutput,
    /// Counterexample
    Counterexample,
    /// Static analysis report
    StaticAnalysis,
    /// Measurement/metric
    Metric,
    /// Audit log entry
    AuditLog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceSource {
    /// Generated by the adapter
    Adapter,
    /// External verifier
    Verifier,
    /// External tool
    Tool,
    /// Manual/human
    Human,
    /// System/environment
    System,
}

/// Execution metrics (mandatory)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub execution_id: String,
    pub capability_id: String,
    pub tool_version: String,
    pub definition_hash: String,
    pub wall_time_ms: u64,
    pub cpu_time_ms: u64,
    pub peak_memory_mb: u64,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,
    pub result_status: ResultStatus,
    pub verification_status: VerificationStatus,
    pub error_count: u32,
    pub timeout_count: u32,
    pub resource_limit_count: u32,
    pub custom_metrics: HashMap<String, f64>,
}

impl ExecutionMetrics {
    pub fn new(execution_id: String, capability_id: String, tool_version: String, definition_hash: String) -> Self {
        Self {
            execution_id,
            capability_id,
            tool_version,
            definition_hash,
            wall_time_ms: 0,
            cpu_time_ms: 0,
            peak_memory_mb: 0,
            tokens_input: 0,
            tokens_output: 0,
            network_bytes_sent: 0,
            network_bytes_received: 0,
            result_status: ResultStatus::Unknown,
            verification_status: VerificationStatus::Unknown,
            error_count: 0,
            timeout_count: 0,
            resource_limit_count: 0,
            custom_metrics: HashMap::new(),
        }
    }
}

/// Verification status for metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    Verified,
    PartiallyVerified,
    NotVerified,
    VerificationFailed,
    Unknown,
}

/// Security audit for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAudit {
    pub execution_id: String,
    pub integrity_verified: bool,
    pub definition_hash_match: bool,
    pub executable_hash_match: bool,
    pub permissions_checked: bool,
    pub sandbox_active: bool,
    pub network_policy_enforced: bool,
    pub arguments_validated: bool,
    pub result_validated: bool,
    pub audit_entries: Vec<AuditEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: SystemTime,
    pub event: AuditEvent,
    pub details: String,
    pub severity: AuditSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEvent {
    PreflightCheck,
    ExecutionStart,
    PermissionCheck,
    IntegrityCheck,
    ArgumentValidation,
    ResourceLimitCheck,
    NetworkPolicyCheck,
    ResultValidation,
    ExecutionEnd,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditSeverity {
    Info,
    Warning,
    Critical,
    Violation,
}

/// Validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub score: f64, // 0.0 to 1.0
    pub issues: Vec<ValidationIssue>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: ValidationSeverity,
    pub code: String,
    pub message: String,
    pub field: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Adapter features
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdapterFeatures {
    pub supports_async: bool,
    pub supports_streaming: bool,
    pub supports_checkpointing: bool,
    pub supports_recovery: bool,
    pub supports_batching: bool,
    pub max_concurrent_executions: usize,
    pub supported_input_types: Vec<String>,
    pub supported_output_types: Vec<String>,
}

/// Resource requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub min_cpu_cores: f64,
    pub min_memory_mb: u64,
    pub min_disk_mb: u64,
    pub requires_gpu: bool,
    pub requires_network: bool,
    pub requires_sandbox: bool,
}

/// Contract errors
#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("Preflight check failed: {0}")]
    PreflightFailed(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    #[error("Security violation: {0}")]
    SecurityViolation(String),
    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Tool not available: {0}")]
    NotAvailable(String),
    #[error("Integrity check failed: {0}")]
    IntegrityCheckFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Integrity contract - must be verified before every execution
pub trait IntegrityContract {
    /// Verify tool identity
    fn verify_identity(&self) -> Result<IdentityVerification, ContractError>;
    
    /// Verify definition fingerprint
    fn verify_definition(&self, expected_hash: &str) -> Result<bool, ContractError>;
    
    /// Verify executable/image hash
    fn verify_executable(&self, expected_hash: &str) -> Result<bool, ContractError>;
    
    /// Verify dependency versions
    fn verify_dependencies(&self) -> Result<DependencyVerification, ContractError>;
    
    /// Verify provenance
    fn verify_provenance(&self) -> Result<ProvenanceVerification, ContractError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityVerification {
    pub verified: bool,
    pub tool_name: String,
    pub tool_version: String,
    pub instance_id: String,
    pub checks: Vec<IdentityCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityCheck {
    pub name: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyVerification {
    pub verified: bool,
    pub dependencies: Vec<DependencyCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyCheck {
    pub name: String,
    pub expected_version: String,
    pub actual_version: String,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceVerification {
    pub verified: bool,
    pub source: String,
    pub build_id: Option<String>,
    pub signature_verified: bool,
    pub checksums: HashMap<String, String>,
}

/// Measurement contract - all adapters must provide metrics
pub trait MeasurementContract {
    /// Record a metric
    fn record_metric(&self, name: &str, value: f64, labels: HashMap<String, String>);
    
    /// Record a histogram
    fn record_histogram(&self, name: &str, value: f64, labels: HashMap<String, String>);
    
    /// Get current metrics snapshot
    fn get_metrics(&self) -> MetricsSnapshot;
    
    /// Export metrics in Prometheus format
    fn export_prometheus(&self) -> String;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: SystemTime,
    pub counters: HashMap<String, u64>,
    pub gauges: HashMap<String, f64>,
    pub histograms: HashMap<String, HistogramData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramData {
    pub count: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub buckets: Vec<HistogramBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramBucket {
    pub upper_bound: f64,
    pub count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_status() {
        assert!(ResultStatus::Verified.is_success());
        assert!(ResultStatus::Passed.is_success());
        assert!(ResultStatus::Failed.is_failure());
        assert!(ResultStatus::CounterexampleFound.is_failure());
        assert!(ResultStatus::Unknown.is_inconclusive());
        assert!(ResultStatus::Timeout.is_inconclusive());
    }

    #[test]
    fn test_evidence_types() {
        assert_eq!(EvidenceType::Proof as u8, 0);
        assert_eq!(EvidenceType::Metric as u8, 5);
    }

    #[test]
    fn test_execution_budget_default() {
        let budget = ExecutionBudget::default();
        assert_eq!(budget.max_cpu_seconds, 60.0);
        assert_eq!(budget.max_memory_mb, 512);
    }

    #[test]
    fn test_security_audit() {
        let audit = SecurityAudit {
            execution_id: "test".to_string(),
            integrity_verified: true,
            definition_hash_match: true,
            executable_hash_match: true,
            permissions_checked: true,
            sandbox_active: true,
            network_policy_enforced: true,
            arguments_validated: true,
            result_validated: true,
            audit_entries: vec![],
        };
        assert!(audit.integrity_verified);
    }
}