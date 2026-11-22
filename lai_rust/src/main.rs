//! lai - Local AI Code Assistant
//! A comprehensive CLI tool for reviewing, repairing, and improving code projects
//! using local LLMs (Qwen3-8B via llama.cpp)

mod commands;
mod llm;
mod services;
mod tools;
mod automode;
mod agents;
mod session;
mod thinking;
mod config;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Current version (mirrors the Python project's version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(name = "lai", version = VERSION, about = "Local AI Code Assistant - CLI tool for reviewing, repairing, and improving code using local LLMs", long_about = None)]
struct Cli {
    /// Model to use
    #[arg(short = 'm', long, global = true, default_value = "qwen3-8b")]
    model: String,

    /// Base URL for API (llama.cpp/OpenAI-compatible)
    #[arg(short = 'u', long, global = true, default_value = "http://localhost:8080/v1")]
    base_url: String,

    /// Temperature for generation
    #[arg(short = 't', long, global = true)]
    temp: Option<f64>,

    /// Max tokens for generation
    #[arg(long, global = true)]
    max_tokens: Option<i32>,

    /// Enable streaming output
    #[arg(long, global = true)]
    stream: bool,

    /// Disable streaming output
    #[arg(long, global = true)]
    no_stream: bool,

    /// Enable thinking mode
    #[arg(long, global = true)]
    thinking: bool,

    /// Disable thinking mode
    #[arg(long, global = true)]
    no_thinking: bool,

    /// Enable tools
    #[arg(long, global = true)]
    tools: bool,

    /// Disable tools
    #[arg(long, global = true)]
    no_tools: bool,

    /// Auto mode
    #[arg(long, global = true)]
    auto: bool,

    /// YOLO mode (no confirmation)
    #[arg(long, global = true)]
    yolo: bool,

    /// Verbose output
    #[arg(short = 'v', long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Quiet mode
    #[arg(short = 'q', long, global = true)]
    quiet: bool,

    /// Allow remote endpoints (bypass Rule 4)
    #[arg(long, global = true)]
    allow_remote: bool,

    /// Language for output (es/en)
    #[arg(long, global = true, default_value = "es")]
    lang: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Chat with the AI
    Chat {
        /// Prompt to send
        prompt: Vec<String>,
        /// Interactive mode
        #[arg(short, long)]
        interactive: bool,
    },
    /// Generate text
    Generate {
        prompt: Vec<String>,
    },
    /// Generate code file
    Code {
        /// Description of the code to generate
        description: String,
        /// Output filename
        filename: String,
        /// Max iterations for refinement
        #[arg(long, default_value = "3")]
        max_iterations: i32,
    },
    /// Think deeply about a problem
    Think {
        problem: String,
    },
    /// Run autonomous agent (AutoMode)
    Auto {
        /// Goal for the agent
        goal: String,
        /// Maximum iterations
        #[arg(long, default_value = "10")]
        max_iterations: i32,
        /// Maximum tool turns
        #[arg(long, default_value = "20")]
        max_turns: i32,
        /// Disable cost tracking
        #[arg(long)]
        no_cost: bool,
    },
    /// Run a Python script
    Run {
        script: String,
    },
    /// Start OpenAI-compatible API server
    Serve {
        /// Bind host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Bind port
        #[arg(short, long, default_value = "8082")]
        port: u16,
    },
    /// Check local services status
    Status,
    /// List available models
    Models,
    /// List available tools
    Tools,
    /// Show version
    Version,
    /// Search code/files
    Search {
        pattern: String,
        #[arg(default_value = ".")]
        path: String,
        #[arg(short = 't', long)]
        file_type: Option<String>,
        #[arg(short = 'i', long)]
        ignore_case: bool,
    },
    /// Find files by pattern
    Find {
        pattern: String,
        #[arg(default_value = ".")]
        path: String,
        #[arg(short = 't', long)]
        file_type: Option<String>,
        #[arg(long)]
        max_depth: Option<usize>,
    },
    /// Fix code issues
    Fix {
        path: String,
        #[arg(long, default_value = "all")]
        fix_type: String,
        #[arg(long)]
        auto: bool,
    },
    /// Code review
    Review {
        path: String,
        #[arg(long, default_value = "general")]
        focus: String,
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Show diff between files
    Diff {
        file1: String,
        file2: String,
        #[arg(long, default_value = "3")]
        context: usize,
    },
    /// Run tests
    Test {
        #[arg(default_value = ".")]
        path: String,
        #[arg(long)]
        pattern: Option<String>,
        #[arg(short, long)]
        verbose: bool,
        #[arg(long)]
        coverage: bool,
    },
    /// Initialize a new project
    Init {
        #[arg(default_value = "python")]
        project_type: String,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(default_value = ".")]
        path: String,
        #[arg(long)]
        git: bool,
    },
    /// Manage configuration
    Config {
        #[arg(default_value = "list")]
        action: String,
        #[arg(long)]
        key: Option<String>,
        #[arg(long)]
        value: Option<String>,
        #[arg(long)]
        global: bool,
    },
    /// Install dependencies
    Install {
        packages: Vec<String>,
        #[arg(long)]
        requirements: Option<String>,
        #[arg(long)]
        dev: bool,
    },
    /// Update dependencies
    Update {
        packages: Vec<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Execute command in project context
    Exec {
        command: Vec<String>,
        #[arg(long)]
        cwd: Option<String>,
    },
    /// Diagnose environment
    Doctor {
        #[arg(long)]
        fix: bool,
    },
    /// Clean build artifacts
    Clean {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        cache: bool,
        #[arg(long)]
        build: bool,
    },
    /// Generate shell completion
    Completion {
        #[arg(default_value = "bash")]
        shell: String,
    },
    /// Project generation (multi-file)
    Project {
        description: String,
        #[arg(default_value = ".")]
        path: String,
        #[arg(long)]
        spec: Option<String>,
    },
    /// Repair project (analyze and fix)
    Repair {
        #[arg(default_value = ".")]
        path: String,
        #[arg(long)]
        auto: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    if cli.verbose > 0 {
        tracing_subscriber::fmt()
            .with_max_level(match cli.verbose {
                1 => tracing::Level::INFO,
                2 => tracing::Level::DEBUG,
                _ => tracing::Level::TRACE,
            })
            .init();
    }

    // Run command
    match cli.command {
        Commands::Chat { prompt, interactive } => {
            let stream = if cli.no_stream { false } else { cli.stream || true };
            let thinking = if cli.no_thinking { false } else { cli.thinking || true };
            let temp = cli.temp.unwrap_or(0.7);
            let max_tokens = cli.max_tokens.unwrap_or(4096);
            commands::cmd_chat(
                &prompt,
                interactive,
                stream,
                thinking,
                temp,
                max_tokens,
                &cli.model,
                &cli.base_url,
                &cli.lang,
                cli.allow_remote,
            ).await;
        }
        Commands::Generate { prompt } => {
            let temp = cli.temp.unwrap_or(0.7);
            let max_tokens = cli.max_tokens.unwrap_or(2048);
            commands::cmd_generate(
                &prompt,
                cli.stream,
                cli.thinking,
                temp,
                max_tokens,
                &cli.model,
                &cli.base_url,
            ).await;
        }
        Commands::Code { description, filename, max_iterations } => {
            commands::cmd_code(
                &description,
                &filename,
                cli.auto,
                cli.yolo,
                max_iterations,
                &cli.model,
                &cli.base_url,
            ).await;
        }
        Commands::Think { problem } => {
            commands::cmd_think(&problem, &cli.model, &cli.base_url).await;
        }
        Commands::Auto { goal, max_iterations, max_turns, no_cost } => {
            commands::cmd_auto(
                &goal,
                max_iterations,
                max_turns,
                !no_cost,
                &cli.model,
                &cli.base_url,
                cli.allow_remote,
                cli.tools || cli.auto,
            ).await;
        }
        Commands::Run { script } => {
            commands::cmd_run(&script);
        }
        Commands::Serve { host, port } => {
            commands::cmd_serve(&host, port, &cli.model, &cli.base_url).await;
        }
        Commands::Status => {
            commands::cmd_status();
        }
        Commands::Models => {
            commands::cmd_models();
        }
        Commands::Tools => {
            commands::cmd_tools();
        }
        Commands::Version => {
            commands::cmd_version();
        }
        Commands::Search { pattern, path, file_type, ignore_case } => {
            commands::cmd_search(&pattern, &path, file_type.as_deref(), ignore_case);
        }
        Commands::Find { pattern, path, file_type, max_depth } => {
            commands::cmd_find(&pattern, &path, file_type.as_deref(), max_depth);
        }
        Commands::Fix { path, fix_type, auto } => {
            commands::cmd_fix(&path, &fix_type, auto);
        }
        Commands::Review { path, focus, format } => {
            commands::cmd_review(&path, &focus, &format, &cli.model, &cli.base_url).await;
        }
        Commands::Diff { file1, file2, context } => {
            commands::cmd_diff(&file1, &file2, context);
        }
        Commands::Test { path, pattern, verbose, coverage } => {
            commands::cmd_test(&path, pattern.as_deref(), verbose, coverage);
        }
        Commands::Init { project_type, name, path, git } => {
            commands::cmd_init(&project_type, name.as_deref(), &path, git);
        }
        Commands::Config { action, key, value, global } => {
            commands::cmd_config(&action, key.as_deref(), value.as_deref(), global);
        }
        Commands::Install { packages, requirements, dev } => {
            commands::cmd_install(&packages, requirements.as_deref(), dev);
        }
        Commands::Update { packages, all, dry_run } => {
            commands::cmd_update(&packages, all, dry_run);
        }
        Commands::Exec { command, cwd } => {
            commands::cmd_exec(&command, cwd.as_deref());
        }
        Commands::Doctor { fix } => {
            commands::cmd_doctor(fix);
        }
        Commands::Clean { all, cache, build } => {
            commands::cmd_clean(all, cache, build);
        }
        Commands::Completion { shell } => {
            commands::cmd_completion(&shell);
        }
        Commands::Project { description, path, spec } => {
            commands::cmd_project(&description, &path, spec.as_deref(), &cli.model, &cli.base_url).await;
        }
        Commands::Repair { path, auto } => {
            commands::cmd_repair(&path, auto, &cli.model, &cli.base_url).await;
        }
    }

    Ok(())
}