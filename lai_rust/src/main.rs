//! lai - Local AI Code Assistant
//! Minimal working Rust implementation.
//! Focus: generate code via llama.cpp, validate syntax, save file.

mod commands;
mod llm;
mod policy;
mod services;
mod safety;
mod tools;
mod telemetry;
mod repomap;
mod context;
mod harness;
mod integrity;
mod sandbox;
mod verifier;
mod spec;
mod memory;
mod bench;
mod mcp;
mod trajectory;
mod change_engine;

use clap::Parser;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(clap::Parser, Debug)]
#[command(author, version, about = "lai - Local AI Code Assistant", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long, default_value = "qwen3-8b", help = "Model to use")]
    model: String,

    #[arg(long, default_value = "http://localhost:8080/v1", help = "Base URL")]
    base_url: String,

    #[arg(long, help = "Allow non-local endpoints")]
    allow_remote: bool,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Chat with AI
    Chat {
        #[arg(default_value = "", help = "Prompt")]
        prompt: Vec<String>,
    },
    /// Generate code
    Code {
        #[arg(help = "Description")]
        description: String,
        #[arg(help = "Output file")]
        filename: String,
        #[arg(long = "yolo", help = "Save without confirmation")]
        yolo: bool,
    },
    /// Check status
    Status,
    /// Run an autonomous task through the ExecutionHarness (deterministic runtime)
    Harness {
        #[arg(help = "Task description")]
        task: String,
        #[arg(long, help = "YOLO mode: auto-approve confirmations (Deny remains enforced)")]
        yolo: bool,
        #[arg(long, help = "Require a verified sandbox for command execution (fail-closed)")]
        sandbox: bool,
        #[arg(long, help = "Isolate the task in a dedicated git worktree (transversal 4.1)")]
        worktree: bool,
        #[arg(long, help = "Merge the worktree branch back on success (requires --worktree)")]
        worktree_merge: bool,
    },
    /// Run the internal benchmark suite (lai bench)
    Bench {
        #[arg(long, default_value = "benchmarks/tasks", help = "Tasks directory (*.json)")]
        tasks: String,
        #[arg(long, default_value = ".benchmarks/results", help = "Output directory")]
        out: String,
        /// Compare two benchmark result files and show the delta table
        #[arg(long, help = "Path to baseline result file")]
        compare_with: Option<String>,
    },
    /// Recover execution state: list or show checkpoints of a task
    Recover {
        #[arg(long, help = "Task id (task-*-xxxxxxxx); if omitted, list all tasks")]
        task: Option<String>,
        #[arg(long, default_value = "/tmp/lai-checkpoints", help = "Checkpoint directory")]
        checkpoints: String,
    },
    /// Analyze registered trajectories (auto-improvement metrics, v4.0 Phase 6)
    Trajectories,
    /// Start MCP server (stdio JSON-RPC) exposing lai tools
    Mcp,
}

#[tokio::main]
async fn main() {
    // Initialize telemetry first - before any other operations
    let telemetry_config = crate::telemetry::TelemetryConfig {
        level: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        log_dir: std::env::var("LAI_LOG_DIR").ok().map(std::path::PathBuf::from),
        json_output: std::env::var("LAI_LOG_JSON").is_ok(),
        console_output: true,
        debug_artifacts: std::env::var("LAI_DEBUG_ARTIFACTS").is_ok(),
        debug_artifacts_dir: std::env::var("LAI_DEBUG_TESTS").ok().map(std::path::PathBuf::from),
        max_file_size_mb: 100,
        max_files: 10,
        span_events: true,
    };
    let _ = crate::telemetry::init_telemetry(telemetry_config);

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Chat { prompt, .. }) => {
            commands::cmd_chat(
                &prompt, false, false, false, false, false, None, None,
                &cli.model, &cli.base_url, "es", cli.allow_remote,
            ).await;
        }
        Some(Commands::Code {
            description,
            filename,
            yolo,
        }) => {
            commands::cmd_code(
                &description, &filename, false, yolo, 2,
                &cli.model, &cli.base_url, cli.allow_remote,
            ).await;
        }
        Some(Commands::Status) => {
            commands::cmd_status().await;
        }
        Some(Commands::Harness { task, yolo, sandbox, worktree, worktree_merge }) => {
            commands::cmd_harness(&task, yolo, sandbox, worktree, worktree_merge, &cli.model, &cli.base_url, cli.allow_remote).await;
        }
        Some(Commands::Bench { tasks, out, compare_with }) => {
            commands::cmd_bench(&tasks, &out, &cli.model, &cli.base_url, cli.allow_remote, compare_with.as_deref()).await;
        }
        Some(Commands::Recover { task, checkpoints }) => {
            commands::cmd_recover(task.as_deref(), &checkpoints).await;
        }
        Some(Commands::Trajectories) => {
            commands::cmd_trajectories().await;
        }
        Some(Commands::Mcp) => {
            commands::cmd_mcp().await;
        }
        None => {
            println!("lai - Local AI Code Assistant v{}", VERSION);
            println!("Use `lai --help` for usage.");
        }
    }
}