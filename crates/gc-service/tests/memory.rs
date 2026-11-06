//! Agent memory: what the Agent learns and changes for a person accumulates in memory/<slug>.md
//! through MCP, survives merges, and is readable by later sessions.
use gc_service::app::App;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

#[tokio::test]
async fn agent_memory_accumulates_through_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(
        &dir.path().join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    let root = app.open(&dir.path().join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    let slug = app.call(&root, "contact.create", json!({"Name":"User One","Email":"one@example.com"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    let other = app.call(&root, "contact.create", json!({"Name":"User One (work)"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    let mcp = async |name: &str, arguments: Value| app.mcp(json!({"workspace":root,"method":"tools/call","params":{"name":name,"arguments":arguments}})).await;

    let tools = app.mcp(json!({"workspace":root,"method":"tools/list"})).await.unwrap();
    let names: Vec<&str> = tools["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    for name in ["workspace.guide", "memory.read", "memory.append", "contact.follow_up", "list.get", "list.draft_save", "list.decline", "preferences.add"] {
        assert!(names.contains(&name), "{name} missing");
    }

    // Empty at first; the Agent records a fact and a commitment.
    assert_eq!(mcp("memory.read", json!({"slug":slug})).await.unwrap()["structuredContent"]["text"], "");
    mcp("memory.append", json!({"slug":slug,"kind":"fact","note":"Stuck exporting to PDF (source mail/2026/a.md)"})).await.unwrap();
    mcp("memory.append", json!({"slug":slug,"kind":"commitment","note":"Promised a fix by Friday"})).await.unwrap();
    assert!(mcp("memory.append", json!({"slug":slug,"kind":"gossip","note":"x"})).await.is_err());
    assert!(mcp("memory.append", json!({"slug":slug,"kind":"fact","note":"  "})).await.is_err());

    // Changes through MCP are logged automatically.
    mcp("contact.update", json!({"slug":slug,"values":{"Next action":"Answer the export question by Friday"}})).await.unwrap();
    let follow = mcp("contact.follow_up", json!({"slug":slug,"date":"2026-11-07"})).await.unwrap();
    assert_eq!(follow["structuredContent"]["follow_up"], "2026-11-07");
    assert_eq!(app.service(&root).unwrap().contact(&slug).await.unwrap().text("Follow up"), "2026-11-07");
    let text = app.call(&root, "memory.get", json!({"slug":slug})).await.unwrap()["text"].as_str().unwrap().to_owned();
    for expected in ["# Agent log · User One", "Fact · Agent", "Stuck exporting to PDF", "Commitment · Agent", "Change · Agent", "Next action=Answer the export question by Friday", "Set follow-up date 2026-11-07"] {
        assert!(text.contains(expected), "{expected} not in {text}");
    }
    let order = |s: &str| text.find(s).unwrap();
    assert!(order("Stuck exporting") < order("Promised a fix") && order("Promised a fix") < order("Next action="), "append-only, in order");
    // The memory is a readable workspace document, but not writable by generic tools.
    let read = mcp("document.read", json!({"path":format!("memory/{slug}.md")})).await.unwrap();
    assert!(read["structuredContent"]["text"].as_str().unwrap().contains("Stuck exporting"));
    assert!(mcp("document.replace", json!({"path":format!("memory/{slug}.md"),"expected":"Stuck","text":"x"})).await.is_err());

    // Merging keeps the duplicate's memory under the surviving contact.
    mcp("memory.append", json!({"slug":other,"kind":"preference","note":"Prefers WeChat"})).await.unwrap();
    app.call(&root, "contact.merge", json!({"winner":slug,"loser":other})).await.unwrap();
    let merged = app.call(&root, "memory.get", json!({"slug":slug})).await.unwrap()["text"].as_str().unwrap().to_owned();
    assert!(merged.contains("Prefers WeChat") && merged.contains("Stuck exporting"), "{merged}");
    assert!(!Path::new(&root).join(format!("memory/{other}.md")).exists());
}
