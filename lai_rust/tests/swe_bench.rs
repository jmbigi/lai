//! swe_bench.rs - SWE-bench / Terminal-Bench style evaluation for lai
//! Runs real coding tasks against local models and measures performance.
//! Documents the honest gap vs. cloud frontier models.

use anyhow::Result;
use lai::benchmark::{
    contracts::*,
    framework::{BenchmarkConfig, BenchmarkRunner, OutputFormat},
};
use lai::commands;
use lai::init_wizard::cmd_init;
use lai::mcp_server::cmd_mcp;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;

/// A coding task for evaluation
#[derive(Debug, Clone)]
pub struct CodingTask {
    pub name: String,
    pub description: String,
    pub setup: Option<String>,        // Shell commands to set up the task
    pub verification: Option<String>, // Shell commands to verify the result
    pub expected_files: Vec<String>,
    pub difficulty: TaskDifficulty,
    pub category: TaskCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskDifficulty {
    Easy,   // Single function, clear spec
    Medium, // Multiple functions, some design decisions
    Hard,   // Multi-file, architectural decisions
    Expert, // Complex refactor, deep understanding needed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskCategory {
    BugFix,
    FeatureAdd,
    Refactor,
    TestGeneration,
    Documentation,
    SecurityFix,
}

/// Result of running a coding task
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_name: String,
    pub success: bool,
    pub duration_ms: u64,
    pub tokens_used: Option<u64>,
    pub files_changed: Vec<String>,
    pub verification_passed: bool,
    pub error_message: Option<String>,
    pub model_response: String,
}

/// Benchmark configuration for local models
pub struct LocalModelBenchmark {
    pub model_name: String,
    pub base_url: String,
    pub tasks: Vec<CodingTask>,
    pub results: Vec<TaskResult>,
    pub total_duration: Duration,
    pub total_tokens: u64,
}

impl LocalModelBenchmark {
    pub fn new(model_name: String, base_url: String) -> Self {
        Self {
            model_name,
            base_url,
            tasks: Self::default_tasks(),
            results: Vec::new(),
            total_duration: Duration::ZERO,
            total_tokens: 0,
        }
    }

