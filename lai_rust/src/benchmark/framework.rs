/// Benchmark Framework for lai Capabilities
/// 
/// Provides standardized benchmarking for all capabilities to ensure
/// measurable improvements and reproducible comparisons.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::benchmark::contracts::{CapabilityAdapter, ExecutionRequest, ExecutionResult, ExecutionBudget, ExecutionContext, SecurityContext, ResultStatus, ResourceRequirements};

/// Benchmark configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub benchmark_id: String,
    pub name: String,
    pub description: String,
    pub capability_id: String,
    pub test_cases: Vec<BenchmarkTestCase>,
    pub baseline: Option<BenchmarkBaseline>,
    pub success_criteria: SuccessCriteria,
    pub resource_limits: ResourceRequirements,
    pub iterations: usize,
    pub warmup_iterations: usize,
    pub statistical_significance: f64, // p-value threshold
    pub output_format: OutputFormat,
    pub output_path: Option<PathBuf>,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            benchmark_id: Uuid::new_v4().to_string(),
            name: "Unnamed Benchmark".to_string(),
            description: String::new(),
            capability_id: String::new(),
            test_cases: Vec::new(),
            baseline: None,
            success_criteria: SuccessCriteria::default(),
            resource_limits: ResourceRequirements {
                min_cpu_cores: 1.0,
                min_memory_mb: 1024,
                min_disk_mb: 100,
                requires_gpu: false,
                requires_network: false,
                requires_sandbox: true,
            },
            iterations: 10,
            warmup_iterations: 2,
            statistical_significance: 0.05,
            output_format: OutputFormat::Json,
            output_path: None,
        }
    }
}

/// Individual test case for benchmarking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkTestCase {
    pub test_id: String,
    pub name: String,
    pub description: String,
    pub input: serde_json::Value,
    pub parameters: HashMap<String, serde_json::Value>,
    pub expected_status: Option<ResultStatus>,
    pub expected_output: Option<serde_json::Value>,
    pub timeout_seconds: u64,
    pub tags: Vec<String>,
}

/// Baseline for comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkBaseline {
    pub baseline_id: String,
    pub name: String,
    pub description: String,
    pub results: Vec<TestCaseResult>,
    pub created_at: SystemTime,
    pub tool_versions: HashMap<String, String>,
    pub environment: BenchmarkEnvironment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkEnvironment {
    pub os: String,
    pub cpu: String,
    pub memory_gb: u64,
    pub gpu: Option<String>,
    pub rust_version: String,
    pub tool_versions: HashMap<String, String>,
}

/// Success criteria for benchmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriteria {
    pub min_success_rate: f64, // 0.0 to 1.0
    pub max_avg_latency_ms: Option<u64>,
    pub max_p95_latency_ms: Option<u64>,
    pub max_memory_mb: Option<u64>,
    pub min_verification_rate: f64,
    pub max_error_rate: f64,
    pub require_statistical_significance: bool,
}

impl Default for SuccessCriteria {
    fn default() -> Self {
        Self {
            min_success_rate: 0.95,
            max_avg_latency_ms: Some(5000),
            max_p95_latency_ms: Some(10000),
            max_memory_mb: Some(1024),
            min_verification_rate: 0.8,
            max_error_rate: 0.05,
            require_statistical_significance: true,
        }
    }
}

/// Output format for benchmark results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFormat {
    Json,
    Csv,
    Html,
    Markdown,
    Prometheus,
}

/// Test case result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseResult {
    pub test_id: String,
    pub execution_id: String,
    pub status: ResultStatus,
    pub latency_ms: u64,
    pub cpu_time_ms: u64,
    pub peak_memory_mb: u64,
    pub tokens_used: u64,
    pub verification_status: VerificationStatus,
    pub error: Option<String>,
    pub output: Option<serde_json::Value>,
    pub evidence_count: usize,
    pub iterations: Vec<IterationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationResult {
    pub iteration: usize,
    pub latency_ms: u64,
    pub cpu_time_ms: u64,
    pub memory_mb: u64,
    pub status: ResultStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    Verified,
    PartiallyVerified,
    NotVerified,
    VerificationFailed,
    Unknown,
}

