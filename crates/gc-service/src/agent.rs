//! Tool-free text generation for kit AI fields (`ai/run`). Contact analysis, drafting and list
//! preparation run in the Agent terminal through MCP instead (see terminal.rs and mcp.rs).
//!
//! Generation runs the user's own Claude Code once in print mode with no tools, no MCP servers
//! and no settings files; the app bundles no assistant. Runs are serialized so a table filling
//! many AI fields sends one request at a time.
use crate::app::App;
use serde_json::{Value, json};
use std::{path::PathBuf, process::Stdio, sync::LazyLock, time::Duration};
use tokio::sync::Mutex;

static ONE_AT_A_TIME: LazyLock<Mutex<()>> = LazyLock::new(Mutex::default);
const TIMEOUT: Duration = Duration::from_secs(180);

impl App {
    pub async fn generate(&self, root: &str, request: Value) -> Result<Value, String> {
        self.service(root)?;
        let config = self.config();
        let program = crate::terminal::cli_program("claude", config["terminal"]["claude"].as_str())
            .ok_or("AI fields need Claude Code. Install it and sign in, then try again.")?;
        let prompt = format!("Return only the requested text. No tools or actions. Messages: {}", request["messages"]);
        let _turn = ONE_AT_A_TIME.lock().await;
        // A CLI installed with npm is a Node script: let it find the `node` installed next to it.
        let mut paths = vec![program.parent().map(PathBuf::from).unwrap_or_default(), "/opt/homebrew/bin".into(), "/usr/local/bin".into()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let child = tokio::process::Command::new(&program)
            .args(["-p", "--output-format", "text", "--tools", "", "--strict-mcp-config", "--setting-sources", ""])
            .args(["--no-session-persistence", "--disable-slash-commands"])
            .arg(&prompt)
            // Outside the workspace, so no project instructions or files are picked up.
            .current_dir(std::env::temp_dir())
            .env("PATH", std::env::join_paths(paths).map_err(|e| e.to_string())?)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Couldn't start Claude Code: {e}"))?;
        let output = tokio::time::timeout(TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| "Claude Code didn't answer within 3 minutes.".to_string())?
            .map_err(|e| format!("Claude Code failed: {e}"))?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !output.status.success() || text.is_empty() {
            let detail = String::from_utf8_lossy(&output.stderr);
            let detail: String = detail.trim().chars().rev().take(400).collect::<Vec<_>>().into_iter().rev().collect();
            let detail = if detail.is_empty() { text } else { detail };
            return Err(format!("Claude Code couldn't generate the field. If you haven't yet, run `claude` in Terminal once to sign in. {detail}").trim().to_owned());
        }
        Ok(json!({"text": text}))
    }
}
