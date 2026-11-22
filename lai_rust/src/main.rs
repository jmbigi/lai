//! lai - Local AI Code Assistant
//! A comprehensive CLI tool for reviewing, repairing, and improving code projects
//! using local LLMs (Qwen3-8B via llama.cpp)

mod commands;
mod llm;
mod services;
mod safety;
// mod tools;        // Has issues with quote crate
// mod automode;     // Has borrow checker issues
// mod agents;       // Has duplicate imports
// mod session;      // OK
// mod thinking;     // OK
// mod config;       // OK

use crate::commands::*;
use clap::Parser;

/// Current version (mirrors the Python project's version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// lai - Local AI Code Assistant
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long, default_value = "qwen3-8b", help = "Model to use")]
    model: String,

    #[arg(
        long,
        default_value = "http://localhost:8080/v1",
        help = "Base URL for API"
    )]
    base_url: String,

    #[arg(long, default_value = "ollama", help = "Model provider")]
    provider: String,

    #[arg(long, default_value = None, help = "API key for remote providers")]
    api_key: Option<String>,

    #[arg(
        long,
        help = "Allow non-local endpoints (bypasses Rule 4 local-only guard)"
    )]
    allow_remote: bool,

    #[arg(
        short = 'l',
        long = "lang",
        default_value = "es",
        help = "Language (es|en, default: es)"
    )]
    lang: String,

    #[arg(
        short = 't',
        long = "temp",
        default_value = "0.7",
        help = "Temperature"
    )]
    temp: f64,

    #[arg(long = "max-tokens", default_value = "4096", help = "Max tokens")]
    max_tokens: i32,

    #[arg(long = "stream", help = "Stream output")]
    stream: bool,

    #[arg(long = "no-stream", help = "Disable streaming")]
    no_stream: bool,

    #[arg(long = "thinking", help = "Enable thinking mode")]
    thinking: bool,

    #[arg(long = "no-thinking", help = "Disable thinking mode")]
    no_thinking: bool,

    #[arg(long = "tools", help = "Enable tools")]
    tools: bool,

    #[arg(long = "no-tools", help = "Disable tools")]
    no_tools: bool,

    #[arg(
        long = "yolo",
        help = "YOLO mode: no confirmations, no approvals, full autonomy"
    )]
    yolo: bool,

    #[arg(long = "auto", help = "Auto mode")]
    auto: bool,

    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, help = "Verbose output")]
    verbose: u8,

    #[arg(short = 'q', long = "quiet", help = "Quiet mode")]
    quiet: bool,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Chat with AI
    Chat {
        #[arg(default_value = "", help = "Prompt")]
        prompt: Vec<String>,
        #[arg(short = 'i', long = "interactive", help = "Interactive REPL")]
        interactive: bool,
        #[arg(long = "stream", help = "Stream output")]
        stream: bool,
        #[arg(long = "no-stream", help = "Disable streaming")]
        no_stream: bool,
        #[arg(long = "thinking", help = "Enable thinking mode")]
        thinking: bool,
        #[arg(long = "no-thinking", help = "Disable thinking mode")]
        no_thinking: bool,
        #[arg(short = 't', long = "temp", help = "Temperature")]
        temp: Option<f64>,
        #[arg(long = "max-tokens", help = "Max tokens")]
        max_tokens: Option<i32>,
    },
    /// Generate text
    Generate {
        #[arg(default_value = "", help = "Prompt")]
        prompt: Vec<String>,
        #[arg(long = "stream", help = "Stream output")]
        stream: bool,
        #[arg(long = "thinking", help = "Enable thinking mode")]
        thinking: bool,
        #[arg(short = 't', long = "temp", help = "Temperature")]
        temp: Option<f64>,
        #[arg(long = "max-tokens", help = "Max tokens")]
        max_tokens: Option<i32>,
    },
    /// Generate code
    Code {
        #[arg(help = "Description")]
        description: String,
        #[arg(help = "Output file")]
        filename: String,
        #[arg(long = "auto")]
        auto: bool,
        #[arg(long = "yolo")]
        yolo: bool,
        #[arg(
            long = "max-iterations",
            default_value = "3",
            help = "Max iterations for code generation"
        )]
        max_iterations: i32,
    },
    /// Thinking mode
    Think {
        #[arg(help = "Problem")]
        problem: String,
    },
    /// Auto mode
    Auto {
        #[arg(help = "Goal")]
        goal: String,
        #[arg(long = "max-iterations", default_value = "10", help = "Max iterations")]
        max_iterations: i32,
        #[arg(
            long = "max-turns",
            default_value = "20",
            help = "Max tool turns (default: 20)"
        )]
        max_turns: i32,
        #[arg(long = "no-cost", help = "Disable cost tracking")]
        no_cost: bool,
    },
    /// Run script
    Run {
        #[arg(help = "Script file")]
        script: String,
    },
    /// Start API server
    Serve {
        #[arg(
            long,
            default_value = "127.0.0.1",
            help = "Bind address (default: loopback)"
        )]
        host: String,
        #[arg(long, default_value = "8082", help = "Port (default: 8082)")]
        port: u16,
    },
    /// Check status
    Status,
    /// List models
    Models,
    /// List tools
    Tools,
    /// Show version
    Version,
    /// Generate shell completion script
    Completion {
        #[arg(default_value = "bash", help = "Shell type (bash|zsh|fish)")]
        shell: String,
    },
    /// Search code/files
    Search {
        #[arg(help = "Pattern to search")]
        pattern: String,
        #[arg(default_value = ".", help = "Path to search")]
        path: String,
        #[arg(help = "File type filter (e.g. py, js, md)")]
        r#type: Option<String>,
        #[arg(short = 'i', long = "ignore-case", help = "Case insensitive")]
        ignore_case: bool,
        #[arg(short = 'r', long = "recursive", help = "Recursive search")]
        recursive: bool,
    },
    /// Find files
    Find {
        #[arg(help = "File name pattern")]
        pattern: String,
        #[arg(default_value = ".", help = "Path to search")]
        path: String,
        #[arg(short = 't', long = "type", help = "File type (f=file, d=dir, l=link)")]
        r#type: Option<String>,
        #[arg(long = "max-depth", help = "Max depth")]
        max_depth: Option<usize>,
    },
    /// Fix code issues
    Fix {
        #[arg(help = "File or directory to fix")]
        path: String,
        #[arg(long = "auto", help = "Auto-fix without confirmation")]
        auto: bool,
        #[arg(long = "type", default_value = "all", help = "Type of fix")]
        fix_type: String,
    },
    /// Alias for fix
    Repair {
        #[arg(help = "File or directory to repair")]
        path: String,
        #[arg(long = "auto")]
        auto: bool,
    },
    /// Code review
    Review {
        #[arg(help = "File or directory to review")]
        path: String,
        #[arg(long = "focus", default_value = "all", help = "Focus area")]
        focus: String,
        #[arg(long = "format", default_value = "text", help = "Output format")]
        format: String,
    },
    /// Show diff
    Diff {
        #[arg(help = "First file")]
        file1: String,
        #[arg(help = "Second file")]
        file2: String,
        #[arg(long = "context", default_value = "3", help = "Context lines")]
        context: usize,
    },
    /// Run tests
    Test {
        #[arg(default_value = ".", help = "Path to test")]
        path: String,
        #[arg(help = "Test file pattern")]
        pattern: Option<String>,
        #[arg(short = 'v', long = "verbose")]
        verbose: bool,
        #[arg(long = "coverage")]
        coverage: bool,
    },
    /// Initialize project
    Init {
        #[arg(long = "type", default_value = "python", help = "Project type")]
        project_type: String,
        #[arg(help = "Project name")]
        name: Option<String>,
        #[arg(long = "path", default_value = ".", help = "Target directory")]
        path: String,
        #[arg(long = "git", help = "Initialize git repo")]
        git: bool,
    },
    /// Generate complete project from description
    Project {
        #[arg(help = "Project description")]
        description: Vec<String>,
        #[arg(long = "path", default_value = ".", help = "Target directory")]
        path: String,
        #[arg(long = "git", help = "Initialize git repo")]
        git: bool,
        #[arg(long = "install", help = "Install dependencies after generation")]
        install: bool,
        #[arg(short = 'y', long = "yes", help = "Skip confirmation prompt")]
        yes: bool,
        #[arg(long = "yolo", help = "YOLO mode (no confirmations)")]
        yolo: bool,
    },
    /// Manage configuration
    Config {
        #[arg(help = "Action")]
        action: String,
        #[arg(help = "Config key")]
        key: Option<String>,
        #[arg(help = "Config value")]
        value: Option<String>,
        #[arg(long = "global", help = "Global config")]
        global: bool,
    },
    /// Install dependencies
    Install {
        #[arg(help = "Packages to install")]
        packages: Vec<String>,
        #[arg(short = 'r', long = "requirements", help = "Requirements file")]
        requirements: Option<String>,
        #[arg(long = "dev", help = "Install dev dependencies")]
        dev: bool,
    },
    /// Update dependencies
    Update {
        #[arg(help = "Packages to update")]
        packages: Vec<String>,
        #[arg(short = 'a', long = "all", help = "Update all")]
        all: bool,
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
    /// Execute command in project context
    Exec {
        #[arg(help = "Command to execute", num_args = 1..)]
        command: Vec<String>,
        #[arg(long = "cwd", help = "Working directory")]
        cwd: Option<String>,
    },
    /// Diagnose environment
    Doctor {
        #[arg(long = "fix", help = "Auto-fix issues")]
        fix: bool,
    },
    /// Clean build artifacts
    Clean {
        #[arg(long = "all", help = "Clean all")]
        all: bool,
        #[arg(long = "cache", help = "Clean cache")]
        cache: bool,
        #[arg(long = "build", help = "Clean build artifacts")]
        build: bool,
    },
    /// Extract text from image with OCR (tesseract)
    Ocr {
        #[arg(help = "Path to image")]
        image: String,
        #[arg(
            long = "lang",
            default_value = "spa+eng",
            help = "Languages for tesseract"
        )]
        lang: String,
        #[arg(short = 'o', long = "output", help = "Save extracted text to file")]
        output: Option<String>,
    },
    /// Start MCP server exposing lai tools
    Mcp,
    /// System One decision client
    SystemOne {
        #[arg(
            long = "endpoint",
            default_value = "http://localhost:11434/v1/systemone",
            help = "System One endpoint"
        )]
        endpoint: String,
        #[arg(long = "model", default_value = "nimble", help = "Decision model name")]
        model: String,
        #[arg(long = "type", required = true, help = "Decision type")]
        decision_type: String,
        #[arg(long = "state", required = true, help = "State/context")]
        state: String,
        #[arg(long = "question", required = true, help = "Question")]
        question: String,
        #[arg(long = "options", help = "Options for choice type")]
        options: Vec<String>,
        #[arg(long = "scale", default_value = "1-10", help = "Scale for score type")]
        scale: String,
        #[arg(
            long = "temp",
            default_value = "0.7",
            help = "Temperature for generation"
        )]
        temp: f64,
        #[arg(long = "precision", default_value = "q4_k_m", help = "Model precision")]
        precision: String,
        #[arg(long = "json", help = "JSON output")]
        json: bool,
    },
    /// Discover and compare local/remote AI models
    Deguro {
        #[arg(
            long = "allow-remote",
            help = "Also scan remote endpoints (non loopback). Requires explicit consent."
        )]
        allow_remote: bool,
        #[arg(
            short = 'e',
            long = "endpoint",
            help = "Additional endpoint to scan (can repeat)"
        )]
        endpoint: Vec<String>,
        #[arg(
            long = "timeout",
            default_value = "3.0",
            help = "Timeout per endpoint in seconds"
        )]
        timeout: f64,
        #[arg(
            short = 't',
            long = "tarea",
            default_value = "",
            help = "Task to run in benchmark between discovered models"
        )]
        tarea: String,
        #[arg(
            short = 'c',
            long = "criterios",
            default_value = "",
            help = "Evaluation criteria separated by comma"
        )]
        criterios: String,
        #[arg(
            long = "repetir",
            default_value = "1",
            help = "Number of repetitions per model"
        )]
        repetir: i32,
    },
    /// Semantic search with local RAG
    Rag {
        #[arg(help = "Search query")]
        query: String,
        #[arg(short = 'c', long = "collection", help = "Collection name to search")]
        collection: Option<String>,
        #[arg(long = "top-k", default_value = "5", help = "Number of results")]
        top_k: i32,
        #[arg(long = "index", help = "Index current directory before searching")]
        index: bool,
        #[arg(long = "chunked", help = "Use smart chunking for code files")]
        chunked: bool,
        #[arg(long = "hybrid", help = "Use hybrid search (vector + keyword)")]
        hybrid: bool,
        #[arg(
            long = "chunk-size",
            default_value = "2000",
            help = "Max chunk size for chunked indexing"
        )]
        chunk_size: i32,
    },
    /// Sub-agents (multi-agent)
    Agents {
        #[arg(help = "Action: list, run, pipeline, collaborative")]
        action: String,
        #[arg(
            short = 'a',
            long = "agent",
            default_value = "code",
            help = "Sub-agent name"
        )]
        agent: String,
        #[arg(
            short = 't',
            long = "task",
            default_value = "",
            help = "Task for the sub-agent"
        )]
        task: String,
        #[arg(
            short = 'g',
            long = "goal",
            default_value = "",
            help = "Goal for collaborative flow"
        )]
        goal: String,
    },
    /// Code completion (LSP-like)
    Complete {
        #[arg(help = "File to complete")]
        file: String,
        #[arg(
            short = 'l',
            long = "line",
            default_value = "0",
            help = "Cursor line (0-indexed)"
        )]
        line: i32,
        #[arg(
            short = 'c',
            long = "column",
            default_value = "0",
            help = "Cursor column (0-indexed)"
        )]
        column: i32,
        #[arg(long = "top-k", default_value = "10", help = "Number of results")]
        top_k: i32,
        #[arg(long = "index", help = "Index project before completing")]
        index: bool,
    },
    /// List code symbols
    Symbols {
        #[arg(help = "Specific file (optional)")]
        file: Option<String>,
        #[arg(
            short = 'k',
            long = "kind",
            help = "Filter by type: class, function, method, import, variable"
        )]
        kind: Option<String>,
    },
    /// Go to symbol definition
    Goto {
        #[arg(help = "Symbol name to find")]
        symbol: String,
    },
    /// Generate repository map
    Repomap {
        #[arg(index = 1, default_value = ".", help = "Path to scan")]
        path: String,
        #[arg(long = "max-chars", default_value = "8000", help = "Max characters")]
        max_chars: i32,
    },
    /// Interactive terminal
    Interactive {
        #[arg(long = "no-stream", help = "Disable response streaming")]
        no_stream: bool,
        #[arg(long = "thinking", help = "Enable thinking mode")]
        thinking: bool,
        #[arg(long = "no-thinking", help = "Disable thinking mode")]
        no_thinking: bool,
        #[arg(
            short = 't',
            long = "temp",
            default_value = "0.7",
            help = "Model temperature"
        )]
        temp: f64,
        #[arg(long = "max-tokens", default_value = "4096", help = "Max tokens")]
        max_tokens: i32,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.quiet {
        // Quiet mode: suppress non-essential output
    }

    match cli.command {
        Some(Commands::Chat {
            prompt,
            interactive,
            stream,
            no_stream,
            thinking,
            no_thinking,
            temp,
            max_tokens,
        }) => {
            cmd_chat(
                &prompt,
                interactive,
                stream,
                no_stream,
                thinking,
                no_thinking,
                temp,
                max_tokens,
                &cli.model,
                &cli.base_url,
                &cli.lang,
                cli.allow_remote,
            )
            .await;
        }
        Some(Commands::Generate {
            prompt,
            stream,
            thinking,
            temp,
            max_tokens,
        }) => {
            cmd_generate(
                &prompt,
                stream,
                thinking,
                temp,
                max_tokens,
                &cli.model,
                &cli.base_url,
                cli.allow_remote,
            )
            .await;
        }
        Some(Commands::Code {
            description,
            filename,
            auto,
            yolo,
            max_iterations,
        }) => {
            cmd_code(
                &description,
                &filename,
                auto,
                yolo,
                max_iterations,
                &cli.model,
                &cli.base_url,
                cli.allow_remote,
            )
            .await;
        }
        Some(Commands::Think { problem }) => {
            cmd_think(&problem, &cli.model, &cli.base_url, cli.allow_remote).await;
        }
        Some(Commands::Auto {
            goal,
            max_iterations,
            max_turns,
            no_cost,
        }) => {
            cmd_auto(
                &goal,
                max_iterations as usize,
                max_turns as usize,
                !no_cost,
                &cli.model,
                &cli.base_url,
                cli.allow_remote,
                cli.tools || cli.auto,
            )
            .await;
        }
        Some(Commands::Run { script }) => {
            cmd_run(&script);
        }
        Some(Commands::Serve { host, port }) => {
            cmd_serve(&host, port, &cli.model, &cli.base_url);
        }
        Some(Commands::Status) => {
            cmd_status().await;
        }
        Some(Commands::Models) => {
            cmd_models();
        }
        Some(Commands::Tools) => {
            cmd_tools();
        }
        Some(Commands::Version) => {
            cmd_version();
        }
        Some(Commands::Completion { shell }) => {
            cmd_completion(&shell);
        }
        Some(Commands::Search {
            pattern,
            path,
            r#type,
            ignore_case,
            recursive: _,
        }) => {
            cmd_search(&pattern, &path, r#type.as_deref(), ignore_case);
        }
        Some(Commands::Find {
            pattern,
            path,
            r#type,
            max_depth,
        }) => {
            cmd_find(&pattern, &path, r#type.as_deref(), max_depth);
        }
        Some(Commands::Fix {
            path,
            auto,
            fix_type,
        }) => {
            cmd_fix(&path, &fix_type, auto);
        }
        Some(Commands::Repair { path, auto }) => {
            cmd_fix(&path, "all", auto);
        }
        Some(Commands::Review {
            path,
            focus,
            format,
        }) => {
            cmd_review(
                &path,
                &focus,
                &format,
                &cli.model,
                &cli.base_url,
                cli.allow_remote,
            )
            .await;
        }
        Some(Commands::Diff {
            file1,
            file2,
            context,
        }) => {
            cmd_diff(&file1, &file2, context);
        }
        Some(Commands::Test {
            path,
            pattern,
            verbose,
            coverage,
        }) => {
            cmd_test(&path, pattern.as_deref(), verbose, coverage);
        }
        Some(Commands::Init {
            project_type,
            name,
            path,
            git,
        }) => {
            cmd_init(&project_type, name.as_deref(), &path, git);
        }
        Some(Commands::Project {
            description,
            path: _,
            git: _,
            install: _,
            yes: _,
            yolo: _,
        }) => {
            println!(
                "Project generation: not yet ported to Rust. Description: {}",
                description.join(" ")
            );
        }
        Some(Commands::Config {
            action,
            key,
            value,
            global,
        }) => {
            cmd_config(&action, key.as_deref(), value.as_deref(), global);
        }
        Some(Commands::Install {
            packages,
            requirements,
            dev,
        }) => {
            cmd_install(&packages, requirements.as_deref(), dev);
        }
        Some(Commands::Update {
            packages,
            all,
            dry_run,
        }) => {
            cmd_update(&packages, all, dry_run);
        }
        Some(Commands::Exec { command, cwd }) => {
            cmd_exec(&command, cwd.as_deref());
        }
        Some(Commands::Doctor { fix }) => {
            cmd_doctor(fix);
        }
        Some(Commands::Clean { all, cache, build }) => {
            cmd_clean(all, cache, build);
        }
        Some(Commands::Ocr {
            image: _,
            lang: _,
            output: _,
        }) => {
            println!("OCR: not yet ported to Rust.");
        }
        Some(Commands::Mcp) => {
            println!("MCP server: not yet ported to Rust.");
        }
        Some(Commands::SystemOne { .. }) => {
            println!("System One: not yet ported to Rust.");
        }
        Some(Commands::Deguro { .. }) => {
            println!("Deguro: not yet ported to Rust.");
        }
        Some(Commands::Rag { .. }) => {
            println!("RAG: not yet ported to Rust.");
        }
        Some(Commands::Agents { .. }) => {
            println!("Agents: not yet ported to Rust.");
        }
        Some(Commands::Complete { .. }) => {
            println!("Complete: not yet ported to Rust.");
        }
        Some(Commands::Symbols { .. }) => {
            println!("Symbols: not yet ported to Rust.");
        }
        Some(Commands::Goto { .. }) => {
            println!("Goto: not yet ported to Rust.");
        }
        Some(Commands::Repomap { path, max_chars }) => {
            cmd_repomap(&path, max_chars as usize);
        }
        Some(Commands::Interactive {
            no_stream,
            thinking,
            no_thinking,
            temp,
            max_tokens,
        }) => {
            cmd_chat(
                &[],
                true,
                !no_stream,
                no_stream,
                thinking,
                no_thinking,
                Some(temp),
                Some(max_tokens),
                &cli.model,
                &cli.base_url,
                &cli.lang,
                cli.allow_remote,
            )
            .await;
        }
        None => {
            // No command: enter interactive chat mode by default
            cmd_chat(
                &[],
                true,
                cli.stream,
                cli.no_stream,
                cli.thinking,
                cli.no_thinking,
                Some(cli.temp),
                Some(cli.max_tokens),
                &cli.model,
                &cli.base_url,
                &cli.lang,
                cli.allow_remote,
            )
            .await;
        }
    }
}