/// Aggregate benchmark result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub benchmark_id: String,
    pub benchmark_name: String,
    pub capability_id: String,
    pub capability_version: String,
    pub started_at: SystemTime,
    pub completed_at: SystemTime,
    pub total_duration_ms: u64,
    pub test_results: Vec<TestCaseResult>,
    pub aggregate_metrics: AggregateMetrics,
    pub comparison: Option<BenchmarkComparison>,
    pub success: bool,
    pub success_details: String,
    pub environment: BenchmarkEnvironment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateMetrics {
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub timeout_tests: usize,
    pub error_tests: usize,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub median_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub avg_cpu_time_ms: f64,
    pub avg_memory_mb: f64,
    pub peak_memory_mb: u64,
    pub total_tokens: u64,
    pub avg_tokens_per_test: f64,
    pub verification_rate: f64,
    pub error_rate: f64,
    pub std_dev_latency_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkComparison {
    pub baseline_id: String,
    pub baseline_name: String,
    pub latency_change_pct: f64,
    pub memory_change_pct: f64,
    pub success_rate_change_pct: f64,
    pub verification_rate_change_pct: f64,
    pub statistically_significant: bool,
    pub p_value: Option<f64>,
    pub effect_size: Option<f64>,
    pub recommendation: ComparisonRecommendation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonRecommendation {
    Adopt,        // Significant improvement
    Consider,     // Improvement but not significant
    Reject,       // No improvement or regression
    Investigate,  // Mixed results
}

/// Benchmark runner
pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    adapter: Arc<dyn CapabilityAdapter>,
    results: Arc<RwLock<Vec<TestCaseResult>>>,
}

impl BenchmarkRunner {
    pub fn new(config: BenchmarkConfig, adapter: Arc<dyn CapabilityAdapter>) -> Self {
        Self {
            config,
            adapter,
            results: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Run the benchmark
    pub async fn run(&self) -> Result<BenchmarkResult, BenchmarkError> {
        let start_time = Instant::now();
        let started_at = SystemTime::now();

        // Warmup runs
        for test_case in &self.config.test_cases {
            for _ in 0..self.config.warmup_iterations {
                let _ = self.run_single_test(test_case).await;
            }
        }

        // Actual benchmark runs
        let mut all_results = Vec::new();
        
        for test_case in &self.config.test_cases {
            let mut test_results = Vec::new();
            
            for iteration in 0..self.config.iterations {
                let result = self.run_single_test(test_case).await?;
                test_results.push(IterationResult {
                    iteration,
                    latency_ms: result.metrics.wall_time_ms,
                    cpu_time_ms: result.metrics.cpu_time_ms,
                    memory_mb: result.metrics.peak_memory_mb,
                    status: result.status,
                    error: if result.status == ResultStatus::Error || result.status == ResultStatus::Failed {
                        result.output.get("error").and_then(|v| v.as_str()).map(|s| s.to_string())
                    } else {
                        None
                    },
                });
            }
            
            // Aggregate iterations for this test case
            let aggregated = self.aggregate_iterations(test_case, &test_results);
            all_results.push(aggregated);
        }

        // Store results
        *self.results.write().await = all_results.clone();

        let completed_at = SystemTime::now();
        let total_duration = start_time.elapsed();

        // Calculate aggregate metrics
        let aggregate = self.calculate_aggregate(&all_results);

        // Compare with baseline if provided
        let comparison = if let Some(baseline) = &self.config.baseline {
            Some(self.compare_with_baseline(&aggregate, baseline)?)
        } else {
            None
        };

        // Evaluate success criteria
        let (success, success_details) = self.evaluate_success(&aggregate, comparison.as_ref());

        let result = BenchmarkResult {
            benchmark_id: self.config.benchmark_id.clone(),
            benchmark_name: self.config.name.clone(),
            capability_id: self.config.capability_id.clone(),
            capability_version: self.adapter.tool_version(),
            started_at,
            completed_at,
            total_duration_ms: total_duration.as_millis() as u64,
            test_results: all_results,
            aggregate_metrics: aggregate,
            comparison,
            success,
            success_details,
            environment: self.capture_environment(),
        };

        // Export results
        if let Some(output_path) = &self.config.output_path {
            self.export_results(&result, output_path).await?;
        }

        Ok(result)
    }

    /// Run a single test case
    async fn run_single_test(&self, test_case: &BenchmarkTestCase) -> Result<ExecutionResult, BenchmarkError> {
        let execution_id = Uuid::new_v4().to_string();
        
        let request = ExecutionRequest {
            execution_id: execution_id.clone(),
            task_id: format!("benchmark-{}", test_case.test_id),
            capability_id: self.config.capability_id.clone(),
            input: test_case.input.clone(),
            parameters: test_case.parameters.clone(),
            budget: ExecutionBudget {
                max_cpu_seconds: test_case.timeout_seconds as f64,
                max_wall_time_seconds: test_case.timeout_seconds,
                ..Default::default()
            },
            context: ExecutionContext {
                project_root: PathBuf::from("."),
                working_directory: PathBuf::from("."),
                environment: HashMap::new(),
                previous_results: HashMap::new(),
            },
            security_context: SecurityContext {
                allowed_paths: vec![],
                denied_paths: vec![],
                allowed_commands: vec![],
                network_policy: crate::benchmark::contracts::NetworkPolicy::None,
                sandbox_enabled: true,
            },
        };

        // Preflight check
        let preflight = self.adapter.preflight(&request).await
            .map_err(|e| BenchmarkError::PreflightFailed(e.to_string()))?;
        
        if !preflight.allowed {
            return Err(BenchmarkError::PreflightFailed(
                preflight.reason.unwrap_or_else(|| "Preflight check failed".to_string())
            ));
        }

        // Execute with timeout
        let execution_future = self.adapter.execute(request);
        let timeout_duration = Duration::from_secs(test_case.timeout_seconds);
        
        let result = tokio::time::timeout(timeout_duration, execution_future).await
            .map_err(|_| BenchmarkError::Timeout(test_case.test_id.clone()))?
            .map_err(|e| BenchmarkError::ExecutionFailed(e.to_string()))?;

        // Validate result
        let validation = self.adapter.validate(&result).await;
        if !validation.valid {
            return Err(BenchmarkError::ValidationFailed(
                validation.issues.iter().map(|i| i.message.clone()).collect::<Vec<_>>().join(", ")
            ));
        }

        Ok(result)
    }

    /// Aggregate multiple iterations
    fn aggregate_iterations(&self, test_case: &BenchmarkTestCase, iterations: &[IterationResult]) -> TestCaseResult {
        let successful: Vec<_> = iterations.iter().filter(|i| i.status.is_success()).collect();
        let failed: Vec<_> = iterations.iter().filter(|i| i.status.is_failure()).collect();
        let timeouts: Vec<_> = iterations.iter().filter(|i| i.status == ResultStatus::Timeout).collect();
        let errors: Vec<_> = iterations.iter().filter(|i| i.status == ResultStatus::Error).collect();

        let latencies: Vec<u64> = successful.iter().map(|i| i.latency_ms).collect();
        let avg_latency = if latencies.is_empty() { 0 } else { latencies.iter().sum::<u64>() / latencies.len() as u64 };
        
        let sorted_latencies = {
            let mut l = latencies.clone();
            l.sort();
            l
        };
        
        let median_latency = if sorted_latencies.is_empty() { 0 } else { sorted_latencies[sorted_latencies.len() / 2] };
        let p95_idx = (sorted_latencies.len() as f64 * 0.95).ceil() as usize;
        let p95_latency = if sorted_latencies.is_empty() { 0 } else { sorted_latencies[p95_idx.min(sorted_latencies.len() - 1)] };
        let p99_idx = (sorted_latencies.len() as f64 * 0.99).ceil() as usize;
        let p99_latency = if sorted_latencies.is_empty() { 0 } else { sorted_latencies[p99_idx.min(sorted_latencies.len() - 1)] };

        let cpu_times: Vec<u64> = successful.iter().map(|i| i.cpu_time_ms).collect();
        let avg_cpu_time = if cpu_times.is_empty() { 0 } else { cpu_times.iter().sum::<u64>() / cpu_times.len() as u64 };
        
        let memories: Vec<u64> = successful.iter().map(|i| i.memory_mb).collect();
        let avg_memory = if memories.is_empty() { 0 } else { memories.iter().sum::<u64>() / memories.len() as u64 };
        let peak_memory = memories.iter().max().copied().unwrap_or(0);

        // Calculate std dev
        let std_dev = if latencies.len() > 1 {
            let mean = avg_latency as f64;
            let variance = latencies.iter()
                .map(|&x| (x as f64 - mean).powi(2))
                .sum::<f64>() / (latencies.len() - 1) as f64;
            variance.sqrt()
        } else { 0.0 };

        TestCaseResult {
            test_id: test_case.test_id.clone(),
            execution_id: Uuid::new_v4().to_string(),
            status: if successful.len() == iterations.len() { ResultStatus::Passed } 
                    else if failed.len() > 0 { ResultStatus::Failed }
                    else if timeouts.len() > 0 { ResultStatus::Timeout }
                    else { ResultStatus::Error },
            latency_ms: avg_latency,
            cpu_time_ms: avg_cpu_time,
            peak_memory_mb: peak_memory,
            tokens_used: 0,
            verification_status: VerificationStatus::Unknown,
            error: errors.first().and_then(|e| e.error.clone()),
            output: None,
            evidence_count: 0,
            iterations: iterations.to_vec(),
        }
    }

    /// Calculate aggregate metrics
    fn calculate_aggregate(&self, results: &[TestCaseResult]) -> AggregateMetrics {
        let total = results.len();
        let passed = results.iter().filter(|r| r.status.is_success()).count();
        let failed = results.iter().filter(|r| r.status.is_failure()).count();
        let timeouts = results.iter().filter(|r| r.status == ResultStatus::Timeout).count();
        let errors = results.iter().filter(|r| r.status == ResultStatus::Error).count();

        let latencies: Vec<u64> = results.iter().map(|r| r.latency_ms).collect();
        let avg_latency = latencies.iter().sum::<u64>() as f64 / total.max(1) as f64;
        
        let mut sorted_latencies = latencies.clone();
        sorted_latencies.sort();
        let median = if sorted_latencies.is_empty() { 0 } else { sorted_latencies[sorted_latencies.len() / 2] };
        let p95_idx = (sorted_latencies.len() as f64 * 0.95).ceil() as usize;
        let p95 = if sorted_latencies.is_empty() { 0 } else { sorted_latencies[p95_idx.min(sorted_latencies.len() - 1)] };
        let p99_idx = (sorted_latencies.len() as f64 * 0.99).ceil() as usize;
        let p99 = if sorted_latencies.is_empty() { 0 } else { sorted_latencies[p99_idx.min(sorted_latencies.len() - 1)] };

        let cpu_times: Vec<u64> = results.iter().map(|r| r.cpu_time_ms).collect();
        let avg_cpu = cpu_times.iter().sum::<u64>() as f64 / total.max(1) as f64;

        let memories: Vec<u64> = results.iter().map(|r| r.peak_memory_mb).collect();
        let avg_mem = memories.iter().sum::<u64>() as f64 / total.max(1) as f64;
        let peak_mem = memories.iter().max().copied().unwrap_or(0);

        let total_tokens: u64 = results.iter().map(|r| r.tokens_used).sum();

        // Std dev
        let std_dev = if total > 1 {
            let variance = latencies.iter()
                .map(|&x| (x as f64 - avg_latency).powi(2))
                .sum::<f64>() / (total - 1) as f64;
            variance.sqrt()
        } else { 0.0 };

        AggregateMetrics {
            total_tests: total,
            passed_tests: passed,
            failed_tests: failed,
            timeout_tests: timeouts,
            error_tests: errors,
            success_rate: passed as f64 / total.max(1) as f64,
            avg_latency_ms: avg_latency,
            median_latency_ms: median as f64,
            p95_latency_ms: p95 as f64,
            p99_latency_ms: p99 as f64,
            avg_cpu_time_ms: avg_cpu,
            avg_memory_mb: avg_mem,
            peak_memory_mb: peak_mem,
            total_tokens,
            avg_tokens_per_test: total_tokens as f64 / total.max(1) as f64,
            verification_rate: 0.0,
            error_rate: (failed + errors) as f64 / total.max(1) as f64,
            std_dev_latency_ms: std_dev,
        }
    }

    /// Compare with baseline
    fn compare_with_baseline(&self, current: &AggregateMetrics, baseline: &BenchmarkBaseline) -> Result<BenchmarkComparison, BenchmarkError> {
        let baseline_results = &baseline.results;
        
        if baseline_results.is_empty() {
            return Err(BenchmarkError::BaselineEmpty);
        }

        let baseline_success_rate = baseline_results.iter().filter(|r| r.status.is_success()).count() as f64 / baseline_results.len().max(1) as f64;
        let baseline_avg_latency = baseline_results.iter().map(|r| r.latency_ms).sum::<u64>() as f64 / baseline_results.len().max(1) as f64;
        let baseline_avg_memory = baseline_results.iter().map(|r| r.peak_memory_mb).sum::<u64>() as f64 / baseline_results.len().max(1) as f64;
        
        let latency_change = ((current.avg_latency_ms - baseline_avg_latency) / baseline_avg_latency.max(0.001)) * 100.0;
        let memory_change = ((current.avg_memory_mb - baseline_avg_memory) / baseline_avg_memory.max(0.001)) * 100.0;
        let success_rate_change = ((current.success_rate - baseline_success_rate) / baseline_success_rate.max(0.001)) * 100.0;
        
        // Simple t-test for statistical significance (simplified)
        let p_value = self.calculate_p_value(current, baseline_results);
        let statistically_significant = p_value < self.config.statistical_significance;
        
        // Effect size (Cohen's d)
        let effect_size = if current.std_dev_latency_ms > 0.0 {
            (current.avg_latency_ms - baseline_avg_latency) / current.std_dev_latency_ms
        } else { 0.0 };

        let recommendation = if statistically_significant && latency_change < 0.0 && success_rate_change >= 0.0 {
            ComparisonRecommendation::Adopt
        } else if latency_change < 0.0 && success_rate_change >= 0.0 {
            ComparisonRecommendation::Consider
        } else if latency_change > 10.0 || success_rate_change < -5.0 {
            ComparisonRecommendation::Reject
        } else {
            ComparisonRecommendation::Investigate
        };

        Ok(BenchmarkComparison {
            baseline_id: baseline.baseline_id.clone(),
            baseline_name: baseline.name.clone(),
            latency_change_pct: latency_change,
            memory_change_pct: memory_change,
            success_rate_change_pct: success_rate_change,
            verification_rate_change_pct: 0.0,
            statistically_significant,
            p_value: Some(p_value),
            effect_size: Some(effect_size),
            recommendation,
        })
    }

    /// Simplified p-value calculation (Welch's t-test approximation)
    fn calculate_p_value(&self, current: &AggregateMetrics, baseline_results: &[TestCaseResult]) -> f64 {
        let baseline_latencies: Vec<f64> = baseline_results.iter().map(|r| r.latency_ms as f64).collect();
        let current_latencies: Vec<f64> = self.results.blocking_read().iter()
            .flat_map(|r| r.iterations.iter().map(|i| i.latency_ms as f64))
            .collect();
        
        if baseline_latencies.is_empty() || current_latencies.is_empty() {
            return 1.0;
        }

        let baseline_mean = baseline_latencies.iter().sum::<f64>() / baseline_latencies.len() as f64;
        let current_mean = current_latencies.iter().sum::<f64>() / current_latencies.len() as f64;
        
        let baseline_var = baseline_latencies.iter()
            .map(|&x| (x - baseline_mean).powi(2))
            .sum::<f64>() / (baseline_latencies.len() - 1).max(1) as f64;
        let current_var = current_latencies.iter()
            .map(|&x| (x - current_mean).powi(2))
            .sum::<f64>() / (current_latencies.len() - 1).max(1) as f64;

        // Welch's t-test
        let t = (current_mean - baseline_mean) / 
            ((baseline_var / baseline_latencies.len() as f64 + current_var / current_latencies.len() as f64).sqrt());
        
        // Approximate p-value from t-statistic
        if t.abs() > 2.0 { 0.05 } else if t.abs() > 1.5 { 0.15 } else { 0.5 }
    }

    /// Evaluate success criteria
    fn evaluate_success(&self, aggregate: &AggregateMetrics, comparison: Option<&BenchmarkComparison>) -> (bool, String) {
        let mut checks = Vec::new();
        let mut details = Vec::new();

        // Success rate check
        if aggregate.success_rate >= self.config.success_criteria.min_success_rate {
            checks.push(true);
            details.push(format!("Success rate: {:.1}% (threshold: {:.1}%)", 
                aggregate.success_rate * 100.0, self.config.success_criteria.min_success_rate * 100.0));
        } else {
            checks.push(false);
            details.push(format!("FAIL: Success rate: {:.1}% (threshold: {:.1}%)", 
                aggregate.success_rate * 100.0, self.config.success_criteria.min_success_rate * 100.0));
        }

        // Latency checks
        if let Some(max_avg) = self.config.success_criteria.max_avg_latency_ms {
            if aggregate.avg_latency_ms <= max_avg as f64 {
                checks.push(true);
                details.push(format!("Avg latency: {:.0}ms (threshold: {}ms)", aggregate.avg_latency_ms, max_avg));
            } else {
                checks.push(false);
                details.push(format!("FAIL: Avg latency: {:.0}ms (threshold: {}ms)", aggregate.avg_latency_ms, max_avg));
            }
        }

        if let Some(max_p95) = self.config.success_criteria.max_p95_latency_ms {
            if aggregate.p95_latency_ms <= max_p95 as f64 {
                checks.push(true);
                details.push(format!("P95 latency: {:.0}ms (threshold: {}ms)", aggregate.p95_latency_ms, max_p95));
            } else {
                checks.push(false);
                details.push(format!("FAIL: P95 latency: {:.0}ms (threshold: {}ms)", aggregate.p95_latency_ms, max_p95));
            }
        }

        // Memory check
        if let Some(max_mem) = self.config.success_criteria.max_memory_mb {
            if aggregate.avg_memory_mb <= max_mem as f64 {
                checks.push(true);
                details.push(format!("Avg memory: {:.0}MB (threshold: {}MB)", aggregate.avg_memory_mb, max_mem));
            } else {
                checks.push(false);
                details.push(format!("FAIL: Avg memory: {:.0}MB (threshold: {}MB)", aggregate.avg_memory_mb, max_mem));
            }
        }

        // Error rate check
        if aggregate.error_rate <= self.config.success_criteria.max_error_rate {
            checks.push(true);
            details.push(format!("Error rate: {:.1}% (threshold: {:.1}%)", 
                aggregate.error_rate * 100.0, self.config.success_criteria.max_error_rate * 100.0));
        } else {
            checks.push(false);
            details.push(format!("FAIL: Error rate: {:.1}% (threshold: {:.1}%)", 
                aggregate.error_rate * 100.0, self.config.success_criteria.max_error_rate * 100.0));
        }

        // Statistical significance check
        if self.config.success_criteria.require_statistical_significance {
            if let Some(comp) = comparison {
                if comp.statistically_significant {
                    checks.push(true);
                    details.push("Statistical significance: PASSED".to_string());
                } else {
                    checks.push(false);
                    details.push("FAIL: Not statistically significant".to_string());
                }
            }
        }

        let all_passed = checks.iter().all(|&c| c);
        let success_details = details.join("; ");

        (all_passed, success_details)
    }

    /// Capture environment info
    fn capture_environment(&self) -> BenchmarkEnvironment {
        BenchmarkEnvironment {
            os: std::env::consts::OS.to_string(),
            cpu: "unknown".to_string(),
            memory_gb: 0,
            gpu: None,
            rust_version: env!("CARGO_PKG_VERSION").to_string(),
            tool_versions: HashMap::new(),
        }
    }

    /// Export results
    async fn export_results(&self, result: &BenchmarkResult, path: &PathBuf) -> Result<(), BenchmarkError> {
        let content = match self.config.output_format {
            OutputFormat::Json => serde_json::to_string_pretty(result)?,
            OutputFormat::Csv => self.export_csv(result)?,
            OutputFormat::Markdown => self.export_markdown(result)?,
            OutputFormat::Html => self.export_html(result)?,
            OutputFormat::Prometheus => self.export_prometheus(result)?,
        };

        tokio::fs::write(path, content).await
            .map_err(|e| BenchmarkError::ExportFailed(e.to_string()))?;

        Ok(())
    }

    fn export_csv(&self, result: &BenchmarkResult) -> Result<String, BenchmarkError> {
        let mut csv = String::new();
        csv.push_str("test_id,status,latency_ms,cpu_time_ms,memory_mb,tokens,success\n");
        
        for test in &result.test_results {
            csv.push_str(&format!(
                "{},{:?},{},{},{},{},{}\n",
                test.test_id, test.status, test.latency_ms, test.cpu_time_ms,
                test.peak_memory_mb, test.tokens_used, test.status.is_success()
            ));
        }
        
        Ok(csv)
    }

    fn export_markdown(&self, result: &BenchmarkResult) -> Result<String, BenchmarkError> {
        let mut md = String::new();
        md.push_str(&format!("# Benchmark Results: {}\n\n", result.benchmark_name));
        md.push_str(&format!("**Capability**: {} v{}  \n", result.capability_id, result.capability_version));
        md.push_str(&format!("**Date**: {:?}  \n", result.started_at));
        md.push_str(&format!("**Duration**: {}ms  \n\n", result.total_duration_ms));
        
        md.push_str("## Aggregate Metrics\n\n");
        md.push_str(&format!("| Metric | Value |\n"));
        md.push_str("|--------|-------|\n");
        md.push_str(&format!("| Success Rate | {:.1}% |\n", result.aggregate_metrics.success_rate * 100.0));
        md.push_str(&format!("| Avg Latency | {:.0}ms |\n", result.aggregate_metrics.avg_latency_ms));
        md.push_str(&format!("| P95 Latency | {:.0}ms |\n", result.aggregate_metrics.p95_latency_ms));
        md.push_str(&format!("| P99 Latency | {:.0}ms |\n", result.aggregate_metrics.p99_latency_ms));
        md.push_str(&format!("| Avg Memory | {:.0}MB |\n", result.aggregate_metrics.avg_memory_mb));
        md.push_str(&format!("| Peak Memory | {}MB |\n", result.aggregate_metrics.peak_memory_mb));
        md.push_str(&format!("| Error Rate | {:.1}% |\n", result.aggregate_metrics.error_rate * 100.0));
        md.push_str(&format!("| Std Dev Latency | {:.0}ms |\n\n", result.aggregate_metrics.std_dev_latency_ms));
        
        if let Some(comp) = &result.comparison {
            md.push_str("## Comparison with Baseline\n\n");
            md.push_str(&format!("**Baseline**: {}  \n", comp.baseline_name));
            md.push_str(&format!("| Metric | Change |\n"));
            md.push_str("|--------|--------|\n");
            md.push_str(&format!("| Latency | {:+.1}% |\n", comp.latency_change_pct));
            md.push_str(&format!("| Memory | {:+.1}% |\n", comp.memory_change_pct));
            md.push_str(&format!("| Success Rate | {:+.1}% |\n", comp.success_rate_change_pct));
            md.push_str(&format!("| Statistically Significant | {} |\n", comp.statistically_significant));
            md.push_str(&format!("| P-value | {:.4} |\n", comp.p_value.unwrap_or(0.0)));
            md.push_str(&format!("| Effect Size | {:.2} |\n", comp.effect_size.unwrap_or(0.0)));
            md.push_str(&format!("| Recommendation | {:?} |\n\n", comp.recommendation));
        }
        
        md.push_str("## Test Results\n\n");
        md.push_str("| Test | Status | Latency (ms) | Memory (MB) |\n");
        md.push_str("|------|--------|--------------|-------------|\n");
        
        for test in &result.test_results {
            md.push_str(&format!("| {} | {:?} | {} | {} |\n", 
                test.test_id, test.status, test.latency_ms, test.peak_memory_mb));
        }
        
        Ok(md)
    }

    fn export_html(&self, result: &BenchmarkResult) -> Result<String, BenchmarkError> {
        let markdown = self.export_markdown(result)?;
        // Simple HTML conversion
        let html = markdown
            .replace("\n", "<br>")
            .replace("# ", "<h1>")
            .replace("## ", "<h2>")
            .replace("| ", "<td>")
            .replace(" |", "</td>");
        Ok(format!("<html><body>{}</body></html>", html))
    }

    fn export_prometheus(&self, result: &BenchmarkResult) -> Result<String, BenchmarkError> {
        let mut prom = String::new();
        let m = &result.aggregate_metrics;
        
        prom.push_str(&format!("lai_benchmark_success_rate {{capability=\"{}\"}} {}\n", 
            result.capability_id, m.success_rate));
        prom.push_str(&format!("lai_benchmark_avg_latency_ms {{capability=\"{}\"}} {}\n", 
            result.capability_id, m.avg_latency_ms));
        prom.push_str(&format!("lai_benchmark_p95_latency_ms {{capability=\"{}\"}} {}\n", 
            result.capability_id, m.p95_latency_ms));
        prom.push_str(&format!("lai_benchmark_p99_latency_ms {{capability=\"{}\"}} {}\n", 
            result.capability_id, m.p99_latency_ms));
        prom.push_str(&format!("lai_benchmark_avg_memory_mb {{capability=\"{}\"}} {}\n", 
            result.capability_id, m.avg_memory_mb));
        prom.push_str(&format!("lai_benchmark_peak_memory_mb {{capability=\"{}\"}} {}\n", 
            result.capability_id, m.peak_memory_mb));
        prom.push_str(&format!("lai_benchmark_error_rate {{capability=\"{}\"}} {}\n", 
            result.capability_id, m.error_rate));
        prom.push_str(&format!("lai_benchmark_std_dev_latency_ms {{capability=\"{}\"}} {}\n", 
            result.capability_id, m.std_dev_latency_ms));
        
        Ok(prom)
    }
}

/// Benchmark errors
#[derive(Debug, thiserror::Error)]
pub enum BenchmarkError {
    #[error("Preflight failed: {0}")]
    PreflightFailed(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    #[error("Timeout on test: {0}")]
    Timeout(String),
    #[error("Baseline is empty")]
    BaselineEmpty,
    #[error("Export failed: {0}")]
    ExportFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Benchmark suite for multiple capabilities
pub struct BenchmarkSuite {
    benchmarks: Vec<BenchmarkRunner>,
    suite_id: String,
    name: String,
}

impl BenchmarkSuite {
    pub fn new(name: String) -> Self {
        Self {
            benchmarks: Vec::new(),
            suite_id: Uuid::new_v4().to_string(),
            name,
        }
    }

    pub fn add_benchmark(&mut self, runner: BenchmarkRunner) {
        self.benchmarks.push(runner);
    }

    pub async fn run_all(&self) -> Result<SuiteResult, BenchmarkError> {
        let start_time = Instant::now();
        let mut results = Vec::new();

        for benchmark in &self.benchmarks {
            let result = benchmark.run().await?;
            results.push(result);
        }

        let suite_result = SuiteResult {
            suite_id: self.suite_id.clone(),
            suite_name: self.name.clone(),
            started_at: SystemTime::now(),
            completed_at: SystemTime::now(),
            total_duration_ms: start_time.elapsed().as_millis() as u64,
            benchmark_results: results,
            overall_success: true,
        };

        Ok(suite_result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteResult {
    pub suite_id: String,
    pub suite_name: String,
    pub started_at: SystemTime,
    pub completed_at: SystemTime,
    pub total_duration_ms: u64,
    pub benchmark_results: Vec<BenchmarkResult>,
    pub overall_success: bool,
}

/// Predefined benchmark configurations for lai capabilities
pub mod standard_benchmarks {
    use super::*;

    /// Create benchmark config for Hybrid RAG retrieval
    pub fn hybrid_rag_benchmark() -> BenchmarkConfig {
        BenchmarkConfig {
            benchmark_id: "hybrid-rag-retrieval".to_string(),
            name: "Hybrid RAG Retrieval Benchmark".to_string(),
            description: "Measures retrieval quality and latency for hybrid lexical+semantic search".to_string(),
            capability_id: "lai.retrieval.hybrid".to_string(),
            test_cases: vec![
                BenchmarkTestCase {
                    test_id: "exact_match".to_string(),
                    name: "Exact Symbol Match".to_string(),
                    description: "Retrieve exact function/class names".to_string(),
                    input: serde_json::json!({"query": "DafnyVerifier", "type": "exact"}),
                    parameters: HashMap::new(),
                    expected_status: Some(ResultStatus::Passed),
                    expected_output: None,
                    timeout_seconds: 10,
                    tags: vec!["exact".to_string(), "symbols".to_string()],
                },
                BenchmarkTestCase {
                    test_id: "semantic_search".to_string(),
                    name: "Semantic Search".to_string(),
                    description: "Retrieve semantically related code".to_string(),
                    input: serde_json::json!({"query": "formal verification of neural networks", "type": "semantic"}),
                    parameters: HashMap::new(),
                    expected_status: Some(ResultStatus::Passed),
                    expected_output: None,
                    timeout_seconds: 30,
                    tags: vec!["semantic".to_string(), "concepts".to_string()],
                },
            ],
            baseline: None,
            success_criteria: SuccessCriteria {
                min_success_rate: 0.95,
                max_avg_latency_ms: Some(100),
                max_p95_latency_ms: Some(500),
                max_memory_mb: Some(256),
                min_verification_rate: 0.8,
                max_error_rate: 0.02,
                require_statistical_significance: true,
            },
            iterations: 20,
            warmup_iterations: 3,
            ..Default::default()
        }
    }

    /// Create benchmark config for Joern CPG analysis
    pub fn joern_cpg_benchmark() -> BenchmarkConfig {
        BenchmarkConfig {
            benchmark_id: "joern-cpg-analysis".to_string(),
            name: "Joern CPG Analysis Benchmark".to_string(),
            description: "Measures code property graph generation and query performance".to_string(),
            capability_id: "lai.code_analysis.cpg".to_string(),
            test_cases: vec![
                BenchmarkTestCase {
                    test_id: "cpg_generation".to_string(),
                    name: "CPG Generation".to_string(),
                    description: "Generate CPG for a Rust project".to_string(),
                    input: serde_json::json!({"project_path": ".", "language": "rust"}),
                    parameters: HashMap::new(),
                    expected_status: Some(ResultStatus::Passed),
                    expected_output: None,
                    timeout_seconds: 120,
                    tags: vec!["cpg".to_string(), "generation".to_string()],
                },
                BenchmarkTestCase {
                    test_id: "vulnerability_query".to_string(),
                    name: "Vulnerability Query".to_string(),
                    description: "Run SQL injection detection query".to_string(),
                    input: serde_json::json!({"query": "sql_injection", "cpg": "test"}),
                    parameters: HashMap::new(),
                    expected_status: Some(ResultStatus::Passed),
                    expected_output: None,
                    timeout_seconds: 30,
                    tags: vec!["vulnerability".to_string(), "security".to_string()],
                },
            ],
            baseline: None,
            success_criteria: SuccessCriteria {
                min_success_rate: 0.9,
                max_avg_latency_ms: Some(5000),
                max_p95_latency_ms: Some(15000),
                max_memory_mb: Some(2048),
                min_verification_rate: 0.7,
                max_error_rate: 0.05,
                require_statistical_significance: true,
            },
            iterations: 10,
            warmup_iterations: 2,
            ..Default::default()
        }
    }

    /// Create benchmark config for Hypothesis property testing
    pub fn hypothesis_testing_benchmark() -> BenchmarkConfig {
        BenchmarkConfig {
            benchmark_id: "hypothesis-property-testing".to_string(),
            name: "Hypothesis Property Testing Benchmark".to_string(),
            description: "Measures property-based testing effectiveness and performance".to_string(),
            capability_id: "lai.testing.property".to_string(),
            test_cases: vec![
                BenchmarkTestCase {
                    test_id: "idempotence_test".to_string(),
                    name: "Idempotence Property".to_string(),
                    description: "Test f(f(x)) == f(x) for normalizer".to_string(),
                    input: serde_json::json!({"property": "idempotence", "function": "normalize_path"}),
                    parameters: HashMap::new(),
                    expected_status: Some(ResultStatus::Passed),
                    expected_output: None,
                    timeout_seconds: 60,
                    tags: vec!["property".to_string(), "idempotence".to_string()],
                },
            ],
            baseline: None,
            success_criteria: SuccessCriteria {
                min_success_rate: 0.95,
                max_avg_latency_ms: Some(10000),
                max_p95_latency_ms: Some(30000),
                max_memory_mb: Some(512),
                min_verification_rate: 0.9,
                max_error_rate: 0.01,
                require_statistical_significance: true,
            },
            iterations: 15,
            warmup_iterations: 2,
            ..Default::default()
        }
    }

    /// Get all standard benchmarks
    pub fn all_benchmarks() -> Vec<BenchmarkConfig> {
        vec![
            hybrid_rag_benchmark(),
            joern_cpg_benchmark(),
            hypothesis_testing_benchmark(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_config_default() {
        let config = BenchmarkConfig::default();
        assert_eq!(config.iterations, 10);
        assert_eq!(config.warmup_iterations, 2);
        assert_eq!(config.statistical_significance, 0.05);
    }

    #[test]
    fn test_success_criteria_default() {
        let criteria = SuccessCriteria::default();
        assert_eq!(criteria.min_success_rate, 0.95);
        assert_eq!(criteria.max_error_rate, 0.05);
    }

    #[test]
    fn test_standard_benchmarks() {
        let benchmarks = standard_benchmarks::all_benchmarks();
        assert_eq!(benchmarks.len(), 3);
        assert!(benchmarks.iter().any(|b| b.capability_id == "lai.retrieval.hybrid"));
        assert!(benchmarks.iter().any(|b| b.capability_id == "lai.code_analysis.cpg"));
        assert!(benchmarks.iter().any(|b| b.capability_id == "lai.testing.property"));
    }
}