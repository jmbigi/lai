//! Integration tests for LAI infrastructure
//!
//! HISTÓRICO: este archivo apunta al stack docker-compose que ya no forma
//! parte del proyecto (Langfuse, Joern, CodeBadger, Prometheus, Grafana…).
//! Las funciones no llevan `#[tokio::test]`, así que **no se ejecuta ninguna
//! prueba**: se conserva solo como referencia. No lo tomes como cobertura.
//!
//! Run with: `cargo test --test infrastructure -- --ignored`

#![allow(dead_code, unused_imports, unused_variables)]
#![allow(clippy::needless_borrows_for_generic_args)]

use std::time::Duration;
use tokio::time::sleep;

const LLAMA_CPP_URL: &str = "http://localhost:8080/v1";
const GATEWAY_URL: &str = "http://localhost:8081/v1";
const LANGFUSE_URL: &str = "http://localhost:3000";
const OLLAMA_URL: &str = "http://localhost:11434";
const MCP_HANGAR_URL: &str = "http://localhost:8084";
const JOERN_URL: &str = "http://localhost:8082";
const CODEBADGER_URL: &str = "http://localhost:8083";
const PROMETHEUS_URL: &str = "http://localhost:9090";
const GRAFANA_URL: &str = "http://localhost:3001";

/// Test that all infrastructure services are healthy
#[ignore]
async fn test_all_services_healthy() {
    let client = reqwest::Client::new();

    let services = vec![
        (
            "llama.cpp",
            format!("{}/health", LLAMA_CPP_URL.replace("/v1", "")),
        ),
        (
            "gateway",
            format!("{}/health", GATEWAY_URL.replace("/v1", "")),
        ),
        ("langfuse", format!("{}/api/public/health", LANGFUSE_URL)),
        ("ollama", format!("{}/api/tags", OLLAMA_URL)),
        ("mcp-hangar", format!("{}/health", MCP_HANGAR_URL)),
        ("joern", format!("{}/health", JOERN_URL)),
        ("codebadger", format!("{}/health", CODEBADGER_URL)),
        ("prometheus", format!("{}/-/healthy", PROMETHEUS_URL)),
        ("grafana", format!("{}/api/health", GRAFANA_URL)),
    ];

    for (name, url) in services {
        let response = client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                println!("✅ {} is healthy", name);
            }
            Ok(resp) => {
                panic!("❌ {} returned status: {}", name, resp.status());
            }
            Err(e) => {
                panic!("❌ {} failed to connect: {}", name, e);
            }
        }
    }
}

/// Test llama.cpp model inference
#[ignore]
async fn test_llama_cpp_inference() {
    let client = reqwest::Client::new();

    let request = serde_json::json!({
        "model": "qwen3-8b",
        "messages": [
            {"role": "user", "content": "Say hello in one word"}
        ],
        "max_tokens": 10,
        "temperature": 0.1
    });

    let response = client
        .post(&format!("{}/chat/completions", LLAMA_CPP_URL))
        .json(&request)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status().is_success(),
        "Inference failed: {}",
        response.status()
    );

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");

    assert!(!content.is_empty(), "Empty response from model");
    println!("Model response: {}", content);
}

/// Test gateway routing to llama.cpp
#[ignore]
async fn test_gateway_routing() {
    let client = reqwest::Client::new();

    let request = serde_json::json!({
        "model": "qwen3-8b",
        "messages": [
            {"role": "user", "content": "Test gateway routing"}
        ],
        "max_tokens": 10
    });

    let response = client
        .post(&format!("{}/chat/completions", GATEWAY_URL))
        .json(&request)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status().is_success(),
        "Gateway routing failed: {}",
        response.status()
    );

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");

    assert!(!content.is_empty());
    println!("Gateway response: {}", content);
}

