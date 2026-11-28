//! commands.rs - CLI command implementations with quality cycles

use crate::context::{format_context_for_llm, ContextAnalyzer};
use crate::llm::{LlmClient, Message};
use crate::services::Services;
use crate::tools::ToolRegistry;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Helper macro for creating HashMaps with serde_json::Value
macro_rules! hashmap {
    ($( $key:expr => $val:expr ),* $(,)?) => {{
        let mut map: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
        $( map.insert($key, $val.into()); )*
        map
    }};
}

/// Analyze project context by reading README, AGENTS.md, CLAUDE.md, key files, and structure
fn analyze_project_context(cwd: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "=== PROJECT CONTEXT: {} ===",
        cwd.file_name().unwrap_or_default().to_string_lossy()
    ));
    parts.push(format!("Path: {}", cwd.display()));
    parts.push(String::new());

    // Read README files
    let readme_files = [
        "README.md",
        "README.txt",
        "README.rst",
        "readme.md",
        "readme.txt",
    ];
    for readme in readme_files {
        let readme_path = cwd.join(readme);
        if readme_path.exists() {
            if let Ok(content) = fs::read_to_string(&readme_path) {
                let truncated = content.chars().take(5000).collect::<String>();
                parts.push(format!("=== {} ===", readme));
                parts.push(truncated);
                parts.push(String::new());
                break; // Only read the first README found
            }
        }
    }

    // Read AGENTS.md if exists
    let agents_path = cwd.join("AGENTS.md");
    if agents_path.exists() {
        if let Ok(content) = fs::read_to_string(&agents_path) {
            let truncated = content.chars().take(5000).collect::<String>();
            parts.push("=== AGENTS.md ===".to_string());
            parts.push(truncated);
            parts.push(String::new());
        }
    }

    // Read CLAUDE.md if exists (for Claude Code compatibility)
    let claude_path = cwd.join("CLAUDE.md");
    if claude_path.exists() {
        if let Ok(content) = fs::read_to_string(&claude_path) {
            let truncated = content.chars().take(5000).collect::<String>();
            parts.push("=== CLAUDE.md ===".to_string());
            parts.push(truncated);
            parts.push(String::new());
        }
    }

    // Get key project files
    let key_files = [
        "pyproject.toml",
        "setup.py",
        "setup.cfg",
        "requirements.txt",
        "Cargo.toml",
        "go.mod",
        "package.json",
        "pom.xml",
        "Dockerfile",
        "docker-compose.yml",
        "docker-compose.yaml",
        "Makefile",
        "makefile",
        ".gitignore",
        ".env.example",
    ];

    parts.push("=== KEY PROJECT FILES ===".to_string());
    for kf in key_files {
        let kf_path = cwd.join(kf);
        if kf_path.exists() {
            if let Ok(content) = fs::read_to_string(&kf_path) {
                let truncated = content.chars().take(2000).collect::<String>();
                parts.push(format!("--- {} ---", kf));
                parts.push(truncated);
                parts.push(String::new());
            }
        }
    }

    // Get directory structure (top level)
    parts.push("=== PROJECT STRUCTURE (top level) ===".to_string());
    if let Ok(entries) = fs::read_dir(cwd) {
        let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        sorted.sort_by_key(|e| e.file_name());
        for entry in sorted {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let full = entry.path();
            if full.is_dir() {
                // Count Python/Rust files in directory (non-recursive to avoid build artifacts)
                let mut count = 0;
                if let Ok(dir_entries) = fs::read_dir(&full) {
                    for e in dir_entries.filter_map(|e| e.ok()) {
                        if e.path().is_file() {
                            if let Some(ext) = e.path().extension().and_then(|s| s.to_str()) {
                                if ext == "py" || ext == "rs" {
                                    count += 1;
                                }
                            }
                        }
                    }
                }
                parts.push(format!("📁 {}/ ({} source files)", name, count));
            } else {
                parts.push(format!("📄 {}", name));
            }
        }
    }

    parts.push(String::new());
    parts.push("=== END PROJECT CONTEXT ===".to_string());

    parts.join("\n")
}

