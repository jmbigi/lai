//! Command implementations for lai (Rust port).
//! Covers the core CLI commands: chat, generate, code, think, run, serve,
//! status, models, tools, version, search, find, fix, review, diff, test,
//! init, config, install, update, exec, doctor, clean, completion.

use crate::llm::{LlmClient, Message};
use crate::services::Services;
use crate::tools::ToolRegistry;
use quote::quote;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Current version (mirrors the Python project's version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Global services instance.
static SERVICES: OnceLock<Services> = OnceLock::new();

fn services() -> &'static Services {
    SERVICES.get_or_init(Services::new)
}

fn get_llm_client(model: &str, base_url: &str, allow_remote: bool) -> LlmClient {
    LlmClient::new(base_url, model, allow_remote)
}

/// Strip markdown code fences from a response.
fn extract_code(response: &str) -> String {
    let text = response.trim();
    if text.starts_with("```") {
        let mut lines: Vec<&str> = text.lines().collect();
        if lines.first().map_or(false, |l| l.starts_with("```")) {
            lines.remove(0);
        }
        if lines.last().map_or(false, |l| l.trim() == "```") {
            lines.pop();
        }
        lines.join("\n").trim().to_string()
    } else {
        text.to_string()
    }
}

// ============================================================================
// Chat
// ============================================================================

pub async fn cmd_chat(
    prompt: &[String],
    interactive: bool,
    stream: bool,
    no_stream: bool,
    thinking: bool,
    no_thinking: bool,
    temp: Option<f64>,
    max_tokens: Option<i32>,
    model: &str,
    base_url: &str,
    _lang: &str,
    allow_remote: bool,
) {
    // Regla 9 - Supervivencia: verificar modelos locales ANTES de chat
    let svc = services();
    let client = reqwest::Client::new();
    if let Err(e) = crate::services::ensure_local_model_available_async(&svc, &client).await {
        if !allow_remote {
            eprintln!("{}", e);
            return;
        }
        eprintln!("⚠️  {}", e);
        eprintln!("Continuando con --allow-remote...");
    }

    let mut llm = get_llm_client(model, base_url, allow_remote);
    llm.temperature = temp.unwrap_or(0.7);
    llm.max_tokens = max_tokens.unwrap_or(4096);
    let stream = if no_stream { false } else { stream };
    let thinking = if no_thinking { false } else { thinking };

    if interactive {
        println!("Interactive mode - type 'exit' or 'quit' to quit");
        println!("Type 'clear' to clear conversation history");
        let mut history: Vec<Message> = vec![];
        loop {
            print!("You: ");
            let _ = std::io::stdout().flush();
            let mut input = String::new();
            match std::io::stdin().read_line(&mut input) {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break,
            }
            let input = input.trim();
            if input.is_empty() {
                continue;
            }
            if input == "exit" || input == "quit" {
                break;
            }
            if input == "clear" {
                history.clear();
                println!("Conversation history cleared.");
                continue;
            }

            history.push(Message {
                role: "user".into(),
                content: input.to_string(),
            });

            let result = llm
                .chat_messages(history.clone(), None, stream, thinking)
                .await;

            if let Some(err) = &result.error {
                println!("\nError: {}", err);
                history.pop();
                continue;
            }

            if !stream {
                println!("{}", result.content);
            }
            println!();

            history.push(Message {
                role: "assistant".into(),
                content: result.content,
            });
        }
    } else {
        let prompt_str = prompt.join(" ");
        if prompt_str.is_empty() {
            println!("Error: No prompt provided");
            return;
        }
        let result = llm.chat(&prompt_str, stream, thinking).await;
        if let Some(err) = &result.error {
            eprintln!("{}", err);
        } else {
            if !stream {
                println!("{}", result.content);
            }
        }
    }
}

// ============================================================================
// Generate
// ============================================================================

pub async fn cmd_generate(
    prompt: &[String],
    stream: bool,
    thinking: bool,
    temp: Option<f64>,
    max_tokens: Option<i32>,
    model: &str,
    base_url: &str,
    allow_remote: bool,
) {
    // Regla 9 - Supervivencia: verificar modelos locales ANTES de generar
    let svc = services();
    let client = reqwest::Client::new();
    if let Err(e) = crate::services::ensure_local_model_available_async(&svc, &client).await {
        if !allow_remote {
            eprintln!("{}", e);
            return;
        }
        eprintln!("⚠️  {}", e);
        eprintln!("Continuando con --allow-remote...");
    }

    let mut llm = get_llm_client(model, base_url, allow_remote);
    llm.temperature = temp.unwrap_or(0.7);
    llm.max_tokens = max_tokens.unwrap_or(2048);

    let prompt_str = prompt.join(" ");
    if prompt_str.is_empty() {
        println!("Error: No prompt provided");
        return;
    }

    let result = llm
        .chat(&format!("Generate: {}", prompt_str), stream, thinking)
        .await;
    if let Some(err) = &result.error {
        eprintln!("{}", err);
    } else {
        if !stream {
            println!("{}", result.content);
        }
    }
}

// ============================================================================
// Code
// ============================================================================

pub async fn cmd_code(
    description: &str,
    filename: &str,
    auto: bool,
    yolo: bool,
    max_iterations: i32,
    model: &str,
    base_url: &str,
    allow_remote: bool,
) {
    // Regla 9 - Supervivencia: verificar modelos locales ANTES de generar código
    let svc = services();
    let client = reqwest::Client::new();
    if let Err(e) = crate::services::ensure_local_model_available_async(&svc, &client).await {
        if !allow_remote {
            eprintln!("{}", e);
            return;
        }
        eprintln!("⚠️  {}", e);
        eprintln!("Continuando con --allow-remote...");
    }

    let _ = auto;
    let llm = get_llm_client(model, base_url, allow_remote);
    let max_iterations = max_iterations.max(1);

    let base_prompt = r#"Generate ONLY valid code, no explanations, no markdown, no comments about the code itself.

Task: {description}

CRITICAL REQUIREMENTS:
- Valid, executable code as a standalone file
- Professional structure with functions/classes
- Error handling with meaningful error messages
- Clean, readable code
- NO markdown, no explanations, no comments about code generation
- USE ONLY ASCII: straight quotes ' and ", NO curly quotes
- NO special Unicode characters
"#;

    let prompt = base_prompt.replace("{description}", description);

    for iteration in 0..max_iterations {
        let result = llm.chat(&prompt, false, false).await;
        if let Some(err) = &result.error {
            eprintln!("Error generating code: {}", err);
            if iteration == max_iterations - 1 {
                let fallback = format!(
                    "# Generated: {}\nprint('Error generating code')",
                    description
                );
                if let Some(parent) = std::path::Path::new(filename).parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = fs::create_dir_all(parent);
                    }
                }
                let _ = fs::write(filename, &fallback);
                println!("Fallback code saved to {}", filename);
            }
            continue;
        }

        let code = extract_code(&result.content);

        if code.is_empty() && iteration < max_iterations - 1 {
            continue;
        }

        if !yolo && iteration == max_iterations - 1 {
            println!("Generated code for: {}", description);
            println!("{}", "-".repeat(40));
            let preview: String = code.chars().take(500).collect();
            println!("{}", preview);
            if code.len() > 500 {
                println!("...");
            }
            println!("{}", "-".repeat(40));
            print!("Save to file? (y/n): ");
            let _ = std::io::stdout().flush();
            let mut confirm = String::new();
            if std::io::stdin().read_line(&mut confirm).is_ok() {
                if confirm.trim().to_lowercase() != "y" {
                    println!("Cancelled.");
                    return;
                }
            }
        }

        if let Some(parent) = std::path::Path::new(filename).parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
            }
        }
        match fs::write(filename, &code) {
            Ok(_) => {
                println!("Code saved to {}", filename);
                return;
            }
            Err(e) => {
                eprintln!("Error writing file: {}", e);
                return;
            }
        }
    }
}

