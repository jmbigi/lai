//! lai - Local AI Code Assistant

mod commands;
mod llm;
mod services;

use clap::Parser;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(clap::Parser, Debug)]
#[command(author, version, about = "lai - Local AI Code Assistant", long_about = None)]
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
    Code {
        #[arg(help = "Description")]
        description: String,
        #[arg(help = "Output file")]
        filename: String,
        #[arg(long = "auto", help = "Save without confirmation")]
        auto: bool,
        #[arg(long = "yolo", help = "Save without confirmation")]
        yolo: bool,
        #[arg(
            long = "max-iterations",
            default_value = "2",
            help = "Max repair rounds per candidate"
        )]
        max_iterations: i32,
        #[arg(
            long = "candidates",
            default_value = "2",
            help = "Independent candidates (best-of-N, stops early when verified)"
        )]
        candidates: i32,
        #[arg(long = "no-tests", help = "Skip spec-derived test generation/execution")]
        no_tests: bool,
        #[arg(long = "no-lint", help = "Skip ruff critical-lint check")]
        no_lint: bool,
        #[arg(long = "no-verify", help = "Skip verification (legacy behaviour)")]
        no_verify: bool,
        #[arg(
            long = "strict",
            help = "Do not write the file unless the candidate is verified"
        )]
        strict: bool,
    },
    Think {
        #[arg(help = "Problem")]
        problem: String,
    },
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
    Run {
        #[arg(help = "Script file")]
        script: String,
    },
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
    Status,
    Models,
    Tools,
    Version,
    Completion {
        #[arg(default_value = "bash", help = "Shell type (bash|zsh|fish)")]
        shell: String,
    },
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
    Fix {
        #[arg(help = "File or directory to fix")]
        path: String,
        #[arg(long = "auto", help = "Auto-fix without confirmation")]
        auto: bool,
        #[arg(long = "type", default_value = "all", help = "Type of fix")]
        fix_type: String,
    },
    Repair {
        #[arg(help = "File or directory to repair")]
        path: String,
        #[arg(long = "auto")]
        auto: bool,
    },
    Review {
        #[arg(help = "File or directory to review")]
        path: String,
        #[arg(long = "focus", default_value = "all", help = "Focus area")]
        focus: String,
        #[arg(long = "format", default_value = "text", help = "Output format")]
        format: String,
    },
    Diff {
        #[arg(help = "First file")]
        file1: String,
        #[arg(help = "Second file")]
        file2: String,
        #[arg(long = "context", default_value = "3", help = "Context lines")]
        context: usize,
    },
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
    Install {
        #[arg(help = "Packages to install")]
        packages: Vec<String>,
        #[arg(short = 'r', long = "requirements", help = "Requirements file")]
        requirements: Option<String>,
        #[arg(long = "dev", help = "Install dev dependencies")]
        dev: bool,
    },
    Update {
        #[arg(help = "Packages to update")]
        packages: Vec<String>,
        #[arg(short = 'a', long = "all", help = "Update all")]
        all: bool,
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
    Exec {
        #[arg(help = "Command to execute", num_args = 1..)]
        command: Vec<String>,
        #[arg(long = "cwd", help = "Working directory")]
        cwd: Option<String>,
    },
    Doctor {
        #[arg(long = "fix", help = "Auto-fix issues")]
        fix: bool,
    },
    Clean {
        #[arg(long = "all", help = "Clean all")]
        all: bool,
        #[arg(long = "cache", help = "Clean cache")]
        cache: bool,
        #[arg(long = "build", help = "Clean build artifacts")]
        build: bool,
    },
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
    Mcp,
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
            commands::cmd_chat(
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
            let prompt_str = prompt.join(" ");
            let mut llm = crate::llm::LlmClient::new(&cli.model, &cli.base_url, cli.allow_remote);
            let result = llm.chat(&prompt_str, stream, thinking).await;
            if let Some(err) = &result.error {
                eprintln!("{}", err);
            } else {
                println!("{}", result.content);
            }
        }
        Some(Commands::Code {
            description,
            filename,
            auto,
            yolo,
            max_iterations,
            candidates,
            no_tests,
            no_lint,
            no_verify,
            strict,
        }) => {
            commands::cmd_code(
                &description,
                &filename,
                auto,
                yolo,
                max_iterations,
                candidates,
                !no_tests,
                !no_lint,
                !no_verify,
                strict,
                &cli.model,
                &cli.base_url,
                cli.allow_remote,
            )
            .await;
        }
        Some(Commands::Think { problem }) => {
            commands::cmd_think(&problem, &cli.model, &cli.base_url, cli.allow_remote).await;
        }
        Some(Commands::Auto {
            goal,
            max_iterations,
            max_turns,
            no_cost,
        }) => {
            commands::cmd_auto(
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
            commands::cmd_run(&script);
        }
        Some(Commands::Serve { host, port }) => {
            commands::cmd_serve(&host, port, &cli.model, &cli.base_url).await;
        }
        Some(Commands::Status) => {
            commands::cmd_status().await;
        }
        Some(Commands::Models) => {
            commands::cmd_models();
        }
        Some(Commands::Tools) => {
            commands::cmd_tools();
        }
        Some(Commands::Version) => {
            commands::cmd_version();
        }
        Some(Commands::Completion { shell }) => {
            commands::cmd_completion(&shell);
        }
        Some(Commands::Search {
            pattern,
            path,
            r#type,
            ignore_case,
            recursive: _,
        }) => {
            commands::cmd_search(&pattern, &path, r#type.as_deref(), ignore_case);
        }
        Some(Commands::Find {
            pattern,
            path,
            r#type,
            max_depth,
        }) => {
            commands::cmd_find(&pattern, &path, r#type.as_deref(), max_depth);
        }
        Some(Commands::Fix {
            path,
            auto,
            fix_type,
        }) => {
            commands::cmd_fix(&path, &fix_type, auto);
        }
        Some(Commands::Repair { path, auto }) => {
            commands::cmd_fix(&path, "all", auto);
        }
        Some(Commands::Review {
            path,
            focus,
            format,
        }) => {
            commands::cmd_review(
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
            commands::cmd_diff(&file1, &file2, context);
        }
        Some(Commands::Test {
            path,
            pattern,
            verbose,
            coverage,
        }) => {
            commands::cmd_test(&path, pattern.as_deref(), verbose, coverage);
        }
        Some(Commands::Init {
            project_type,
            name,
            path,
            git,
        }) => {
            commands::cmd_init(&project_type, name.as_deref(), &path, git);
        }
        Some(Commands::Project {
            description,
            path: _,
            git: _,
            install: _,
            yes: _,
            yolo: _,
        }) => {
            commands::cmd_project(&description.join(" ")).await;
        }
        Some(Commands::Config {
            action,
            key,
            value,
            global,
        }) => {
            commands::cmd_config(&action, key.as_deref(), value.as_deref(), global);
        }
        Some(Commands::Install {
            packages,
            requirements,
            dev,
        }) => {
            commands::cmd_install(&packages, requirements.as_deref(), dev);
        }
        Some(Commands::Update {
            packages,
            all,
            dry_run,
        }) => {
            commands::cmd_update(&packages, all, dry_run);
        }
        Some(Commands::Exec { command, cwd }) => {
            commands::cmd_exec(&command, cwd.as_deref());
        }
        Some(Commands::Doctor { fix }) => {
            commands::cmd_doctor(fix);
        }
        Some(Commands::Clean { all, cache, build }) => {
            commands::cmd_clean(all, cache, build);
        }
        Some(Commands::Ocr {
            image,
            lang,
            output,
        }) => {
            commands::cmd_ocr(&image, &lang, output.as_deref());
        }
        Some(Commands::Mcp) => {
            commands::cmd_mcp().await;
        }
        Some(Commands::Interactive {
            no_stream,
            thinking,
            no_thinking,
            temp,
            max_tokens,
        }) => {
            commands::cmd_chat(
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
                cli.allow_remote,
            )
            .await;
        }
        None => {
            println!("lai - Local AI Code Assistant v{}", VERSION);
            println!("Use `lai --help` for usage.");
        }
    }
}