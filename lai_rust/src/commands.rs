//! commands.rs - CLI command implementations with quality cycles

use crate::benchmark::framework::{BenchmarkConfig, BenchmarkRunner, OutputFormat};
use crate::benchmark::standard_benchmarks;
use crate::context::{ContextAnalyzer, format_context_for_llm};
use crate::llm::{LlmClient, Message, ToolCall};
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
    parts.push(format!("=== PROJECT CONTEXT: {} ===", cwd.file_name().unwrap_or_default().to_string_lossy()));
    parts.push(format!("Path: {}", cwd.display()));
    parts.push(String::new());

    // Read README files
    let readme_files = ["README.md", "README.txt", "README.rst", "readme.md", "readme.txt"];
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
        "pyproject.toml", "setup.py", "setup.cfg", "requirements.txt",
        "Cargo.toml", "go.mod", "package.json", "pom.xml",
        "Dockerfile", "docker-compose.yml", "docker-compose.yaml",
        "Makefile", "makefile",
        ".gitignore", ".env.example",
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
            if name.starts_with('.') { continue; }
            let full = entry.path();
            if full.is_dir() {
                // Count Python/Rust files in directory (non-recursive to avoid build artifacts)
                let mut count = 0;
                if let Ok(dir_entries) = fs::read_dir(&full) {
                    for e in dir_entries.filter_map(|e| e.ok()) {
                        if e.path().is_file() {
                            if let Some(ext) = e.path().extension().and_then(|s| s.to_str()) {
                                if ext == "py" || ext == "rs" { count += 1; }
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

pub async fn cmd_chat(prompt: &[String], model: &str, base_url: &str, _allow_remote: bool) {
    let llm = LlmClient::new(base_url.to_string(), model.to_string());
    let prompt_str = prompt.join(" ");
    
    // Load project context
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let analyzer = ContextAnalyzer::new(cwd.clone());
    let project_context = analyzer.analyze().map(|ctx| format_context_for_llm(&ctx)).unwrap_or_else(|_| analyze_project_context(&cwd));
    
    // Get tool schemas
    let tools = ToolRegistry::new();
    let tool_schemas: Vec<serde_json::Value> = tools.get_tools_schema().into_iter().map(|s| {
        serde_json::json!({
            "type": "function",
            "function": serde_json::to_value(s).unwrap()
        })
    }).collect();
    
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
            
            let result = llm.chat_with_tools(
                messages.clone(),
                Some(tool_schemas.clone()),
                Some("auto".to_string()),
            ).await;
            
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
    println!("\n📁 Project context loaded ({} chars)", project_context.len());
    
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
            println!("{}", &project_context[..std::cmp::min(3000, project_context.len())]);
            if project_context.len() > 3000 {
                println!("... (truncated, total {} chars)", project_context.len());
            }
            continue;
        }
        if input == "analyze" || input == "analizar" || input == "analizar proyecto" || input == "analyze project" {
            println!("🔍 Re-analyzing project...");
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let analyzer = ContextAnalyzer::new(cwd);
            let new_context = analyzer.analyze().map(|ctx| format_context_for_llm(&ctx)).unwrap_or_else(|_| analyze_project_context(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))));
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
            
            let result = llm.chat_with_tools(
                messages.clone(),
                Some(tool_schemas.clone()),
                Some("auto".to_string()),
            ).await;
            
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
    _allow_remote: bool,
) {
    let client = LlmClient::new(base_url.to_string(), model.to_string());
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
                    format!("class Test:\n{}", new_method.lines().map(|l| format!("    {}", l)).collect::<Vec<_>>().join("\n"))
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
            let indented_method: Vec<String> = new_method.lines()
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
                let args_map: HashMap<String, Value> = serde_json::from_value(args).unwrap_or_default();
                match func(args_map).await {
                    Ok(result) => {
                        println!("{}", result);
                    }
                    Err(e) => {
                        eprintln!("Error writing file: {}", e);
                        return;
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
                let args_map: HashMap<String, Value> = serde_json::from_value(args).unwrap_or_default();
                match func(args_map).await {
                    Ok(result) => {
                        println!("{}", result);
                    }
                    Err(e) => {
                        eprintln!("Error writing file: {}", e);
                        return;
                    }
                }
            } else {
                eprintln!("write_file tool not available");
            }
        }
    } else {
        eprintln!("Failed to generate valid code after {} iterations", max_iterations);
    }
}

fn compute_unified_diff(old: &str, new: &str, filename: &str) -> String {
    use std::process::Command;
    use std::io::Write;
    
    // Write temp files
    let old_file = format!("/tmp/{}.old", filename.replace('/', "_"));
    let new_file = format!("/tmp/{}.new", filename.replace('/', "_"));
    
    if let (Ok(_), Ok(_)) = (fs::write(&old_file, old), fs::write(&new_file, new)) {
        let output = Command::new("diff")
            .args(["-u", &old_file, &new_file])
            .output();
        
        let _ = fs::remove_file(&old_file);
        let _ = fs::remove_file(&new_file);
        
        if let Ok(out) = output {
            let diff = String::from_utf8_lossy(&out.stdout).to_string();
            // Fix the header lines to use proper paths
            let mut lines: Vec<String> = diff.lines().map(|s| s.to_string()).collect();
            if lines.len() >= 2 {
                let old_header = format!("--- a/{}", filename);
                let new_header = format!("+++ b/{}", filename);
                lines[0] = old_header;
                lines[1] = new_header;
            }
            return lines.join("\n");
        }
    }
    String::new()
}

pub async fn cmd_status() {
    println!("lai - Local AI Code Assistant");
    println!("Services: checking...");
    let services = Services::new();
    let client = reqwest::Client::new();
    crate::services::check_all_services_async(&services, &client).await;
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

pub async fn cmd_auto(task: &str, max_iterations: u32, output: Option<&str>, model: &str, base_url: &str, _allow_remote: bool) {
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
    let project_context = analyzer.analyze().map(|ctx| format_context_for_llm(&ctx)).unwrap_or_else(|_| analyze_project_context(&cwd));
    
    let llm = LlmClient::new(base_url.to_string(), model.to_string());
    let tools = ToolRegistry::new();
    
    // Build tool schema descriptions for the system prompt
    let tool_schemas = tools.get_tools_schema();
    let tools_description = build_tools_description(&tool_schemas);
    
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
- "read_file": READ a file. Args: {{"path": "file.py"}}
- "run_python": RUN PYTHON CODE (not commands). Args: {{"code": "print(2+2)"}} - ONLY valid Python code!
- "run_command": RUN SHELL COMMANDS. Args: {{"command": "ls -la"}} - Use for shell commands!
- "list_dir": LIST directory. Args: {{"path": "."}}
- "grep": SEARCH text. Args: {{"pattern": "text", "path": "."}}

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
    
    let llm = LlmClient::new(base_url.to_string(), model.to_string());
    let tools = ToolRegistry::new();
    
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
    
    for iteration in 1..=max_iterations {
        println!("\n--- Iteration {} of {} ---", iteration, max_iterations);
        
        // Use regular chat (no native tool calling)
        let result = llm.chat_with_tools(
            messages.clone(),
            None,  // No native tool schemas
            None,  // No tool_choice
        ).await;
        
        match result {
            Ok(msg) => {
                let content = strip_think_tags(&msg.content);
                if !content.is_empty() {
                    println!("{}", content);
                }
                messages.push(msg.clone());
                
                // Parse simulated tool calls from JSON code blocks
                let tool_calls = parse_simulated_tool_calls(&msg.content);
                
                if tool_calls.is_empty() {
                    println!("✅ Task completed (no tool calls requested)");
                    break;
                }
                
                println!("🔧 Executing {} tool call(s)...", tool_calls.len());
                
                for call in tool_calls {
                    let name = &call["tool"];
                    let name_str = name.as_str().unwrap_or("");
                    let mut arguments = call["arguments"].as_object()
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
                            tool_call_id: None,
                            name: Some(name_str.to_string()),
                            tool_calls: None,
                        });
                        continue;
                    }
                    
                    println!("  → {} ({})", name_str, serde_json::to_string(&arguments).unwrap_or_default());
                    
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
                    
                    messages.push(crate::llm::Message {
                        role: "tool".into(),
                        content: tool_result,
                        tool_call_id: None,
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

fn parse_simulated_tool_calls(response: &str) -> Vec<Value> {
    let mut calls = Vec::new();
    let mut search_start = 0;
    
    while let Some(start) = response[search_start..].find("```json") {
        let abs_start = search_start + start + 7; // skip ```json
        if let Some(end) = response[abs_start..].find("```") {
            let json_str = &response[abs_start..abs_start + end].trim();
            if let Ok(val) = serde_json::from_str::<Value>(json_str) {
                // Validate: must have "tool" (string) and "arguments" (object)
                if let (Some(tool), Some(args)) = (val.get("tool"), val.get("arguments")) {
                    if tool.is_string() && args.is_object() {
                        calls.push(val);
                    }
                }
            }
            search_start = abs_start + end + 3;
        } else {
            break;
        }
    }
    
    calls
}

fn normalize_tool_arguments(tool: &str, args: HashMap<String, Value>) -> HashMap<String, Value> {
    match tool {
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
                        || trimmed.starts_with("mv ") {
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
                        || trimmed.starts_with("mv ") {
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
        "write_file" => {
            // Ensure path and content are strings
            let mut normalized = HashMap::new();
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                normalized.insert("path".to_string(), Value::String(path.to_string()));
            }
            if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
                normalized.insert("content".to_string(), Value::String(content.to_string()));
            }
            normalized
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
    use crate::benchmark::framework::{BenchmarkConfig, BenchmarkRunner, OutputFormat};
    use crate::benchmark::standard_benchmarks;
    use std::sync::Arc;

    match command {
        crate::benchmark::BenchCommands::All { output, format, iterations } => {
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
                        println!("✅ {} - Success: {} - Details: {}", 
                            result.benchmark_name, result.success, result.success_details);
                        all_results.push(result);
                    }
                    Err(e) => {
                        eprintln!("❌ Benchmark failed: {}", e);
                    }
                }
            }
            
            println!("\n📊 All benchmarks completed. {} run.", all_results.len());
        }
        crate::benchmark::BenchCommands::Run { capability, output, format, iterations } => {
            println!("🏃 Running benchmark for capability: {}", capability);
            
            let benchmarks = standard_benchmarks::all_benchmarks();
            if let Some(config) = benchmarks.into_iter().find(|b| b.capability_id == capability) {
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
                        println!("✅ {} - Success: {} - Details: {}", 
                            result.benchmark_name, result.success, result.success_details);
                    }
                    Err(e) => {
                        eprintln!("❌ Benchmark failed: {}", e);
                    }
                }
            } else {
                eprintln!("❌ Unknown capability: {}. Use 'lai bench list' to see available.", capability);
            }
        }
        crate::benchmark::BenchCommands::List => {
            println!("📋 Available benchmarks:");
            for b in standard_benchmarks::all_benchmarks() {
                println!("  - {} ({})", b.capability_id, b.name);
                println!("    {}", b.description);
            }
        }
        crate::benchmark::BenchCommands::Compare { capability, baseline } => {
            println!("📊 Comparing {} against baseline: {}", capability, baseline);
            println!("   (Baseline comparison functionality to be implemented)");
        }
    }
}

// Mock adapter for benchmark testing - replace with real adapters when available
use async_trait::async_trait;
use crate::benchmark::contracts::*;
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
    fn capability_id(&self) -> String { self.capability_id.clone() }
    fn capability_name(&self) -> String { "Mock Adapter".to_string() }
    fn adapter_version(&self) -> String { "1.0.0".to_string() }
    fn tool_version(&self) -> String { "1.0.0".to_string() }
    fn definition_hash(&self) -> String { "mock_hash".to_string() }
    
    async fn health_check(&self) -> HealthCheckResult {
        HealthCheckResult {
            healthy: true,
            tool_version: "1.0.0".to_string(),
            definition_hash: "mock_hash".to_string(),
            checks: vec![],
            timestamp: SystemTime::now(),
        }
    }
    
    async fn preflight(&self, request: &ExecutionRequest) -> Result<PreflightResult, ContractError> {
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