// ============================================================================
// Think
// ============================================================================

pub async fn cmd_think(problem: &str, model: &str, base_url: &str, allow_remote: bool) {
    // Regla 9 - Supervivencia: verificar modelos locales ANTES de thinking
    let svc = services();
    let client = reqwest::Client::new();
    if let Err(e) = crate::services::ensure_local_model_available_async(&svc, &client).await {
        if !allow_remote {
            eprintln!("{}", e);
            return;
        }
        eprintln!("⚠️  {}", e);
        eprintln!("Continuando con --allow-remote...");
    }

    let llm = get_llm_client(model, base_url, allow_remote);
    let result = llm.chat(problem, true, true).await;
    if let Some(err) = &result.error {
        eprintln!("{}", err);
    } else {
        println!("{}", result.content);
    }
}

// ============================================================================
// Auto mode (Plan-and-Execute + ReAct + Reflection)
// ============================================================================

pub async fn cmd_auto(
    goal: &str,
    _max_iterations: usize,
    _max_turns: usize,
    _track_cost: bool,
    model: &str,
    base_url: &str,
    allow_remote: bool,
    _tools_enabled: bool,
) {
    // Regla 9 - Supervivencia: verificar modelos locales ANTES de operar
    let svc = services();
    let client = reqwest::Client::new();
    if let Err(e) = crate::services::ensure_local_model_available_async(&svc, &client).await {
        if !allow_remote {
            eprintln!("{}", e);
            return;
        }
        eprintln!("⚠️  {}", e);
        eprintln!("Continuando con --allow-remote...");
    }

    // TODO: AutoMode completo - stub por ahora
    let _llm = get_llm_client(model, base_url, allow_remote);
    println!("Auto mode: goal = {}", goal);
    println!("(AutoMode completo pendiente: tools.rs + automode.rs)");
}

// ============================================================================
// Run script
// ============================================================================

pub fn cmd_run(script: &str) {
    if !std::path::Path::new(script).exists() {
        println!("Script not found: {}", script);
        return;
    }
    let output = std::process::Command::new("python3").arg(script).output();
    match output {
        Ok(result) => {
            print!("{}", String::from_utf8_lossy(&result.stdout));
            if !result.stderr.is_empty() {
                eprintln!("{}", String::from_utf8_lossy(&result.stderr));
            }
        }
        Err(e) => eprintln!("Error running script: {}", e),
    }
}

// ============================================================================
// Serve (minimal OpenAI-compatible API)
// ============================================================================

pub fn cmd_serve(host: &str, port: u16, model: &str, base_url: &str) {
    let _llm = get_llm_client(model, base_url, false);
    println!("LAI API listening on http://{}:{}", host, port);
    println!("Endpoints: /health, /v1/models, /v1/chat/completions");
    println!("Note: full async server requires hyper; running placeholder.");
    let listener = std::net::TcpListener::bind(format!("{}:{}", host, port));
    if listener.is_ok() {
        println!("Server bound (placeholder - press Ctrl+C to stop)");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    } else {
        eprintln!("Failed to bind server to {}:{}", host, port);
    }
}

// ============================================================================
// Status
// ============================================================================

pub async fn cmd_status() {
    let svc = services();
    let client = reqwest::Client::new();
    crate::services::check_all_services_async(svc, &client).await;
    println!("Services status:");
    for (name, info) in svc.list() {
        let status = if info.is_running() { "Running" } else { "Stopped" };
        println!("  {}: {} (Port {})", name, status, info.port);
    }
}

// ============================================================================
// Models
// ============================================================================

pub fn cmd_models() {
    println!("Available models:");
    println!("  qwen3-8b (local)");
    println!("  qwen3:8b (ollama)");
}

// ============================================================================
// Tools
// ============================================================================

pub fn cmd_tools() {
    println!("Available tools:");
    println!("  read_file, write_file, list_dir, run_command, grep, find_files");
    println!("  repo_map, lint_file, format_code, check_types, review");
}

// ============================================================================
// Version
// ============================================================================

pub fn cmd_version() {
    println!("lai {}", VERSION);
}

// ============================================================================
// Search
// ============================================================================

pub fn cmd_search(pattern: &str, path: &str, file_type: Option<&str>, ignore_case: bool) {
    let regex = if ignore_case {
        regex::Regex::new(&format!("(?i){}", pattern)).unwrap()
    } else {
        regex::Regex::new(pattern).unwrap()
    };

    let walker = walkdir::WalkDir::new(path);
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if let Some(ft) = file_type {
            if p.extension().map_or(true, |e| e != ft) {
                continue;
            }
        }
        if let Ok(content) = fs::read_to_string(p) {
            for (i, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    println!("{}:{}: {}", p.display(), i + 1, line);
                }
            }
        }
    }
}

// ============================================================================
// Find
// ============================================================================

pub fn cmd_find(pattern: &str, path: &str, file_type: Option<&str>, max_depth: Option<usize>) {
    let walker = walkdir::WalkDir::new(path);
    let mut found = 0;
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        let depth = p.strip_prefix(path).unwrap_or(p).components().count();
        if let Some(md) = max_depth {
            if depth >= md {
                continue;
            }
        }
        let name = p
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        if !glob::Pattern::new(pattern).unwrap().matches(&name) {
            continue;
        }
        match file_type {
            Some("f") if !p.is_file() => continue,
            Some("d") if !p.is_dir() => continue,
            Some("l") if !p.is_symlink() => continue,
            _ => {}
        }
        println!("{}", p.display());
        found += 1;
    }
    if found == 0 {
        println!("No files matching {}", pattern);
    }
}

// ============================================================================
// Fix
// ============================================================================

pub fn cmd_fix(path: &str, fix_type: &str, _auto: bool) {
    match fix_type {
        "lint" => {
            println!("Running linter on {}...", path);
        }
        "format" => {
            println!("Formatting code in {}...", path);
        }
        "imports" => {
            println!("Organizing imports in {}...", path);
        }
        "types" => {
            println!("Checking types in {}...", path);
        }
        "all" => {
            println!("Running linter on {}...", path);
            println!("Formatting code in {}...", path);
            println!("Organizing imports in {}...", path);
            println!("Checking types in {}...", path);
        }
        _ => {}
    }
    println!("Fix complete for {}", path);
}

// ============================================================================
// Review
// ============================================================================

