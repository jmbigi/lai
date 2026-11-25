//! mcp_server.rs - Model Context Protocol server for lai
//! Exposes lai tools via MCP (JSON-RPC 2.0) for integration with
//! Cursor, Cline, Claude Code, and other MCP-compatible clients.
//! 100% local, loopback-only, zero cloud dependencies.

use crate::tools::ToolRegistry;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// MCP protocol version
const MCP_VERSION: &str = "2024-11-05";

/// JSON-RPC 2.0 request
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

/// MCP initialization params
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct InitializeParams {
    protocol_version: String,
    capabilities: Option<ClientCapabilities>,
    client_info: Option<ClientInfo>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ClientCapabilities {
    roots: Option<RootCapabilities>,
    sampling: Option<Value>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RootCapabilities {
    list_changed: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ClientInfo {
    name: String,
    version: String,
}

/// MCP server capabilities
#[derive(Debug, Serialize)]
struct ServerCapabilities {
    tools: ToolsCapability,
    resources: ResourcesCapability,
    prompts: PromptsCapability,
    logging: LoggingCapability,
}

#[derive(Debug, Serialize)]
struct ToolsCapability {
    list_changed: bool,
}

#[derive(Debug, Serialize)]
struct ResourcesCapability {
    subscribe: bool,
    list_changed: bool,
}

#[derive(Debug, Serialize)]
struct PromptsCapability {
    list_changed: bool,
}

#[derive(Debug, Serialize)]
struct LoggingCapability {}

/// Tool definition for MCP
#[derive(Debug, Serialize)]
struct McpTool {
    name: String,
    description: String,
    input_schema: Value,
}

/// Resource definition for MCP
#[derive(Debug, Serialize)]
struct McpResource {
    uri: String,
    name: String,
    description: Option<String>,
    mime_type: Option<String>,
}

/// Prompt definition for MCP
#[derive(Debug, Serialize)]
struct McpPrompt {
    name: String,
    description: String,
    arguments: Option<Vec<PromptArgument>>,
}

#[derive(Debug, Serialize)]
struct PromptArgument {
    name: String,
    description: String,
    required: bool,
}

/// MCP server state
pub struct McpServer {
    tool_registry: Arc<ToolRegistry>,
    addr: SocketAddr,
    initialized: Arc<Mutex<bool>>,
}

impl McpServer {
    pub fn new(tool_registry: Arc<ToolRegistry>, addr: SocketAddr) -> Self {
        Self {
            tool_registry,
            addr,
            initialized: Arc::new(Mutex::new(false)),
        }
    }

    /// Run the MCP server
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(self.addr)
            .await
            .with_context(|| format!("Failed to bind MCP server to {}", self.addr))?;

        info!("MCP server listening on {}", self.addr);

        loop {
            let (stream, addr) = listener.accept().await?;
            let server = self.clone();

            tokio::spawn(async move {
                if let Err(e) = server.handle_connection(stream, addr).await {
                    warn!("MCP connection error from {}: {}", addr, e);
                }
            });
        }
    }

    /// Handle a single MCP connection (stdio or TCP)
    async fn handle_connection(&self, stream: TcpStream, addr: SocketAddr) -> Result<()> {
        debug!("New MCP connection from {}", addr);

        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut writer = write_half;

        let mut line = String::new();
        while reader.read_line(&mut line).await? > 0 {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                line.clear();
                continue;
            }

            debug!("MCP request: {}", trimmed);

            let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(req) => req,
                Err(e) => {
                    self.send_error(
                        &mut writer,
                        None,
                        -32700,
                        "Parse error",
                        Some(json!(e.to_string())),
                    )
                    .await?;
                    line.clear();
                    continue;
                }
            };

            let response = self.handle_request(request).await;
            let response_json = serde_json::to_string(&response)?;
            writer
                .write_all(format!("{}\n", response_json).as_bytes())
                .await?;
            writer.flush().await?;

            line.clear();
        }

        Ok(())
    }

    /// Handle an MCP request
    async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request).await,
            "initialized" => self.handle_initialized(request).await,
            "tools/list" => self.handle_tools_list(request).await,
            "tools/call" => self.handle_tools_call(request).await,
            "resources/list" => self.handle_resources_list(request).await,
            "resources/read" => self.handle_resources_read(request).await,
            "prompts/list" => self.handle_prompts_list(request).await,
            "prompts/get" => self.handle_prompts_get(request).await,
            "ping" => self.handle_ping(request).await,
            _ => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                    data: None,
                }),
            },
        }
    }

    async fn handle_initialize(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let mut initialized = self.initialized.lock().await;
        *initialized = true;

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "protocolVersion": MCP_VERSION,
                "capabilities": ServerCapabilities {
                    tools: ToolsCapability { list_changed: true },
                    resources: ResourcesCapability { subscribe: true, list_changed: true },
                    prompts: PromptsCapability { list_changed: true },
                    logging: LoggingCapability {},
                },
                "serverInfo": {
                    "name": "lai",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            })),
            error: None,
        }
    }

    async fn handle_initialized(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        // Client confirmed initialization - no response needed for notification
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({})),
            error: None,
        }
    }

    async fn handle_tools_list(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let tools = self.tool_registry.get_tools_schema();
        let mcp_tools: Vec<McpTool> = tools
            .into_iter()
            .map(|t| McpTool {
                name: t.name,
                description: t.description,
                input_schema: t.parameters,
            })
            .collect();

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({ "tools": mcp_tools })),
            error: None,
        }
    }

    async fn handle_tools_call(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params = request.params.as_ref().and_then(|p| p.as_object());
        let tool_name = params.and_then(|p| p.get("name")).and_then(|v| v.as_str());
        let arguments = params
            .and_then(|p| p.get("arguments"))
            .cloned()
            .unwrap_or(json!({}));

        let tool_name = match tool_name {
            Some(name) => name,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Missing tool name".to_string(),
                        data: None,
                    }),
                };
            }
        };

        // Convert arguments to HashMap
        let args_map: HashMap<String, Value> = match serde_json::from_value(arguments) {
            Ok(map) => map,
            Err(e) => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: format!("Invalid arguments: {}", e),
                        data: None,
                    }),
                };
            }
        };

        // Call the tool
        match self.tool_registry.call_tool(tool_name, args_map).await {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({
                    "content": [{ "type": "text", "text": result }],
                    "isError": false,
                })),
                error: None,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({
                    "content": [{ "type": "text", "text": format!("Error: {}", e) }],
                    "isError": true,
                })),
                error: None,
            },
        }
    }

    async fn handle_resources_list(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        // Expose key project files as resources
        let resources = vec![
            McpResource {
                uri: "file:///workspace/AGENTS.md".to_string(),
                name: "AGENTS.md".to_string(),
                description: Some("Agent instructions and project rules".to_string()),
                mime_type: Some("text/markdown".to_string()),
            },
            McpResource {
                uri: "file:///workspace/README.md".to_string(),
                name: "README.md".to_string(),
                description: Some("Project overview".to_string()),
                mime_type: Some("text/markdown".to_string()),
            },
        ];

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({ "resources": resources })),
            error: None,
        }
    }

    async fn handle_resources_read(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params = request.params.as_ref().and_then(|p| p.as_object());
        let uri = params.and_then(|p| p.get("uri")).and_then(|v| v.as_str());

        let uri = match uri {
            Some(u) => u,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Missing URI".to_string(),
                        data: None,
                    }),
                };
            }
        };

        // Read file from URI (only allow workspace files)
        let path = uri.strip_prefix("file:///workspace/").unwrap_or(uri);
        let full_path = std::path::Path::new("/workspace").join(path);

        // Security: ensure path is within workspace
        if !full_path.starts_with("/workspace") {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Access denied: path outside workspace".to_string(),
                    data: None,
                }),
            };
        }

        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: format!("Failed to read resource: {}", e),
                        data: None,
                    }),
                };
            }
        };

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "text/markdown",
                    "text": content,
                }]
            })),
            error: None,
        }
    }

    async fn handle_prompts_list(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let prompts = vec![
            McpPrompt {
                name: "code_review".to_string(),
                description: "Review code for quality, security, and best practices".to_string(),
                arguments: Some(vec![PromptArgument {
                    name: "file_path".to_string(),
                    description: "Path to file to review".to_string(),
                    required: true,
                }]),
            },
            McpPrompt {
                name: "generate_tests".to_string(),
                description: "Generate unit tests for a function or module".to_string(),
                arguments: Some(vec![PromptArgument {
                    name: "file_path".to_string(),
                    description: "Path to file to test".to_string(),
                    required: true,
                }]),
            },
            McpPrompt {
                name: "refactor".to_string(),
                description: "Refactor code for clarity, performance, or maintainability"
                    .to_string(),
                arguments: Some(vec![
                    PromptArgument {
                        name: "file_path".to_string(),
                        description: "Path to file to refactor".to_string(),
                        required: true,
                    },
                    PromptArgument {
                        name: "goal".to_string(),
                        description:
                            "Refactoring goal (e.g., 'improve performance', 'reduce complexity')"
                                .to_string(),
                        required: false,
                    },
                ]),
            },
        ];

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({ "prompts": prompts })),
            error: None,
        }
    }

    async fn handle_prompts_get(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params = request.params.as_ref().and_then(|p| p.as_object());
        let name = params.and_then(|p| p.get("name")).and_then(|v| v.as_str());
        let arguments = params
            .and_then(|p| p.get("arguments"))
            .and_then(|v| v.as_object());

        let (prompt_text, description) = match name {
            Some("code_review") => {
                let file = arguments
                    .and_then(|a| a.get("file_path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                (format!("Review the code in {} for quality, security issues, and best practices. Provide specific, actionable feedback.", file),
                 "Code review prompt".to_string())
            }
            Some("generate_tests") => {
                let file = arguments
                    .and_then(|a| a.get("file_path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                (format!("Generate comprehensive unit tests for the code in {}. Cover edge cases, error paths, and typical usage.", file),
                 "Test generation prompt".to_string())
            }
            Some("refactor") => {
                let file = arguments
                    .and_then(|a| a.get("file_path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let goal = arguments
                    .and_then(|a| a.get("goal"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("improve maintainability");
                (format!("Refactor the code in {} to {}. Focus on clarity, performance, and maintainability.", file, goal),
                 "Refactoring prompt".to_string())
            }
            _ => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: format!("Unknown prompt: {:?}", name),
                        data: None,
                    }),
                };
            }
        };

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({
                "description": description,
                "messages": [{ "role": "user", "content": { "type": "text", "text": prompt_text } }],
            })),
            error: None,
        }
    }

    async fn handle_ping(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(json!({})),
            error: None,
        }
    }

    async fn send_error<W: tokio::io::AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        id: Option<Value>,
        code: i32,
        message: &str,
        data: Option<Value>,
    ) -> Result<()> {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.to_string(),
                data,
            }),
        };
        let response_json = serde_json::to_string(&response)?;
        writer
            .write_all(format!("{}\n", response_json).as_bytes())
            .await?;
        writer.flush().await?;
        Ok(())
    }
}

