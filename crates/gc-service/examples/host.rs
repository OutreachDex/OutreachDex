//! Local integration harness: real application services, JSON-lines transport instead of Tauri.
use gc_service::app::App;
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncBufReadExt, BufReader};
#[tokio::main]
async fn main() {
    let data = tempfile::tempdir().unwrap();
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let events = output.clone();
    let app = App::new(
        data.path(),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::var_os("GC_TEST_DESKTOP_BINARY")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::current_exe().unwrap()),
        Arc::new(move |event, value| {
            let mut out = events.lock().unwrap();
            writeln!(out, "{}", json!({"event":event,"payload":value})).unwrap();
            out.flush().unwrap();
        }),
    )
    .unwrap();
    app.start_bridge().await.unwrap();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await.unwrap() {
        let input: Value = serde_json::from_str(&line).unwrap();
        let app = app.clone();
        let output = output.clone();
        tokio::spawn(async move {
            let result = dispatch(&app, &input).await;
            let response = match result {
                Ok(result) => json!({"id":input["id"],"result":result}),
                Err(error) => json!({"id":input["id"],"error":error}),
            };
            let mut out = output.lock().unwrap();
            writeln!(out, "{response}").unwrap();
            out.flush().unwrap();
        });
    }
}
async fn dispatch(app: &App, input: &Value) -> Result<Value, String> {
    let args = &input["args"];
    let text = |key: &str| args[key].as_str().unwrap_or("");
    let root = text("root");
    match input["command"].as_str().unwrap_or("") {
        "workspace_open" => app.open_with(Path::new(text("path")), args["create"] == true, args["template"].as_str()),
        "app_call" => app.call(root, text("action"), args["args"].clone()).await,
        "file_read" => Ok(json!(
            app.service(root)?
                .docs
                .workspace
                .read(
                    text("path"),
                    args["offset"].as_u64().map(|v| v as usize),
                    args["length"].as_u64().map(|v| v as usize)
                )
                .map_err(|e| e.to_string())?
        )),
        "file_stamp" => Ok(json!(
            app.service(root)?
                .docs
                .workspace
                .stamp(text("path"))
                .map_err(|e| e.to_string())?
        )),
        "file_write" => {
            let result = app
                .service(root)?
                .docs
                .workspace
                .write(
                    text("path"),
                    &serde_json::from_value::<Vec<u8>>(args["bytes"].clone())
                        .map_err(|e| e.to_string())?,
                    text("mode"),
                    args["expected"].as_str(),
                )
                .map_err(|e| e.to_string())?;
            (app.notify)("workspace-changed", json!({"root":root}));
            Ok(json!(result))
        }
        "document_register" => {
            app.service(root)?
                .docs
                .register(text("path"), text("session"))
                .await?;
            Ok(Value::Null)
        }
        "document_unregister" => {
            app.service(root)?
                .docs
                .unregister(text("path"), text("session"));
            Ok(Value::Null)
        }
        "document_reply" => {
            app.service(root)?.docs.reply(
                text("id"),
                text("session"),
                if let Some(e) = args["error"].as_str() {
                    Err(e.into())
                } else {
                    Ok(args["result"].clone())
                },
            )?;
            Ok(Value::Null)
        }
        "mcp" => app.mcp(args.clone()).await,
        "generate" => Ok(json!({"text":"A fixture-generated summary."})),
        _ => Err("Unknown harness command".into()),
    }
}
