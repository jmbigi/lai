/// Benchmark Module for lai
///
/// Provides standardized benchmarking framework for all capabilities:
/// - Mandatory contracts for adapters
/// - Benchmark runner with statistical analysis
/// - Baseline comparison
/// - Standard benchmark configurations
pub mod contracts;
pub mod framework;

pub use contracts::{
    AdapterFeatures, AuditEntry, AuditEvent, AuditSeverity, CapabilityAdapter, ContractError,
    DependencyCheck, DependencyVerification, Evidence, EvidenceSource, EvidenceType,
    ExecutionBudget, ExecutionContext, ExecutionMetrics, ExecutionRequest, ExecutionResult,
    HealthCheck, HealthCheckResult, HealthSeverity, HistogramBucket, HistogramData, IdentityCheck,
    IdentityVerification, IntegrityContract, MeasurementContract, MetricsSnapshot, NetworkPolicy,
    PreflightResult, ProvenanceVerification, ResourceEstimate, ResourceRequirements, ResultStatus,
    SecurityAudit, SecurityContext, ValidationIssue, ValidationResult, ValidationSeverity,
    VerificationStatus,
};

pub use framework::{
    standard_benchmarks, AggregateMetrics, BenchCommands, BenchmarkBaseline, BenchmarkComparison,
    BenchmarkConfig, BenchmarkEnvironment, BenchmarkError, BenchmarkResult, BenchmarkRunner,
    BenchmarkSuite, BenchmarkTestCase, ComparisonRecommendation, IterationResult, OutputFormat,
    SuccessCriteria, SuiteResult, TestCaseResult,
};