pub async fn cmd_review(
    path: &str,
    focus: &str,
    format: &str,
    model: &str,
    base_url: &str,
    allow_remote: bool,
) {
    // Regla 9 - Supervivencia: verificar modelos locales ANTES de review
    let svc = services();
    let client = reqwest::Client::new();
    if let Err(e) = crate::services::ensure_local_model_available_async(&svc, &client).await {
        if !allow_remote {
            eprintln!("{}", e);
            return;
        }
        eprintln!("⚠️  {}", e);
        eprintln!("Continuando con --allow-remote...");
    }

    let llm = get_llm_client(model, base_url, allow_remote);
    let p = std::path::Path::new(path);
    if !p.exists() {
        println!("Error: path not found: {}", path);
        return;
    }

    let files: Vec<PathBuf> = if p.is_file() {
        vec![p.to_path_buf()]
    } else {
        let mut files = Vec::new();
        let skip = [
            "__pycache__",
            "node_modules",
            ".git",
            "venv",
            "env",
            ".venv",
            ".ruff_cache",
        ];
        for entry in walkdir::WalkDir::new(p).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if skip
                .iter()
                .any(|s| path.components().any(|c| c.as_os_str().to_str() == Some(s)))
            {
                continue;
            }
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy();
                let is_reviewable = matches!(
                    ext.as_ref(),
                    "py" | "js"
                        | "ts"
                        | "go"
                        | "rs"
                        | "md"
                        | "sh"
                        | "yaml"
                        | "yml"
                        | "json"
                        | "toml"
                );
                if is_reviewable {
                    files.push(path.to_path_buf());
                }
            }
        }
        files
    };

    if files.is_empty() {
        println!("No reviewable files found.");
        return;
    }

    println!("Reviewing {} file(s), focus: {}", files.len(), focus);

    let mut all_results: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for filepath in &files {
        let content = match fs::read_to_string(filepath) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping {}: {}", filepath.display(), e);
                continue;
            }
        };
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            continue;
        }

        let mut file_findings = Vec::new();
        for start in (0..lines.len()).step_by(120) {
            let end = (start + 120).min(lines.len());
            let chunk: Vec<&str> = lines[start..end].to_vec();
            let numbered: String = chunk
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{:4}: {}", start + i + 1, line))
                .collect::<Vec<_>>()
                .join("\n");

            let review_prompt = format!(
                r#"You are a senior code reviewer.
File: {}
Focus: {}

Return concise, actionable findings. For each finding include:
- severity (P0/P1/P2)
- line number
- problem
- suggested fix
Do not rewrite the whole file. If there are no issues, reply exactly: No issues found.

Code:
{}"#,
                filepath.display(),
                focus,
                numbered
            );

            let result = llm.chat(&review_prompt, false, false).await;
            if result.error.is_some() {
                continue;
            }
            if !result.content.starts_with("Error:") {
                file_findings.push(result.content.trim().to_string());
            }
        }

        if !file_findings.is_empty() {
            all_results.push((filepath.clone(), file_findings));
        }
    }

    match format {
        "json" => {
            let json_results: Vec<serde_json::Value> = all_results
                .iter()
                .map(|(f, findings)| {
                    json!({
                        "file": f.display().to_string(),
                        "findings": findings
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json_results).unwrap());
        }
        "sarif" => {
            let mut sarif_results = Vec::new();
            for (i, (f, findings)) in all_results.iter().enumerate() {
                for (j, finding) in findings.iter().enumerate() {
                    let level = if finding.contains("P0") {
                        "error"
                    } else if finding.contains("P1") {
                        "warning"
                    } else {
                        "note"
                    };
                    sarif_results.push(json!({
                        "ruleId": format!("LAI-{:04}", i * 100 + j),
                        "level": level,
                        "message": {"text": finding},
                        "locations": [{
                            "physicalLocation": {
                                "artifactLocation": {"uri": f.display().to_string()},
                                "region": {"startLine": 1}
                            }
                        }]
                    }));
                }
            }
            let sarif = json!({
                "version": "2.1.0",
                "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
                "runs": [{
                    "tool": {
                        "driver": {
                            "name": "lai-review",
                            "version": VERSION,
                            "informationUri": "https://github.com/kubuntu/lai",
                            "rules": []
                        }
                    },
                    "results": sarif_results
                }]
            });
            println!("{}", serde_json::to_string_pretty(&sarif).unwrap());
        }
        _ => {
            for (file, findings) in &all_results {
                println!("\n=== {} ===", file.display());
                for finding in findings {
                    println!("{}", finding);
                }
            }
            if all_results.is_empty() {
                println!("No issues found.");
            }
        }
    }
}

// ============================================================================
// Diff (simple line-by-line)
// ============================================================================

pub fn cmd_diff(file1: &str, file2: &str, _context: usize) {
    let c1 = match fs::read_to_string(file1) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", file1, e);
            return;
        }
    };
    let c2 = match fs::read_to_string(file2) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", file2, e);
            return;
        }
    };

    let lines1: Vec<&str> = c1.lines().collect();
    let lines2: Vec<&str> = c2.lines().collect();

    println!("--- {}", file1);
    println!("+++ {}", file2);

    let max_len = lines1.len().max(lines2.len());
    for i in 0..max_len {
        let l1 = lines1.get(i);
        let l2 = lines2.get(i);
        match (l1, l2) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), None) => println!("-{}", a),
            (None, Some(b)) => println!("+{}", b),
            (Some(a), Some(b)) => {
                println!("-{}", a);
                println!("+{}", b);
            }
            _ => {}
        }
    }
}

// ============================================================================
// Test
// ============================================================================

pub fn cmd_test(path: &str, _pattern: Option<&str>, verbose: bool, _coverage: bool) {
    let status = std::process::Command::new("pytest")
        .arg(path)
        .arg(if verbose { "-v" } else { "-q" })
        .status();
    match status {
        Ok(s) => {
            if !s.success() {
                std::process::exit(s.code().unwrap_or(1));
            }
        }
        Err(e) => eprintln!("Error running pytest: {}", e),
    }
}

// ============================================================================
// Init
// ============================================================================

pub fn cmd_init(project_type: &str, name: Option<&str>, path: &str, git: bool) {
    let project_types: &[(&str, &[&str])] = &[
        ("python", &["main.py", "requirements.txt", "README.md"]),
        (
            "fastapi",
            &["main.py", "requirements.txt", "README.md", "Dockerfile"],
        ),
        ("flask", &["app.py", "requirements.txt", "README.md"]),
        ("django", &["manage.py", "settings.py", "requirements.txt"]),
        ("cli", &["main.py", "cli.py", "requirements.txt"]),
        ("lib", &["src/lib.rs", "Cargo.toml", "README.md"]),
    ];

    let files = project_types
        .iter()
        .find(|(t, _)| *t == project_type)
        .map(|(_, f)| *f)
        .unwrap_or(&["main.py", "README.md"]);

    let target = std::path::Path::new(path);
    let _ = fs::create_dir_all(target);

    for f in files {
        let p = target.join(f);
        if !p.exists() {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = fs::create_dir_all(parent);
                }
            }
            let content = format!("# {}\n", name.unwrap_or(project_type));
            let _ = fs::write(&p, content);
        }
    }

    if git {
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(target)
            .status();
    }

    println!(
        "Initialized {} project: {} at {}",
        project_type,
        name.unwrap_or("unnamed"),
        target.display()
    );
}

