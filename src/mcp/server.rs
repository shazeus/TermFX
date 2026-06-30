use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
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

pub async fn run_http_server(project_path: PathBuf, address: SocketAddr) -> anyhow::Result<()> {
    let project = Project::load(&project_path)?;
    let registry = ToolRegistry::new(project_path, Arc::new(Mutex::new(project)));
    let listener = TcpListener::bind(address).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let registry = registry.clone();
        tokio::spawn(async move {
            let _ = handle_http_connection(stream, registry).await;
        });
    }
}

async fn handle_http_connection(
    mut stream: TcpStream,
    registry: ToolRegistry,
) -> anyhow::Result<()> {
    let mut buffer = vec![0u8; 64 * 1024];
    let bytes_read = stream.read(&mut buffer).await?;
    if bytes_read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    if request.starts_with("OPTIONS ") {
        write_http_response(&mut stream, 204, "").await?;
        return Ok(());
    }
    if request.starts_with("GET /health ") || request.starts_with("GET / ") {
        write_http_response(
            &mut stream,
            200,
            &json!({"status": "ok", "server": "termfx"}).to_string(),
        )
        .await?;
        return Ok(());
    }

    let Some(header_end) = request.find("\r\n\r\n") else {
        write_http_response(
            &mut stream,
            400,
            &json!({"jsonrpc":"2.0","error":{"code":-32700,"message":"invalid http request"}})
                .to_string(),
        )
        .await?;
        return Ok(());
    };
    let body_start = header_end + 4;
    let body = &buffer[body_start..bytes_read];
    let parsed = serde_json::from_slice::<JsonRpcRequest>(body);
    let response = match parsed {
        Ok(request) => handle_request(request, &registry).await,
        Err(error) => Some(JsonRpcResponse::error(
            None,
            -32700,
            format!("parse error: {error}"),
        )),
    };

    if let Some(response) = response {
        let body = serde_json::to_string(&response)?;
        write_http_response(&mut stream, 200, &body).await?;
    } else {
        write_http_response(&mut stream, 202, "").await?;
    }

    Ok(())
}

async fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    body: &str,
) -> anyhow::Result<()> {
    let status_text = match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: POST, OPTIONS, GET\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

pub async fn handle_request(
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
