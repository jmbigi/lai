//! Integration tests for benchmark framework

use std::sync::Arc;
use lai::benchmark::{
    framework::{BenchmarkConfig, BenchmarkRunner, standard_benchmarks, OutputFormat},
    contracts::*,
};
use std::time::SystemTime;

/// Mock adapter for testing benchmark framework
struct TestAdapter {
    capability_id: String,
    should_succeed: bool,
    latency_ms: u64,
}

impl TestAdapter {
    fn new(capability_id: String, should_succeed: bool, latency_ms: u64) -> Self {
        Self {
            capability_id,
            should_succeed,
            latency_ms,
        }
    }
}

#[async_trait::async_trait]
impl CapabilityAdapter for TestAdapter {
    fn capability_id(&self) -> String { self.capability_id.clone() }
    fn capability_name(&self) -> String { "Test Adapter".to_string() }
    fn adapter_version(&self) -> String { "1.0.0".to_string() }
    fn tool_version(&self) -> String { "1.0.0".to_string() }
    fn definition_hash(&self) -> String { "test_hash".to_string() }
    
    async fn health_check(&self) -> HealthCheckResult {
        HealthCheckResult {
            healthy: true,
            tool_version: "1.0.0".to_string(),
            definition_hash: "test_hash".to_string(),
            checks: vec![],
            timestamp: SystemTime::now(),
        }
    }
    
    async fn preflight(&self, request: &ExecutionRequest) -> Result<PreflightResult, ContractError> {
        Ok(PreflightResult {
            allowed: true,
            reason: None,
            warnings: vec![],
            estimated_resources: ResourceEstimate {
                estimated_cpu_seconds: 1.0,
                estimated_memory_mb: 100,
                estimated_network_mb: 0,
                estimated_tokens: 100,
            },
            required_permissions: vec![],
        })
    }
    
    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult, ContractError> {
        let start = std::time::Instant::now();
        // Simulate latency
        tokio::time::sleep(tokio::time::Duration::from_millis(self.latency_ms)).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        
        let execution_id = request.execution_id.clone();
        let capability_id = request.capability_id.clone();
        
        if self.should_succeed {
            let mut metrics = ExecutionMetrics::new(
                execution_id.clone(),
                capability_id.clone(),
                "1.0.0".to_string(),
                "test_hash".to_string(),
            );
            metrics.wall_time_ms = elapsed_ms;
            metrics.cpu_time_ms = elapsed_ms / 2;
            metrics.peak_memory_mb = 50;
            
            Ok(ExecutionResult {
                execution_id: execution_id.clone(),
                capability_id: capability_id.clone(),
                status: ResultStatus::Passed,
                output: serde_json::json!({"result": "ok"}),
                evidence: vec![],
                metrics,
                security_audit: SecurityAudit {
                    execution_id: execution_id.clone(),
                    integrity_verified: true,
                    definition_hash_match: true,
                    executable_hash_match: true,
                    permissions_checked: true,
                    sandbox_active: true,
                    network_policy_enforced: true,
                    arguments_validated: true,
                    result_validated: true,
                    audit_entries: vec![],
                },
                started_at: SystemTime::now(),
                completed_at: SystemTime::now(),
            })
        } else {
            Ok(ExecutionResult {
                execution_id: execution_id.clone(),
                capability_id: capability_id.clone(),
                status: ResultStatus::Failed,
                output: serde_json::json!({"error": "simulated failure"}),
                evidence: vec![],
                metrics: ExecutionMetrics::new(
                    execution_id.clone(),
                    capability_id.clone(),
                    "1.0.0".to_string(),
                    "test_hash".to_string(),
                ),
                security_audit: SecurityAudit {
                    execution_id: execution_id.clone(),
                    integrity_verified: true,
                    definition_hash_match: true,
                    executable_hash_match: true,
                    permissions_checked: true,
                    sandbox_active: true,
                    network_policy_enforced: true,
                    arguments_validated: true,
                    result_validated: true,
                    audit_entries: vec![],
                },
                started_at: SystemTime::now(),
                completed_at: SystemTime::now(),
            })
        }
    }
    
