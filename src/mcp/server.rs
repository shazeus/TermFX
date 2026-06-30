use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use crate::mcp::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::mcp::tools::ToolRegistry;
use crate::project::Project;

pub async fn run_stdio_server(project_path: PathBuf) -> anyhow::Result<()> {
    let project = Project::load(&project_path)?;
    let registry = ToolRegistry::new(project_path, Arc::new(Mutex::new(project)));
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = io::stdout();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let parsed = serde_json::from_str::<JsonRpcRequest>(&line);
        let response = match parsed {
            Ok(request) => handle_request(request, &registry).await,
            Err(error) => Some(JsonRpcResponse::error(
                None,
                -32700,
                format!("parse error: {error}"),
            )),
        };

        if let Some(response) = response {
            let json = serde_json::to_string(&response)?;
            stdout.write_all(json.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

async fn handle_request(
    request: JsonRpcRequest,
    registry: &ToolRegistry,
) -> Option<JsonRpcResponse> {
    let id = request.id.clone();
    if request.jsonrpc != "2.0" {
        return Some(JsonRpcResponse::error(id, -32600, "jsonrpc must be 2.0"));
    }

    match request.method.as_str() {
        "initialize" => Some(JsonRpcResponse::result(id, initialize_result())),
        "notifications/initialized" => None,
        "ping" => Some(JsonRpcResponse::result(id, json!({}))),
        "tools/list" => Some(JsonRpcResponse::result(id, registry.list_tools())),
        "tools/call" => match registry
            .call_tool(request.params.unwrap_or_else(|| json!({})))
            .await
        {
            Ok(result) => Some(JsonRpcResponse::result(id, result)),
            Err(error) => Some(JsonRpcResponse::error(id, -32602, error.to_string())),
        },
        method => Some(JsonRpcResponse::error(
            id,
            -32601,
            format!("method not found: {method}"),
        )),
    }
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {
            "tools": {
                "listChanged": false
            }
        },
        "serverInfo": {
            "name": "termfx",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}
