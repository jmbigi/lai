//! core - Contratos y tipos base compartidos

pub mod contracts;

// Re-exports
pub use contracts::{
    ResultStatus, Evidence, EvidenceType, ExecutionMetrics, PreflightResult,
    IntegrityCheck, CheckSeverity, ResourceLimits, ExecutionRequest, ExecutionContext,
    VerificationLevel, ExecutionResult, HealthStatus, CapabilityAdapter, ValidationResult,
    ValidationIssue, ValidationSeverity, AdapterError, IntegrityContract, ToolIdentity,
    DependencyInfo, PolicyDecision,
};