//! Agent ↔ app communication: MCP calls are attributed to their terminal session and pushed to the
//! UI as activity; task.report carries the Agent's status; ui.open asks the UI to show a result;
//! context.get includes what the user has selected.
use gc_service::app::App;
use portable_pty::{CommandBuilder, PtySize};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[tokio::test(flavor = "multi_thread")]
async fn activity_status_open_and_context() {
    let dir = tempfile::tempdir().unwrap();
    let events: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
    let sink = events.clone();
    let app = App::new(
        &dir.path().join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(move |event: &str, value: Value| sink.lock().unwrap().push((event.to_owned(), value))),
    )
    .unwrap();
    let root = app.open(&dir.path().join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    let slug = app.call(&root, "contact.create", json!({"Name":"Activity Person"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    let other = app.call(&root, "contact.create", json!({"Name":"Second Person"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    // A real terminal row, so its task status shows in terminal.list.
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", "read -r line"]);
    cmd.cwd(&root);
    let session = app
        .terminals
        .spawn(app.db.clone(), &root, "fixture", "session-one".into(), cmd, PtySize { cols: 80, rows: 24, pixel_width: 0, pixel_height: 0 })
        .unwrap();
    assert_eq!(session, "session-one");
    let mcp = async |session: &str, name: &str, arguments: Value| {
        app.mcp(json!({"workspace":root,"session":session,"method":"tools/call","params":{"name":name,"arguments":arguments}})).await
    };
    let sent = |name: &str| events.lock().unwrap().iter().filter(|(e, _)| e == name).map(|(_, v)| v.clone()).collect::<Vec<_>>();

    // Calls are recorded per session, with the person they concern; refusals too.
    mcp(&session, "contact.get", json!({"slug":slug})).await.unwrap();
    mcp(&session, "memory.append", json!({"slug":slug,"kind":"fact","note":"Testing with Activity"})).await.unwrap();
    assert!(mcp(&session, "memory.append", json!({"slug":slug,"kind":"nope","note":"x"})).await.is_err());
    mcp("other-session", "contact.get", json!({"slug":other})).await.unwrap();
    let mine = app.call(&root, "agent.activity", json!({"session":session})).await.unwrap();
    let mine = mine.as_array().unwrap();
    assert_eq!(mine.len(), 3, "{mine:?}");
    assert!(mine.iter().all(|a| a["contact"] == slug.as_str()));
    assert_eq!(mine[1]["detail"], "Testing with Activity");
    assert_eq!(mine[2]["ok"], false);
    assert!(!mine[2]["detail"].as_str().unwrap().is_empty(), "the refusal reason is kept");
    assert_eq!(app.call(&root, "agent.activity", json!({})).await.unwrap().as_array().unwrap().len(), 4);
    assert!(sent("agent-activity").iter().any(|a| a["session"] == "other-session" && a["contact"] == other.as_str()));

    // task.report: validated, pushed with progress, and shown on the session.
    assert!(mcp(&session, "task.report", json!({"status":"finished","summary":"x"})).await.is_err());
    assert!(mcp(&session, "task.report", json!({"status":"done","summary":"  "})).await.is_err());
    mcp(&session, "task.report", json!({"status":"working","summary":"Preparing the list","done":1,"total":3})).await.unwrap();
    let progress = sent("agent-activity").into_iter().rfind(|a| a["tool"] == "task.report" && a["ok"] == true).unwrap();
    assert_eq!(progress["progress"], json!({"done":1,"total":3}));
    mcp(&session, "task.report", json!({"status":"needs_input","summary":"Whom first?"})).await.unwrap();
    let terminals = app.call(&root, "terminal.list", json!({})).await.unwrap();
    let row = terminals.as_array().unwrap().iter().find(|t| t["id"] == session.as_str()).unwrap().clone();
    assert_eq!(row["task"]["status"], "needs_input");
    assert_eq!(row["task"]["summary"], "Whom first?");

    // ui.open: only existing things, then an agent-open event for the UI.
    assert!(mcp(&session, "ui.open", json!({"kind":"draft","target":"drafts/missing.md"})).await.is_err());
    assert!(mcp(&session, "ui.open", json!({"kind":"document","target":"../secret.md"})).await.is_err());
    assert!(mcp(&session, "ui.open", json!({"kind":"contact","target":"nobody"})).await.is_err());
    let draft = mcp(&session, "draft.save", json!({"contact_id":slug,"channel":"email","subject":"Hi","body":"Hello"})).await.unwrap();
    let path = draft["structuredContent"]["path"].as_str().unwrap().to_owned();
    mcp(&session, "ui.open", json!({"kind":"draft","target":path})).await.unwrap();
    mcp(&session, "ui.open", json!({"kind":"contact","target":slug})).await.unwrap();
    let opened = sent("agent-open");
    assert_eq!(opened.len(), 2);
    assert_eq!(opened[0]["kind"], "draft");
    assert_eq!(opened[0]["target"], path.as_str());
    assert_eq!(opened[0]["session"], session.as_str());

    // context.get: page, list and the people picked in bulk, with names.
    app.call(&root, "context.set", json!({"contact":null,"document":null,"view":"people","selection":[slug, other]})).await.unwrap();
    let context = mcp(&session, "context.get", json!({})).await.unwrap()["structuredContent"].clone();
    assert_eq!(context["view"], "people");
    assert_eq!(context["selection_count"], 2);
    assert_eq!(context["selection"][1]["name"], "Second Person");

    // The latest analysis is readable on the left with when and by which session it was written.
    assert_eq!(app.call(&root, "agent.analysis", json!({"slug":slug})).await.unwrap()["text"], "");
    mcp(&session, "dossier.write_section", json!({"slug":slug,"section":"analysis","content":"## Conclusion\nCares about the import format.\n\n## Evidence\n- Interaction history [source](people/x.md)\n\n## Open questions\n- Budget"})).await.unwrap();
    let analysis = app.call(&root, "agent.analysis", json!({"slug":slug})).await.unwrap();
    assert!(analysis["text"].as_str().unwrap().starts_with("## Conclusion"), "{analysis}");
    assert!(!analysis["text"].as_str().unwrap().contains("agent:analysis"), "markers are not shown");
    assert_eq!(analysis["session"], session.as_str());
    assert!(analysis["at"].as_i64().unwrap() > 0);

    // Today lists each session's latest outcome with the person it worked on.
    mcp(&session, "task.report", json!({"status":"done","summary":"Understood Activity Person"})).await.unwrap();
    mcp("other-session", "task.report", json!({"status":"working","summary":"still going"})).await.unwrap();
    let recent = app.call(&root, "agent.recent", json!({})).await.unwrap();
    let recent = recent.as_array().unwrap();
    assert_eq!(recent.len(), 1, "working sessions are not outcomes yet: {recent:?}");
    assert_eq!(recent[0]["summary"], "Understood Activity Person");
    assert_eq!(recent[0]["contact"], slug.as_str());
    app.call(&root, "terminal.stop", json!({"id":session})).await.unwrap();
}
