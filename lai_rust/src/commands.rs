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
) {
    println!("🤖 Auto mode: {}", task);
    println!("   Max iterations: {}", max_iterations);
    if let Some(out) = output {
        println!("   Output dir: {}", out);
    }
    println!("   Model: {}", model);
    println!("   Base URL: {}", base_url);

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
    let tool_schemas = tools.get_tools_schema();
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

    let system_prompt = format!(
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

IMPORTANT: Only output the JSON code block when you want to call a tool. Otherwise, respond normally with text.

TOOL USAGE GUIDE:
- "write_file": CREATE a new file. Args: {{"path": "file.py", "content": "code here"}}
- "read_file": READ a file or a 1-based line range. Args: {{"path": "file.py"}} or {{"path": "file.py", "start_line": 120, "end_line": 220}}. Use a range for large files; the default is capped at 400 lines.
- "run_python": RUN PYTHON CODE (not commands). Args: {{"code": "print(2+2)"}} - ONLY valid Python code!
- "run_command": RUN SHELL COMMANDS. Args: {{"command": "ls -la"}} - Use for shell commands!
- "list_dir": LIST directory. Args: {{"path": "."}}
- "grep": SEARCH files. Args: {{"pattern": "text", "path": "."}}; optional "regex": true, "case_insensitive": true, "glob": "*.rs". Prefer grep to locate symbols before reading whole files.
- "find_files": FIND files by glob. Args: {{"pattern": "src/**/*.rs", "path": "."}}

RULES:
- To CREATE a file: MUST use "write_file" with "path" and "content"
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
    );

    let llm =
        LlmClient::new(base_url.to_string(), model.to_string()).with_allow_remote(allow_remote);

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

    for iteration in 1..=max_iterations {
        println!("\n--- Iteration {} of {} ---", iteration, max_iterations);

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
            Ok(msg) => {
                let content = strip_think_tags(&msg.content);
                if !content.is_empty() {
                    println!("{}", content);
                }
                messages.push(msg.clone());

                // Native tool calls take precedence; fall back to JSON code blocks.
                let mut tool_calls = parse_native_tool_calls(&msg.tool_calls);
                if tool_calls.is_empty() {
                    tool_calls = parse_simulated_tool_calls(&msg.content);
                }

                if tool_calls.is_empty() {
                    if nudges < MAX_NUDGES && iteration < max_iterations {
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

                    let tool_result = if let Some(func) = tools.tools.get(name_str) {
                        match func(arguments).await {
                            Ok(r) => {
                                println!("  ✓ Done");
                                r
                            }
                            Err(e) => {
                                println!("  ✗ Error: {}", e);
                                format!("Error executing {}: {}", name_str, e)
                            }
                        }
                    } else {
                        println!("  ✗ Unknown tool: {}", name_str);
                        format!("Unknown tool: {}", name_str)
                    };

                    // Bound the tool output: reading a large file can otherwise
                    // blow the model's context window and evict the task itself.
                    let tool_result = truncate_tool_output(name_str, &tool_result);

                    messages.push(crate::llm::Message {
                        role: "tool".into(),
                        content: tool_result,
                        tool_call_id: call_id,
                        name: Some(name_str.to_string()),
                        tool_calls: None,
                    });
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }
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

fn normalize_tool_arguments(tool: &str, args: HashMap<String, Value>) -> HashMap<String, Value> {
    match tool {
        // Normalise write_file: keep path/content as strings and repair payloads
        // where the model double-escaped newlines (whole file on one line).
        "write_file" => {
            let mut normalized = HashMap::new();
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                normalized.insert("path".to_string(), Value::String(path.to_string()));
            }
            if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
                normalized.insert(
                    "content".to_string(),
                    Value::String(unescape_literal_escapes(content)),
                );
            }
            normalized
        }
        "run_python" => {
            // Model often passes "command" or "path" instead of "code"
            if let Some(code) = args.get("code").cloned() {
                // Check if it's a shell command (starts with python, ls, cat, etc.)
                if let Some(code_str) = code.as_str() {
                    let trimmed = code_str.trim();
                    if trimmed.starts_with("python ")
                        || trimmed.starts_with("ls ")
                        || trimmed.starts_with("cat ")
                        || trimmed.starts_with("echo ")
                        || trimmed.starts_with("cd ")
                        || trimmed.starts_with("mkdir ")
                        || trimmed.starts_with("rm ")
                        || trimmed.starts_with("cp ")
                        || trimmed.starts_with("mv ")
                    {
                        // This is a shell command, not Python code!
                        // Return error by putting it in a special key that will be caught
                        return hashmap!["_error".to_string() => Value::String(format!(
                            "ERROR: '{}' is a shell command, not Python code. Use 'run_command' tool instead with 'command' argument.",
                            trimmed
                        ))];
                    }
                }
                return hashmap!["code".to_string() => code];
            }
            if let Some(cmd) = args.get("command").cloned() {
                // Check if it's a shell command
                if let Some(cmd_str) = cmd.as_str() {
                    let trimmed = cmd_str.trim();
                    if trimmed.starts_with("python ")
                        || trimmed.starts_with("ls ")
                        || trimmed.starts_with("cat ")
                        || trimmed.starts_with("echo ")
                        || trimmed.starts_with("cd ")
                        || trimmed.starts_with("mkdir ")
                        || trimmed.starts_with("rm ")
                        || trimmed.starts_with("cp ")
                        || trimmed.starts_with("mv ")
                    {
                        return hashmap!["_error".to_string() => Value::String(format!(
                            "ERROR: '{}' is a shell command, not Python code. Use 'run_command' tool instead with 'command' argument.",
                            trimmed
                        ))];
                    }
                }
                // Assume it's Python code if not detected as shell command
                return hashmap!["code".to_string() => cmd];
            }
            // If it has path but no code, create a simple write
            if let Some(path) = args.get("path").cloned() {
                return hashmap!["code".to_string() => format!("print('Writing to {}')", path)];
            }
            // Default fallback
            hashmap!["code".to_string() => Value::String("print('Hello')".to_string())]
        }
        "run_command" => {
            let mut normalized = HashMap::new();
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                normalized.insert("command".to_string(), Value::String(cmd.to_string()));
            }
            if let Some(cwd) = args.get("cwd").and_then(|v| v.as_str()) {
                normalized.insert("cwd".to_string(), Value::String(cwd.to_string()));
            }
            normalized
        }
        _ => args,
    }
}

pub async fn cmd_repomap(path: Option<&str>, file: Option<&str>) {
    println!("📦 RepoMap");
    if let Some(p) = path {
        println!("   Path: {}", p);
    }
    if let Some(f) = file {
        println!("   File: {}", f);
    }
    println!("   (RepoMap functionality to be implemented)");
}

pub async fn cmd_baseline(command: crate::baseline::BaselineCommands) {
    println!("📊 Baseline command: {:?}", command);
    println!("   (Baseline functionality to be implemented)");
}

pub async fn cmd_verify(path: &str, level: Option<u32>) {
    println!("✅ Verify: {} (level: {:?})", path, level);
    println!("   (Verification functionality to be implemented)");
}

pub async fn cmd_diagnose(input: &str, root: Option<&str>) {
    println!("🔍 Diagnose: {} (root: {:?})", input, root);
    println!("   (Diagnosis functionality to be implemented)");
}

pub async fn cmd_repair(file: &str, error: Option<&str>, max_attempts: u32) {
    println!("🔧 Repair: {} (attempts: {})", file, max_attempts);
    if let Some(e) = error {
        println!("   Error context: {}", e);
    }
    println!("   (Repair functionality to be implemented)");
}

pub async fn cmd_checkpoint(command: crate::memory_store::CheckpointCommands) {
    println!("📌 Checkpoint command: {:?}", command);
    println!("   (Checkpoint functionality to be implemented)");
}

pub async fn cmd_memory(command: crate::memory_store::MemoryCommands) {
    println!("🧠 Memory command: {:?}", command);
    println!("   (Memory functionality to be implemented)");
}

pub async fn cmd_plan(command: crate::planner::PlanCommands) {
    println!("📋 Plan command: {:?}", command);
    println!("   (Planner functionality to be implemented)");
}

pub async fn cmd_metrics() {
    println!("📈 Metrics");
    println!("   (Telemetry/metrics functionality to be implemented)");
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
                            "✅ {} - Success: {} - Details: {}",
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
                            "✅ {} - Success: {} - Details: {}",
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