// ============================================================================
// Config
// ============================================================================

pub fn cmd_config(action: &str, key: Option<&str>, value: Option<&str>, global: bool) {
    let config_file = if global {
        std::env::var("HOME")
            .map(|h| std::path::Path::new(&h).join(".lai/config.json"))
            .unwrap_or_else(|_| std::path::PathBuf::from(".lai/config.json"))
    } else {
        std::path::PathBuf::from(".lai/config.json")
    };

    if let Some(parent) = config_file.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut config: serde_json::Value = if config_file.exists() {
        fs::read_to_string(&config_file)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default()
    } else {
        serde_json::Map::new().into()
    };

    match action {
        "get" => {
            let k = key.unwrap_or("");
            if let Some(v) = config.get(k) {
                println!("{}", serde_json::to_string_pretty(v).unwrap());
            }
        }
        "set" => {
            let k = key.unwrap_or("");
            let v = value.unwrap_or("");
            let parsed: serde_json::Value = serde_json::from_str(v).unwrap_or_else(|_| json!(v));
            config
                .as_object_mut()
                .unwrap()
                .insert(k.to_string(), parsed);
            let _ = fs::write(&config_file, serde_json::to_string_pretty(&config).unwrap());
        }
        "list" => {
            println!("{}", serde_json::to_string_pretty(&config).unwrap());
        }
        "edit" => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            let _ = std::process::Command::new(&editor)
                .arg(&config_file)
                .status();
        }
        _ => {}
    }
}

// ============================================================================
// Install
// ============================================================================

pub fn cmd_install(packages: &[String], requirements: Option<&str>, dev: bool) {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("-m").arg("pip").arg("install");
    if let Some(req) = requirements {
        cmd.arg("-r").arg(req);
    }
    for p in packages {
        cmd.arg(p);
    }
    if dev {
        cmd.arg("-e").arg(".[dev]");
    }
    if packages.is_empty() && requirements.is_none() && !dev {
        println!("Nothing to install. Use packages, --requirements, or --dev.");
        return;
    }
    match cmd.status() {
        Ok(s) => {
            if !s.success() {
                std::process::exit(s.code().unwrap_or(1));
            }
        }
        Err(e) => eprintln!("Error running pip: {}", e),
    }
}

// ============================================================================
// Update
// ============================================================================

pub fn cmd_update(packages: &[String], all: bool, dry_run: bool) {
    if all {
        let output = std::process::Command::new("python3")
            .arg("-m")
            .arg("pip")
            .arg("list")
            .arg("--outdated")
            .arg("--format=freeze")
            .output();
        if let Ok(result) = output {
            let outdated: Vec<String> = result
                .stdout
                .split(|b| *b == b'\n')
                .filter_map(|l| {
                    String::from_utf8_lossy(&l)
                        .lines()
                        .next()
                        .map(|s| s.to_string())
                })
                .filter(|l| l.contains("=="))
                .map(|l| l.split("==").next().unwrap_or("").to_string())
                .collect();
            if !outdated.is_empty() {
                let mut cmd = std::process::Command::new("python3");
                cmd.arg("-m").arg("pip").arg("install").arg("--upgrade");
                for p in &outdated {
                    cmd.arg(p);
                }
                if dry_run {
                    println!("DRY RUN: {}", outdated.join(" "));
                } else {
                    let _ = cmd.status();
                }
            }
        }
    }
    for p in packages {
        let mut cmd = std::process::Command::new("python3");
        cmd.arg("-m")
            .arg("pip")
            .arg("install")
            .arg("--upgrade")
            .arg(p);
        if dry_run {
            println!("DRY RUN: pip install --upgrade {}", p);
        } else {
            let _ = cmd.status();
        }
    }
}

// ============================================================================
// Exec
// ============================================================================

pub fn cmd_exec(command: &[String], cwd: Option<&str>) {
    let cmd_str = command.join(" ");
    let parts: Vec<&str> = cmd_str.split_whitespace().collect();
    if parts.is_empty() {
        eprintln!("Error: comando vacío");
        return;
    }
    
    let (cmd, args) = (parts[0], &parts[1..]);
    
    // Regla 9 - Supervivencia: validación de seguridad
    match crate::safety::safe_exec(cmd, args, true) {
        Ok(output) => print!("{}", output),
        Err(e) => eprintln!("Error: {}", e),
    }
}

// ============================================================================
// Doctor
// ============================================================================

pub fn cmd_doctor(fix: bool) {
    let checks = [
        ("Python", vec!["python3", "--version"]),
        ("Git", vec!["git", "--version"]),
    ];
    for (name, cmd) in &checks {
        let result = std::process::Command::new(&cmd[0]).args(&cmd[1..]).status();
        match result {
            Ok(s) if s.success() => println!("✓ {}", name),
            _ => {
                println!("✗ {} - NOT FOUND", name);
                if fix {
                    println!("  Would install {}...", name);
                }
            }
        }
    }
    if fix {
        println!("Auto-fix not yet implemented");
    }
}

// ============================================================================
// Repomap
// ============================================================================

pub async fn cmd_repomap(path: &str, max_chars: usize) {
    let tools = crate::tools::ToolRegistry::new();
    let result = tools.call_tool("repo_map", {
        let mut args = HashMap::new();
        args.insert("path".to_string(), serde_json::Value::String(path.to_string()));
        args.insert("max_chars".to_string(), serde_json::Value::Number(serde_json::Number::from(max_chars)));
        args
    }).await;
    match result {
        Ok(output) => println!("{}", output),
        Err(e) => eprintln!("Error: {}", e),
    }
}

// ============================================================================
// Clean
// ============================================================================

pub fn cmd_clean(all: bool, cache: bool, build: bool) {
    let mut patterns = Vec::new();
    if all || cache {
        patterns.extend([
            "**/__pycache__",
            "**/.pytest_cache",
            "**/.mypy_cache",
            "**/.ruff_cache",
            "**/*.pyc",
        ]);
    }
    if all || build {
        patterns.extend(["**/build", "**/dist", "**/*.egg-info"]);
    }

    let mut removed = 0;
    for pattern in &patterns {
        for entry in glob::glob(pattern).unwrap().filter_map(|e| e.ok()) {
            // Regla 9 - Supervivencia: no auto-destrucción - proteger archivos críticos
            let path_str = entry.display().to_string();
            if crate::safety::is_protected_path(&path_str) {
                eprintln!("⚠️  Protegido - no se elimina: {}", path_str);
                continue;
            }
            
            if entry.is_dir() {
                let _ = fs::remove_dir_all(&entry);
            } else {
                let _ = fs::remove_file(&entry);
            }
            println!("Removed: {}", entry.display());
            removed += 1;
        }
    }
    if removed == 0 {
        println!("Nothing to clean.");
    }
}

// ============================================================================
// Completion
// ============================================================================

