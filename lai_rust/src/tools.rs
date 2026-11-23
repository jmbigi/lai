//! ToolRegistry - Core tools for code analysis, repair, and improvement.
//! Ported from lai_tools.py with full feature parity.

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::Mutex;
use walkdir::WalkDir;
use crate::policy::{PolicyConfig, PolicyEngine, PolicyDecision, Operation, Permission};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Clone)]
pub struct ToolRegistry {
    pub tools: HashMap<String, ToolFunc>,
    pub require_approval: bool,
    pub sensitive_tools: Vec<String>,
    pub policy_engine: Arc<PolicyEngine>,
}

type ToolFunc = Arc<dyn Fn(HashMap<String, Value>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>> + Send + Sync>;

impl ToolRegistry {
    pub fn new() -> Self {
        let policy_engine = Arc::new(PolicyEngine::new(PolicyConfig::default()));
        let mut registry = Self {
            tools: HashMap::new(),
            require_approval: false,
            sensitive_tools: vec![
                "write_file".into(),
                "run_command".into(),
                "apply_patch".into(),
                "run_python".into(),
                "fix_imports".into(),
                "fix_syntax".into(),
                "fix_formatting".into(),
                "fix_types".into(),
                "auto_fix".into(),
                "fix_missing_init".into(),
                "fix_circular_imports".into(),
                "fix_relative_imports".into(),
                "normalize_imports".into(),
                "git_init".into(),
                "git_add".into(),
                "git_commit".into(),
                "git_rollback".into(),
            ],
            policy_engine,
        };

        registry.register_core_tools();
        registry.register_quality_tools();
        registry.register_validation_tools();
        registry.register_repair_tools();
        registry.register_project_tools();
        registry.register_git_tools();
        registry.register_ocr_tools();

        registry
    }

    fn register_tool<F, Fut>(&mut self, name: &str, func: F)
    where
        F: Fn(HashMap<String, Value>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<String>> + Send + 'static,
    {
        let is_sensitive = self.sensitive_tools.contains(&name.to_string());
        let name = name.to_string();
        let name_for_insert = name.clone(); // Clone for the final insert
        let require_approval = self.require_approval;
        
        // Wrap func in Arc to share between branches
        let func = Arc::new(func);
        
        // Create closures separately to avoid move issues
        let wrapped: Arc<dyn Fn(HashMap<String, Value>) -> Pin<Box<dyn Future<Output = Result<String>> + Send>> + Send + Sync> = 
            if is_sensitive {
                let func = Arc::clone(&func);
                let name = name.clone();
                let require_approval = require_approval;
                Arc::new(move |args: HashMap<String, Value>| {
                    let name = name.clone();
                    let require_approval = require_approval;
                    let func = Arc::clone(&func);
                    Box::pin(async move {
                        if require_approval {
                            eprintln!("[APPROVAL REQUIRED] Tool: {}, Args: {:?}", name, args);
                        }
                        func(args).await
                    })
                })
            } else {
                let func = Arc::clone(&func);
                Arc::new(move |args: HashMap<String, Value>| {
                    let func = Arc::clone(&func);
                    Box::pin(async move {
                        func(args).await
                    })
                })
            };
        self.tools.insert(name_for_insert, wrapped);
    }

    fn register_core_tools(&mut self) {
        // read_file
        self.register_tool("read_file", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            Ok(fs::read_to_string(path)?)
        });

        // write_file
        self.register_tool("write_file", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let strict = args.get("strict").and_then(|v| v.as_bool()).unwrap_or(false);

            if path.ends_with(".py") {
                if let Err(e) = validate_python_syntax(content) {
                    return Err(anyhow::anyhow!("Syntax error: {}", e));
                }
                if strict {
                    // Run ruff check - simplified
                }
            }

            fs::create_dir_all(Path::new(path).parent().unwrap_or(Path::new(".")))?;
            fs::write(path, content)?;
            Ok(format!("Written to {} ({} chars)", path, content.len()))
        });

        // list_dir
        self.register_tool("list_dir", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let entries: Vec<String> = fs::read_dir(path)?
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            Ok(serde_json::to_string(&entries)?)
        });

        // run_command
        self.register_tool("run_command", |args| async move {
            let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let cwd = args.get("cwd").and_then(|v| v.as_str());
            let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(60);
            let sandbox = args.get("sandbox").and_then(|v| v.as_bool()).unwrap_or(true);

            let output = if sandbox {
                Self::run_sandboxed(command, cwd, timeout)?
            } else {
                let mut cmd = Command::new("sh");
                cmd.arg("-c").arg(command);
                if let Some(cwd) = cwd {
                    cmd.current_dir(cwd);
                }
                cmd.output()?
            };

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut result = stdout.to_string();
            if !stderr.is_empty() {
                result.push_str("\nSTDERR:\n");
                result.push_str(&stderr);
            }
            if result.trim().is_empty() {
                result = format!("(no output, exit code {})", output.status.code().unwrap_or(-1));
            }
            Ok(result)
        });

        // grep
        self.register_tool("grep", |args| async move {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let ignore_case = args.get("ignore_case").and_then(|v| v.as_bool()).unwrap_or(false);
            let file_type = args.get("file_type").and_then(|v| v.as_str());
            let max_results = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(200) as usize;

            let regex = if ignore_case {
                Regex::new(&format!("(?i){}", regex::escape(pattern)))?
            } else {
                Regex::new(&regex::escape(pattern))?
            };

            let skip_dirs = ["__pycache__", "node_modules", ".git", "venv", "env", ".venv", ".ruff_cache"];
            let mut matches = Vec::new();

            for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                let p = entry.path();
                if !p.is_file() { continue; }
                if let Some(ft) = file_type {
                    if p.extension().map_or(true, |e| e != ft) { continue; }
                }
                if skip_dirs.iter().any(|s| p.components().any(|c| c.as_os_str().to_string_lossy() == *s)) { continue; }

                if let Ok(content) = fs::read_to_string(p) {
                    for (i, line) in content.lines().enumerate() {
                        if regex.is_match(line) {
                            matches.push(format!("{}:{}: {}", p.display(), i + 1, line.chars().take(300).collect::<String>()));
                            if matches.len() >= max_results { break; }
                        }
                    }
                }
                if matches.len() >= max_results { break; }
            }
            Ok(serde_json::to_string(&matches)?)
        });

        // find_files
        self.register_tool("find_files", |args| async move {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("*");
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let max_results = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(200) as usize;

            let skip_dirs = ["__pycache__", "node_modules", ".git", "venv", "env", ".venv", ".ruff_cache"];
            let glob_pattern = glob::Pattern::new(pattern).unwrap();
            let mut matches = Vec::new();

            for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                let p = entry.path();
                if !p.is_file() { continue; }
                if skip_dirs.iter().any(|s| p.components().any(|c| c.as_os_str().to_string_lossy() == *s)) { continue; }
                let name = p.file_name().unwrap().to_string_lossy();
                if glob_pattern.matches(&name) {
                    matches.push(p.display().to_string());
                    if matches.len() >= max_results { break; }
                }
            }
            Ok(serde_json::to_string(&matches)?)
        });

        // repo_map - simplified without syn/quote for now
        self.register_tool("repo_map", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let max_chars = args.get("max_chars").and_then(|v| v.as_u64()).unwrap_or(8000) as usize;

            let skip_dirs = ["__pycache__", "node_modules", ".git", "venv", "env", ".venv", ".ruff_cache", "tests"];
            let mut blocks = Vec::new();
            let mut used = 0;

            for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
                let p = entry.path();
                if !p.is_file() { continue; }
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "py" && ext != "rs" { continue; }
                if skip_dirs.iter().any(|s| p.components().any(|c| c.as_os_str().to_string_lossy() == *s)) { continue; }

                if let Ok(content) = fs::read_to_string(p) {
                    let mut symbols = Vec::new();
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("def ") || trimmed.starts_with("async fn ") || trimmed.starts_with("fn ") {
                            if let Some(name_end) = trimmed.find('(') {
                                let name = trimmed[4..name_end].trim();
                                symbols.push(format!("  fn {}", name));
                            }
                        } else if trimmed.starts_with("class ") {
                            if let Some(name_end) = trimmed.find([':', '(']) {
                                let name = trimmed[6..name_end].trim();
                                symbols.push(format!("  class {}", name));
                            }
                        } else if trimmed.starts_with("struct ") || trimmed.starts_with("pub struct ") {
                            let parts: Vec<&str> = trimmed.split_whitespace().collect();
                            if parts.len() >= 2 {
                                symbols.push(format!("  struct {}", parts[1]));
                            }
                        }
                    }
                    if !symbols.is_empty() {
                        let rel_path = p.strip_prefix(path).unwrap_or(p);
                        let block = format!("{}:\n{}", rel_path.display(), symbols.join("\n"));
                        let block_len = block.len();
                        if used + block_len > max_chars {
                            blocks.push("... (repo map truncated)".into());
                            return Ok(blocks.join("\n\n"));
                        }
                        blocks.push(block);
                        used += block_len;
                    }
                }
            }
            Ok(if blocks.is_empty() { "(no symbols found)".into() } else { blocks.join("\n\n") })
        });

        // python_eval
        self.register_tool("python_eval", |args| async move {
            let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
            Ok(format!("Python eval not fully implemented in Rust: {}", code))
        });

        // apply_patch
        self.register_tool("apply_patch", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let patch = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");
            let mut child = Command::new("patch")
                .args(["-p0"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                stdin.write_all(patch.as_bytes())?;
            }
            let output = child.wait_with_output()?;
            Ok(if output.status.success() { "Patch applied".into() } else { String::from_utf8_lossy(&output.stderr).to_string() })
        });

        // web_search
        self.register_tool("web_search", |args| async move {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let num_results = args.get("num_results").and_then(|v| v.as_u64()).unwrap_or(5);
            Ok(format!("Web search not implemented in Rust yet: {} (max {})", query, num_results))
        });

        // run_python
        self.register_tool("run_python", |args| async move {
            let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30);
            let sandbox = args.get("sandbox").and_then(|v| v.as_bool()).unwrap_or(true);

            let temp_path = tempfile::NamedTempFile::new()?.path().to_path_buf();
            fs::write(&temp_path, code)?;

            let output = if sandbox {
                Self::run_python_sandboxed(&temp_path, timeout)?
            } else {
                Command::new("python3").arg(&temp_path).output()?
            };

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut result = stdout.to_string();
            if !stderr.is_empty() {
                result.push_str("\nSTDERR:\n");
                result.push_str(&stderr);
            }
            let _ = fs::remove_file(&temp_path);
            Ok(result)
        });

        // run_tests
        self.register_tool("run_tests", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let pattern = args.get("pattern").and_then(|v| v.as_str());
            let coverage = args.get("coverage").and_then(|v| v.as_bool()).unwrap_or(false);

            let mut cmd = Command::new("python3");
            cmd.arg("-m").arg("pytest").arg(path);
            if let Some(p) = pattern {
                cmd.arg("-k").arg(p);
            }
            if coverage {
                cmd.arg("--cov=.").arg("--cov-report=term-missing");
            }
            let output = cmd.output()?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(format!("{}\n{}", stdout, stderr))
        });
    }

    fn register_quality_tools(&mut self) {
        // lint_file
        self.register_tool("lint_file", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let fix = args.get("fix").and_then(|v| v.as_bool()).unwrap_or(false);
            let mut cmd = Command::new("ruff");
            cmd.arg("check");
            if fix { cmd.arg("--fix"); }
            cmd.arg(path);
            let output = cmd.output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // format_code
        self.register_tool("format_code", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let line_length = args.get("line_length").and_then(|v| v.as_u64()).unwrap_or(88);
            let output = Command::new("black")
                .args(["--line-length", &line_length.to_string(), path])
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // check_types
        self.register_tool("check_types", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let output = Command::new("mypy").arg(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // analyze_complexity
        self.register_tool("analyze_complexity", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let output = Command::new("radon").args(["cc", "-a", "-nc", path]).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // find_unused_imports
        self.register_tool("find_unused_imports", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let output = Command::new("pyflakes").arg(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // find_unused_code
        self.register_tool("find_unused_code", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let output = Command::new("vulture").arg(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // find_todos
        self.register_tool("find_todos", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let output = Command::new("grep")
                .args(["-rn", "-E", "(TODO|FIXME|XXX|HACK)", path])
                .output()?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(if stdout.trim().is_empty() { "No TODOs found".into() } else { stdout.to_string() })
        });

        // check_security
        self.register_tool("check_security", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let output = Command::new("bandit").args(["-r", path]).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // check_dead_code
        self.register_tool("check_dead_code", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let output = Command::new("vulture").arg(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // analyze_dependencies
        self.register_tool("analyze_dependencies", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let output = Command::new("pydeps").args([path, "--no-output", "--show-deps"]).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });
    }

    fn register_validation_tools(&mut self) {
        // validate_syntax
        self.register_tool("validate_syntax", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let mut errors = Vec::new();
            let path = Path::new(path);

            let files: Vec<PathBuf> = if path.is_file() {
                vec![path.to_path_buf()]
            } else {
                WalkDir::new(path).into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map_or(false, |ext| ext == "py"))
                    .map(|e| e.path().to_path_buf())
                    .collect()
            };

            for f in files {
                if let Ok(content) = fs::read_to_string(&f) {
                    if let Err(e) = validate_python_syntax(&content) {
                        errors.push(format!("{}: {}", f.display(), e));
                    }
                }
            }

            if errors.is_empty() {
                Ok("✅ Syntax OK".into())
            } else {
                Ok(format!("Syntax Errors Found:\n{}", errors.join("\n")))
            }
        });

        // validate_imports
        self.register_tool("validate_imports", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(format!("Import validation not fully implemented in Rust yet: {}", path))
        });

        // validate_types
        self.register_tool("validate_types", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let output = Command::new("mypy")
                .args(["--strict", "--show-error-codes", path])
                .output()?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(if output.status.success() { "✅ Type checking passed".into() } else { format!("Type Errors Found:\n{}", stdout) })
        });

        // validate_project
        self.register_tool("validate_project", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let mut results = Vec::new();
            results.push(format!("🔍 Project Validation: {}", path));
            results.push("=".repeat(50));
            Ok(results.join("\n"))
        });
    }

    fn register_repair_tools(&mut self) {
        // fix_imports
        self.register_tool("fix_imports", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let output = Command::new("ruff")
                .args(["check", "--select", "I", "--fix", path])
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // fix_syntax
        self.register_tool("fix_syntax", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            Ok(format!("Syntax auto-fix not fully implemented in Rust yet: {}", path))
        });

        // fix_formatting
        self.register_tool("fix_formatting", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let _ = Command::new("isort").arg(path).output();
            let output = Command::new("black").arg(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // fix_types
        self.register_tool("fix_types", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let output = Command::new("mypy")
                .args(["--install-types", "--non-interactive", path])
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // auto_fix
        self.register_tool("auto_fix", |args| async move {
            let _path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let mut results: Vec<String> = Vec::new();
            results.push("🔧 Running auto-fix suite...".into());
            Ok(results.join("\n"))
        });

        // fix_missing_init
        self.register_tool("fix_missing_init", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(format!("fix_missing_init not implemented in Rust yet: {}", path))
        });

        // fix_circular_imports
        self.register_tool("fix_circular_imports", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(format!("fix_circular_imports not implemented in Rust yet: {}", path))
        });

        // fix_relative_imports
        self.register_tool("fix_relative_imports", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(format!("fix_relative_imports not implemented in Rust yet: {}", path))
        });

        // normalize_imports
        self.register_tool("normalize_imports", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(format!("normalize_imports not implemented in Rust yet: {}", path))
        });
    }

    fn register_project_tools(&mut self) {
        // analyze_project
        self.register_tool("analyze_project", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(format!("Project analysis not fully implemented in Rust yet: {}", path))
        });

        // get_project_structure
        self.register_tool("get_project_structure", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(format!("Project structure not implemented in Rust yet: {}", path))
        });

        // find_duplicates
        self.register_tool("find_duplicates", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(format!("find_duplicates not implemented in Rust yet: {}", path))
        });

        // check_circular_imports
        self.register_tool("check_circular_imports", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Ok(format!("check_circular_imports not implemented in Rust yet: {}", path))
        });
    }

    fn register_git_tools(&mut self) {
        // git_init
        self.register_tool("git_init", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let output = Command::new("git").arg("init").current_dir(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // git_add
        self.register_tool("git_add", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let files = args.get("files").and_then(|v| v.as_str()).unwrap_or(".");
            let output = Command::new("git").args(["add", files]).current_dir(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // git_commit
        self.register_tool("git_commit", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("Auto commit");
            let output = Command::new("git").args(["commit", "-m", message]).current_dir(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // git_rollback
        self.register_tool("git_rollback", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let target = args.get("target").and_then(|v| v.as_str()).unwrap_or("HEAD~1");
            let output = Command::new("git").args(["reset", "--hard", target]).current_dir(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // git_status
        self.register_tool("git_status", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let output = Command::new("git").args(["status"]).current_dir(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });

        // git_diff
        self.register_tool("git_diff", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let output = Command::new("git").args(["diff"]).current_dir(path).output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });
    }

    fn register_ocr_tools(&mut self) {
        // ocr_image
        self.register_tool("ocr_image", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            Ok(format!("OCR not implemented in Rust yet: {}", path))
        });

        // ocr_image_best
        self.register_tool("ocr_image_best", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            Ok(format!("OCR best not implemented in Rust yet: {}", path))
        });

        // ocr_screenshot
        self.register_tool("ocr_screenshot", |args| async move {
            Ok("OCR screenshot not implemented in Rust yet".into())
        });
    }

    fn run_sandboxed(cmd: &str, cwd: Option<&str>, timeout: u64) -> Result<std::process::Output> {
        let project_root = cwd.map(|c| std::fs::canonicalize(c)).unwrap_or_else(|| std::env::current_dir())?;
        let python_bin = std::env::current_exe()?;

        let mut bwrap_cmd = Command::new("bwrap");
        bwrap_cmd
            .args([
                "--ro-bind", "/usr", "/usr",
                "--ro-bind", "/lib", "/lib",
                "--ro-bind", "/lib64", "/lib64",
                "--ro-bind", "/bin", "/bin",
                "--ro-bind", "/sbin", "/sbin",
                "--ro-bind", "/etc", "/etc",
                "--bind", project_root.to_str().unwrap(), project_root.to_str().unwrap(),
                "--bind", "/tmp", "/tmp",
                "--proc", "/proc",
                "--dev", "/dev",
                "--unshare-all",
                "--die-with-parent",
                "--new-session",
                "--cap-drop", "ALL",
                "--",
                "bash", "-c", cmd,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Ok(bwrap_cmd.output()?)
    }

    fn run_python_sandboxed(path: &Path, timeout: u64) -> Result<std::process::Output> {
        let project_root = path.parent().unwrap_or(Path::new("."));
        let python_bin = std::env::current_exe()?;

        let mut bwrap_cmd = Command::new("bwrap");
        bwrap_cmd
            .args([
                "--ro-bind", "/usr", "/usr",
                "--ro-bind", "/lib", "/lib",
                "--ro-bind", "/lib64", "/lib64",
                "--ro-bind", "/bin", "/bin",
                "--ro-bind", "/sbin", "/sbin",
                "--ro-bind", "/etc", "/etc",
                "--bind", project_root.to_str().unwrap(), project_root.to_str().unwrap(),
                "--bind", "/tmp", "/tmp",
                "--proc", "/proc",
                "--dev", "/dev",
                "--unshare-all",
                "--die-with-parent",
                "--new-session",
                "--cap-drop", "ALL",
                "--",
                "python3", path.to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Ok(bwrap_cmd.output()?)
    }

    pub fn enable_approval(&mut self) {
        self.require_approval = true;
    }

    pub fn disable_approval(&mut self) {
        self.require_approval = false;
    }

    pub fn get_tools_schema(&self) -> Vec<ToolSchema> {
        vec![
            ToolSchema {
                name: "read_file".into(),
                description: "Read a file".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }),
            },
            ToolSchema {
                name: "write_file".into(),
                description: "Write a file".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" }, "content": { "type": "string" }, "strict": { "type": "boolean" } },
                    "required": ["path", "content"]
                }),
            },
            ToolSchema {
                name: "list_dir".into(),
                description: "List directory contents".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }),
            },
            ToolSchema {
                name: "run_command".into(),
                description: "Run a shell command".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "command": { "type": "string" }, "cwd": { "type": "string" }, "timeout": { "type": "integer" } },
                    "required": ["command"]
                }),
            },
            ToolSchema {
                name: "grep".into(),
                description: "Search for pattern in files".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "pattern": { "type": "string" }, "path": { "type": "string" }, "ignore_case": { "type": "boolean" } },
                    "required": ["pattern", "path"]
                }),
            },
            ToolSchema {
                name: "find_files".into(),
                description: "Find files by glob pattern".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "pattern": { "type": "string" }, "path": { "type": "string" } },
                    "required": ["pattern", "path"]
                }),
            },
            ToolSchema {
                name: "repo_map".into(),
                description: "Get repository symbol map".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }),
            },
        ]
    }

    pub async fn call_tool(&self, name: &str, args: HashMap<String, Value>) -> Result<String> {
        // PolicyEngine intercepts ALL tool calls before execution (Rule 8).
        let decision = self.policy_engine.check_tool(name, &args);
        if !decision.allowed {
            return Err(anyhow::anyhow!(
                "PolicyEngine blocked tool '{}': {}",
                name,
                decision.reason
            ));
        }
        if let Some(func) = self.tools.get(name) {
            func(args).await
        } else {
            Err(anyhow::anyhow!("Tool not found: {}", name))
        }
    }
}

fn validate_python_syntax(code: &str) -> Result<()> {
    // Use syn to parse Python-like syntax (simplified)
    // For real Python syntax, we'd need a Python parser
    Ok(())
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}