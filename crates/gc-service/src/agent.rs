//! Tool-free text generation for kit AI fields (`ai/run`). Contact analysis, drafting and list
//! preparation run in the Agent terminal through MCP instead (see terminal.rs and mcp.rs).
use crate::app::App;
use serde_json::{Value, json};

impl App {
    pub async fn generate(&self, root: &str, request: Value) -> Result<Value, String> {
        self.service(root)?;
        // Claude ACP documents tools:[] in session metadata. Codex ACP does not currently
        // provide an equivalent tool-free session contract, so use this provider explicitly.
        let config = self.config();
        let program = config["claude"]["program"]
            .as_str()
            .ok_or("Claude ACP is required for AI fields")?;
        if crate::app::find_program(program).is_none() {
            return Err("AI fields require Claude ACP to be installed and signed in.".into());
        }
        let output = self
            .agents
            .run(
                gc_agent::Request {
                    id: uuid::Uuid::new_v4().to_string(),
                    program: program.into(),
                    args: config["claude"]["args"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default(),
                    workspace: root.into(),
                    prompt: format!(
                        "Return only the requested text. No tools or actions. Messages: {}",
                        request["messages"]
                    ),
                    mcp: json!([]),
                    generation: true,
                },
                self.notify.clone(),
            )
            .await?;
        Ok(json!({"text":output}))
    }
}