// Need Clone for spawning
impl Clone for McpServer {
    fn clone(&self) -> Self {
        Self {
            tool_registry: self.tool_registry.clone(),
            addr: self.addr,
            initialized: self.initialized.clone(),
        }
    }
}

/// Run MCP server over stdio (for Claude Code, etc.)
pub async fn run_mcp_stdio(tool_registry: Arc<ToolRegistry>) -> Result<()> {
    let server = McpServer::new(tool_registry, "127.0.0.1:0".parse().unwrap());

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);

    let mut line = String::new();
    while reader.read_line(&mut line).await? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: None,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: "Parse error".to_string(),
                        data: Some(json!(e.to_string())),
                    }),
                };
                let response_json = serde_json::to_string(&response)?;
                stdout
                    .write_all(format!("{}\n", response_json).as_bytes())
                    .await?;
                stdout.flush().await?;
                line.clear();
                continue;
            }
        };

        let response = server.handle_request(request).await;
        let response_json = serde_json::to_string(&response)?;
        stdout
            .write_all(format!("{}\n", response_json).as_bytes())
            .await?;
        stdout.flush().await?;

        line.clear();
    }

    Ok(())
}

/// CLI command to start MCP server
pub async fn cmd_mcp(port: Option<u16>, stdio: bool) -> Result<()> {
    use crate::tools::ToolRegistry;

    let tool_registry = Arc::new(ToolRegistry::new());

    if stdio {
        info!("Starting MCP server over stdio");
        run_mcp_stdio(tool_registry).await?;
    } else {
        let port = port.unwrap_or(8083);
        let addr = format!("127.0.0.1:{}", port).parse()?;
        info!("Starting MCP server on {}", addr);
        let server = McpServer::new(tool_registry, addr);
        server.run().await?;
    }

    Ok(())
}
