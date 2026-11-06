//! Browser test transport: the real Rust workspace/file implementation over JSON lines.
use gc_core::Workspace;
use table_sdk::{KitDocument, TableDocument};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    path::Path,
};

fn request(
    workspace: &mut Option<Workspace>,
    command: &str,
    args: &Value,
) -> Result<Value, String> {
    let text = |key| args[key].as_str().ok_or_else(|| format!("missing {key}"));
    if command == "workspace_open" {
        let path = text("path")?;
        let opened = if args["create"] == true {
            Workspace::create(Path::new(path))
        } else {
            Workspace::open(Path::new(path))
        }
        .map_err(|e| e.to_string())?;
        let result = json!({"root": opened.root()});
        *workspace = Some(opened);
        return Ok(result);
    }
    let workspace = workspace.as_ref().ok_or("workspace not open")?;
    if workspace.root() != Path::new(text("root")?) {
        return Err("wrong workspace".into());
    }
    let path = text("path")?;
    if command == "contact_count" || command == "contact_rows" {
        let file = workspace
            .read(path, None, None)
            .map_err(|e| e.to_string())?;
        let mut doc = TableDocument::open(path, &file.bytes)?;
        let tables = doc.execute("table.tables.list", json!({}))?;
        let table = tables["result"]["items"]
            .as_array()
            .ok_or("tables")?
            .iter()
            .find(|t| t["title"] == "contacts")
            .ok_or("contacts")?;
        let result =
            doc.execute("table.records.list", json!({"tableId": table["id"]}))?["result"].clone();
        return Ok(if command == "contact_count" {
            result["total"].clone()
        } else {
            result
        });
    }
    match command {
        "file_read" => workspace
            .read(
                path,
                args["offset"].as_u64().map(|n| n as usize),
                args["length"].as_u64().map(|n| n as usize),
            )
            .map(|v| json!(v)),
        "file_stamp" => workspace.stamp(path).map(|v| json!(v)),
        "file_write" => {
            let bytes: Vec<u8> =
                serde_json::from_value(args["bytes"].clone()).map_err(|e| e.to_string())?;
            workspace
                .write(path, &bytes, text("mode")?, args["expected"].as_str())
                .map(|v| json!(v))
        }
        _ => return Err(format!("unknown command {command}")),
    }
    .map_err(|e| e.to_string())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = None;
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line?)?;
        let result = request(
            &mut workspace,
            input["command"].as_str().ok_or("command")?,
            &input["args"],
        );
        let output = match result {
            Ok(value) => json!({"id": input["id"], "result": value}),
            Err(error) => json!({"id": input["id"], "error": error}),
        };
        println!("{output}");
        io::stdout().flush()?;
    }
    Ok(())
}