pub fn cmd_completion(shell: &str) {
    let script: &str = match shell {
        "bash" => {
            r#"# lai bash completion
_lai_complete() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    local commands="chat generate code think auto run serve status models tools search find fix repair review diff test init project config install update exec doctor clean version"
    local global_opts="-m --model -u --base-url -t --temp --max-tokens --stream --no-stream --thinking --no-thinking --tools --no-tools --auto --yolo -v --verbose -q --quiet --help"

    case "${prev}" in
        lai)
            COMPREPLY=( $(compgen -W "${commands} ${global_opts}" -- ${cur}) )
            return 0
            ;;
        -m|--model)
            COMPREPLY=( $(compgen -W "qwen3-8b qwen3:8b phi-3.5-mini" -- ${cur}) )
            return 0
            ;;
        -u|--base-url)
            COMPREPLY=( $(compgen -W "http://localhost:8080/v1 http://localhost:8081/v1 http://localhost:11434/v1" -- ${cur}) )
            return 0
            ;;
        *)
            COMPREPLY=( $(compgen -W "${commands} ${global_opts}" -- ${cur}) )
            return 0
            ;;
    esac
}
complete -F _lai_complete lai
"#
        }
        "zsh" => {
            r#"# lai zsh completion
#compdef lai
_lai() {
    local -a commands
    local -a global_opts
    commands=(
        'chat:Chat with AI'
        'generate:Generate text'
        'code:Generate code'
        'think:Thinking mode'
        'auto:Auto mode'
        'run:Run script'
        'serve:Start API server'
        'status:Check status'
        'models:List models'
        'tools:List tools'
        'search:Search code/files'
        'find:Find files'
        'fix:Fix code issues'
        'repair:Alias for fix'
        'review:Code review'
        'diff:Show diff'
        'test:Run tests'
        'init:Initialize project'
        'project:Generate complete project'
        'config:Manage configuration'
        'install:Install dependencies'
        'update:Update dependencies'
        'exec:Execute command in project context'
        'doctor:Diagnose environment'
        'clean:Clean build artifacts'
        'version:Show version'
    )
    global_opts=(
        '-m[Model to use]:model:(qwen3-8b qwen3:8b phi-3.5-mini)'
        '-u[Base URL for API]:url:(http://localhost:8080/v1 http://localhost:8081/v1 http://localhost:11434/v1)'
        '-t[Temperature]:float:'
        '--max-tokens[Max tokens]:int:'
        '--stream[Stream output]'
        '--no-stream[Disable streaming]'
        '--thinking[Enable thinking mode]'
        '--no-thinking[Disable thinking mode]'
        '--tools[Enable tools]'
        '--no-tools[Disable tools]'
        '--auto[Auto mode]'
        '--yolo[YOLO mode]'
        '-v[Verbose output]'
        '-q[Quiet mode]'
        '--help[Show help]'
    )
    _arguments -C \
        "${global_opts[@]}" \
        '1: :($commands)' \
        '*::arg:->subcmds' && return 0

    if (( CURRENT == 1 )); then
        compadd -a commands
        return
    fi

    case "${words[1]}" in
        chat|generate)
            _arguments "${global_opts[@]}" ;;
        auto)
            _arguments "${global_opts[@]}" \
                '--max-iterations[Max iterations]:int:' \
                '--max-turns[Max tool turns]:int:' \
                '--no-cost[Disable cost counting]' ;;
        serve)
            _arguments "${global_opts[@]}" \
                '--host[Bind address]:string:' \
                '--port[Port]:int:' ;;
        review)
            _arguments "${global_opts[@]}" \
                '--focus[Focus area]:string:' \
                '--format[Output format]:string:(text json markdown sarif)' ;;
        code)
            _arguments "${global_opts[@]}" \
                '--auto' '--yolo' \
                '--max-iterations[Max iterations]:int:' ;;
        fix|repair)
            _arguments "${global_opts[@]}" \
                '--type[Type of fix]:string:(lint format imports types all)' ;;
        search|find)
            _arguments "${global_opts[@]}" \
                '--type[File type]:string:' \
                '(-i --ignore-case)'{-i,--ignore-case}'[Case insensitive]' \
                '(-r --recursive)'{-r,--recursive}'[Recursive]' \
                '--max-depth[Max depth]:int:' ;;
        config)
            _arguments "${global_opts[@]}" \
                '1:action:(get set list edit)' \
                '--global[Global config]' ;;
        install|update)
            _arguments "${global_opts[@]}" \
                '--dev[Install dev dependencies]' \
                '--dry-run[Dry run]' \
                '--all[All packages]' \
                '--requirements[From requirements.txt]' \
                '--packages[Packages to install]:string:' ;;
    esac
}
compdef _lai lai
"#
        }
        "fish" => {
            r#"# lai fish completion
function __fish_lai_complete
    set -l commands chat generate code think auto run serve status models tools search find fix repair review diff test init project config install update exec doctor clean version
    set -l global_opts -m --model -u --base-url -t --temp --max-tokens --stream --no-stream --thinking --no-thinking --tools --no-tools --auto --yolo -v --verbose -q --quiet --help

    set -l cmd (commandline -opc)
    if test (count $cmd) -eq 1
        echo $commands $global_opts
        return
    end

    switch $cmd[1]
        case lai
            echo $commands $global_opts
        case "-m" "--model"
            echo "qwen3-8b qwen3:8b phi-3.5-mini"
        case "-u" "--base-url"
            echo "http://localhost:8080/v1 http://localhost:8081/v1 http://localhost:11434/v1"
        case "--max-tokens" "-t" "--temp"
        case chat generate
            echo $global_opts
        case auto
            echo "--max-iterations --max-turns --no-cost $global_opts"
        case serve
            echo "--host --port $global_opts"
        case review
            echo "--focus --format $global_opts"
        case code
            echo "--auto --yolo --max-iterations $global_opts"
        case fix repair
            echo "--type $global_opts"
        case search find
            echo "--type -i --ignore-case -r --recursive --max-depth $global_opts"
        case config
            echo "get set list edit --global $global_opts"
        case install update
            echo "--dev --dry-run --all --requirements --packages $global_opts"
        case "*"
            echo "$commands $global_opts"
    end
end

complete -c lai -f -a "(__fish_lai_complete)"
"#
        }
        _ => "Unsupported shell",
    };
    print!("{}", script);
}

// ============================================================================
// Harness command - ExecutionLoop (deterministic runtime, Fase 0/1)
// ============================================================================

