//! commands.rs - CLI command implementations

use crate::llm::{ChatResult, LlmClient, Message};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn get_llm_client(model: &str, base_url: &str, allow_remote: bool) -> LlmClient {
    LlmClient::new(base_url, model, allow_remote)
}

fn strip_think_tags(content: &str) -> String {
    let mut result = content.to_string();
    result = result.replace("<thinking>", "").replace("</thinking>", "");
    result = result.replace("```think", "").replace("```", "");
    result.trim().to_string()
}

fn build_project_snapshot(cwd: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("📁 Proyecto: {}", cwd.display()));
    if cwd.join(".git").is_dir() {
        parts.push("🌿 Git: repositorio detectado".to_string());
    }
    parts.join("\n")
}

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
    allow_remote: bool,
) {
    let mut llm = get_llm_client(model, base_url, allow_remote);
    llm.temperature = temp.unwrap_or(0.7);
    llm.max_tokens = max_tokens.unwrap_or(4096);
    let stream = if no_stream { false } else { stream };
    let thinking = if no_thinking { false } else { thinking };

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    if interactive {
        println!("Interactive mode - type 'exit' or 'quit' to quit");
        println!("Type 'clear' to clear conversation history");
        println!("\n{}", build_project_snapshot(&cwd));
        println!();

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

            let project_snapshot = build_project_snapshot(&cwd);
            let system_context = format!(
                "Eres lai, un asistente de código local. Contexto del proyecto:\n{}\n\nResponde en español. Sé proactivo.",
                project_snapshot
            );

            let mut messages_with_context = vec![Message {
                role: "system".into(),
                content: system_context,
            }];
            messages_with_context.extend(history.clone());

            let result = llm.chat_messages(messages_with_context, None, stream, thinking).await;

            if let Some(err) = &result.error {
                println!("\nError: {}", err);
                continue;
            }

            let content = strip_think_tags(&result.content);
            if !stream {
                println!("{}", content);
            }
            println!();

            history.push(Message {
                role: "user".into(),
                content: input.to_string(),
            });
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
        let project_snapshot = build_project_snapshot(&cwd);
        let enhanced_prompt = format!(
            "Contexto del proyecto:\n{}\n\nUsuario: {}",
            project_snapshot, prompt_str
        );
        let result = llm.chat(&enhanced_prompt, stream, thinking).await;
        if let Some(err) = &result.error {
            eprintln!("{}", err);
        } else {
            if !stream {
                println!("{}", strip_think_tags(&result.content));
            }
        }
    }
}

pub async fn cmd_code(
    description: &str,
    filename: &str,
    _auto: bool,
    _yolo: bool,
    _max_iterations: i32,
    _candidates: i32,
    _no_tests: bool,
    _no_lint: bool,
    _no_verify: bool,
    _strict: bool,
    model: &str,
    base_url: &str,
    allow_remote: bool,
) {
    let mut llm = get_llm_client(model, base_url, allow_remote);
    let prompt = format!("Generate ONLY valid Python code. No markdown. Task: {}\nOutput ONLY the code.", description);
    let result = llm.chat(&prompt, false, true).await;
    if let Some(err) = &result.error {
        eprintln!("Error: {}", err);
        return;
    }
    let code = LlmClient::extract_code(&result.content);
    if let Err(e) = fs::write(filename, &code) {
        eprintln!("Error writing file: {}", e);
        return;
    }
    println!("Code saved to {}", filename);
}

pub async fn cmd_think(problem: &str, model: &str, base_url: &str, allow_remote: bool) {
    let mut llm = get_llm_client(model, base_url, allow_remote);
    let result = llm.chat(problem, false, true).await;
    if let Some(err) = &result.error {
        eprintln!("Error: {}", err);
    } else {
        println!("{}", strip_think_tags(&result.content));
    }
}

pub async fn cmd_auto(
    goal: &str,
    _max_iterations: usize,
    _max_turns: usize,
    _no_cost: bool,
    model: &str,
    base_url: &str,
    allow_remote: bool,
    _tools: bool,
) {
    let mut llm = get_llm_client(model, base_url, allow_remote);
    let prompt = format!("Autonomous task: {}\n\nResponde en español describiendo qué harías para resolver esta tarea de forma autónoma.", goal);
    let result = llm.chat(&prompt, false, true).await;
    if let Some(err) = &result.error {
        eprintln!("Error: {}", err);
    } else {
        println!("{}", strip_think_tags(&result.content));
    }
}

pub fn cmd_run(script: &str) {
    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(script)
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));
    if !output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    }
}

pub async fn cmd_serve(host: &str, port: u16, model: &str, base_url: &str) {
    println!("Serving on {}:{} (model: {})", host, port, model);
}

pub async fn cmd_status() {
    println!("lai - Local AI Code Assistant");
    println!("Services: checking...");
    match reqwest::Client::new()
        .get("http://localhost:8080/health")
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => println!("llama.cpp: OK"),
        _ => println!("llama.cpp: NOT RUNNING (port 8080)"),
    }
}