    async fn validate(&self, result: &ExecutionResult) -> ValidationResult {
        // A failed execution is still a valid benchmark result (it correctly failed)
        // Only mark invalid for unexpected errors
        let is_valid = result.status != ResultStatus::Error;
        
        ValidationResult {
            valid: is_valid,
            score: if result.status.is_success() { 1.0 } else { 0.0 },
            issues: if result.status == ResultStatus::Error {
                vec![ValidationIssue {
                    severity: ValidationSeverity::Error,
                    code: "EXECUTION_ERROR".to_string(),
                    message: "Internal error".to_string(),
                    field: None,
                }]
            } else {
                vec![]
            },
            recommendations: vec![],
        }
    }
    
    async fn get_metrics(&self, _execution_id: &str) -> Option<ExecutionMetrics> {
        None
    }
    
    fn supported_features(&self) -> AdapterFeatures {
        AdapterFeatures::default()
    }
    
    fn resource_requirements(&self) -> ResourceRequirements {
        ResourceRequirements {
            min_cpu_cores: 1.0,
            min_memory_mb: 100,
            min_disk_mb: 10,
            requires_gpu: false,
            requires_network: false,
            requires_sandbox: true,
        }
    }
}

#[tokio::test]
async fn test_benchmark_runner_success() {
    let config = standard_benchmarks::hybrid_rag_benchmark();
    let adapter = Arc::new(TestAdapter::new(config.capability_id.clone(), true, 5));
    let runner = BenchmarkRunner::new(config, adapter);
    
    let result = runner.run().await.expect("Benchmark should complete");
    
    assert!(result.success, "Benchmark should succeed: {}", result.success_details);
    assert_eq!(result.test_results.len(), 2); // exact_match + semantic_search
    assert!(result.aggregate_metrics.success_rate > 0.9);
    println!("Benchmark result: {} - {}", result.benchmark_name, result.success_details);
}

#[tokio::test]
async fn test_benchmark_runner_failure() {
    let config = standard_benchmarks::hybrid_rag_benchmark();
    let adapter = Arc::new(TestAdapter::new(config.capability_id.clone(), false, 5));
    let runner = BenchmarkRunner::new(config, adapter);
    
    let result = runner.run().await.expect("Benchmark should complete");
    
    assert!(!result.success, "Benchmark should fail");
    assert_eq!(result.aggregate_metrics.success_rate, 0.0);
}

#[tokio::test]
async fn test_benchmark_latency_measurement() {
    let mut config = standard_benchmarks::hybrid_rag_benchmark();
    config.iterations = 5;
    config.warmup_iterations = 1;
    
    // Use a longer simulated latency to ensure measurable results
    let adapter = Arc::new(TestAdapter::new(config.capability_id.clone(), true, 50));
    let runner = BenchmarkRunner::new(config, adapter);
    
    let result = runner.run().await.expect("Benchmark should complete");
    
    // Should have measurable latency
    assert!(result.aggregate_metrics.avg_latency_ms > 0.0);
    assert!(result.aggregate_metrics.p95_latency_ms >= result.aggregate_metrics.avg_latency_ms);
    println!("Avg latency: {}ms, P95: {}ms", 
        result.aggregate_metrics.avg_latency_ms, 
        result.aggregate_metrics.p95_latency_ms);
}