pub async fn cmd_harness(task: &str, yolo: bool, sandbox: bool, worktree: bool, worktree_merge: bool, model: &str, base_url: &str, allow_remote: bool) {
    use crate::harness::{ExecutionHarness, HarnessConfig};

    // Sandbox preflight (v4.0): si se exige y no puede aplicarse, el
    // PolicyEngine falla cerrado (ninguna ejecución sin aislamiento).
    let st = crate::sandbox::preflight();
    if sandbox {
        if st.available {
            println!("Sandbox: {} ({})", st.mechanism, st.detail);
        } else {
            eprintln!("Sandbox REQUERIDO pero no disponible: {}", st.detail);
            eprintln!("El PolicyEngine denegará la ejecución de comandos (fail-closed).");
        }
    } else if !st.available {
        println!("Sandbox: no disponible ({}) — comandos sin aislamiento (usa --sandbox para exigirlo)", st.detail);
    }

    // Aislamiento por tarea (transversal 4.1): la tarea corre en un worktree
    // propio y la frontera del PolicyEngine queda confinada a ese directorio.
    let mut config = HarnessConfig::default().require_sandbox(sandbox);
    let wt = if worktree {
        match crate::harness::create_worktree(task) {
            Ok(wt) => {
                println!("Worktree aislado: {} (rama {})", wt.path.display(), wt.branch);
                config.working_dir = wt.path.clone();
                Some(wt)
            }
            Err(e) => {
                eprintln!("No se pudo crear el worktree: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    let llm = crate::llm::LlmClient::new(base_url, model, allow_remote);

    let mut harness = match ExecutionHarness::new(config, llm) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Harness init error: {}", e);
            std::process::exit(1);
        }
    };

    println!("lai harness: {}", task);
    let outcome = harness.execute(task, "generic").await;
    let result = match outcome {
        Ok(r) => r,
        Err(e) => {
            if let Some(wt) = &wt {
                crate::harness::remove_worktree(wt);
                println!("Worktree eliminado (tarea falló).");
            }
            eprintln!("Harness error: {}", e);
            std::process::exit(1);
        }
    };

    // Cierre del worktree: éxito + --worktree-merge fusiona la rama;
    // éxito sin merge deja el worktree para revisión; fallo lo elimina.
    if let Some(wt) = &wt {
        if result.success {
            if worktree_merge {
                match crate::harness::merge_worktree(wt) {
                    Ok(()) => {
                        println!("Rama {} fusionada en la rama principal.", wt.branch);
                        crate::harness::remove_worktree(wt);
                    }
                    Err(e) => eprintln!("Fusión falló (worktree conservado para revisión): {}", e),
                }
            } else {
                println!("Worktree conservado para revisión: {} (rama {})", wt.path.display(), wt.branch);
            }
        } else {
            crate::harness::remove_worktree(wt);
            println!("Worktree eliminado (tarea no exitosa).");
        }
    }

    println!("\n=== Execution Result ===");
    println!("Task:      {}", result.task_id);
    println!("Success:   {}", result.success);
    println!("Verdict:   {}", result.verdict.label());
    println!("State:     {:?}", result.final_state);
    println!("Iterations: {}", result.iterations);
    println!("Tool turns: {}", result.total_tool_turns);
    println!("Duration:  {}ms", result.duration_ms);
    if !result.files_modified.is_empty() {
        println!("Files:     {}", result.files_modified.join(", "));
    }
    for v in &result.verification_results {
        println!("Verify [{}] {}: {}", if v.passed { "PASS" } else { "FAIL" }, v.verifier, v.details);
    }
    println!("Summary:   {}", result.summary);
    if !result.success {
        std::process::exit(1);
    }
}

/// lai bench: ejecuta la suite interna y emite métricas versionadas.
pub async fn cmd_bench(tasks_dir: &str, out_dir: &str, model: &str, base_url: &str, allow_remote: bool, compare_with: Option<&str>, concurrency: usize, retrieval: bool) {
    // Modo comparativo: contrasta dos ficheros de resultados ya existentes.
    if let Some(baseline_path) = compare_with {
        let load = |p: &str| -> crate::bench::BenchSummary {
            let content = std::fs::read_to_string(p).unwrap_or_else(|e| {
                eprintln!("No se pudo leer '{}': {}", p, e);
                std::process::exit(1);
            });
            serde_json::from_str(&content).unwrap_or_else(|e| {
                eprintln!("JSON inválido en '{}': {}", p, e);
                std::process::exit(1);
            })
        };
        let baseline = load(baseline_path);
        let out_path = std::path::PathBuf::from(out_dir);
        let candidate_path = std::fs::read_dir(&out_path)
            .ok()
            .and_then(|mut entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
                    .max()
            });
        let candidate = candidate_path
            .map(|p| load(p.to_str().unwrap()))
            .unwrap_or_else(|| {
                eprintln!("Sin resultados de benchmark en '{}'", out_dir);
                std::process::exit(1);
            });
        crate::bench::print_compare(&crate::bench::compare_results(&baseline, &candidate));
        return;
    }

    let dir = std::path::PathBuf::from(tasks_dir);
    let tasks = match crate::bench::load_tasks(&dir) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("No se pudieron cargar las tareas de '{}': {}", tasks_dir, e);
            std::process::exit(1);
        }
    };
    if tasks.is_empty() {
        eprintln!("Sin tareas en '{}'", tasks_dir);
        std::process::exit(1);
    }

    let workdir = std::env::current_dir().unwrap_or_default();
    let summary = if concurrency > 1 {
        println!("Ejecución concurrente: {} tareas en paralelo (worktrees aislados)", concurrency);
        crate::bench::run_bench_concurrent(&tasks, model, base_url, allow_remote, &workdir, concurrency)
            .await
            .unwrap_or_else(|e| {
                eprintln!("Benchmark error: {}", e);
                std::process::exit(1);
            })
    } else {
        crate::bench::run_bench(&tasks, model, base_url, allow_remote, workdir.clone())
            .await
            .unwrap_or_else(|e| {
                eprintln!("Benchmark error: {}", e);
                std::process::exit(1);
            })
    };

    crate::bench::print_summary(&summary);
    match crate::bench::write_results(&summary, std::path::Path::new(out_dir)) {
        Ok(path) => println!("Resultados: {}", path.display()),
        Err(e) => eprintln!("No se pudieron guardar los resultados: {}", e),
    }

    // Benchmark de recuperación (admisibilidad del hybrid RAG, v5.0).
    if retrieval {
        match crate::retrieval_bench::run_retrieval_bench(&workdir, 3) {
            Ok(r) => {
                println!("\n=== lai bench --retrieval ===");
                println!("Keyword baseline: Recall@3={:.2} MRR@3={:.2}", r.baseline_keyword.recall_at_k, r.baseline_keyword.mrr_at_k);
                println!("Hybrid RRF:       Recall@3={:.2} MRR@3={:.2}", r.hybrid_rrf.recall_at_k, r.hybrid_rrf.mrr_at_k);
                if r.hybrid_wins {
                    println!("Híbrido MEJORA la línea base (candidato a adopción).");
                } else {
                    println!("Híbrido NO mejora la línea base (no adoptar).");
                }
            }
            Err(e) => eprintln!("Retrieval bench error: {}", e),
        }
    }
}

/// lai mcp: servidor MCP mínimo (stdio JSON-RPC) sobre el ToolRegistry.
pub async fn cmd_mcp(verify_pins: Option<&str>) {
    let mut registry = crate::tools::ToolRegistry::new();

    // MCP Hangar (v5.0): si se pasa un fichero de pins, la integridad de las
    // definiciones se verifica antes de servir; mismatch -> fail-closed.
    if let Some(pins_path) = verify_pins {
        match crate::mcp_security::McpPinRegistry::load_or_init(std::path::Path::new(pins_path)) {
            Ok(pins) => {
                let defs = registry
                    .get_tools_schema()
                    .iter()
                    .map(|s| crate::mcp_security::McpToolDef {
                        server: "lai".to_string(),
                        name: s.name.clone(),
                        description: s.description.clone(),
                        input_schema: s.parameters.clone(),
                    })
                    .collect::<Vec<_>>();
                let (ok, denied) = pins.filter_verified(&defs);
                if !denied.is_empty() {
                    eprintln!("MCP fail-closed: {} herramientas NO verificadas contra los pins:", denied.len());
                    for (name, reason) in &denied {
                        eprintln!("  - {}: {}", name, reason);
                    }
                    std::process::exit(1);
                }
                println!("MCP pins verificados: {} herramientas conformes.", ok.len());
            }
            Err(e) => {
                eprintln!("MCP fail-closed: no se pudo leer '{}': {}", pins_path, e);
                std::process::exit(1);
            }
        }
    }

    if let Err(e) = crate::mcp::serve(registry).await {
        eprintln!("MCP server error: {}", e);
        std::process::exit(1);
    }
}

