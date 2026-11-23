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
    CapabilityAdapter, HealthCheckResult, HealthCheck, HealthSeverity,
    PreflightResult, ResourceEstimate, ExecutionRequest, ExecutionBudget,
    ExecutionContext, SecurityContext, NetworkPolicy, ExecutionResult,
    ResultStatus, Evidence, EvidenceType, EvidenceSource, ExecutionMetrics,
    VerificationStatus, SecurityAudit, AuditEntry, AuditEvent, AuditSeverity,
    ValidationResult, ValidationIssue, ValidationSeverity, AdapterFeatures,
    ResourceRequirements, ContractError, IntegrityContract, IdentityVerification,
    IdentityCheck, DependencyVerification, DependencyCheck, ProvenanceVerification,
    MeasurementContract, MetricsSnapshot, HistogramData, HistogramBucket,
};

pub use framework::{
    BenchmarkConfig, BenchmarkTestCase, BenchmarkBaseline, BenchmarkEnvironment,
    SuccessCriteria, OutputFormat, TestCaseResult, IterationResult,
    BenchmarkResult, AggregateMetrics, BenchmarkComparison, ComparisonRecommendation,
    BenchmarkRunner, BenchmarkError, BenchmarkSuite, SuiteResult,
    standard_benchmarks, BenchCommands,
};