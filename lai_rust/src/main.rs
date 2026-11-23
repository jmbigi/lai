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
mod concurrency;
mod contracts;
mod mcp_security;
mod hybrid_rag;
mod cpg;
mod dag;
mod property_testing;
mod formal;
mod nn_verifier;
mod langfuse;
mod graph_orchestrator;
mod memory_eval;
mod retrieval_bench;

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
    /// Recover execution state: list or show checkpoints of a task
    Recover {
        #[arg(long, help = "Task id (task-*-xxxxxxxx); if omitted, list all tasks")]
        task: Option<String>,
        #[arg(long, default_value = "/tmp/lai-checkpoints", help = "Checkpoint directory")]
        checkpoints: String,
    },
    /// Analyze registered trajectories (auto-improvement metrics, v4.0 Phase 6)
    Trajectories,
    /// Run the internal benchmark suite (lai bench)
    Bench {
        #[arg(long, default_value = "benchmarks/tasks", help = "Tasks directory (*.json)")]
        tasks: String,
        #[arg(long, default_value = ".benchmarks/results", help = "Output directory")]
        out: String,
        /// Compare two benchmark result files and show the delta table
        #[arg(long, help = "Path to baseline result file")]
        compare_with: Option<String>,
        /// Run tasks in parallel, each isolated in its own git worktree (DAGent)
        #[arg(long, default_value = "1", help = "Max concurrent tasks (each in its own worktree)")]
        concurrency: usize,
        /// Also run the retrieval benchmark (hybrid RAG vs keyword baseline)
        #[arg(long, help = "Run retrieval bench over the current workspace")]
        retrieval: bool,
    },
    /// Start MCP server (stdio JSON-RPC) exposing lai tools
    Mcp {
        /// Refuse to serve if the pins file does not verify (fail-closed)
        #[arg(long, help = "Path to mcp-pins.json; if set, startup is denied on mismatch")]
        verify_pins: Option<String>,
    },
    /// Pin the current tool definitions to a pins file (MCP Hangar, v5.0)
    McpPin {
        #[arg(long, default_value = ".lai/mcp-pins.json", help = "Output pins file")]
        out: String,
    },
    /// Verify advertised MCP tool definitions against a pins file (fail-closed)
    McpVerify {
        #[arg(long, help = "Pins file (.lai/mcp-pins.json)")]
        pins: String,
        #[arg(long, help = "JSON file with advertised tool definitions [{server,name,description,input_schema}]")]
        defs: String,
    },
    /// Check formal contracts (requires/ensures) declared in a SPEC.md
    Contracts {
        #[arg(long, help = "SPEC.md with a ```json contracts``` block")]
        spec: String,
        #[arg(long, help = "Pre-state JSON")]
        pre: String,
        #[arg(long, help = "Post-state JSON")]
        post: String,
    },
    /// Verify a neural-network robustness property with an external verifier
    NnVerify {
        #[arg(long, help = "JSON file with NnPropertySpec")]
        spec: String,
        #[arg(long, help = "The property is required by verification_scope (fail-closed BLOCKED)")]
        required: bool,
    },
    /// Evaluate MemoryStore recall against a LOCOMO-style dataset (JSONL)
    MemoryEval {
        #[arg(long, help = "Dataset JSONL: {question, expected[]})")]
        dataset: String,
        #[arg(long, default_value = ".lai/memory.db", help = "Memory database")]
        db: String,
        #[arg(long, default_value = "3", help = "k for recall@k")]
        k: usize,
    },
    /// Code Property Graph report over a workspace (taint/slicing, v5.0)
    Cpg {
        #[arg(long, default_value = ".", help = "Workspace root")]
        workspace: String,
        #[arg(long, help = "Show taint findings (sources -> sinks)")]
        taint: bool,
        #[arg(long, help = "Slice from this symbol id (file::kind::name)")]
        slice: Option<String>,
    },
    /// Export trajectories to Langfuse-compatible batches (v5.0)
    TelemetryExport {
        #[arg(long, default_value = ".lai/langfuse", help = "Output directory")]
        dir: String,
        #[arg(long, help = "Langfuse host for POST (default: local batch only)")]
        host: Option<String>,
    },
    /// Run a LangGraph-style explicit graph spec (tool/llm/verify nodes)
    GraphRun {
        #[arg(long, help = "JSON file with GraphSpec")]
        spec: String,
        #[arg(long, help = "The graph result is required (fail-closed BLOCKED)")]
        required: bool,
    },
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
        Some(Commands::Bench { tasks, out, compare_with, concurrency, retrieval }) => {
            commands::cmd_bench(&tasks, &out, &cli.model, &cli.base_url, cli.allow_remote, compare_with.as_deref(), concurrency, retrieval).await;
        }
        Some(Commands::Recover { task, checkpoints }) => {
            commands::cmd_recover(task.as_deref(), &checkpoints).await;
        }
        Some(Commands::Trajectories) => {
            commands::cmd_trajectories().await;
        }
        Some(Commands::Mcp { verify_pins }) => {
            commands::cmd_mcp(verify_pins.as_deref()).await;
        }
        Some(Commands::McpPin { out }) => {
            commands::cmd_mcp_pin(&out);
        }
        Some(Commands::McpVerify { pins, defs }) => {
            commands::cmd_mcp_verify(&pins, &defs);
        }
        Some(Commands::Contracts { spec, pre, post }) => {
            commands::cmd_contracts(&spec, &pre, &post);
        }
        Some(Commands::NnVerify { spec, required }) => {
            commands::cmd_nn_verify(&spec, required);
        }
        Some(Commands::MemoryEval { dataset, db, k }) => {
            commands::cmd_memory_eval(&dataset, &db, k);
        }
        Some(Commands::Cpg { workspace, taint, slice }) => {
            commands::cmd_cpg(&workspace, taint, slice.as_deref());
        }
        Some(Commands::TelemetryExport { dir, host }) => {
            commands::cmd_telemetry_export(&dir, host.as_deref());
        }
        Some(Commands::GraphRun { spec, required }) => {
            commands::cmd_graph_run(&spec, required).await;
        }
        None => {
            println!("lai - Local AI Code Assistant v{}", VERSION);
            println!("Use `lai --help` for usage.");
        }
    }
}