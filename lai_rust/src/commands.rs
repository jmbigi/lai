//! commands.rs - CLI command implementations with quality cycles

use crate::llm::LlmClient;
use crate::services::Services;

pub async fn cmd_chat(prompt: &[String], model: &str, base_url: &str, _allow_remote: bool) {
    let client = LlmClient::new(base_url.to_string(), model.to_string());
    let prompt_str = prompt.join(" ");
    match client.chat(&prompt_str).await {
        Ok(resp) => println!("{}", resp),
        Err(e) => eprintln!("Error: {}", e),
    }
}

pub async fn cmd_code(
    description: &str,
    filename: &str,
    yolo: bool,
    model: &str,
    base_url: &str,
    _allow_remote: bool,
) {
    let client = LlmClient::new(base_url.to_string(), model.to_string());
    let max_iterations: i32 = 3;

    let base_prompt = format!(
        "Generate ONLY valid Python code. No markdown, no explanations. \
         Task: {}\nOutput ONLY the code.",
        description
    );

    let mut prompt = format!("/no_think {}", base_prompt);
    let mut code = String::new();
    let mut success = false;

    for iteration in 0..max_iterations {
        println!("Iteration {} of {}", iteration + 1, max_iterations);

        match client.chat(&prompt).await {
            Ok(raw) => {
                code = extract_code(&raw);
                match validate_code(&code) {
                    Ok(()) => {
                        println!("Validation passed.");
                        success = true;
                        break;
                    }
                    Err(e) => {
                        eprintln!("Validation error (iteration {}): {}", iteration + 1, e);
                        if iteration < max_iterations - 1 {
                            prompt = format!(
                                "/no_think Fix the following syntax error in this Python code:\n\n\
                                Code:\n```python\n{}\n```\n\nError: {}\n\nFix the code and output ONLY the corrected code.",
                                code, e
                            );
                            continue;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error generating code: {}", e);
                if iteration == max_iterations - 1 {
                    eprintln!("Failed after {} iterations", max_iterations);
                    return;
                }
            }
        }
    }

    if success || yolo {
        if let Err(e) = std::fs::write(filename, &code) {
            eprintln!("Error writing file: {}", e);
            return;
        }
        println!("Code saved to {}", filename);
    } else {
        eprintln!("Failed to generate valid code after {} iterations", max_iterations);
    }
}

pub async fn cmd_status() {
    println!("lai - Local AI Code Assistant");
    println!("Services: checking...");
    let mut services = services::Services::new();
    services.check_all();
    for (name, svc) in services.list() {
        println!("  {}: {} (Port {})", name, if svc.is_running() { "Running" } else { "Stopped" }, svc.port);
    }
}

fn extract_code(response: &str) -> String {
    if let Some(start) = content_between(response, "```python\n", "\n```") {
        return start.to_string();
    }
    if let Some(start) = content_between(response, "```\n", "\n```") {
        return start.to_string();
    }
    let mut lines: Vec<&str> = response.lines().collect();
    let mut start_idx = 0;
    for (i, line) in lines.iter().enumerate() {
        let stripped = line.trim();
        if stripped_starts_with_code(stripped) {
            start_idx = i;
            break;
        }
    }
    lines[start_idx..].join("\n").trim().to_string()
}

fn stripped_starts_with_code(line: &str) -> bool {
    line.starts_with("def ")
        || line.starts_with("class ")
        || line.starts_with("import ")
        || line.starts_with("from ")
        || line.starts_with("#!")
        || line.starts_with("print(")
}

fn content_between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let start_idx = text.find(start)?;
    let after = &text[start_idx + start.len()..];
    let end_idx = after.find(end)?;
    Some(&after[..end_idx])
}

fn validate_code(code: &str) -> Result<(), String> {
    if code.trim().is_empty() {
        return Err("Empty code".to_string());
    }
    use std::process::Command;
    use std::process::Stdio;
    let child = std::process::Command::new("python3")
        .arg("-c")
        .arg(format!("import ast; ast.parse({:?})", code))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn error: {}", e))?;
    let output = child.wait_with_output().map_err(|e| format!("wait error: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("Syntax error: {}", err))
    }
}