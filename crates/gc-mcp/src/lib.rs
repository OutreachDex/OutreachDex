//! MCP stdio endpoint. Business execution stays in the desktop owner of the workspace.
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};
pub async fn remote(endpoint: &str, token: &str, request: Value) -> Result<Value, String> {
    let stream = TcpStream::connect(endpoint)
        .await
        .map_err(|e| e.to_string())?;
    let (read, mut write) = stream.into_split();
    write
        .write_all(format!("{}\n", json!({"token":token,"request":request})).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut lines = BufReader::new(read).lines();
    let line = tokio::time::timeout(Duration::from_secs(180), lines.next_line())
        .await
        .map_err(|_| "desktop request timed out".to_string())?
        .map_err(|e| e.to_string())?
        .ok_or("desktop disconnected")?;
    let response: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    if let Some(error) = response["error"].as_str() {
        Err(error.into())
    } else {
        Ok(response["result"].clone())
    }
}
pub async fn serve(workspace: &str) -> Result<(), String> {
    let endpoint = std::env::var("GC_MCP_ENDPOINT")
        .map_err(|_| "Start the OutreachDex desktop app before connecting MCP.")?;
    let token = std::env::var("GC_MCP_TOKEN").map_err(|_| "MCP session token is missing.")?;
    // Set by OutreachDex for Agent terminals, so the app can show which session did what.
    let session = std::env::var("GC_TERMINAL_SESSION").unwrap_or_default();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut output = tokio::io::stdout();
    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        if line.len() > 8 * 1024 * 1024 {
            return Err("MCP frame too large".into());
        }
        let input: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(id) = input.get("id").cloned() else {
            continue;
        };
        let method = input["method"].as_str().unwrap_or("");
        let result = match method {
            "initialize" => Ok(
                json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"globalcontact","version":"0.1.0"},"instructions":"Manage contacts and drafts. For a new request about the current person, call context.get once and pin the returned contact for the task. Explicitly named contacts take precedence. mail.accounts and mail.sync synchronize connected incoming mailboxes, never send. No sending tool exists. Report progress and completion with task.report; show results to the user with ui.open. Use dossier.write_section only for the agent-owned sections. All table commands act on contacts.ktable."}),
            ),
            "ping" => Ok(json!({})),
            "tools/list" | "tools/call" => {
                remote(
                    &endpoint,
                    &token,
                    json!({"method":method,"params":input["params"],"workspace":workspace,"session":session}),
                )
                .await
            }
            _ => Err(format!("Unsupported MCP method: {method}")),
        };
        let response = match result {
            Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
            Err(error) if method == "tools/call" => {
                json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":error}],"isError":true}})
            }
            Err(error) => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":error}}),
        };
        output
            .write_all(format!("{response}\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        output.flush().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}