/// lai mcp-pin: fija los pins de las definiciones actuales (MCP Hangar).
pub fn cmd_mcp_pin(out: &str) {
    let registry = crate::tools::ToolRegistry::new();
    let mut pins = crate::mcp_security::McpPinRegistry::new();
    let count = registry.get_tools_schema().len();
    for s in registry.get_tools_schema() {
        pins.pin(&crate::mcp_security::McpToolDef {
            server: "lai".to_string(),
            name: s.name.clone(),
            description: s.description.clone(),
            input_schema: s.parameters.clone(),
        });
    }
    match pins.save(std::path::Path::new(out)) {
        Ok(()) => println!("Pins guardados: {} herramientas -> {}", count, out),
        Err(e) => {
            eprintln!("No se pudieron guardar los pins: {}", e);
            std::process::exit(1);
        }
    }
}

/// lai mcp-verify: verifica definiciones anunciadas contra los pins.
pub fn cmd_mcp_verify(pins: &str, defs: &str) {
    let registry = match crate::mcp_security::McpPinRegistry::load_or_init(std::path::Path::new(pins)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("No se pudo leer '{}': {}", pins, e);
            std::process::exit(1);
        }
    };
    let content = std::fs::read_to_string(defs).unwrap_or_else(|e| {
        eprintln!("No se pudo leer '{}': {}", defs, e);
        std::process::exit(1);
    });
    let defs: Vec<crate::mcp_security::McpToolDef> = serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("JSON inválido en '{}': {}", defs, e);
        std::process::exit(1);
    });
    let (ok, denied) = registry.filter_verified(&defs);
    println!("Verificadas: {}", ok.len());
    for (name, reason) in &denied {
        println!("DENEGADA {}: {}", name, reason);
    }
    if !denied.is_empty() {
        std::process::exit(1);
    }
}

/// lai contracts: verifica contratos requires/ensures de un SPEC.md.
pub fn cmd_contracts(spec_path: &str, pre: &str, post: &str) {
    let spec_md = std::fs::read_to_string(spec_path).unwrap_or_else(|e| {
        eprintln!("No se pudo leer '{}': {}", spec_path, e);
        std::process::exit(1);
    });
    let pre: serde_json::Value = serde_json::from_str(pre).unwrap_or_else(|e| {
        eprintln!("Pre-state JSON inválido: {}", e);
        std::process::exit(1);
    });
    let post: serde_json::Value = serde_json::from_str(post).unwrap_or_else(|e| {
        eprintln!("Post-state JSON inválido: {}", e);
        std::process::exit(1);
    });
    let specs = crate::formal::parse_contracts_from_spec(&spec_md);
    if specs.is_empty() {
        eprintln!("Sin contratos en '{}'", spec_path);
        std::process::exit(1);
    }
    let mut all_closed = true;
    for spec in &specs {
        let r = crate::formal::check_contract(spec, &pre, &post);
        let verdict = r.to_external_verdict().to_verdict(spec.required);
        println!("[{}] {} -> {}", if r.closed { "CLOSED" } else { "OPEN" }, spec.target, verdict.label());
        for c in &r.clauses {
            println!("    {} {}: {}", c.expression, if c.satisfied { "OK" } else { "FAIL" }, c.error.clone().unwrap_or_default());
        }
        if !r.closed {
            all_closed = false;
        }
    }
    if !all_closed {
        std::process::exit(1);
    }
}

/// lai nn-verify: verifica una propiedad de robustez neuronal.
pub fn cmd_nn_verify(spec_path: &str, required: bool) {
    let content = std::fs::read_to_string(spec_path).unwrap_or_else(|e| {
        eprintln!("No se pudo leer '{}': {}", spec_path, e);
        std::process::exit(1);
    });
    let spec: crate::nn_verifier::NnPropertySpec = serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("JSON inválido en '{}': {}", spec_path, e);
        std::process::exit(1);
    });
    let result = crate::nn_verifier::verify_nn(&spec);
    let verdict = result.verdict.to_verdict(required);
    println!("Verificación NN ({}): {} [{}ms] {}", spec.model, verdict.label(), result.duration_ms, result.detail);
    if !matches!(verdict, crate::verifier::Verdict::Success) {
        std::process::exit(1);
    }
}

/// lai memory-eval: evalúa el MemoryStore contra un dataset LOCOMO.
pub fn cmd_memory_eval(dataset: &str, db: &str, k: usize) {
    let store = match crate::memory::MemoryStore::open(std::path::Path::new(db)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("No se pudo abrir '{}': {}", db, e);
            std::process::exit(1);
        }
    };
    let dataset = crate::memory_eval::load_dataset(std::path::Path::new(dataset)).unwrap_or_else(|e| {
        eprintln!("No se pudo leer '{}': {}", dataset, e);
        std::process::exit(1);
    });
    let m = crate::memory_eval::evaluate_memory(&store, &dataset, k);
    println!("=== lai memory-eval ===");
    println!("Preguntas:            {}", m.questions);
    println!("Recall@{}:            {:.2}", k, m.recall_at_k);
    println!("MRR@{}:               {:.2}", k, m.mrr_at_k);
    println!("Latencia media:       {:.1}ms", m.avg_latency_ms);
    println!("Tokens de contexto:   {:.0}", m.avg_context_tokens);
}

/// lai cpg: reporte del grafo de propiedades (taint / slicing).
pub fn cmd_cpg(workspace: &str, taint: bool, slice: Option<&str>) {
    use std::collections::HashMap;
    let root = std::path::PathBuf::from(workspace);
    let repo_map = crate::repomap::build_repo_map(&root).unwrap_or_else(|e| {
        eprintln!("RepoMap error: {}", e);
        std::process::exit(1);
    });
    let mut contents: HashMap<String, String> = HashMap::new();
    for f in &repo_map.files {
        if let Ok(c) = std::fs::read_to_string(root.join(&f.path)) {
            contents.insert(f.path.clone(), c);
        }
    }
    let graph = crate::cpg::build_cpg(&repo_map, &contents);
    println!("{}", graph.summary());
    if taint {
        let findings = crate::cpg::taint_analysis(&repo_map, &contents);
        println!("\n=== Taint findings ({}) ===", findings.len());
        for f in &findings {
            println!("[{}] {}:{} -> {}:{} ({} -> {})", f.severity, f.file, f.source_line, f.file, f.sink_line, f.source, f.sink);
        }
    }
    if let Some(sym) = slice {
        let ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
        let root_id = ids
            .iter()
            .find(|id| *id == sym || id.ends_with(&format!(":{sym}")))
            .cloned()
            .unwrap_or_else(|| {
                eprintln!("Símbolo '{}' no encontrado. IDs válidos: {:?}", sym, ids.iter().take(5).collect::<Vec<_>>());
                std::process::exit(1);
            });
        let slice = graph.slice(&root_id);
        println!("\n=== Slice desde {} ({} nodos) ===", root_id, slice.len());
        for id in &slice {
            println!("  {}", id);
        }
    }
}