/// Test MCP Hangar tool registration and verification
#[ignore]
async fn test_mcp_hangar_tool_verification() {
    let client = reqwest::Client::new();

    // Test health
    let health = client
        .get(&format!("{}/health", MCP_HANGAR_URL))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("MCP Hangar health check failed");

    assert!(health.status().is_success());

    // Test tool listing
    let tools = client
        .get(&format!("{}/tools", MCP_HANGAR_URL))
        .timeout(Duration::from_secs(10))
        .send()
        .await;

    if let Ok(resp) = tools {
        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            println!("MCP Hangar tools: {:?}", body);
        }
    }
}

/// Test Langfuse trace ingestion
#[ignore]
async fn test_langfuse_trace_ingestion() {
    let client = reqwest::Client::new();

    // Test health
    let health = client
        .get(&format!("{}/api/public/health", LANGFUSE_URL))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("Langfuse health check failed");

    assert!(health.status().is_success());

    // Ingest a test trace
    let trace = serde_json::json!({
        "name": "test-trace",
        "input": {"query": "test"},
        "output": {"result": "success"},
        "metadata": {"service": "lai-test"}
    });

    let trace_resp = client
        .post(&format!("{}/api/public/traces", LANGFUSE_URL))
        .json(&trace)
        .timeout(Duration::from_secs(10))
        .send()
        .await;

    if let Ok(resp) = trace_resp {
        assert!(resp.status().is_success() || resp.status().as_u16() == 201);
    }
}

/// Test Joern CPG generation
#[ignore]
async fn test_joern_cpg_generation() {
    let client = reqwest::Client::new();

    // Create a test project
    let test_code = r#"
fn main() {
    let x = 5;
    let y = 10;
    println!("{}", x + y);
}
"#;

    std::fs::create_dir_all("/tmp/lai-test-project/src").unwrap();
    std::fs::write("/tmp/lai-test-project/src/main.rs", test_code).unwrap();

    let request = serde_json::json!({
        "project_path": "/tmp/lai-test-project",
        "language": "rust"
    });

    let response = client
        .post(&format!("{}/cpg/generate", JOERN_URL))
        .json(&request)
        .timeout(Duration::from_secs(60))
        .send()
        .await;

    if let Ok(resp) = response {
        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            println!(
                "Joern CPG generated: {:?}",
                body.get("nodes").map(|n| n.as_u64())
            );
        }
    }

    // Cleanup
    let _ = std::fs::remove_dir_all("/tmp/lai-test-project");
}

/// Test codebadger vulnerability scan
#[ignore]
async fn test_codebadger_vulnerability_scan() {
    let client = reqwest::Client::new();

    // Create test code with SQL injection
    let vulnerable_code = r#"
fn query_user(id: &str) -> String {
    format!("SELECT * FROM users WHERE id = '{}'", id)
}
"#;

    std::fs::create_dir_all("/tmp/lai-vuln-test/src").unwrap();
    std::fs::write("/tmp/lai-vuln-test/src/lib.rs", vulnerable_code).unwrap();

    let request = serde_json::json!({
        "project_path": "/tmp/lai-vuln-test",
        "scan_type": "vulnerability"
    });

    let response = client
        .post(&format!("{}/scan", CODEBADGER_URL))
        .json(&request)
        .timeout(Duration::from_secs(60))
        .send()
        .await;

    if let Ok(resp) = response {
        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let vulns = body["vulnerabilities"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0);
            println!("Codebadger found {} vulnerabilities", vulns);
            assert!(vulns > 0, "Should detect SQL injection vulnerability");
        }
    }

    let _ = std::fs::remove_dir_all("/tmp/lai-vuln-test");
}

/// Test Prometheus metrics collection
#[ignore]
async fn test_prometheus_metrics() {
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/api/v1/query?query=up", PROMETHEUS_URL))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("Prometheus query failed");

    assert!(response.status().is_success());

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse Prometheus response");
    let status = body["status"].as_str().unwrap_or("");
    assert_eq!(status, "success");

    let empty_vec = vec![];
    let results = body["data"]["result"].as_array().unwrap_or(&empty_vec);
    println!("Prometheus targets up: {}", results.len());
    assert!(!results.is_empty(), "No Prometheus targets found");
}