pub async fn cmd_chat(prompt: &[String], model: &str, base_url: &str, allow_remote: bool) {
    let llm =
        LlmClient::new(base_url.to_string(), model.to_string()).with_allow_remote(allow_remote);
    let prompt_str = prompt.join(" ");

    // Load project context
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let analyzer = ContextAnalyzer::new(cwd.clone());
    let project_context = analyzer
        .analyze()
        .map(|ctx| format_context_for_llm(&ctx))
        .unwrap_or_else(|_| analyze_project_context(&cwd));
    let project_context = truncate_project_context(&project_context);

    // Get tool schemas
    let tools = ToolRegistry::new();
    let tool_schemas: Vec<serde_json::Value> = tools
        .get_tools_schema()
        .into_iter()
        .map(|s| {
            serde_json::json!({
                "type": "function",
                "function": serde_json::to_value(s).unwrap()
            })
        })
        .collect();

    // If prompt is provided, use non-interactive mode with tool calling
    if !prompt_str.is_empty() {
        // Build messages with system context
        let mut messages = vec![
            Message {
                role: "system".into(),
                content: project_context,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            Message {
                role: "user".into(),
                content: prompt_str,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];

        // Tool calling loop
        let mut max_rounds = 15;
        loop {
            max_rounds -= 1;
            if max_rounds == 0 {
                eprintln!("\n⚠️  Max tool rounds reached");
                break;
            }

            let result = llm
                .chat_with_tools(
                    messages.clone(),
                    Some(tool_schemas.clone()),
                    Some("auto".to_string()),
                )
                .await;

            match result {
                Ok(msg) => {
                    if !msg.content.is_empty() {
                        println!("{}", strip_think_tags(&msg.content));
                    }

                    // Add assistant message to history
                    messages.push(msg.clone());

                    // Check for tool calls
                    if let Some(tool_calls) = msg.tool_calls {
                        if tool_calls.is_empty() {
                            // No tool calls, we're done
                            break;
                        }

                        // Execute tool calls
                        for call in tool_calls {
                            let name = call.function.name;
                            let arguments: HashMap<String, serde_json::Value> =
                                serde_json::from_str(&call.function.arguments).unwrap_or_default();

                            // Execute tool
                            let tool_result = if let Some(func) = tools.tools.get(&name) {
                                match func(arguments).await {
                                    Ok(r) => r,
                                    Err(e) => format!("Error executing {}: {}", name, e),
                                }
                            } else {
                                format!("Unknown tool: {}", name)
                            };

                            // Add tool result to messages
                            messages.push(Message {
                                role: "tool".into(),
                                content: tool_result,
                                tool_call_id: Some(call.id),
                                name: Some(name),
                                tool_calls: None,
                            });
                        }
                    } else {
                        // No tool calls field, we're done
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    break;
                }
            }
        }
        return;
    }

    // Interactive mode
    println!("Interactive mode - type 'exit' or 'quit' to quit");
    println!("Type 'clear' to clear conversation history");
    println!("Type 'context' to show project context");
    println!("Type 'analyze' to re-analyze project");
    println!(
        "\n📁 Project context loaded ({} chars)",
        project_context.len()
    );

    let mut history: Vec<Message> = vec![];
    // Inject project context as system message
    history.push(Message {
        role: "system".into(),
        content: project_context.clone(),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });

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
            // Re-add project context
            history.push(Message {
                role: "system".into(),
                content: project_context.clone(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
            println!("Conversation history cleared (project context preserved).");
            continue;
        }
        if input == "context" {
            println!(
                "{}",
                &project_context[..std::cmp::min(3000, project_context.len())]
            );
            if project_context.len() > 3000 {
                println!("... (truncated, total {} chars)", project_context.len());
            }
            continue;
        }
        if input == "analyze"
            || input == "analizar"
            || input == "analizar proyecto"
            || input == "analyze project"
        {
            println!("🔍 Re-analyzing project...");
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let analyzer = ContextAnalyzer::new(cwd);
            let new_context = analyzer
                .analyze()
                .map(|ctx| format_context_for_llm(&ctx))
                .unwrap_or_else(|_| {
                    analyze_project_context(
                        &std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                    )
                });
            // Update the system message with fresh context
            history[0] = Message {
                role: "system".into(),
                content: new_context.clone(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            };
            println!("✅ Project context updated ({} chars)", new_context.len());
            continue;
        }

        history.push(Message {
            role: "user".into(),
            content: input.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        // Tool calling loop for interactive
        let mut messages = history.clone();
        let mut max_rounds = 15;

        loop {
            max_rounds -= 1;
            if max_rounds == 0 {
                eprintln!("\n⚠️  Max tool rounds reached");
                break;
            }

            let result = llm
                .chat_with_tools(
                    messages.clone(),
                    Some(tool_schemas.clone()),
                    Some("auto".to_string()),
                )
                .await;

            match result {
                Ok(msg) => {
                    let content = strip_think_tags(&msg.content);
                    if !content.is_empty() {
                        println!("{}", content);
                    }

                    // Add assistant message to history
                    messages.push(msg.clone());

                    // Check for tool calls
                    if let Some(tool_calls) = msg.tool_calls {
                        if tool_calls.is_empty() {
                            // No tool calls, we're done
                            break;
                        }

                        // Execute tool calls
                        for call in tool_calls {
                            let name = call.function.name;
                            let arguments: HashMap<String, serde_json::Value> =
                                serde_json::from_str(&call.function.arguments).unwrap_or_default();

                            // Execute tool
                            let tool_result = if let Some(func) = tools.tools.get(&name) {
                                match func(arguments).await {
                                    Ok(r) => r,
                                    Err(e) => format!("Error executing {}: {}", name, e),
                                }
                            } else {
                                format!("Unknown tool: {}", name)
                            };

                            // Add tool result to messages
                            messages.push(Message {
                                role: "tool".into(),
                                content: tool_result,
                                tool_call_id: Some(call.id),
                                name: Some(name),
                                tool_calls: None,
                            });
                        }
                    } else {
                        // No tool calls field, we're done
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    break;
                }
            }
        }

        // Update history with the final messages
        history = messages;
        println!();
    }
}

pub async fn cmd_code(
    description: &str,
    filename: &str,
    yolo: bool,
    model: &str,
    base_url: &str,
    allow_remote: bool,
) {
    let client =
        LlmClient::new(base_url.to_string(), model.to_string()).with_allow_remote(allow_remote);
    let tools = ToolRegistry::new();
    let max_iterations: i32 = 3;

    let file_path = Path::new(filename);
    let file_exists = file_path.exists();

    // Read existing content if file exists
    let existing_content = if file_exists {
        fs::read_to_string(file_path).unwrap_or_default()
    } else {
        String::new()
    };

    let base_prompt = if file_exists {
        format!(
            "You are editing an existing Python file. Generate ONLY the new method code to add.\n\n\
             File: {}\n\
             Current content:\n```python\n{}\n```\n\n\
             Task: {}\n\n\
             Output format: Start with METHOD_START and end with METHOD_END.\n\
             Between them, write ONLY the method definition (with 'def ').\n\
             No class, no imports, no explanations, no markdown.\n\n\
             METHOD_START\n    def raiz_cuadrada(self, x):\n        return math.sqrt(x)\nMETHOD_END",
            filename, existing_content, description
        )
    } else {
        format!(
            "Generate ONLY valid Python code. No markdown, no explanations.\n\
             Task: {}\nOutput ONLY the code.",
            description
        )
    };

    let mut prompt = format!("/no_think {}", base_prompt);
    let mut new_method = String::new();
    let mut success = false;

    for iteration in 0..max_iterations {
        println!("Iteration {} of {}", iteration + 1, max_iterations);

        let result = client.chat(&prompt).await;
        match result {
            Ok(content) => {
                // Extract method between METHOD_START and METHOD_END markers
                new_method = if let Some(start) = content.find("METHOD_START") {
                    let after_start = &content[start + "METHOD_START".len()..];
                    if let Some(end) = after_start.find("METHOD_END") {
                        after_start[..end].trim().to_string()
                    } else {
                        extract_code(&content)
                    }
                } else {
                    extract_code(&content)
                };

                // For existing files, validate the new method syntax by embedding in a minimal class
                let test_code = if file_exists {
                    format!(
                        "class Test:\n{}",
                        new_method
                            .lines()
                            .map(|l| format!("    {}", l))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                } else {
                    new_method.clone()
                };

                match validate_code(&test_code) {
                    Ok(()) => {
                        println!("Validation passed.");
                        success = true;
                        break;
                    }
                    Err(e) => {
                        eprintln!("Validation error (iteration {}): {}", iteration + 1, e);
                        if iteration < max_iterations - 1 {
                            prompt = format!(
                                "/no_think Fix the following syntax error:\n\n\
                                Code:\n{}\n\nError: {}\n\nOutput format: Start with METHOD_START and end with METHOD_END.\n\
                                Between them, write ONLY the corrected method definition.\n\n\
                                METHOD_START\n{}\nMETHOD_END",
                                new_method, e, new_method
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
        if file_exists {
            // Insert new method before the last line of the class (before the closing)
            let mut lines: Vec<String> = existing_content.lines().map(|s| s.to_string()).collect();

            // Find the last non-empty line that's not just whitespace (end of class)
            let mut insert_idx = lines.len();
            for (i, line) in lines.iter().enumerate().rev() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    insert_idx = i + 1;
                    break;
                }
            }

            // Normalize indentation: first line (def) gets 4 spaces, body gets 8 spaces
            let indented_method: Vec<String> = new_method
                .lines()
                .enumerate()
                .map(|(i, l)| {
                    let trimmed = l.trim();
                    if trimmed.is_empty() {
                        String::new()
                    } else if i == 0 {
                        // First line (def): 4 spaces
                        format!("    {}", trimmed)
                    } else {
                        // Body lines: 8 spaces (4 for class + 4 for method body)
                        format!("        {}", trimmed)
                    }
                })
                .collect();

            // Insert blank line before new method if needed
            if insert_idx > 0 && !lines[insert_idx - 1].trim().is_empty() {
                lines.insert(insert_idx, String::new());
                insert_idx += 1;
            }

            // Insert the new method lines
            for (i, method_line) in indented_method.into_iter().enumerate() {
                lines.insert(insert_idx + i, method_line);
            }

            let new_content = lines.join("\n");

            // Write the updated file
            let args = serde_json::json!({
                "path": filename,
                "content": new_content
            });
            if let Some(func) = tools.tools.get("write_file") {
                let args_map: HashMap<String, Value> =
                    serde_json::from_value(args).unwrap_or_default();
                match func(args_map).await {
                    Ok(result) => {
                        println!("{}", result);
                    }
                    Err(e) => {
                        eprintln!("Error writing file: {}", e);
                    }
                }
            } else {
                eprintln!("write_file tool not available");
            }
        } else {
            // Write new file
            let args = serde_json::json!({
                "path": filename,
                "content": new_method
            });
            if let Some(func) = tools.tools.get("write_file") {
                let args_map: HashMap<String, Value> =
                    serde_json::from_value(args).unwrap_or_default();
                match func(args_map).await {
                    Ok(result) => {
                        println!("{}", result);
                    }
                    Err(e) => {
                        eprintln!("Error writing file: {}", e);
                    }
                }
            } else {
                eprintln!("write_file tool not available");
            }
        }
    } else {
        eprintln!(
            "Failed to generate valid code after {} iterations",
            max_iterations
        );
    }
}

pub async fn cmd_status() {
    println!("lai - Local AI Code Assistant");
    println!("Services: checking...");
    let services = Services::new();
    let client = reqwest::Client::new();
    crate::services::check_all_services_async(&services, &client).await;
    for (name, svc) in services.list() {
        println!(
            "  {}: {} (Port {})",
            name,
            if svc.is_running() {
                "Running"
            } else {
                "Stopped"
            },
            svc.port
        );
    }
    println!(
        "  {}",
        crate::sandbox::SandboxReport::collect().short_line()
    );
}

fn extract_code(response: &str) -> String {
    if let Some(start) = content_between(response, "```python\n", "\n```") {
        return start.to_string();
    }
    if let Some(start) = content_between(response, "```\n", "\n```") {
        return start.to_string();
    }
    let lines: Vec<&str> = response.lines().collect();
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

    use std::process::Stdio;
    let child = std::process::Command::new("python3")
        .arg("-c")
        .arg(format!("import ast; ast.parse({:?})", code))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn error: {}", e))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("wait error: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("Syntax error: {}", err))
    }
}

// ===== Missing command stubs =====

fn strip_think_tags(content: &str) -> String {
    let mut r = content.to_string();

    // Handle <thinking>...</thinking> tags (including multiline)
    r = regex::Regex::new(r"(?s)<thinking>.*?</thinking>")
        .unwrap()
        .replace_all(&r, "")
        .to_string();

    // Handle ```thinking...``` code blocks
    r = regex::Regex::new(r"(?s)```thinking.*?```")
        .unwrap()
        .replace_all(&r, "")
        .to_string();

    // Handle ```think...``` code blocks
    r = regex::Regex::new(r"(?s)```think.*?```")
        .unwrap()
        .replace_all(&r, "")
        .to_string();

    // Handle any remaining <think>...</think> tags
    r = regex::Regex::new(r"(?s)<think>.*?</think>")
        .unwrap()
        .replace_all(&r, "")
        .to_string();

    // Clean up leftover ``` markers that might be orphaned
    r = regex::Regex::new(r"(?m)^```\s*$")
        .unwrap()
        .replace_all(&r, "")
        .to_string();

    // Clean up leading/trailing whitespace including newlines
    r = regex::Regex::new(r"^\s+")
        .unwrap()
        .replace_all(&r, "")
        .to_string();
    r = regex::Regex::new(r"\s+$")
        .unwrap()
        .replace_all(&r, "")
        .to_string();

    // Aggressively collapse multiple consecutive newlines at start
    r = regex::Regex::new(r"^\n+")
        .unwrap()
        .replace_all(&r, "")
        .to_string();
    // Collapse multiple consecutive newlines anywhere to max 2
    r = regex::Regex::new(r"\n{3,}")
        .unwrap()
        .replace_all(&r, "\n\n")
        .to_string();

    r.trim().to_string()
}

pub async fn cmd_auto(
    task: &str,
    max_iterations: u32,
    output: Option<&str>,
    model: &str,
    base_url: &str,
    allow_remote: bool,
    profile: &str,
) {
    println!("🤖 Auto mode: {}", task);
    println!("   Max iterations: {}", max_iterations);
    if let Some(out) = output {
        println!("   Output dir: {}", out);
    }
    println!("   Model: {}", model);
    println!("   Base URL: {}", base_url);

    let tool_profile = match crate::tools::ToolProfile::from_str(profile) {
        Some(profile) => profile,
        None => {
            eprintln!(
                "Aviso: perfil '{}' desconocido; se usa 'full' (opciones: full, bash).",
                profile
            );
            crate::tools::ToolProfile::Full
        }
    };
    println!("   Tool profile: {:?}", tool_profile);

    // Load project context
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let analyzer = ContextAnalyzer::new(cwd.clone());
    let project_context = analyzer
        .analyze()
        .map(|ctx| format_context_for_llm(&ctx))
        .unwrap_or_else(|_| analyze_project_context(&cwd));
    let project_context = truncate_project_context(&project_context);

    let tools = ToolRegistry::new();

    // Build tool schema descriptions for the system prompt
    let tool_schemas = tools.get_tools_schema_for(tool_profile);
    let tools_description = build_tools_description(&tool_schemas);

    // Native function-calling schemas ([OI]-compatible) for models that support tools
    let native_tool_schemas: Vec<crate::llm::ToolSchema> = tool_schemas
        .iter()
        .map(|t| crate::llm::ToolSchema {
            schema_type: "function".to_string(),
            function: crate::llm::FunctionSchema {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            },
        })
        .collect();

    let system_prompt = if tool_profile == crate::tools::ToolProfile::BashOnly {
        // Prompt corto a propósito: en 8k-32k tokens cada línea del prompt
        // compite con el código. Un solo comando por turno, edición con
        // heredoc y `git diff` para comprobar el resultado.
        r#"You are a coding agent working inside a git repository (your current directory).
You have ONE tool: `bash`. Call it with a single shell command per turn.

How to work:
- Explore: `ls`, `grep -rn "symbol" .`, `sed -n '1,80p' file`.
- Edit: `python - <<'EOF' ... EOF` or `sed -i`, or `cat > file <<'EOF' ... EOF`.
  Never paste example code from the issue: change the real file.
- Verify: `python -m py_compile file.py`, `pytest -q path::test`, `cargo check`.
- Finish: run `echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT` as the only command of
  a turn. That ends the run; do not combine it with other commands.

Rules:
- One command per turn; the result comes back to you.
- Only change what is necessary to fix the issue. Do not modify tests.
- If a command fails, read the error and try a different approach; do not repeat it.
"#
        .to_string()
    } else {
        format!(
            r#"You are an AI assistant with access to tools. You can use tools by responding with JSON in a code block.

{tools_description}

When you need to use a tool, respond with a JSON code block in this format:

```json
{{
  "tool": "tool_name",
  "arguments": {{
    "arg1": "value1",
    "arg2": "value2"
  }}
}}
```

You can call multiple tools in one response by including multiple JSON blocks. After each tool call, you will receive the result. Continue until the task is complete.

For work with 3+ steps, call `todo` with action=set to record a short plan, then
mark items done as you finish them. Keep it under 6 items.

Workflow for fixing a bug:
1. Read the failing code and the test that fails.
2. Reproduce the failure before changing anything.
3. Make the smallest change that fixes the cause, not the symptom.
4. Run the test again and report the real result. If it still fails, say so.

Never claim a test passed if you did not run it. Never paste example code from
the issue over the real file.

IMPORTANT: Only output the JSON code block when you want to call a tool. Otherwise, respond normally with text.

TOOL USAGE GUIDE:
- "write_file": CREATE a new file, or REPLACE an existing one entirely. Args: {{"path": "file.py", "content": "code here"}}
  It refuses to replace an existing file of 40+ lines with less than 25% of its length (that usually means writing an example over real code). If you really mean it, add "allow_shrink": true.
- "edit": EDIT an existing file by replacing an exact fragment. Args: {{"path": "f.py", "old_string": "exact text", "new_string": "new text"}}. PREFERRED for existing files: copy the fragment exactly from read_file. Fails if it is missing or ambiguous.
- "apply_patch": EDIT an existing file with a unified diff. Args: {{"patch": "--- a/f.py\n+++ b/f.py\n@@ ..."}}. PREFER THIS for changes to existing files: it keeps the rest of the file intact.
- "read_file": READ a file or a 1-based line range. Args: {{"path": "file.py"}} or {{"path": "file.py", "start_line": 120, "end_line": 220}}. Use a range for large files; the default is capped at 400 lines.
- "run_python": RUN PYTHON CODE (not commands). Args: {{"code": "print(2+2)"}} - ONLY valid Python code!
- "run_command": RUN SHELL COMMANDS. Args: {{"command": "ls -la"}} - Use for shell commands!
- "list_dir": LIST directory. Args: {{"path": "."}}
- "grep": SEARCH files. Args: {{"pattern": "text", "path": "."}}; optional "regex": true, "case_insensitive": true, "glob": "*.rs". Prefer grep to locate symbols before reading whole files.
- "find_files": FIND files by glob. Args: {{"pattern": "src/**/*.rs", "path": "."}}

RULES:
- To CREATE a file: MUST use "write_file" with "path" and "content"
- To CHANGE an existing file: use "edit" with the exact fragment, or "apply_patch" with a minimal diff. NEVER rewrite a whole existing file with "write_file" unless you intend to replace all of it.
- Never paste example code from the issue into a source file: the example illustrates the bug, it is not the fix.
- To RUN a Python script file: Use "run_command" with "command": "python file.py"
- To RUN Python code directly: Use "run_python" with "code": "valid python code"
- "run_python" does NOT accept shell commands like "python file.py"

EXAMPLE - Creating a file:
```json
{{
  "tool": "write_file",
  "arguments": {{
    "path": "hello.py",
    "content": "print('Hello World')"
  }}
}}
```

EXAMPLE - Running Python code directly:
```json
{{
  "tool": "run_python",
  "arguments": {{
    "code": "2 + 2"
  }}
}}
```

EXAMPLE - Running a Python script file:
```json
{{
  "tool": "run_command",
  "arguments": {{
    "command": "python hello.py"
  }}
}}
```

EXAMPLE - Reading a file:
```json
{{
  "tool": "read_file",
  "arguments": {{
    "path": "hello.py"
  }}
}}
```

If the user asks you to create a file, your FIRST tool call MUST be "write_file"."#
        )
    };

    // Temperatura baja: un agente de código debe repetir el mismo camino ante
    // el mismo estado. `LAI_TEMPERATURE` permite subirla si se prefiere
    // exploración; por defecto 0.2.
    let temperature = std::env::var("LAI_TEMPERATURE")
        .ok()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(0.2);
    println!("   Temperature: {}", temperature);
    let llm = LlmClient::new(base_url.to_string(), model.to_string())
        .with_allow_remote(allow_remote)
        .with_temperature(temperature);

    let mut messages = vec![
        crate::llm::Message {
            role: "system".into(),
            content: format!("{}\n\n{}", project_context, system_prompt),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        crate::llm::Message {
            role: "user".into(),
            content: task.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];

    // If the model answers in prose without touching any tool, we ask once or
    // twice more before accepting it as a final answer. Without this, small
    // local models often "describe" the code instead of writing it.
    let mut nudges: u32 = 0;
    const MAX_NUDGES: u32 = 2;
    /// Marca que el agente usa para declarar la tarea terminada en el perfil
    /// bash (mini-SWE-agent). Se detecta en la salida de la herramienta.
    const SUBMIT_SENTINEL: &str = "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT";

    // Detección de bucles: un modelo pequeño puede repetir la misma llamada
    // idéntica indefinidamente. Medido en el piloto del 2026-11-26: cuatro
    // iteraciones con el mismo apply_patch, quemando 1500 s sin avanzar.
    let mut repeated_calls: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    const MAX_IDENTICAL_CALLS: u32 = 3;

    // Escalada por presupuesto de turnos: medido el 2026-11-27 con
    // qwen3-8b en astropy-12907, el modelo gastó las 8 iteraciones leyendo
    // archivos y nunca editó. Cuando se agota la mitad del presupuesto sin
    // ningún cambio, se le dice explícitamente cuántos turnos quedan y se le
    // pide que actúe. Una sola vez, para no convertir el bucle en ruido.
    let mut edits_made: u32 = 0;
    let mut escalation_sent = false;
    let escalation_at = (max_iterations / 2).max(1);

    'agent: for iteration in 1..=max_iterations {
        if !escalation_sent && edits_made == 0 && iteration > escalation_at {
            escalation_sent = true;
            let remaining = max_iterations.saturating_sub(iteration) + 1;
            println!(
                "⏳ Sin cambios tras {} iteraciones: pidiendo al modelo que aplique el arreglo",
                iteration - 1
            );
            messages.push(crate::llm::Message {
                role: "user".into(),
                content: format!(
                    "You have {remaining} turn(s) left and you have not modified any file yet.\n\n\
                     Stop exploring. Apply the fix NOW with a tool call:\n\
                     - Use `edit` with the exact fragment you already read (path, old_string, new_string), or\n\
                     - use `write_file` if the file must be replaced entirely.\n\n\
                     If you are missing a piece of information, read only that specific range \
                     (read_file with start_line/end_line) in the same turn and then edit."
                ),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        println!("\n--- Iteration {} of {} ---", iteration, max_iterations);

        // Presupuesto de contexto: antes de cada llamada se eliden los
        // resultados de herramientas antiguos. Sin esto, en una ventana de 8k
        // el primer `read_file` grande expulsa la tarea y el modelo deja de
        // editar código. Se conservan sistema, tarea y los 4 turnos recientes.
        let context_window = crate::context_budget::context_window_from_env();
        let budget = crate::context_budget::budget_chars_from_window(context_window);
        let condensed = crate::context_budget::condense(&mut messages, budget, 4);
        if condensed.changed() {
            println!(
                "  🧹 Contexto condensado: {} → {} caracteres ({} salidas elididas, ventana {} tokens)",
                condensed.chars_before, condensed.chars_after, condensed.elided, context_window
            );
        }

        // Prefer native function calling; models without tool support answer in
        // plain text and we fall back to the JSON-block protocol below.
        let result = llm
            .chat_with_native_tools(
                messages.clone(),
                Some(native_tool_schemas.clone()),
                Some(crate::llm::ToolChoice::Auto),
            )
            .await;

        match result {
            Ok(outcome) => {
                let truncated = outcome.truncated();
                let msg = outcome.message;
                // Una respuesta truncada por `max_tokens` puede contener una
                // tool call a medias; ejecutarla aplicaría un parche corrupto.
                if truncated {
                    eprintln!(
                        "  ⚠ La respuesta se truncó al agotar max_tokens; no se ejecutan \
                         sus tool calls. Se pide al modelo una acción más corta."
                    );
                    messages.push(crate::llm::Message {
                        role: "user".into(),
                        content: "Your previous response was cut off by the output limit. \
                                  Reply with ONE short tool call (or a short final answer), \
                                  without long explanations."
                            .into(),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                    continue;
                }
                let content = strip_think_tags(&msg.content);
                if !content.is_empty() {
                    println!("{}", content);
                }
                messages.push(msg.clone());

                // Native tool calls take precedence; fall back to JSON code
                // blocks and then to XML-style calls (some local models switch
                // format mid-run).
                let mut tool_calls = parse_native_tool_calls(&msg.tool_calls);
                if tool_calls.is_empty() {
                    tool_calls = parse_simulated_tool_calls(&msg.content);
                }
                if tool_calls.is_empty() {
                    tool_calls = parse_xml_tool_calls(&msg.content);
                }

                if tool_calls.is_empty() {
                    // Si el modelo ya trabajó y ahora responde en prosa, lo más
                    // probable es que sea su respuesta final: no se le empuja a
                    // repetir trabajo. Solo se insiste cuando aún no ha usado
                    // ninguna herramienta (el caso de "describir en vez de actuar").
                    let has_worked = edits_made > 0;
                    if !has_worked && nudges < MAX_NUDGES && iteration < max_iterations {
                        nudges += 1;
                        println!(
                            "↻ No tool call emitted; asking the model to act ({}/{})",
                            nudges, MAX_NUDGES
                        );
                        // Restate the task: with small context windows the
                        // original request is the first thing to be evicted.
                        messages.push(crate::llm::Message {
                            role: "user".into(),
                            content: format!(
                                "You stopped without using a tool, but the task is not done yet.\n\n\
TASK:\n{task}\n\n\
Call a tool now. If you already know the fix, call write_file (or apply_patch) with the \
complete corrected file content. Do not reply with analysis or questions — act."
                            ),
                            tool_calls: None,
                            tool_call_id: None,
                            name: None,
                        });
                        continue;
                    }
                    println!("✅ Task completed (no tool calls requested)");
                    break;
                }

                println!("🔧 Executing {} tool call(s)...", tool_calls.len());

                for call in tool_calls {
                    let name = &call["tool"];
                    let name_str = name.as_str().unwrap_or("");
                    // Native tool calls carry an id that must be echoed back.
                    let call_id = call
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let mut arguments = call["arguments"]
                        .as_object()
                        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default();

                    // Fix common model mistakes
                    arguments = normalize_tool_arguments(name_str, arguments);

                    // Check for normalization errors (e.g., shell command passed to run_python)
                    if let Some(error) = arguments.get("_error") {
                        let error_msg = error.as_str().unwrap_or("Unknown error");
                        println!("  ✗ Validation Error: {}", error_msg);
                        let tool_result = error_msg.to_string();

                        messages.push(crate::llm::Message {
                            role: "tool".into(),
                            content: tool_result,
                            tool_call_id: call_id.clone(),
                            name: Some(name_str.to_string()),
                            tool_calls: None,
                        });
                        continue;
                    }

                    println!(
                        "  → {} ({})",
                        name_str,
                        serde_json::to_string(&arguments).unwrap_or_default()
                    );

                    // La misma llamada, con los mismos argumentos, ya falló
                    // varias veces: no la ejecutamos otra vez, se le dice al
                    // modelo que cambie de estrategia.
                    let signature = format!(
                        "{}:{}",
                        name_str,
                        serde_json::to_string(&arguments).unwrap_or_default()
                    );
                    let seen = repeated_calls.entry(signature).or_insert(0);
                    *seen += 1;
                    if *seen > MAX_IDENTICAL_CALLS {
                        let warning = format!(
                            "You have already called {} with these exact arguments {} times \
                             without making progress. Do NOT repeat it. Read the file again \
                             with read_file (using start_line/end_line) to see its real content, \
                             then change your approach or explain why you are blocked.",
                            name_str, *seen
                        );
                        println!("  ⚠ repeated call #{}, skipping execution", *seen);
                        // Tras varias repeticiones el modelo no está leyendo la
                        // advertencia: seguir girando solo quema tiempo. Medido
                        // el 2026-11-27 en pytest-10081: 9 repeticiones del
                        // mismo read_file y parche vacío al final.
                        if *seen > MAX_IDENTICAL_CALLS + 3 {
                            println!(
                                "⛔ El modelo repite la misma llamada sin avanzar: se detiene el bucle."
                            );
                            break 'agent;
                        }
                        messages.push(crate::llm::Message {
                            role: "tool".into(),
                            content: warning,
                            tool_call_id: call_id.clone(),
                            name: Some(name_str.to_string()),
                            tool_calls: None,
                        });
                        continue;
                    }

                    // Las escrituras pasan por el PolicyEngine (frontera del
                    // workspace, .git, credenciales y fuentes de lai). Los
                    // comandos solo pasan si LAI_COMMAND_ALLOWLIST=1: la
                    // allowlist por defecto es más restrictiva que lo que
                    // necesita un agente de código y bloquearía `cd`, `sed`,
                    // `make`, etc.
                    let check_commands = crate::policy::command_allowlist_from_env();
                    let result = if matches!(
                        name_str,
                        "write_file"
                            | "apply_patch"
                            | "edit"
                            | "git_add"
                            | "git_commit"
                            | "git_rollback"
                    ) || (check_commands
                        && matches!(name_str, "run_command" | "bash" | "run_python"))
                    {
                        tools.call_tool(name_str, arguments).await
                    } else if let Some(func) = tools.tools.get(name_str) {
                        func(arguments).await
                    } else {
                        Err(anyhow::anyhow!("Unknown tool: {}", name_str))
                    };

                    let tool_result = match result {
                        Ok(r) => {
                            println!("  ✓ Done");
                            if matches!(name_str, "write_file" | "apply_patch" | "edit") {
                                edits_made += 1;
                            }
                            r
                        }
                        Err(e) => {
                            println!("  ✗ Error: {}", e);
                            format!("Error executing {}: {}", name_str, e)
                        }
                    };

                    // Bound the tool output: reading a large file can otherwise
                    // blow the model's context window and evict the task itself.
                    let tool_result = truncate_tool_output(name_str, &tool_result);

                    messages.push(crate::llm::Message {
                        role: "tool".into(),
                        content: tool_result.clone(),
                        tool_call_id: call_id,
                        name: Some(name_str.to_string()),
                        tool_calls: None,
                    });

                    // Sentinel de entrega (técnica de mini-SWE-agent): en el
                    // perfil bash el modelo no puede "terminar" sin llamar a
                    // una herramienta. Si emite este comando, la tarea se da
                    // por entregada y el bucle se detiene.
                    if matches!(name_str, "bash" | "run_command")
                        && tool_result.contains(SUBMIT_SENTINEL)
                    {
                        println!("✅ Tarea entregada por el agente (submit).");
                        break 'agent;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }

    // `--output` escribía solo un mensaje por pantalla. Ahora deja un registro
    // legible de la sesión: qué leyó el agente, qué ejecutó y con qué resultado.
    if let Some(dir) = output {
        if let Err(e) = write_auto_transcript(Path::new(dir), task, model, &messages) {
            eprintln!("Aviso: no se pudo escribir el registro en '{}': {e}", dir);
        }
    }
}

/// Escribe la transcripción de una sesión de `auto` en el directorio indicado.
fn write_auto_transcript(
    dir: &Path,
    task: &str,
    model: &str,
    messages: &[crate::llm::Message],
) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("lai-auto-{stamp}.md"));

    let mut out = String::new();
    out.push_str(&format!("# lai auto — {stamp}\n\n"));
    out.push_str(&format!("- Modelo: `{model}`\n"));
    out.push_str(&format!("- Tarea: {task}\n\n"));

    for message in messages {
        match message.role.as_str() {
            "system" => continue,
            "user" => out.push_str(&format!("## Usuario\n\n{}\n\n", message.content)),
            "assistant" => {
                if !message.content.trim().is_empty() {
                    out.push_str(&format!("## Asistente\n\n{}\n\n", message.content));
                }
                if let Some(calls) = &message.tool_calls {
                    for call in calls {
                        out.push_str(&format!(
                            "### → `{}`\n\n```json\n{}\n```\n\n",
                            call.function.name, call.function.arguments
                        ));
                    }
                }
            }
            "tool" => out.push_str(&format!(
                "### ← resultado de `{}`\n\n```\n{}\n```\n\n",
                message.name.as_deref().unwrap_or("tool"),
                message.content
            )),
            _ => {}
        }
    }

    fs::write(&path, out)?;
    println!("📝 Registro de la sesión: {}", path.display());
    Ok(path)
}

/// Maximum characters of a tool result kept in the conversation history.
///
/// Local models often run with a 4k-8k context window; a single `read_file` on a
/// large source file would otherwise evict the task description and the agent
/// would start answering questions instead of editing code.
const MAX_TOOL_OUTPUT_CHARS: usize = 6_000;

/// Maximum characters of automatic project context injected in chat/auto.
///
/// The analyzer can return README, module lists and objectives; on small models
/// this preamble can consume most of the window before the user's task arrives.
const MAX_PROJECT_CONTEXT_CHARS: usize = 8_000;

fn truncate_project_context(context: &str) -> String {
    if context.chars().count() <= MAX_PROJECT_CONTEXT_CHARS {
        return context.to_string();
    }
    let keep = MAX_PROJECT_CONTEXT_CHARS / 2;
    let head: String = context.chars().take(keep).collect();
    let tail: String = context
        .chars()
        .rev()
        .take(keep)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let omitted = context.chars().count() - head.chars().count() - tail.chars().count();
    format!(
        "{head}\n\n[... {omitted} characters of project context omitted to protect the \
         model context window ...]\n\n{tail}"
    )
}

/// Truncate a tool result keeping the head and the tail, with a clear marker.
fn truncate_tool_output(tool: &str, output: &str) -> String {
    if output.chars().count() <= MAX_TOOL_OUTPUT_CHARS {
        return output.to_string();
    }
    let keep = MAX_TOOL_OUTPUT_CHARS / 2;
    let head: String = output.chars().take(keep).collect();
    let tail: String = output
        .chars()
        .rev()
        .take(keep)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let omitted = output.chars().count() - head.chars().count() - tail.chars().count();
    format!(
        "{head}\n\n[... {omitted} characters omitted from {tool} output; \
read a specific range or use grep to see more ...]\n\n{tail}"
    )
}

/// Convert literal escape sequences (`\n`, `\t`, `\"`) into real characters.
///
/// Only applied when the payload has no real newline at all: in that case the
/// model double-escaped the JSON and the file would end up as a single line.
fn unescape_literal_escapes(content: &str) -> String {
    if content.contains('\n') {
        return content.to_string();
    }
    content
        .replace("\\r\\n", "\n")
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\\"", "\"")
}

fn build_tools_description(schemas: &[crate::tools::ToolSchema]) -> String {
    let mut desc = String::from("Available tools:\n");
    for schema in schemas {
        desc.push_str(&format!(
            "- {}: {}\n  Parameters: {}\n",
            schema.name, schema.description, schema.parameters
        ));
    }
    desc
}

/// Parse tool calls emitted by the model through native function calling.
fn parse_native_tool_calls(tool_calls: &Option<Vec<crate::llm::ToolCall>>) -> Vec<Value> {
    let mut calls = Vec::new();
    if let Some(list) = tool_calls {
        for tc in list {
            let arguments: Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
            // Some models emit arguments as a nested string; unwrap it once.
            let arguments = match arguments {
                Value::String(inner) => serde_json::from_str(&inner).unwrap_or(Value::Null),
                other => other,
            };
            calls.push(serde_json::json!({
                "tool": tc.function.name,
                "arguments": arguments,
                "id": tc.id,
            }));
        }
    }
    calls
}

fn parse_simulated_tool_calls(response: &str) -> Vec<Value> {
    let mut calls = Vec::new();
    let mut search_start = 0;

    while let Some(start) = response[search_start..].find("```") {
        let fence_start = search_start + start;
        let after_fence = fence_start + 3;
        // Opcional: etiqueta de lenguaje tras las comillas (```json, ```JSON).
        let mut body_start = after_fence;
        while let Some(c) = response[body_start..].chars().next() {
            if c.is_ascii_alphanumeric() {
                body_start += c.len_utf8();
            } else {
                break;
            }
        }
        let lang = response[after_fence..body_start]
            .trim()
            .to_ascii_lowercase();
        if let Some(end) = response[body_start..].find("```") {
            let json_str = response[body_start..body_start + end].trim();
            let looks_like_json = json_str.starts_with('{');
            if (lang.is_empty() || lang == "json") && looks_like_json {
                if let Some(call) = parse_tool_call_json(json_str) {
                    calls.push(call);
                }
            }
            search_start = body_start + end + 3;
        } else {
            break;
        }
    }

    calls
}

/// Fallback para modelos que emiten tool calls en XML en lugar de JSON:
///
/// ```text
/// <tool_call><function=read_file><parameter=path>src/lib.rs</parameter></function></tool_call>
/// ```
///
/// llama.cpp fija el parser al primer formato que detecta en una corrida; si el
/// modelo cambia de JSON a XML a mitad, la segunda llamada llega como texto y el
/// bucle la interpretaría como "tarea terminada" (caso documentado por Bees).
/// Este fallback recupera esas llamadas en vez de terminar en silencio.
fn parse_xml_tool_calls(response: &str) -> Vec<Value> {
    let mut calls = Vec::new();
    let mut rest = response;
    while let Some(pos) = rest.find("<tool_call>") {
        let after = &rest[pos + "<tool_call>".len()..];
        let Some(end) = after.find("</tool_call>") else {
            break;
        };
        let block = &after[..end];
        if let Some(call) = parse_xml_tool_call_block(block) {
            calls.push(call);
        }
        rest = &after[end + "</tool_call>".len()..];
    }

    // Algunos modelos omiten el envoltorio y emiten solo `<function=...>`.
    if calls.is_empty() {
        if let Some(start) = response.find("<function=") {
            if let Some(call) = parse_xml_tool_call_block(&response[start..]) {
                calls.push(call);
            }
        }
    }
    calls
}

fn parse_xml_tool_call_block(block: &str) -> Option<Value> {
    let name_start = block.find("<function=")? + "<function=".len();
    let name_rest = &block[name_start..];
    let name_end = name_rest.find(|c: char| c == '>' || c.is_whitespace())?;
    let name = name_rest[..name_end]
        .trim()
        .trim_end_matches('/')
        .to_string();
    if name.is_empty() {
        return None;
    }

    let mut args = serde_json::Map::new();
    let mut rest = &name_rest[name_end..];
    while let Some(pos) = rest.find("<parameter=") {
        let after = &rest[pos + "<parameter=".len()..];
        let Some(key_end) = after.find('>') else {
            break;
        };
        let key = after[..key_end].trim().to_string();
        let value_rest = &after[key_end + 1..];
        let Some(value_end) = value_rest.find("</parameter>") else {
            break;
        };
        let mut value = value_rest[..value_end].to_string();
        // Desescapar las entidades XML habituales.
        value = value
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&");
        if !key.is_empty() {
            args.insert(key, Value::String(value));
        }
        rest = &value_rest[value_end + "</parameter>".len()..];
    }

    Some(serde_json::json!({
        "tool": name,
        "arguments": Value::Object(args),
    }))
}

/// Escapa los caracteres de control literales que aparecen dentro de cadenas
/// JSON. Los modelos pequeños escriben código con saltos de línea reales dentro
/// del string `content`, produciendo JSON inválido que `serde_json` rechaza.
fn repair_json_control_chars(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 32);
    let mut in_string = false;
    let mut escaped = false;
    for ch in raw.chars() {
        if in_string {
            if escaped {
                out.push(ch);
                escaped = false;
                continue;
            }
            match ch {
                '\\' => {
                    out.push(ch);
                    escaped = true;
                }
                '"' => {
                    out.push(ch);
                    in_string = false;
                }
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
                c => out.push(c),
            }
        } else {
            if ch == '"' {
                in_string = true;
            }
            out.push(ch);
        }
    }
    out
}

/// Parsea el contenido de un bloque ```json y lo normaliza a
/// `{"tool": <nombre>, "arguments": <objeto>}`.
fn parse_tool_call_json(json_str: &str) -> Option<Value> {
    let mut val = serde_json::from_str::<Value>(json_str)
        .or_else(|_| serde_json::from_str::<Value>(&repair_json_control_chars(json_str)))
        .ok()?;

    // Acepta los alias que usan los modelos: tool / name / tool_name / function.
    let name = ["tool", "name", "tool_name", "function"]
        .iter()
        .find_map(|key| val.get(*key).and_then(|v| v.as_str()))
        .map(|s| s.to_string())?;

    // `arguments` puede venir como objeto o como string con JSON dentro.
    let args = match val.get_mut("arguments") {
        Some(Value::String(s)) => serde_json::from_str::<Value>(s).ok()?,
        Some(v @ Value::Object(_)) => v.clone(),
        _ => return None,
    };
    if !args.is_object() {
        return None;
    }

    Some(serde_json::json!({ "tool": name, "arguments": args }))
}

/// Heurística para distinguir una orden de shell de código Python cuando el
/// modelo usa el campo equivocado. `python3` se incluye porque es el nombre
/// habitual del intérprete en Linux.
fn looks_like_shell_command(text: &str) -> bool {
    [
        "python ", "python3 ", "ls ", "cat ", "echo ", "cd ", "mkdir ", "rm ", "cp ", "mv ",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
}

fn normalize_tool_arguments(tool: &str, args: HashMap<String, Value>) -> HashMap<String, Value> {
    match tool {
        // Normalise write_file: keep path/content as strings and repair payloads
        // where the model double-escaped newlines (whole file on one line).
        "write_file" => {
            // Conservar argumentos como `allow_shrink`: la guarda de
            // write_file le pide al modelo reintentar con esa bandera, y el
            // normalizador la borraba, así que el reintento nunca funcionaba.
            let mut normalized = args;
            if let Some(path) = normalized
                .get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            {
                normalized.insert("path".to_string(), Value::String(path));
            }
            if let Some(content) = normalized
                .get("content")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            {
                normalized.insert(
                    "content".to_string(),
                    Value::String(unescape_literal_escapes(&content)),
                );
            }
            normalized
        }
        "run_python" => {
            // Model often passes "command" or "path" instead of "code"
            let result = if let Some(code) = args.get("code").cloned() {
                // Check if it's a shell command (starts with python, ls, cat, etc.)
                if let Some(code_str) = code.as_str() {
                    let trimmed = code_str.trim();
                    if looks_like_shell_command(trimmed) {
                        // This is a shell command, not Python code!
                        // Return error by putting it in a special key that will be caught
                        hashmap!["_error".to_string() => Value::String(format!(
                            "ERROR: '{}' is a shell command, not Python code. Use 'run_command' tool instead with 'command' argument.",
                            trimmed
                        ))]
                    } else {
                        hashmap!["code".to_string() => code]
                    }
                } else {
                    hashmap!["code".to_string() => code]
                }
            } else if let Some(cmd) = args.get("command").cloned() {
                // Check if it's a shell command
                if let Some(cmd_str) = cmd.as_str() {
                    let trimmed = cmd_str.trim();
                    if looks_like_shell_command(trimmed) {
                        hashmap!["_error".to_string() => Value::String(format!(
                            "ERROR: '{}' is a shell command, not Python code. Use 'run_command' tool instead with 'command' argument.",
                            trimmed
                        ))]
                    } else {
                        // Assume it's Python code if not detected as shell command
                        hashmap!["code".to_string() => cmd]
                    }
                } else {
                    hashmap!["code".to_string() => cmd]
                }
            } else if let Some(path) = args.get("path").cloned() {
                // If it has path but no code, create a simple write
                hashmap!["code".to_string() => format!("print('Writing to {}')", path)]
            } else {
                // Default fallback
                hashmap!["code".to_string() => Value::String("print('Hello')".to_string())]
            };

            // Preservar el timeout explícito del modelo.
            let mut result = result;
            if let Some(timeout) = args.get("timeout").cloned() {
                result.insert("timeout".to_string(), timeout);
            }
            result
        }
        "run_command" => {
            // Conservar `timeout` y cualquier otro argumento futuro.
            let mut normalized = args;
            if let Some(cmd) = normalized
                .get("command")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            {
                normalized.insert("command".to_string(), Value::String(cmd));
            }
            if let Some(cwd) = normalized
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(str::to_string)
            {
                normalized.insert("cwd".to_string(), Value::String(cwd));
            }
            normalized
        }
        _ => args,
    }
}

/// Métricas simples y verificables de un workspace (sin LLM).
///
/// Se usan para `lai metrics` y para `lai baseline create/compare`: archivos
/// Python/Rust, símbolos y líneas. No son un benchmark: son datos del árbol.
fn workspace_metrics(root: &Path) -> std::collections::HashMap<String, f64> {
    use std::collections::HashMap;
    let map = crate::repomap::RepoMap::build(root);
    let mut metrics: HashMap<String, f64> = HashMap::new();
    let mut lines = 0usize;
    let mut symbols = 0usize;
    for file in &map.files {
        symbols += file.symbols.len();
        if let Ok(content) = fs::read_to_string(&file.path) {
            lines += content.lines().count();
        }
    }
    metrics.insert("files".to_string(), map.files.len() as f64);
    metrics.insert("symbols".to_string(), symbols as f64);
    metrics.insert("lines".to_string(), lines as f64);
    metrics
}

pub async fn cmd_repomap(path: Option<&str>, file: Option<&str>) {
    let root = path
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    if !root.exists() {
        eprintln!("RepoMap: la ruta '{}' no existe", root.display());
        return;
    }
    let map = crate::repomap::RepoMap::build(&root);
    let symbols: usize = map.files.iter().map(|f| f.symbols.len()).sum();

    if let Some(file) = file {
        // Acepta ruta relativa, absoluta o solo el nombre del archivo.
        let wanted = file.trim_start_matches("./");
        let found: Vec<_> = map
            .files
            .iter()
            .filter(|info| {
                let rel = info.path.strip_prefix(&root).unwrap_or(&info.path);
                rel.to_string_lossy() == wanted
                    || info.path.file_name().map(|n| n == wanted).unwrap_or(false)
            })
            .collect();
        if found.is_empty() {
            println!(
                "RepoMap: no se encontró '{}' ({} archivos indexados)",
                file,
                map.files.len()
            );
            return;
        }
        for info in found {
            let rel = info.path.strip_prefix(&root).unwrap_or(&info.path);
            println!("{}", rel.display());
            if info.symbols.is_empty() {
                println!("  (sin símbolos detectados)");
            }
            for symbol in &info.symbols {
                println!("  {:>5}  {} {}", symbol.line, symbol.kind, symbol.name);
            }
        }
        return;
    }

    println!(
        "📦 RepoMap: {} archivos, {} símbolos en {}",
        map.files.len(),
        symbols,
        root.display()
    );
    println!();
    println!("{}", map.to_text(8_000));
}

pub async fn cmd_baseline(command: crate::baseline::BaselineCommands) {
    use crate::baseline::{Baseline, BaselineMetric, BaselineStore};

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let store = match BaselineStore::new(&cwd) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("Baseline: no se pudo abrir la base de datos: {e}");
            return;
        }
    };

    // `list` muestra IDs de 8 caracteres: aceptar prefijos evita tener que
    // copiar el UUID completo.
    let resolve_id = |id: &str| -> Option<String> {
        if let Ok(Some(_)) = store.get(id) {
            return Some(id.to_string());
        }
        store
            .list(200)
            .ok()
            .and_then(|baselines| {
                baselines
                    .into_iter()
                    .find(|baseline| baseline.id.starts_with(id))
            })
            .map(|baseline| baseline.id)
    };

    let to_metric_map = |raw: &std::collections::HashMap<String, f64>| {
        raw.iter()
            .map(|(name, value)| (BaselineMetric::Custom(name.clone()), *value))
            .collect::<std::collections::HashMap<_, _>>()
    };

    match command {
        crate::baseline::BaselineCommands::List => match store.list(20) {
            Ok(baselines) if baselines.is_empty() => {
                println!("No hay baselines. Crea uno con: lai baseline create <nombre> <ruta>");
            }
            Ok(baselines) => {
                println!("{:<10} {:<24} {:<26} COMMIT", "ID", "NOMBRE", "FECHA");
                for baseline in baselines {
                    println!(
                        "{:<10} {:<24} {:<26} {}",
                        &baseline.id[..8.min(baseline.id.len())],
                        baseline.name,
                        baseline.created_at,
                        &baseline.git_commit[..8.min(baseline.git_commit.len())]
                    );
                }
            }
            Err(e) => eprintln!("Baseline: {e}"),
        },
        crate::baseline::BaselineCommands::Show { id } => match resolve_id(&id)
            .ok_or_else(|| format!("Baseline no encontrado: {id}"))
            .and_then(|full| store.get(&full))
        {
            Ok(Some(baseline)) => {
                println!("Baseline {}", baseline.id);
                println!("  Nombre:   {}", baseline.name);
                println!("  Ruta:     {}", baseline.project_path);
                println!("  Commit:   {}", baseline.git_commit);
                println!("  Rama:     {}", baseline.git_branch);
                println!("  Creado:   {}", baseline.created_at);
                if baseline.metrics.is_empty() {
                    println!("  Métricas: (ninguna)");
                } else {
                    println!("  Métricas:");
                    let mut entries: Vec<_> = baseline.metrics.iter().collect();
                    entries.sort_by_key(|a| a.0.as_str());
                    for (metric, value) in entries {
                        println!("    {:<20} {:.2}", metric.as_str(), value);
                    }
                }
            }
            Ok(None) => eprintln!("Baseline no encontrado: {id}"),
            Err(e) => eprintln!("Baseline: {e}"),
        },
        crate::baseline::BaselineCommands::Create { name, path } => {
            let project = PathBuf::from(&path);
            if !project.exists() {
                eprintln!("Baseline: la ruta '{}' no existe", path);
                return;
            }
            let mut baseline = match Baseline::new(&name, "workspace metrics", &project) {
                Ok(baseline) => baseline,
                Err(e) => {
                    eprintln!(
                        "Baseline: no se pudo leer git en '{}': {e}. \
                         Crea el baseline dentro de un repositorio git.",
                        path
                    );
                    return;
                }
            };
            for (metric, value) in workspace_metrics(&project) {
                baseline.add_metric(BaselineMetric::Custom(metric), value);
            }
            match store.create(&baseline) {
                Ok(()) => {
                    println!("Baseline creado: {}", baseline.id);
                    println!("  Nombre: {}", baseline.name);
                    for (metric, value) in &baseline.metrics {
                        println!("  {:<12} {:.0}", metric.as_str(), value);
                    }
                }
                Err(e) => eprintln!("Baseline: {e}"),
            }
        }
        crate::baseline::BaselineCommands::Compare { id, path } => {
            let project = PathBuf::from(&path);
            let current = to_metric_map(&workspace_metrics(&project));
            let full_id = match resolve_id(&id) {
                Some(full) => full,
                None => {
                    eprintln!("Baseline no encontrado: {id}");
                    return;
                }
            };
            match store.compare(&full_id, &current) {
                Ok(report) => {
                    println!("{}", report.summary);
                    println!(
                        "  métricas comparadas: {}, sin cambios: {}",
                        report.metrics_compared, report.unchanged
                    );
                    for alert in &report.regressions {
                        println!(
                            "  ⚠ {}: {:.0} → {:.0} ({:+.1}%) [{}]",
                            alert.metric.as_str(),
                            alert.baseline_value,
                            alert.current_value,
                            alert.delta_percent,
                            alert.severity.as_str()
                        );
                    }
                    for alert in &report.improvements {
                        println!(
                            "  ✅ {}: {:.0} → {:.0} ({:+.1}%)",
                            alert.metric.as_str(),
                            alert.baseline_value,
                            alert.current_value,
                            alert.delta_percent
                        );
                    }
                }
                Err(e) => eprintln!("Baseline: {e}"),
            }
        }
    }
}

pub async fn cmd_verify(path: &str, level: Option<u32>) {
    use crate::planner::{VerificationLevel, VerificationScope};
    use crate::verification_engine::{VerificationEngine, VerificationVerdict};

    let root = PathBuf::from(path);
    if !root.exists() {
        eprintln!("Verify: la ruta '{}' no existe", path);
        return;
    }

    let scope = match level {
        Some(number) => match VerificationLevel::from_u32(number) {
            Some(level) => VerificationScope {
                in_scope: vec![level],
                out_of_scope: vec![],
                rationale: format!("--level {number}"),
            },
            None => {
                eprintln!("Verify: nivel inválido '{}' (válidos: 1-8)", number);
                return;
            }
        },
        None => VerificationScope::default(),
    };

    let engine = VerificationEngine::new(&root);
    match engine.verify("cli-verify", None, scope).await {
        Ok(report) => {
            println!("✅ Verificación de {}", root.display());
            for result in &report.results {
                println!(
                    "  {} [{}] {:.0}/100 en {:?}: {}",
                    if result.passed { "PASS" } else { "FAIL" },
                    result.level.name(),
                    result.score,
                    result.duration,
                    result.details.lines().next().unwrap_or("")
                );
            }
            println!(
                "Veredicto: {:?} en {:?}",
                report.overall_verdict, report.total_duration
            );
            if matches!(
                report.overall_verdict,
                VerificationVerdict::Failed | VerificationVerdict::Blocked
            ) {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Verify: {e}");
            std::process::exit(1);
        }
    }
}

/// Extrae rutas de archivo mencionadas en un mensaje de error.
fn files_in_error(text: &str) -> Vec<String> {
    let regex = regex::Regex::new(r#"[\w./\\-]+\.(py|rs|go|js|ts|tsx|jsx|java|c|cc|cpp|h)"#)
        .expect("regex válida");
    let mut files: Vec<String> = regex
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect();
    files.sort();
    files.dedup();
    files.truncate(20);
    files
}

pub async fn cmd_diagnose(input: &str, root: Option<&str>) {
    use crate::diagnostic_engine::{DiagnosticConfig, DiagnosticContext, DiagnosticEngine};

    let workspace = root
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // `input` puede ser el mensaje de error o un archivo que lo contiene.
    let error_text = if Path::new(input).is_file() {
        match fs::read_to_string(input) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("Diagnose: no se pudo leer '{}': {e}", input);
                return;
            }
        }
    } else {
        input.to_string()
    };

    let engine = match DiagnosticEngine::new(workspace.clone(), DiagnosticConfig::default()) {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("Diagnose: {e}");
            return;
        }
    };
    let context = DiagnosticContext {
        related_files: files_in_error(&error_text),
        ..Default::default()
    };

    match engine.diagnose(&error_text, context).await {
        Ok(result) => {
            if result.diagnoses.is_empty() {
                println!("Diagnose: no se reconoció ningún patrón en el error.");
                println!("Sugerencia: pega el mensaje completo o pasa un archivo con el log.");
                return;
            }
            println!(
                "🔍 Diagnose: {} candidato(s) en {} ms",
                result.diagnoses.len(),
                result.execution_time_ms
            );
            for (index, diagnosis) in result.diagnoses.iter().enumerate() {
                println!(
                    "\n{}. [{}] {} (confianza {:.0}%)",
                    index + 1,
                    diagnosis.category.name(),
                    diagnosis.message,
                    diagnosis.confidence * 100.0
                );
                if let Some(location) = &diagnosis.location {
                    println!(
                        "   Ubicación: {}:{}",
                        location.file.display(),
                        location.line
                    );
                }
                if let Some(cause) = &diagnosis.root_cause {
                    println!("   Causa raíz: {cause}");
                }
                if let Some(fix) = &diagnosis.suggested_fix {
                    println!("   Arreglo sugerido: {fix}");
                }
            }
        }
        Err(e) => eprintln!("Diagnose: {e}"),
    }
}

pub async fn cmd_repair(file: &str, error: Option<&str>, max_attempts: u32) {
    // La reparación automática real (aplicar cambios + verificar + rollback)
    // vive en `RepairEngine`, pero necesita un diagnóstico con id estable y un
    // modelo local para proponer el parche. Sin `--error` no hay nada que
    // diagnosticar, así que se dice explícitamente en vez de simular trabajo.
    println!("🔧 Repair: {} (intentos: {})", file, max_attempts);
    if !Path::new(file).exists() {
        eprintln!("Repair: el archivo '{}' no existe", file);
        return;
    }
    match error {
        Some(error) => {
            println!("   Diagnóstico del error:");
            cmd_diagnose(error, None).await;
            println!(
                "\n   Para aplicar un arreglo, usa `lai auto` con la tarea y el error: \
                 el bucle aplica cambios, verifica y revierte si falla."
            );
        }
        None => {
            println!(
                "   Falta --error: sin el mensaje de error no hay diagnóstico que reparar.\n   \
                 Ejemplo: lai repair src/main.rs --error \"E0308 mismatched types\""
            );
        }
    }
}

pub async fn cmd_checkpoint(command: crate::memory_store::CheckpointCommands) {
    use crate::change_engine::{ChangeEngine, ChangeEngineConfig};

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let engine = match ChangeEngine::new(cwd, ChangeEngineConfig::default()) {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("Checkpoint: {e}");
            return;
        }
    };
    let checkpoints = engine.list_checkpoints();
    match command {
        crate::memory_store::CheckpointCommands::List => {
            if checkpoints.is_empty() {
                println!(
                    "No hay checkpoints en esta sesión. Los checkpoints se crean al aplicar \
                     cambios con `lai auto` y viven en el proceso que los creó."
                );
                return;
            }
            println!("{:<10} {:<26} DESCRIPCIÓN", "ID", "FECHA");
            for checkpoint in checkpoints {
                println!(
                    "{:<10} {:<26} {}",
                    &checkpoint.id[..8.min(checkpoint.id.len())],
                    checkpoint.timestamp,
                    checkpoint
                        .metadata
                        .get("description")
                        .cloned()
                        .unwrap_or_default()
                );
            }
        }
        crate::memory_store::CheckpointCommands::Show { id } => {
            match checkpoints.iter().find(|c| c.id == id) {
                Some(checkpoint) => {
                    println!("Checkpoint {}", checkpoint.id);
                    println!(
                        "  Descripción: {}",
                        checkpoint
                            .metadata
                            .get("description")
                            .cloned()
                            .unwrap_or_default()
                    );
                    println!("  Creado:      {}", checkpoint.timestamp);
                    println!("  Archivos:    {}", checkpoint.file_snapshots.len());
                    for file in checkpoint.file_snapshots.keys() {
                        println!("    {}", file.display());
                    }
                }
                None => eprintln!(
                    "Checkpoint no encontrado en esta sesión: {id} (los checkpoints no \
                     persisten entre procesos)"
                ),
            }
        }
        crate::memory_store::CheckpointCommands::Create { description } => {
            match engine.create_checkpoint("cli", &description) {
                Ok(id) => println!("Checkpoint creado: {id} ({description})"),
                Err(e) => eprintln!("Checkpoint: {e}"),
            }
        }
        crate::memory_store::CheckpointCommands::Rollback { id } => match engine.rollback(&id) {
            Ok(()) => println!("Rollback aplicado al checkpoint {id}"),
            Err(e) => eprintln!("Checkpoint: {e}"),
        },
    }
}

pub async fn cmd_memory(command: crate::memory_store::MemoryCommands) {
    use crate::memory_store::{MemoryEntry, MemoryStore, MemoryType};

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let store = match MemoryStore::new(&cwd) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("Memory: {e}");
            return;
        }
    };

    let parse_type =
        |value: Option<&str>| -> Option<MemoryType> { value.and_then(MemoryType::from_string) };

    match command {
        crate::memory_store::MemoryCommands::List { r#type } => {
            match store.list_memories(parse_type(r#type.as_deref()), 20) {
                Ok(entries) if entries.is_empty() => {
                    println!(
                        "No hay memorias guardadas en {}/.lai/memory.db",
                        cwd.display()
                    );
                }
                Ok(entries) => {
                    for entry in entries {
                        println!(
                            "{} [{}] {} ({})",
                            &entry.id[..8.min(entry.id.len())],
                            entry.memory_type.as_str(),
                            entry.content.lines().next().unwrap_or(""),
                            entry.source
                        );
                    }
                }
                Err(e) => eprintln!("Memory: {e}"),
            }
        }
        crate::memory_store::MemoryCommands::Search {
            query,
            r#type,
            limit,
        } => match store.search(&query, parse_type(r#type.as_deref()), limit, false) {
            Ok(results) if results.is_empty() => println!("Sin resultados para '{query}'"),
            Ok(results) => {
                for result in results {
                    println!(
                        "{:.2} [{}] {}",
                        result.score,
                        result.entry.memory_type.as_str(),
                        result.entry.content.lines().next().unwrap_or("")
                    );
                }
            }
            Err(e) => eprintln!("Memory: {e}"),
        },
        crate::memory_store::MemoryCommands::Store {
            content,
            r#type,
            source,
            tags,
        } => {
            let memory_type = match MemoryType::from_string(&r#type) {
                Some(memory_type) => memory_type,
                None => {
                    eprintln!(
                        "Memory: tipo inválido '{}' (semantic, episodic, working, documentary)",
                        r#type
                    );
                    return;
                }
            };
            let tags: Vec<String> = tags
                .map(|tags| {
                    tags.split(',')
                        .map(|tag| tag.trim().to_string())
                        .filter(|tag| !tag.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let entry = MemoryEntry::new(memory_type, content, source, None, None, tags, 1.0, None);
            match store.store(entry) {
                Ok(id) => println!("Memoria guardada: {id}"),
                Err(e) => eprintln!("Memory: {e}"),
            }
        }
    }
}

pub async fn cmd_plan(command: crate::planner::PlanCommands) {
    println!("📋 Plan command: {:?}", command);
    println!(
        "   El planificador requiere un modelo local para generar el DAG y SPEC.md.\n   \
         Usa `lai auto` con la tarea: genera el plan, aplica cambios y verifica."
    );
}

pub async fn cmd_metrics() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    println!("📈 Métricas del workspace: {}", cwd.display());
    let metrics = workspace_metrics(&cwd);
    for key in ["files", "symbols", "lines"] {
        if let Some(value) = metrics.get(key) {
            println!("  {:<10} {:.0}", key, value);
        }
    }

    // La telemetría en proceso solo tiene datos si algo la inicializó; se
    // muestra tal cual en vez de inventar cifras.
    let telemetry = crate::telemetry::Telemetry::instance().lock().unwrap();
    match telemetry.as_ref() {
        Some(telemetry)
            if telemetry.metrics().requests > 0 || telemetry.metrics().tool_calls > 0 =>
        {
            let metrics = telemetry.metrics();
            println!("  Sesión:");
            println!("    peticiones   {}", metrics.requests);
            println!("    tool calls   {}", metrics.tool_calls);
            println!("    llm calls    {}", metrics.llm_calls);
            println!("    errores      {}", metrics.errors);
            println!("    uptime       {:?}", metrics.uptime);
        }
        _ => println!(
            "  Telemetría: sin actividad en este proceso (los contadores son por sesión; \
             `lai auto` y `lai chat` los alimentan)."
        ),
    }
}

pub async fn cmd_bench(command: crate::benchmark::BenchCommands) {
    use crate::benchmark::framework::{BenchmarkRunner, OutputFormat};
    use crate::benchmark::standard_benchmarks;
    use std::sync::Arc;

    match command {
        crate::benchmark::BenchCommands::All {
            output,
            format,
            iterations,
        } => {
            println!("🏃 Running all standard benchmarks...");
            println!(
                "⚠ Los adaptadores reales no están registrados: estos resultados provienen de \
                 MockBenchmarkAdapter y NO miden nada del sistema (Regla 6)."
            );

            let benchmarks = standard_benchmarks::all_benchmarks();
            let mut all_results = Vec::new();

            for mut config in benchmarks {
                if let Some(iters) = iterations {
                    config.iterations = iters;
                }
                if let Some(ref fmt) = format {
                    config.output_format = match fmt.as_str() {
                        "json" => OutputFormat::Json,
                        "csv" => OutputFormat::Csv,
                        "markdown" => OutputFormat::Markdown,
                        "html" => OutputFormat::Html,
                        "prometheus" => OutputFormat::Prometheus,
                        _ => OutputFormat::Json,
                    };
                }
                if let Some(ref out) = output {
                    config.output_path = Some(out.into());
                }

                // Use a mock adapter for now - real adapters would be registered
                let adapter = Arc::new(MockBenchmarkAdapter::new(config.capability_id.clone()));
                let runner = BenchmarkRunner::new(config, adapter);

                match runner.run().await {
                    Ok(result) => {
                        println!(
                            "⚠ MOCK {} - Success: {} - Details: {}",
                            result.benchmark_name, result.success, result.success_details
                        );
                        all_results.push(result);
                    }
                    Err(e) => {
                        eprintln!("❌ Benchmark failed: {}", e);
                    }
                }
            }

            println!("\n📊 All benchmarks completed. {} run.", all_results.len());
        }
        crate::benchmark::BenchCommands::Run {
            capability,
            output,
            format,
            iterations,
        } => {
            println!("🏃 Running benchmark for capability: {}", capability);
            println!("⚠ Adaptador mock: el resultado NO es una medición real (Regla 6).");

            let benchmarks = standard_benchmarks::all_benchmarks();
            if let Some(config) = benchmarks
                .into_iter()
                .find(|b| b.capability_id == capability)
            {
                let mut config = config;
                if let Some(iters) = iterations {
                    config.iterations = iters;
                }
                if let Some(ref fmt) = format {
                    config.output_format = match fmt.as_str() {
                        "json" => OutputFormat::Json,
                        "csv" => OutputFormat::Csv,
                        "markdown" => OutputFormat::Markdown,
                        "html" => OutputFormat::Html,
                        "prometheus" => OutputFormat::Prometheus,
                        _ => OutputFormat::Json,
                    };
                }
                if let Some(ref out) = output {
                    config.output_path = Some(out.into());
                }

                let adapter = Arc::new(MockBenchmarkAdapter::new(config.capability_id.clone()));
                let runner = BenchmarkRunner::new(config, adapter);

                match runner.run().await {
                    Ok(result) => {
                        println!(
                            "⚠ MOCK {} - Success: {} - Details: {}",
                            result.benchmark_name, result.success, result.success_details
                        );
                    }
                    Err(e) => {
                        eprintln!("❌ Benchmark failed: {}", e);
                    }
                }
            } else {
                eprintln!(
                    "❌ Unknown capability: {}. Use 'lai bench list' to see available.",
                    capability
                );
            }
        }
        crate::benchmark::BenchCommands::List => {
            println!("📋 Available benchmarks:");
            for b in standard_benchmarks::all_benchmarks() {
                println!("  - {} ({})", b.capability_id, b.name);
                println!("    {}", b.description);
            }
        }
        crate::benchmark::BenchCommands::Compare {
            capability,
            baseline,
        } => {
            println!("📊 Comparing {} against baseline: {}", capability, baseline);
            println!("   (Baseline comparison functionality to be implemented)");
        }
    }
}

// Mock adapter for benchmark testing - replace with real adapters when available
use crate::benchmark::contracts::*;
use async_trait::async_trait;
use std::time::SystemTime;

struct MockBenchmarkAdapter {
    capability_id: String,
}

impl MockBenchmarkAdapter {
    fn new(capability_id: String) -> Self {
        Self { capability_id }
    }
}

#[async_trait]
impl CapabilityAdapter for MockBenchmarkAdapter {
    fn capability_id(&self) -> String {
        self.capability_id.clone()
    }
    fn capability_name(&self) -> String {
        "Mock Adapter".to_string()
    }
    fn adapter_version(&self) -> String {
        "1.0.0".to_string()
    }
    fn tool_version(&self) -> String {
        "1.0.0".to_string()
    }
    fn definition_hash(&self) -> String {
        "mock_hash".to_string()
    }

    async fn health_check(&self) -> HealthCheckResult {
        HealthCheckResult {
            healthy: true,
            tool_version: "1.0.0".to_string(),
            definition_hash: "mock_hash".to_string(),
            checks: vec![],
            timestamp: SystemTime::now(),
        }
    }

    async fn preflight(
        &self,
        _request: &ExecutionRequest,
    ) -> Result<PreflightResult, ContractError> {
        Ok(PreflightResult {
            allowed: true,
            reason: None,
            warnings: vec![],
            estimated_resources: ResourceEstimate {
                estimated_cpu_seconds: 1.0,
                estimated_memory_mb: 100,
                estimated_network_mb: 0,
                estimated_tokens: 100,
            },
            required_permissions: vec![],
        })
    }

    async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult, ContractError> {
        // Simulate work
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let execution_id = request.execution_id.clone();
        let capability_id = request.capability_id.clone();

        Ok(ExecutionResult {
            execution_id: execution_id.clone(),
            capability_id: capability_id.clone(),
            status: ResultStatus::Passed,
            output: serde_json::json!({}),
            evidence: vec![],
            metrics: ExecutionMetrics::new(
                execution_id.clone(),
                capability_id.clone(),
                "1.0.0".to_string(),
                "mock_hash".to_string(),
            ),
            security_audit: SecurityAudit {
                execution_id,
                integrity_verified: true,
                definition_hash_match: true,
                executable_hash_match: true,
                permissions_checked: true,
                sandbox_active: true,
                network_policy_enforced: true,
                arguments_validated: true,
                result_validated: true,
                audit_entries: vec![],
            },
            started_at: SystemTime::now(),
            completed_at: SystemTime::now(),
        })
    }

    async fn validate(&self, _result: &ExecutionResult) -> ValidationResult {
        ValidationResult {
            valid: true,
            score: 1.0,
            issues: vec![],
            recommendations: vec![],
        }
    }

    async fn get_metrics(&self, _execution_id: &str) -> Option<ExecutionMetrics> {
        None
    }

    fn supported_features(&self) -> AdapterFeatures {
        AdapterFeatures::default()
    }

    fn resource_requirements(&self) -> ResourceRequirements {
        ResourceRequirements {
            min_cpu_cores: 1.0,
            min_memory_mb: 100,
            min_disk_mb: 10,
            requires_gpu: false,
            requires_network: false,
            requires_sandbox: true,
        }
    }
}

#[cfg(test)]
mod normalization_tests {
    use super::*;

    #[test]
    fn unescape_converts_literal_newlines() {
        let input = "fn main() {\\n    println!(\"hi\");\\n}\\n";
        let out = unescape_literal_escapes(input);
        assert!(out.contains('\n'));
        assert!(!out.contains("\\n"));
        assert_eq!(out.lines().count(), 3);
    }

    #[test]
    fn unescape_leaves_real_newlines_untouched() {
        let input = "fn main() {\n    let s = \"a\\\\nb\";\n}\n";
        assert_eq!(unescape_literal_escapes(input), input);
    }

    #[test]
    fn normalize_write_file_fixes_double_escaped_content() {
        let mut args = HashMap::new();
        args.insert("path".to_string(), Value::String("src/lib.rs".into()));
        args.insert(
            "content".to_string(),
            Value::String("pub fn f() -> i32 {\\n    1\\n}\\n".into()),
        );
        let out = normalize_tool_arguments("write_file", args);
        let content = out.get("content").unwrap().as_str().unwrap();
        assert!(content.contains('\n'));
    }

    #[test]
    fn normalize_write_file_preserves_allow_shrink() {
        let mut args = HashMap::new();
        args.insert("path".to_string(), Value::String("src/lib.rs".into()));
        args.insert("content".to_string(), Value::String("short\n".into()));
        args.insert("allow_shrink".to_string(), Value::Bool(true));

        let out = normalize_tool_arguments("write_file", args);

        assert_eq!(out.get("allow_shrink"), Some(&Value::Bool(true)));
        assert_eq!(out.get("content").unwrap(), "short\n");
    }

    #[test]
    fn normalize_run_command_preserves_timeout() {
        let mut args = HashMap::new();
        args.insert("command".to_string(), Value::String("cargo test".into()));
        args.insert("timeout".to_string(), Value::from(42));

        let out = normalize_tool_arguments("run_command", args);

        assert_eq!(out.get("timeout"), Some(&Value::from(42)));
        assert_eq!(out.get("command").unwrap(), "cargo test");
    }

    #[test]
    fn normalize_run_python_preserves_timeout() {
        let mut args = HashMap::new();
        args.insert("code".to_string(), Value::String("print(1)".into()));
        args.insert("timeout".to_string(), Value::from(7));

        let out = normalize_tool_arguments("run_python", args);

        assert_eq!(out.get("timeout"), Some(&Value::from(7)));
        assert_eq!(out.get("code").unwrap(), "print(1)");
    }

    #[test]
    fn native_tool_calls_are_parsed_with_ids() {
        let calls = Some(vec![crate::llm::ToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: crate::llm::ToolFunction {
                name: "write_file".to_string(),
                arguments: "{\"path\":\"a.rs\",\"content\":\"x\"}".to_string(),
            },
        }]);
        let parsed = parse_native_tool_calls(&calls);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["tool"], "write_file");
        assert_eq!(parsed[0]["id"], "call_1");
        assert_eq!(parsed[0]["arguments"]["path"], "a.rs");
    }

    #[test]
    fn simulated_call_with_raw_newlines_is_repaired() {
        // Caso real de qwen2.5-coder:3b: el `content` lleva saltos de linea
        // literales, asi que el JSON es invalido para serde_json.
        let response = "```json\n{\n  \"tool\": \"write_file\",\n  \"arguments\": {\n    \"path\": \"src/lib.rs\",\n    \"content\": \"pub fn f() -> i32 {\n    1\n}\"\n  }\n}\n```";
        let calls = parse_simulated_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["tool"], "write_file");
        let content = calls[0]["arguments"]["content"].as_str().unwrap();
        assert!(content.contains("pub fn f()"));
        assert!(content.contains('\n'));
    }

    #[test]
    fn simulated_call_accepts_name_alias() {
        let response = "```json\n{\"name\": \"write_file\", \"arguments\": {\"path\": \"a.rs\", \"content\": \"x\"}}\n```";
        let calls = parse_simulated_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["tool"], "write_file");
        assert_eq!(calls[0]["arguments"]["path"], "a.rs");
    }

    #[test]
    fn simulated_call_accepts_string_arguments() {
        let response = "```json\n{\"tool\": \"read_file\", \"arguments\": \"{\\\"path\\\": \\\"a.rs\\\"}\"}\n```";
        let calls = parse_simulated_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["arguments"]["path"], "a.rs");
    }

    #[test]
    fn simulated_call_accepts_uppercase_fence() {
        let response = "```JSON\n{\"tool\": \"git_status\", \"arguments\": {}}\n```";
        let calls = parse_simulated_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["tool"], "git_status");
    }

    #[test]
    fn simulated_call_ignores_rust_code_blocks() {
        let response = "```rust\nfn main() { println!(\"hi\"); }\n```";
        assert!(parse_simulated_tool_calls(response).is_empty());
    }

    #[test]
    fn simulated_call_requires_arguments() {
        let response = "```json\n{\"tool\": \"write_file\"}\n```";
        assert!(parse_simulated_tool_calls(response).is_empty());
    }

    #[test]
    fn repair_keeps_escaped_quotes_intact() {
        let raw = "{\"content\": \"let s = \\\"a\\\";\nlet t = 1;\"}";
        let repaired = repair_json_control_chars(raw);
        let val: Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(val["content"], "let s = \"a\";\nlet t = 1;");
    }

    #[test]
    fn xml_tool_call_is_parsed() {
        let response = "<tool_call><function=read_file><parameter=path>src/lib.rs</parameter></function></tool_call>";
        let calls = parse_xml_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["tool"], "read_file");
        assert_eq!(calls[0]["arguments"]["path"], "src/lib.rs");
    }

    #[test]
    fn xml_tool_call_parses_multiple_parameters() {
        let response = "<tool_call><function=write_file><parameter=path>a.txt</parameter><parameter=content>hi &amp; bye</parameter></function></tool_call>";
        let calls = parse_xml_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["arguments"]["content"], "hi & bye");
    }

    #[test]
    fn xml_tool_call_without_wrapper_is_parsed() {
        let response = "<function=run_command><parameter=command>ls -la</parameter></function>";
        let calls = parse_xml_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["tool"], "run_command");
        assert_eq!(calls[0]["arguments"]["command"], "ls -la");
    }

    #[test]
    fn xml_tool_call_ignores_plain_text() {
        assert!(parse_xml_tool_calls("no hay llamadas aquí").is_empty());
    }
}

#[cfg(test)]
mod cli_helper_tests {
    use super::*;

    #[test]
    fn files_in_error_extracts_paths_with_line_numbers() {
        let error = "Traceback (most recent call last):\n  File \"django/db/models/base.py\", line 42\n  File \"./src/lib.rs\", line 7";
        let files = files_in_error(error);
        assert!(files.contains(&"django/db/models/base.py".to_string()));
        assert!(files.contains(&"./src/lib.rs".to_string()));
    }

    #[test]
    fn files_in_error_ignores_plain_words() {
        assert!(files_in_error("NameError: name 'foo' is not defined").is_empty());
    }

    #[test]
    fn auto_transcript_records_task_calls_and_results() {
        let dir = tempfile::tempdir().unwrap();
        let messages = vec![
            Message {
                role: "system".into(),
                content: "reglas internas".into(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            Message {
                role: "user".into(),
                content: "arregla el bug".into(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            Message {
                role: "assistant".into(),
                content: String::new(),
                tool_calls: Some(vec![crate::llm::ToolCall {
                    id: "c1".into(),
                    call_type: "function".into(),
                    function: crate::llm::ToolFunction {
                        name: "read_file".into(),
                        arguments: "{\"path\":\"a.py\"}".into(),
                    },
                }]),
                tool_call_id: None,
                name: None,
            },
            Message {
                role: "tool".into(),
                content: "contenido".into(),
                tool_calls: None,
                tool_call_id: Some("c1".into()),
                name: Some("read_file".into()),
            },
        ];

        let path = write_auto_transcript(dir.path(), "arregla el bug", "m", &messages).unwrap();
        let text = fs::read_to_string(path).unwrap();

        assert!(text.contains("arregla el bug"));
        assert!(text.contains("`read_file`"));
        assert!(text.contains("contenido"));
        assert!(!text.contains("reglas internas"), "el system no se duplica");
    }

    #[test]
    fn workspace_metrics_counts_python_and_rust() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.py"), "def uno():\n    pass\n").unwrap();
        fs::write(dir.path().join("b.rs"), "pub fn dos() {}\n").unwrap();
        fs::write(dir.path().join("ignored.txt"), "x\n").unwrap();

        let metrics = workspace_metrics(dir.path());
        assert_eq!(metrics.get("files"), Some(&2.0));
        assert_eq!(metrics.get("symbols"), Some(&2.0));
        assert_eq!(metrics.get("lines"), Some(&3.0));
    }
}

#[cfg(test)]
mod truncation_tests {
    use super::*;

    #[test]
    fn short_output_is_untouched() {
        let out = "hello world";
        assert_eq!(truncate_tool_output("read_file", out), out);
    }

    #[test]
    fn long_output_keeps_head_and_tail() {
        let out: String = "a".repeat(20_000);
        let truncated = truncate_tool_output("read_file", &out);
        assert!(truncated.chars().count() < 20_000);
        assert!(truncated.contains("characters omitted from read_file output"));
        assert!(truncated.starts_with("aaaa"));
        assert!(truncated.ends_with("aaaa"));
    }
}