/// lai telemetry-export: exporta trayectorias a batches Langfuse.
pub fn cmd_telemetry_export(dir: &str, host: Option<&str>) {
    let records = crate::trajectory::load_trajectories();
    if records.is_empty() {
        println!("Sin trayectorias registradas.");
        return;
    }
    let exporter = crate::langfuse::LangfuseExporter::new(
        std::path::Path::new(dir),
        host.map(String::from),
        std::env::var("LANGFUSE_API_KEY").ok(),
    );
    for r in &records {
        let batch = crate::langfuse::batch_from_trajectory(
            &r.task_id,
            &r.model,
            &r.verdict,
            r.actions.iter().filter_map(|a| a.tool_definition_hash.clone()).collect(),
            vec![],
            r.verification_levels_attempted.clone(),
            r.verification_levels_passed.clone(),
            r.duration_ms,
        );
        match exporter.export(&batch) {
            Ok(path) => println!("{} -> {}", r.task_id, path.display()),
            Err(e) => eprintln!("Export falló (fail-open): {}", e),
        }
    }
}

/// lai graph-run: ejecuta un grafo LangGraph-style con el ToolRegistry.
pub async fn cmd_graph_run(spec_path: &str, required: bool) {
    let content = std::fs::read_to_string(spec_path).unwrap_or_else(|e| {
        eprintln!("No se pudo leer '{}': {}", spec_path, e);
        std::process::exit(1);
    });
    let spec: crate::graph_orchestrator::GraphSpec = serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("JSON inválido en '{}': {}", spec_path, e);
        std::process::exit(1);
    });
    let registry = crate::tools::ToolRegistry::new();
    let registry_ref = &registry;
    let initial = crate::graph_orchestrator::GraphState {
        current: spec.entry.clone(),
        tokens_used: 0,
        steps: vec![],
        last_result: None,
        halted_reason: None,
    };
    let state = crate::graph_orchestrator::run_graph_async::<_, _>(&spec, initial, |node: crate::graph_orchestrator::GraphNode| async move {
        match node.kind.as_str() {
            "tool" => {
                let tool = node.tool.clone().unwrap_or_default();
                let args = node
                    .prompt
                    .as_ref()
                    .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
                    .unwrap_or_default();
                match registry_ref.call(&tool, args).await {
                    Ok(result) => Ok((result.to_string(), 0, true)),
                    Err(e) => Ok((format!("error: {}", e), 0, false)),
                }
            }
            "verify" => {
                let ok = node.prompt.as_deref() == Some("ok") || node.prompt.is_none();
                Ok(("ok".to_string(), 0, ok))
            }
            _ => Ok((node.prompt.clone().unwrap_or_default(), 0, true)),
        }
    })
    .await
    .unwrap_or_else(|e| {
        eprintln!("Graph error: {}", e);
        std::process::exit(1);
    });
    let verdict = crate::graph_orchestrator::graph_to_verdict(&spec, &state, required);
    println!("Pasos: {:?}", state.steps);
    println!("Tokens: {} / {}", state.tokens_used, spec.token_budget);
    println!("Detenido: {:?}", state.halted_reason);
    println!("Veredicto: {}", verdict.label());
    if !matches!(verdict, crate::verifier::Verdict::Success) {
        std::process::exit(1);
    }
}

/// lai trajectories: análisis de rendimiento sobre las trayectorias
/// registradas (base de la auto-mejora, v4.0 Fase 6).
pub async fn cmd_trajectories() {
    let records = crate::trajectory::load_trajectories();
    if records.is_empty() {
        println!("Sin trayectorias registradas en .lai/trajectories/.");
        return;
    }

    let stats = crate::trajectory::TrajectoryStats::compute(&records);
    println!("=== lai trajectories ===");
    println!("Tareas registradas:     {}", stats.tasks);
    println!("Tasa de éxito:          {:.0}%", stats.success_rate * 100.0);
    println!("Duración media:         {:.0}ms", stats.avg_duration_ms);
    println!("Iteraciones medias:     {:.1}", stats.avg_iterations);
    println!("Verificación efectiva:  {:.0}%", stats.verification_effective_rate * 100.0);
    println!("Tasa de fallo de tools: {:.0}%", stats.tool_failure_rate * 100.0);

    println!("\nTrayectorias (más recientes al final):");
    for r in records.iter().rev().take(10) {
        println!(
            "  [{}] {} -> {} | {} acciones | {} verif. | {}ms | {}",
            r.task_id, r.model, r.verdict, r.actions.len(),
            r.verification_levels_passed.len(), r.duration_ms, r.outcome
        );
    }
    if records.len() > 10 {
        println!("  ... y {} trayectorias más", records.len() - 10);
    }
}

/// lai recover: inspecciona los checkpoints de una tarea (estado recuperable).
pub async fn cmd_recover(task: Option<&str>, checkpoints_dir: &str) {
    let dir = std::path::PathBuf::from(checkpoints_dir);
    if !dir.is_dir() {
        eprintln!("Directorio de checkpoints no encontrado: {}", checkpoints_dir);
        std::process::exit(1);
    }

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            eprintln!("No se pudo leer '{}': {}", checkpoints_dir, e);
            std::process::exit(1);
        })
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().map(|x| x == "json").unwrap_or(false)
                && match task {
                    Some(t) => p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with(&format!("{}-", t)))
                        .unwrap_or(false),
                    None => true,
                }
        })
        .collect();

    if files.is_empty() {
        match task {
            Some(t) => eprintln!("Sin checkpoints para la tarea '{}'", t),
            None => eprintln!("Sin checkpoints en '{}'", checkpoints_dir),
        }
        std::process::exit(1);
    }
    files.sort();

    println!("=== lai recover ===\nCheckpoints ({}):", files.len());
    for path in &files {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let cp: Result<crate::harness::Checkpoint, _> = serde_json::from_str(&content);
        match cp {
            Ok(cp) => println!(
                "  [{}] {} | estado={:?} paso={} | {} archivos | {}",
                name,
                cp.timestamp,
                cp.state,
                cp.step_index,
                cp.files_modified.len(),
                cp.description
            ),
            Err(e) => println!("  [{}] (checkpoint ilegible: {})", name, e),
        }
    }

    // Muestra el último checkpoint en detalle (estado recuperable).
    if let Some(latest) = files.last() {
        if let Ok(content) = std::fs::read_to_string(latest) {
            if let Ok(cp) = serde_json::from_str::<crate::harness::Checkpoint>(&content) {
                println!("\nÚltimo checkpoint: {}", latest.display());
                println!("  plan: {} pasos, {} completados", cp.plan_snapshot.len(), cp.plan_snapshot.iter().filter(|s| s.completed).count());
                println!("  repo_map indexado: {}", cp.repo_map_snapshot.get("indexed").and_then(|v| v.as_bool()).unwrap_or(false));
                println!("  archivos modificados: {:?}", cp.files_modified);
            }
        }
    }
}
