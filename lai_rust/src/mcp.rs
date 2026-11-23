//! mcp.rs - Servidor MCP mínimo (stdio, JSON-RPC 2.0) sobre el ToolRegistry.
//! Interoperabilidad (v4.0, Mejora 8): expone las herramientas internas sin
//! duplicar su implementación. Requisitos de integridad:
//! - Toda invocación pasa por ToolRegistry::call_tool (integrity + PolicyEngine).
//! - Las salidas de herramientas se tratan como DATOS NO CONFIABLES.
//! - Tokens: el servidor es local (stdio); no hay token passthrough.

use crate::tools::ToolRegistry;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Sirve peticiones JSON-RPC por stdin/stdout hasta EOF.
pub async fn serve(registry: ToolRegistry) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin).lines();
    let mut writer = stdout;

    while let Ok(Some(line)) = reader.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue, // JSON-RPC inválido: se ignora (no se responde)
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let response = dispatch(&registry, id, method, req.get("params").cloned().unwrap_or(json!({}))).await;
        if writer.write_all(format!("{}\n", response).as_bytes()).await.is_err() {
            break;
        }
        let _ = writer.flush().await;
    }
    Ok(())
}

async fn dispatch(registry: &ToolRegistry, id: Option<Value>, method: &str, params: Value) -> Value {
    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "lai", "version": env!("CARGO_PKG_VERSION") }
            }
        }),
        "tools/list" => {
            let tools: Vec<Value> = registry
                .get_tools_schema()
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.parameters,
                    })
                })
                .collect();
            json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": tools } })
        }
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args: HashMap<String, Value> = params
                .get("arguments")
                .and_then(|a| a.as_object())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            // call_tool aplica integridad (fail-closed) + PolicyEngine.
            match registry.call_tool(name, args).await {
                Ok(out) => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "content": [{ "type": "text", "text": out }] }
                }),
                Err(e) => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32000, "message": format!("{}", e) }
                }),
            }
        }
        "notifications/initialized" => json!({ "jsonrpc": "2.0", "id": id }),
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("method not found: {}", method) }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_initialize_and_tools_list() {
        let registry = ToolRegistry::new();
        let init = dispatch(&registry, Some(json!(1)), "initialize", json!({})).await;
        assert_eq!(init["result"]["serverInfo"]["name"], "lai");

        let list = dispatch(&registry, Some(json!(2)), "tools/list", json!({})).await;
        let tools = list["result"]["tools"].as_array().unwrap();
        assert!(!tools.is_empty(), "el ToolRegistry debe exponer herramientas");
        assert!(tools.iter().all(|t| t["name"].is_string()));
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let registry = ToolRegistry::new();
        let resp = dispatch(&registry, Some(json!(3)), "no/pe", json!({})).await;
        assert_eq!(resp["error"]["code"], -32601);
    }
}