pub fn cmd_models() {
    println!("Available models: qwen3-8b (via llama.cpp)");
}

pub fn cmd_tools() {
    println!("Available tools: chat, generate, code, think, auto, run, serve, status, models, tools, version");
}

pub fn cmd_version() {
    println!("lai 0.1.0");
}

pub fn cmd_completion(_shell: &str) {
    println!("Shell completion not yet implemented.");
}

pub fn cmd_search(_pattern: &str, _path: &str, _rtype: Option<&str>, _ignore_case: bool) {
    println!("Search not yet implemented.");
}

pub fn cmd_find(_pattern: &str, _path: &str, _rtype: Option<&str>, _max_depth: Option<usize>) {
    println!("Find not yet implemented.");
}

pub fn cmd_fix(_path: &str, _fix_type: &str, _auto: bool) {
    println!("Fix not yet implemented standalone. Use `lai auto` for autonomous repair.");
}

pub async fn cmd_review(
    _path: &str,
    _focus: &str,
    _format: &str,
    model: &str,
    base_url: &str,
    allow_remote: bool,
) {
    let mut llm = get_llm_client(model, base_url, allow_remote);
    let prompt = format!("Code review: {}\n\nResponde en español analizando el código y sugiriendo mejoras.", _path);
    let result = llm.chat(&prompt, false, true).await;
    if let Some(err) = &result.error {
        eprintln!("Error: {}", err);
    } else {
        println!("{}", strip_think_tags(&result.content));
    }
}

pub fn cmd_diff(_file1: &str, _file2: &str, _context: usize) {
    println!("Diff not yet implemented.");
}

pub fn cmd_test(_path: &str, _pattern: Option<&str>, _verbose: bool, _coverage: bool) {
    println!("Test not yet implemented standalone. Use `lai auto` for autonomous testing.");
}

pub fn cmd_init(_project_type: &str, _name: Option<&str>, _path: &str, _git: bool) {
    println!("Init not yet implemented.");
}

pub async fn cmd_project(_description: &str) {
    println!("Project generation not yet implemented.");
}

pub fn cmd_config(_action: &str, _key: Option<&str>, _value: Option<&str>, _global: bool) {
    println!("Config not yet implemented.");
}

pub fn cmd_install(_packages: &[String], _requirements: Option<&str>, _dev: bool) {
    println!("Install not yet implemented.");
}

pub fn cmd_update(_packages: &[String], _all: bool, _dry_run: bool) {
    println!("Update not yet implemented.");
}

pub fn cmd_exec(_command: &[String], _cwd: Option<&str>) {
    println!("Exec not yet implemented.");
}

pub fn cmd_doctor(_fix: bool) {
    println!("Doctor not yet implemented.");
}

pub fn cmd_clean(_all: bool, _cache: bool, _build: bool) {
    println!("Clean not yet implemented.");
}

pub fn cmd_ocr(_image: &str, _lang: &str, _output: Option<&str>) {
    println!("OCR not yet implemented.");
}

pub async fn cmd_mcp() {
    println!("MCP not yet implemented.");
}

pub async fn cmd_system_one(
    _endpoint: &str,
    _model: &str,
    _decision_type: &str,
    _state: &str,
    _question: &str,
    _options: &[String],
    _scale: &str,
    _temp: f64,
    _json: bool,
) {
    println!("System One not yet implemented.");
}

pub async fn cmd_deguro_compare(
    _allow_remote: bool,
    _timeout: f64,
    _endpoint: Vec<String>,
    _prompt: &str,
    _tarea: &str,
    _criterios: &str,
    _repetir: u32,
) {
    println!("Deguro compare not yet implemented.");
}

pub async fn cmd_deguro_discover(_allow_remote: bool, _timeout: f64, _endpoint: Vec<String>) {
    println!("Deguro discover not yet implemented.");
}

pub async fn cmd_rag_search(_query: &str, _collection: Option<&str>, _top_k: usize, _hybrid: bool) {
    println!("RAG search not yet implemented.");
}

pub async fn cmd_rag_index(
    _collection: &str,
    _path: &str,
    _exts: &[String],
    _chunked: bool,
    _chunk_size: usize,
) {
    println!("RAG index not yet implemented.");
}

pub async fn cmd_agents(
    _action: String,
    _agent: String,
    _task: String,
    _goal: String,
) {
    println!("Agents not yet implemented.");
}

pub async fn cmd_complete(
    _file: &str,
    _line: usize,
    _column: usize,
    _top_k: usize,
    _index: bool,
) {
    println!("Complete not yet implemented.");
}

pub fn cmd_symbols(_file: Option<&str>, _kind: Option<&str>) {
    println!("Symbols not yet implemented.");
}

pub async fn cmd_goto(_symbol: &str) {
    println!("Goto not yet implemented.");
}

pub async fn cmd_repomap(_path: &str, _max_chars: usize) {
    println!("Repomap not yet implemented standalone.");
}