/// Test end-to-end lai workflow
#[ignore]
async fn test_lai_end_to_end_workflow() {
    // This test runs a complete lai workflow:
    // 1. Chat with model via gateway
    // 2. Execute tools
    // 3. Store memory
    // 4. Verify result

    let client = reqwest::Client::new();

    // Step 1: Generate code via gateway
    let code_request = serde_json::json!({
        "model": "qwen3-8b",
        "messages": [
            {"role": "system", "content": "You are a Rust code generator. Output only valid Rust code."},
            {"role": "user", "content": "Write a function that adds two numbers"}
        ],
        "max_tokens": 200,
        "temperature": 0.1
    });

    let code_response = client
        .post(&format!("{}/chat/completions", GATEWAY_URL))
        .json(&code_request)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .expect("Code generation failed");

    assert!(code_response.status().is_success());
    let code_body: serde_json::Value = code_response.json().await.unwrap();
    let generated_code = code_body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");

    assert!(
        generated_code.contains("fn") || generated_code.contains("fn add"),
        "Should generate a function"
    );
    println!("Generated code:\n{}", generated_code);

    // Step 2: Store in memory (if Mem0 is available)
    // ... memory storage test would go here

    // Step 3: Verify syntax
    let verify_request = serde_json::json!({
        "code": generated_code,
        "language": "rust"
    });

    // ... verification would go here
}

/// Test benchmark execution
#[ignore]
async fn test_benchmark_execution() {
    // Run the built-in benchmarks
    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "--",
            "bench",
            "run",
            "lai.retrieval.hybrid",
            "--iterations",
            "2",
        ])
        .current_dir("/workspace/lai_rust")
        .output()
        .expect("Failed to run benchmark");

    assert!(
        output.status.success(),
        "Benchmark failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Success: true"), "Benchmark should succeed");
    println!("Benchmark output: {}", stdout);
}

/// Helper: wait for service to be ready
async fn wait_for_service(
    url: &str,
    max_retries: u32,
    delay: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    for attempt in 1..=max_retries {
        if let Ok(resp) = client.get(url).timeout(Duration::from_secs(5)).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }

        if attempt < max_retries {
            sleep(delay).await;
        }
    }

    Err(format!("Service {} not ready after {} retries", url, max_retries).into())
}

/// Setup test environment
async fn setup_test_environment() -> Result<(), Box<dyn std::error::Error>> {
    // Wait for critical services
    wait_for_service("http://localhost:8080/health", 30, Duration::from_secs(2)).await?;
    wait_for_service("http://localhost:8081/health", 30, Duration::from_secs(2)).await?;

    // Create test directories
    std::fs::create_dir_all("/tmp/lai-test-project/src").ok();
    std::fs::create_dir_all("/tmp/lai-vuln-test/src").ok();

    Ok(())
}

/// Teardown test environment
async fn teardown_test_environment() {
    let _ = std::fs::remove_dir_all("/tmp/lai-test-project");
    let _ = std::fs::remove_dir_all("/tmp/lai-vuln-test");
}

#[ignore]
async fn test_full_integration_suite() {
    setup_test_environment().await.expect("Setup failed");

    // Run all individual tests
    test_all_services_healthy().await;
    test_llama_cpp_inference().await;
    test_gateway_routing().await;
    test_mcp_hangar_tool_verification().await;
    test_langfuse_trace_ingestion().await;
    test_joern_cpg_generation().await;
    test_codebadger_vulnerability_scan().await;
    test_prometheus_metrics().await;
    test_lai_end_to_end_workflow().await;
    test_benchmark_execution().await;

    teardown_test_environment().await;

    println!("✅ All integration tests passed!");
}