#[tokio::test]
async fn test_benchmark_output_formats() {
    // Test JSON export
    let config1 = {
        let mut c = standard_benchmarks::hybrid_rag_benchmark();
        c.iterations = 2;
        c.warmup_iterations = 0;
        c.output_format = OutputFormat::Json;
        c
    };
    let adapter1 = Arc::new(TestAdapter::new(config1.capability_id.clone(), true, 1));
    let runner1 = BenchmarkRunner::new(config1, adapter1);
    
    let result1 = runner1.run().await.expect("Benchmark should complete");
    
    // Test JSON export
    let json = serde_json::to_string_pretty(&result1).expect("JSON serialization");
    assert!(json.contains("benchmark_name"));
    assert!(json.contains("aggregate_metrics"));
    
    // Test CSV export
    let config2 = {
        let mut c = standard_benchmarks::hybrid_rag_benchmark();
        c.iterations = 2;
        c.warmup_iterations = 0;
        c.output_format = OutputFormat::Csv;
        c
    };
    let adapter2 = Arc::new(TestAdapter::new(config2.capability_id.clone(), true, 1));
    let runner2 = BenchmarkRunner::new(config2, adapter2);
    
    let result2 = runner2.run().await.expect("Benchmark should complete");
    
    // We can't easily test the export without file I/O, but we can verify the format method exists
    let csv = runner2.export_csv(&result2).expect("CSV export");
    assert!(csv.contains("test_id,status,latency_ms"));
    
    // Test Markdown export
    let config3 = {
        let mut c = standard_benchmarks::hybrid_rag_benchmark();
        c.iterations = 2;
        c.warmup_iterations = 0;
        c.output_format = OutputFormat::Markdown;
        c
    };
    let adapter3 = Arc::new(TestAdapter::new(config3.capability_id.clone(), true, 1));
    let runner3 = BenchmarkRunner::new(config3, adapter3);
    
    let result3 = runner3.run().await.expect("Benchmark should complete");
    
    let markdown = runner3.export_markdown(&result3).expect("Markdown export");
    assert!(markdown.contains("# Benchmark Results"));
    assert!(markdown.contains("| Metric | Value |"));
}

#[tokio::test]
async fn test_benchmark_suite() {
    let mut suite = lai::benchmark::framework::BenchmarkSuite::new("test-suite".to_string());
    
    let config1 = standard_benchmarks::hybrid_rag_benchmark();
    let adapter1 = Arc::new(TestAdapter::new(config1.capability_id.clone(), true, 5));
    suite.add_benchmark(BenchmarkRunner::new(config1, adapter1));
    
    let config2 = standard_benchmarks::joern_cpg_benchmark();
    let adapter2 = Arc::new(TestAdapter::new(config2.capability_id.clone(), true, 5));
    suite.add_benchmark(BenchmarkRunner::new(config2, adapter2));
    
    let suite_result = suite.run_all().await.expect("Suite should complete");
    
    assert_eq!(suite_result.benchmark_results.len(), 2);
    assert!(suite_result.overall_success);
    println!("Suite completed: {} benchmarks", suite_result.benchmark_results.len());
}

#[tokio::test]
async fn test_statistical_significance() {
    // Test that the statistical comparison works
    let mut config = standard_benchmarks::hybrid_rag_benchmark();
    config.iterations = 10;
    config.warmup_iterations = 2;
    config.success_criteria.require_statistical_significance = true;
    
    let adapter = Arc::new(TestAdapter::new(config.capability_id.clone(), true, 5));
    let runner = BenchmarkRunner::new(config, adapter);
    
    let result = runner.run().await.expect("Benchmark should complete");
    
    // Without baseline, comparison should be None
    assert!(result.comparison.is_none());
    
    // With a baseline that's worse, should recommend Adopt
    // (This would require setting up a baseline, which we skip for unit test)
}

#[tokio::test]
async fn test_standard_benchmarks_exist() {
    let benchmarks = standard_benchmarks::all_benchmarks();
    
    assert_eq!(benchmarks.len(), 3);
    
    let ids: Vec<&str> = benchmarks.iter().map(|b| b.capability_id.as_str()).collect();
    assert!(ids.contains(&"lai.retrieval.hybrid"));
    assert!(ids.contains(&"lai.code_analysis.cpg"));
    assert!(ids.contains(&"lai.testing.property"));
    
    for b in benchmarks {
        assert!(!b.name.is_empty());
        assert!(!b.capability_id.is_empty());
        assert!(!b.test_cases.is_empty());
        assert!(b.iterations > 0);
    }
}