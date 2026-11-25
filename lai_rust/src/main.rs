//! lai - Local AI Code Assistant
//! v5.0 Architecture: Deterministic engineering, controlled autonomy, supply-chain security, bounded verifiability, measured efficiency.

use clap::Parser;
use lai::baseline::BaselineCommands;
use lai::benchmark::BenchCommands;
use lai::commands;
use lai::init_wizard::{cmd_doctor, cmd_init};
use lai::mcp_server::cmd_mcp;
use lai::memory_store::{CheckpointCommands, MemoryCommands};
use lai::planner::PlanCommands;
use lai::telemetry;
use lai::tui::TuiApp;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(clap::Parser, Debug)]
#[command(author, version, about = "lai - Local AI Code Assistant v5.0", long_about = None)]
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
    /// Chat with AI (with tool calling)
    Chat {
        #[arg(default_value = "", help = "Prompt")]
        prompt: Vec<String>,
    },
    /// Generate code with quality cycles
    Code {
        #[arg(help = "Description")]
        description: String,
        #[arg(help = "Output file")]
        filename: String,
        #[arg(long = "yolo", help = "Save without confirmation")]
        yolo: bool,
    },
    /// Check service status
    Status,
    /// Run autonomous task with full engineering pipeline
    Auto {
        #[arg(help = "Task description")]
        task: String,
        #[arg(long, default_value = "5", help = "Max iterations")]
        max_iterations: u32,
        #[arg(long, help = "Output directory")]
        output: Option<String>,
    },
    /// Run RepoMap indexer
    RepoMap {
        #[arg(long, help = "Index directory")]
        path: Option<String>,
        #[arg(long, help = "Show symbols for file")]
        file: Option<String>,
    },
    /// Run baseline measurement
    Baseline {
        #[command(subcommand)]
        command: BaselineCommands,
    },
    /// Verify code with multi-level verification
    Verify {
        #[arg(help = "File or directory to verify")]
        path: String,
        #[arg(long, help = "Verification level (1-8)")]
        level: Option<u32>,
    },
    /// Diagnose errors
    Diagnose {
        #[arg(help = "Error message or file")]
        input: String,
        #[arg(long, help = "Project root")]
        root: Option<String>,
    },
    /// Repair code with multi-strategy repair
    Repair {
        #[arg(help = "File to repair")]
        file: String,
        #[arg(long, help = "Error context")]
        error: Option<String>,
        #[arg(long, default_value = "3", help = "Max repair attempts")]
        max_attempts: u32,
    },
    /// Checkpoint management
    Checkpoint {
        #[command(subcommand)]
        command: CheckpointCommands,
    },
    /// Memory store operations
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },
    /// Planner operations
    Plan {
        #[command(subcommand)]
        command: PlanCommands,
    },
    /// Show telemetry/metrics
    Metrics,
    /// Run capability benchmarks
    Bench {
        #[command(subcommand)]
        command: BenchCommands,
    },
    /// Interactive setup wizard (detect hardware, download model, configure)
    Init {
        #[arg(long, help = "Model to use (skip selection)")]
        model: Option<String>,
        #[arg(long, help = "Non-interactive, use recommended model")]
        auto: bool,
        #[arg(long, help = "Use llama.cpp directly (no ollama)")]
        no_ollama: bool,
        #[arg(long, help = "Reset configuration and start fresh")]
        reset: bool,
    },
    /// Diagnose installation issues
    Doctor,
    /// Launch Terminal UI (diff viewer, file tree, chat, tool log)
    Tui,
    /// Start MCP server for tool exposure to Cursor/Cline/Claude Code
    Mcp {
        #[arg(long, help = "Port for TCP server (default: 8083)")]
        port: Option<u16>,
        #[arg(long, help = "Use stdio transport (for Claude Code)")]
        stdio: bool,
    },
    /// Run SWE-bench style coding benchmark against local models
    BenchSwe {
        #[arg(long, help = "Model to benchmark")]
        model: String,
        #[arg(
            long,
            default_value = "http://localhost:8081/v1",
            help = "Base URL for the model"
        )]
        base_url: String,
        #[arg(long, help = "Run only easy tasks (quick mode)")]
        quick: bool,
        #[arg(long, help = "Compare with previous benchmark result file")]
        compare: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize telemetry
    let _ = telemetry::Telemetry::init(telemetry::TelemetryConfig::default());

    match cli.command {
        Some(Commands::Chat { prompt, .. }) => {
            commands::cmd_chat(&prompt, &cli.model, &cli.base_url, cli.allow_remote).await;
        }
        Some(Commands::Code {
            description,
            filename,
            yolo,
        }) => {
            commands::cmd_code(
                &description,
                &filename,
                yolo,
                &cli.model,
                &cli.base_url,
                cli.allow_remote,
            )
            .await;
        }
        Some(Commands::Status) => {
            commands::cmd_status().await;
        }
        Some(Commands::Auto {
            task,
            max_iterations,
            output,
        }) => {
            commands::cmd_auto(
                &task,
                max_iterations,
                output.as_deref(),
                &cli.model,
                &cli.base_url,
                cli.allow_remote,
            )
            .await;
        }
        Some(Commands::RepoMap { path, file }) => {
            commands::cmd_repomap(path.as_deref(), file.as_deref()).await;
        }
        Some(Commands::Baseline { command }) => {
            commands::cmd_baseline(command).await;
        }
        Some(Commands::Verify { path, level }) => {
            commands::cmd_verify(&path, level).await;
        }
        Some(Commands::Diagnose { input, root }) => {
            commands::cmd_diagnose(&input, root.as_deref()).await;
        }
        Some(Commands::Repair {
            file,
            error,
            max_attempts,
        }) => {
            commands::cmd_repair(&file, error.as_deref(), max_attempts).await;
        }
        Some(Commands::Checkpoint { command }) => {
            commands::cmd_checkpoint(command).await;
        }
        Some(Commands::Memory { command }) => {
            commands::cmd_memory(command).await;
        }
        Some(Commands::Plan { command }) => {
            commands::cmd_plan(command).await;
        }
        Some(Commands::Metrics) => {
            commands::cmd_metrics().await;
        }
        Some(Commands::Bench { command }) => {
            commands::cmd_bench(command).await;
        }
        Some(Commands::Init {
            model,
            auto,
            no_ollama,
            reset,
        }) => {
            cmd_init(model, auto, no_ollama, reset).await?;
        }
        Some(Commands::Doctor) => {
            cmd_doctor().await?;
        }
        Some(Commands::Tui) => {
            let project_root = std::env::current_dir()?;
            let mut app = TuiApp::new(project_root);
            app.run().await?;
        }
        Some(Commands::Mcp { port, stdio }) => {
            cmd_mcp(port, stdio).await?;
        }
        Some(Commands::BenchSwe {
            model,
            base_url,
            quick,
            compare,
        }) => {
            if let Some(compare_file) = compare {
                // Compare mode
                let bench_file = std::env::current_dir()?
                    .join("benchmark_results")
                    .join(&model.replace([':', '/'], "_"));
                // This would need the test module's compare_results function
                println!("Compare mode not fully implemented in main binary yet");
            } else {
                // Import the test module's benchmark functions
                // For now, just show help
                println!("SWE-bench evaluation for model: {}", model);
                println!("Run with: cargo test --test swe_bench -- --nocapture");
            }
        }
        None => {
            println!("lai - Local AI Code Assistant v{}", VERSION);
            println!("Use `lai --help` for usage.");
        }
    }
    Ok(())
}
