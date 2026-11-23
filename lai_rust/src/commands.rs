use crate::execution_loop::{ExecConfig, ExecutionLoop, Language};
use crate::llm::LlmClient;
use crate::policy::PolicyEngine;
use crate::tools::ToolRegistry;
use std::sync::Arc;

pub async fn cmd_chat(prompt: &[String], model: &str, base_url: &str, _allow_remote: bool) {
    let client = LlmClient::new(base_url.to_string(), model.to_string());
    let prompt_str = prompt.join(" ");
    match client.chat(&prompt_str, false, true).await {
        Ok(resp) => println!("{}", resp),
        Err(e) => eprintln!("Error: {}", e),
    }
}

pub async fn cmd_code(description: &str, filename: &str, yolo: bool, model: &str, base_url: &str, _allow_remote: bool) {
    let client = LlmClient::new(base_url.to_string(), model.to_string());
    let max_iterations: i32 = 3;
    let base_prompt = format!("Generate ONLY valid Python code. Task: {}\nOutput ONLY the code.", description);
    let mut prompt = format!("/no_think {}", base_prompt);
    let mut code = String::new();
    let mut success = false;
    for iteration in 0..max_iterations {
        println!("Iteration {} of {}", iteration + 1, max_iterations);
        let resp = client.chat(&prompt, false, true).await;
        if let Some(err) = &resp.error {
            eprintln!("Error: {}", err);
            if iteration == max_iterations - 1 { return; }
            continue;
        }
        code = extract_code(&resp.content);
        match validate_code(&code) {
            Ok(()) => { println!("Validation passed."); success = true; break; }
            Err(e) => {
                eprintln!("Validation error: {}", e);
                if iteration < max_iterations - 1 {
                    prompt = format!("/no_think Fix: {} Error: {}", code, e);
                    continue;
                }
            }
        }
    }
    if success || yolo {
        let _ = std::fs::write(filename, &code);
        println!("Code saved to {}", filename);
    } else {
        eprintln!("Failed to generate valid code after {} iterations", max_iterations);
    }
}

pub async fn cmd_status() {
    println!("lai - Local AI Code Assistant");
    match reqwest::Client::new().get("http://localhost:8080/health").send().await {
        Ok(resp) if resp.status().is_success() => println!("llama.cpp: OK"),
        _ => println!("llama.cpp: NOT RUNNING (port 8080)"),
    }
}

fn extract_code(response: &str) -> String {
    if let Some(s) = content_between(response, "```python\n", "\n```") { return s.to_string(); }
    if let Some(s) = content_between(response, "```\n", "\n```") { return s.to_string(); }
    response.to_string()
}

fn content_between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let si = text.find(start)?; let after = &text[si + start.len()..]; let ei = after.find(end)?; Some(&after[..ei])
}

fn validate_code(code: &str) -> Result<(), String> {
    if code.trim().is_empty() { return Err("Empty code".into()); }
    use std::process::{Command, Stdio};
    let child = Command::new("python3").arg("-c").arg(format!("import ast; ast.parse({:?})", code)).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("spawn error: {}", e))?;
    let output = child.wait_with_output().map_err(|e| format!("wait error: {}", e))?;
    if output.status.success() { Ok(()) } else { Err(format!("Syntax error: {}", String::from_utf8_lossy(&output.stderr))) }
}

pub async fn cmd_auto(goal: &str, _max_iterations: usize, max_turns: usize, _track_cost: bool, model: &str, base_url: &str, allow_remote: bool, _tools_enabled: bool) {
    let tools = Arc::new(ToolRegistry::new());
    let llm = Arc::new(LlmClient::new(base_url, model, allow_remote));
    let policy = Arc::new(PolicyEngine::new(Default::default()));
    let config = ExecConfig { max_turns, max_steps: 10, token_budget: 50_000, turn_timeout_secs: 300, yolo: false };
    let mut loop_exec = ExecutionLoop::new(goal.to_string(), tools, llm, policy, Language::Python, config);
    match loop_exec.run().await { Ok(report) => println!("\n{}", report), Err(e) => eprintln!("ExecutionLoop error: {}", e) }
}