    /// Default task suite covering various coding scenarios
    fn default_tasks() -> Vec<CodingTask> {
        vec![
            // Easy tasks
            CodingTask {
                name: "add_two_numbers".to_string(),
                description: "Write a function that adds two numbers and returns the result".to_string(),
                setup: None,
                verification: Some("cargo test".to_string()),
                expected_files: vec!["src/lib.rs".to_string()],
                difficulty: TaskDifficulty::Easy,
                category: TaskCategory::FeatureAdd,
            },
            CodingTask {
                name: "factorial".to_string(),
                description: "Implement a recursive factorial function with proper error handling for overflow".to_string(),
                setup: None,
                verification: Some("cargo test".to_string()),
                expected_files: vec!["src/lib.rs".to_string()],
                difficulty: TaskDifficulty::Easy,
                category: TaskCategory::FeatureAdd,
            },
            CodingTask {
                name: "fix_off_by_one".to_string(),
                description: "Fix the off-by-one error in the loop that processes array elements".to_string(),
                setup: Some(r#"
cat > src/lib.rs << 'EOF'
pub fn process_array(arr: &[i32]) -> Vec<i32> {
    let mut result = Vec::new();
    for i in 0..=arr.len() {  // Bug: should be < arr.len()
        result.push(arr[i] * 2);
    }
    result
}
EOF
"#.to_string()),
                verification: Some("cargo test".to_string()),
                expected_files: vec!["src/lib.rs".to_string()],
                difficulty: TaskDifficulty::Easy,
                category: TaskCategory::BugFix,
            },
            // Medium tasks
            CodingTask {
                name: "csv_parser".to_string(),
                description: "Implement a CSV parser that handles quoted fields, escaped quotes, and different delimiters".to_string(),
                setup: None,
                verification: Some("cargo test".to_string()),
                expected_files: vec!["src/lib.rs".to_string(), "src/csv.rs".to_string()],
                difficulty: TaskDifficulty::Medium,
                category: TaskCategory::FeatureAdd,
            },
            CodingTask {
                name: "async_http_client".to_string(),
                description: "Create an async HTTP client with retry logic, timeout handling, and connection pooling".to_string(),
                setup: Some("cargo add reqwest tokio --features rt-multi-thread,macros".to_string()),
                verification: Some("cargo check".to_string()),
                expected_files: vec!["src/client.rs".to_string()],
                difficulty: TaskDifficulty::Medium,
                category: TaskCategory::FeatureAdd,
            },
            CodingTask {
                name: "refactor_large_function".to_string(),
                description: "Refactor this 200-line function into smaller, testable units with clear responsibilities".to_string(),
                setup: Some(r#"
cat > src/processor.rs << 'EOF'
pub fn process_data(data: &[u8], config: &Config) -> Result<Vec<u8>, Error> {
    // 200 lines of mixed validation, transformation, encoding, compression
    // ...
    Ok(vec![])
}
EOF
"#.to_string()),
                verification: Some("cargo test".to_string()),
                expected_files: vec!["src/processor.rs".to_string(), "src/transform.rs".to_string()],
                difficulty: TaskDifficulty::Medium,
                category: TaskCategory::Refactor,
            },
            // Hard tasks
            CodingTask {
                name: "design_plugin_system".to_string(),
                description: "Design and implement a plugin system with dynamic loading, dependency injection, and hot reload".to_string(),
                setup: Some("cargo add libloading".to_string()),
                verification: Some("cargo test".to_string()),
                expected_files: vec!["src/plugin.rs".to_string(), "src/registry.rs".to_string(), "src/loader.rs".to_string()],
                difficulty: TaskDifficulty::Hard,
                category: TaskCategory::FeatureAdd,
            },
            CodingTask {
                name: "async_state_machine".to_string(),
                description: "Implement a complex async state machine for distributed consensus with leader election".to_string(),
                setup: Some("cargo add tokio async-trait".to_string()),
                verification: Some("cargo test".to_string()),
                expected_files: vec!["src/consensus.rs".to_string(), "src/state.rs".to_string()],
                difficulty: TaskDifficulty::Hard,
                category: TaskCategory::FeatureAdd,
            },
            // Expert tasks
            CodingTask {
                name: "memory_safe_ffi".to_string(),
                description: "Create safe Rust bindings for a C library with complex ownership semantics and callbacks".to_string(),
                setup: Some("cargo add bindgen cc".to_string()),
                verification: Some("cargo test".to_string()),
                expected_files: vec!["src/bindings.rs".to_string(), "src/safe_wrapper.rs".to_string(), "build.rs".to_string()],
                difficulty: TaskDifficulty::Expert,
                category: TaskCategory::FeatureAdd,
            },
        ]
    }

    /// Run all tasks against the local model
    pub async fn run(&mut self) -> Result<()> {
        println!("🏁 Starting benchmark for model: {}", self.model_name);
        println!("📋 {} tasks to evaluate", self.tasks.len());

        let start = Instant::now();

        for (i, task) in self.tasks.iter().enumerate() {
            println!(
                "\n[{}/{}] 🎯 Task: {} ({:?}, {:?})",
                i + 1,
                self.tasks.len(),
                task.name,
                task.difficulty,
                task.category
            );
            println!("   📝 {}", task.description);

            let result = self.run_task(task).await?;
            self.total_duration += Duration::from_millis(result.duration_ms);
            if let Some(tokens) = result.tokens_used {
                self.total_tokens += tokens;
            }
            self.results.push(result.clone());

            if result.success {
                println!("   ✅ Success ({}ms)", result.duration_ms);
            } else {
                println!(
                    "   ❌ Failed: {}",
                    result.error_message.unwrap_or("Unknown error".to_string())
                );
            }
        }

        self.total_duration = start.elapsed();
        self.print_summary();
        self.save_results()?;

        Ok(())
    }

    async fn run_task(&self, task: &CodingTask) -> Result<TaskResult> {
        let temp_dir = TempDir::new()?;
        let project_root = temp_dir.path().to_path_buf();

        // Initialize a minimal Rust project
        self.setup_project(&project_root, task).await?;

        // Run the task using lai auto mode
        let start = Instant::now();

        // Build the prompt for the model
        let prompt = self.build_prompt(task);

        // Run lai auto with the task
        let output = self.run_lai_auto(&project_root, &prompt).await?;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Verify the result
        let (verification_passed, files_changed) = self.verify_result(&project_root, task).await?;

        Ok(TaskResult {
            task_name: task.name.clone(),
            success: verification_passed,
            duration_ms,
            tokens_used: None, // Would need to parse from model response
            files_changed,
            verification_passed,
            error_message: if verification_passed {
                None
            } else {
                Some("Verification failed".to_string())
            },
            model_response: output,
        })
    }

    async fn setup_project(&self, project_root: &PathBuf, task: &CodingTask) -> Result<()> {
        // Create Cargo.toml
        let cargo_toml = r#"
[package]
name = "bench_task"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
thiserror = "1.0"
"#;
        std::fs::write(project_root.join("Cargo.toml"), cargo_toml)?;

        // Create src directory
        std::fs::create_dir_all(project_root.join("src"))?;

        // Create initial lib.rs
        let lib_rs = r#"
//! Benchmark task library

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
    }
}
"#;
        std::fs::write(project_root.join("src/lib.rs"), lib_rs)?;

        // Run any setup commands
        if let Some(setup) = &task.setup {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(setup)
                .current_dir(&project_root)
                .output()?;
            if !output.status.success() {
                eprintln!(
                    "Setup command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        Ok(())
    }

    fn build_prompt(&self, task: &CodingTask) -> String {
        format!(
            r#"You are an expert Rust developer. Complete the following task:

TASK: {}
DESCRIPTION: {}
DIFFICULTY: {:?}
CATEGORY: {:?}

Write clean, idiomatic Rust code with proper error handling, tests, and documentation.
Follow Rust best practices and the project's existing code style.
"#,
            task.name, task.description, task.difficulty, task.category
        )
    }

    async fn run_lai_auto(&self, project_root: &PathBuf, prompt: &str) -> Result<String> {
        // Run lai auto in the project directory
        let output = std::process::Command::new("cargo")
            .args([
                "run",
                "--quiet",
                "--manifest-path",
                "lai_rust/Cargo.toml",
                "--",
                "auto",
                prompt,
                "--max-iterations",
                "10",
            ])
            .current_dir(project_root)
            .env("LAI_MODEL", &self.model_name)
            .env("LAI_BASE_URL", &self.base_url)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            eprintln!("lai auto stderr: {}", stderr);
        }

        Ok(stdout)
    }

    async fn verify_result(
        &self,
        project_root: &PathBuf,
        task: &CodingTask,
    ) -> Result<(bool, Vec<String>)> {
        let mut files_changed = Vec::new();

        // Check expected files exist
        for expected in &task.expected_files {
            let path = project_root.join(expected);
            if path.exists() {
                files_changed.push(expected.clone());
            }
        }

        // Run verification command if provided
        let verification_passed = if let Some(verify_cmd) = &task.verification {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(verify_cmd)
                .current_dir(project_root)
                .output()?;
            output.status.success()
        } else {
            // At minimum, check that code compiles
            let output = std::process::Command::new("cargo")
                .args(["check"])
                .current_dir(project_root)
                .output()?;
            output.status.success()
        };

        Ok((verification_passed, files_changed))
    }

    fn print_summary(&self) {
        let total = self.results.len();
        let passed = self.results.iter().filter(|r| r.success).count();
        let failed = total - passed;
        let avg_duration = if total > 0 {
            self.total_duration.as_millis() / total as u128
        } else {
            0
        };

        println!("\n{}", "=".repeat(60));
        println!("📊 BENCHMARK SUMMARY");
        println!("{}", "=".repeat(60));
        println!("Model: {}", self.model_name);
        println!("Total tasks: {}", total);
        println!(
            "Passed: {} ({:.1}%)",
            passed,
            passed as f64 / total as f64 * 100.0
        );
        println!("Failed: {}", failed);
        println!("Total duration: {:.2}s", self.total_duration.as_secs_f64());
        println!("Avg duration/task: {}ms", avg_duration);
        println!("Total tokens (est): {}", self.total_tokens);
        println!("{}", "=".repeat(60));

        // By difficulty
        for difficulty in [
            TaskDifficulty::Easy,
            TaskDifficulty::Medium,
            TaskDifficulty::Hard,
            TaskDifficulty::Expert,
        ] {
            let diff_tasks: Vec<_> = self
                .results
                .iter()
                .zip(self.tasks.iter())
                .filter(|(_, t)| t.difficulty == difficulty)
                .collect();
            if !diff_tasks.is_empty() {
                let diff_passed = diff_tasks.iter().filter(|(r, _)| r.success).count();
                println!(
                    "  {:?}: {}/{} passed ({:.1}%)",
                    difficulty,
                    diff_passed,
                    diff_tasks.len(),
                    diff_passed as f64 / diff_tasks.len() as f64 * 100.0
                );
            }
        }

        // By category
        for category in [
            TaskCategory::BugFix,
            TaskCategory::FeatureAdd,
            TaskCategory::Refactor,
            TaskCategory::TestGeneration,
            TaskCategory::Documentation,
            TaskCategory::SecurityFix,
        ] {
            let cat_tasks: Vec<_> = self
                .results
                .iter()
                .zip(self.tasks.iter())
                .filter(|(_, t)| t.category == category)
                .collect();
            if !cat_tasks.is_empty() {
                let cat_passed = cat_tasks.iter().filter(|(r, _)| r.success).count();
                println!(
                    "  {:?}: {}/{} passed ({:.1}%)",
                    category,
                    cat_passed,
                    cat_tasks.len(),
                    cat_passed as f64 / cat_tasks.len() as f64 * 100.0
                );
            }
        }
    }

    fn save_results(&self) -> Result<()> {
        let output = serde_json::json!({
            "model": self.model_name,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "total_tasks": self.tasks.len(),
            "passed": self.results.iter().filter(|r| r.success).count(),
            "failed": self.results.iter().filter(|r| !r.success).count(),
            "total_duration_ms": self.total_duration.as_millis(),
            "total_tokens": self.total_tokens,
            "results": self.results.iter().map(|r| serde_json::json!({
                "task": r.task_name,
                "success": r.success,
                "duration_ms": r.duration_ms,
                "verification_passed": r.verification_passed,
                "error": r.error_message,
            })).collect::<Vec<_>>(),
        });

        let filename = format!(
            "benchmark_{}_{}.json",
            self.model_name.replace([':', '/'], "_"),
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );
        std::fs::write(&filename, serde_json::to_string_pretty(&output)?)?;
        println!("💾 Results saved to: {}", filename);
        Ok(())
    }
}

/// Run a quick benchmark (subset of tasks)
pub async fn run_quick_benchmark(model: &str, base_url: &str) -> Result<()> {
    let mut bench = LocalModelBenchmark::new(model.to_string(), base_url.to_string());
    // Only run easy tasks for quick benchmark
    bench.tasks = bench
        .tasks
        .into_iter()
        .filter(|t| t.difficulty == TaskDifficulty::Easy)
        .collect();
    bench.run().await
}

/// Run full benchmark
pub async fn run_full_benchmark(model: &str, base_url: &str) -> Result<()> {
    let mut bench = LocalModelBenchmark::new(model.to_string(), base_url.to_string());
    bench.run().await
}

/// Compare two model results
pub fn compare_results(file1: &str, file2: &str) -> Result<()> {
    let data1: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file1)?)?;
    let data2: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file2)?)?;

    println!("📊 MODEL COMPARISON");
    println!("{}", "=".repeat(60));
    println!(
        "Model 1: {} ({} passed/{} total)",
        data1["model"], data1["passed"], data1["total_tasks"]
    );
    println!(
        "Model 2: {} ({} passed/{} total)",
        data2["model"], data2["passed"], data2["total_tasks"]
    );
    println!("{}", "=".repeat(60));

    // Compare by task
    let results1 = data1["results"].as_array().unwrap();
    let results2 = data2["results"].as_array().unwrap();

    for (r1, r2) in results1.iter().zip(results2.iter()) {
        let name1 = r1["task"].as_str().unwrap();
        let _name2 = r2["task"].as_str().unwrap();
        let pass1 = r1["success"].as_bool().unwrap();
        let pass2 = r2["success"].as_bool().unwrap();
        let dur1 = r1["duration_ms"].as_u64().unwrap();
        let dur2 = r2["duration_ms"].as_u64().unwrap();

        let status = match (pass1, pass2) {
            (true, true) => "✅ Both pass",
            (true, false) => "🔴 Model 1 only",
            (false, true) => "🟢 Model 2 only",
            (false, false) => "⚪ Both fail",
        };

        println!("  {}: {} | {}ms vs {}ms", name1, status, dur1, dur2);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tasks() {
        let bench = LocalModelBenchmark::new("test".to_string(), "http://localhost".to_string());
        assert!(!bench.tasks.is_empty());
        assert!(bench
            .tasks
            .iter()
            .any(|t| t.difficulty == TaskDifficulty::Easy));
        assert!(bench
            .tasks
            .iter()
            .any(|t| t.difficulty == TaskDifficulty::Medium));
        assert!(bench
            .tasks
            .iter()
            .any(|t| t.difficulty == TaskDifficulty::Hard));
        assert!(bench
            .tasks
            .iter()
            .any(|t| t.difficulty == TaskDifficulty::Expert));
    }

    #[test]
    fn test_task_categories() {
        let bench = LocalModelBenchmark::new("test".to_string(), "http://localhost".to_string());
        let categories: std::collections::HashSet<_> =
            bench.tasks.iter().map(|t| t.category).collect();
        assert!(categories.contains(&TaskCategory::BugFix));
        assert!(categories.contains(&TaskCategory::FeatureAdd));
        assert!(categories.contains(&TaskCategory::Refactor));
    }
}
