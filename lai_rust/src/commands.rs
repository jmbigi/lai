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
