//! Saved information: custom contact fields, source attribute history, writing preferences.
use gc_service::app::App;
use serde_json::json;
use std::{path::Path, path::PathBuf, process::Stdio, sync::Arc, time::Duration};

struct Mock(std::process::Child);
impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn custom_fields_history_and_preferences_are_saved() {
    let port = 18789;
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gcsp-mock.mjs");
    let _mock = Mock(
        std::process::Command::new("node")
            .arg(script)
            .args(["--port", &port.to_string(), "--token", "t"])
            .stdout(Stdio::null())
            .spawn()
            .expect("node is required"),
    );
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(
        &dir.path().join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    let root = app.open(&dir.path().join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    let slug = app.call(&root, "contact.create", json!({"Name":"Nora Field","Email":"nora.field@university.example"})).await.unwrap()["slug"].as_str().unwrap().to_owned();

    // Custom profile fields: add, fill, extend a select with a new choice, survive reopening.
    app.call(&root, "contact.field_add", json!({"title":"Phone","type":"text"})).await.unwrap();
    app.call(&root, "contact.field_add", json!({"title":"Relationship","type":"single_select","choices":["Friend"]})).await.unwrap();
    assert!(app.call(&root, "contact.field_add", json!({"title":"Phone","type":"text"})).await.unwrap_err().contains("already exists"));
    assert!(app.call(&root, "contact.field_add", json!({"title":"x","type":"formula"})).await.is_err());
    app.call(&root, "contact.field_choice", json!({"title":"Relationship","choice":"Client"})).await.unwrap();
    app.call(&root, "contact.update", json!({"slug":slug,"values":{"Phone":"+86 138 0000 0000","Relationship":"Client"}})).await.unwrap();
    let fields = app.call(&root, "contact.fields", json!({})).await.unwrap();
    let relation = fields.as_array().unwrap().iter().find(|f| f["title"] == "Relationship").unwrap();
    assert!(relation["choices"].as_array().unwrap().iter().any(|c| c == "Client"));
    assert!(fields.as_array().unwrap().iter().any(|f| f["title"] == "Slug" && f["system"] == true));
    let reopened = App::new(&dir.path().join("data2"), Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"), std::env::current_exe().unwrap(), Arc::new(|_, _| {})).unwrap();
    drop(app);
    let root2 = reopened.open(Path::new(&root), false).unwrap()["root"].as_str().unwrap().to_owned();
    let detail = reopened.call(&root2, "contact.get", json!({"slug":slug})).await.unwrap();
    assert_eq!(detail["contact"]["values"]["Phone"], "+86 138 0000 0000");
    assert_eq!(detail["contact"]["values"]["Relationship"], "Client");
    let app = reopened;
    let root = root2;

    // Source history: a change between syncs is recorded per person, with labels.
    let source = app.call(&root, "source.add", json!({"url":format!("http://127.0.0.1:{port}"),"token":"t"})).await.unwrap();
    app.call(&root, "source.sync", json!({"id":source["id"]})).await.unwrap();
    let person_file = Path::new(&root).join("sources/thumbnailsnap-mock/people/u_1000.md");
    assert!(std::fs::read_to_string(&person_file).unwrap().contains("First sync"));
    // The mock bumps last_active_at for this person, as a real backend would on activity.
    let touched = std::process::Command::new("curl")
        .args(["-s", "-X", "POST", &format!("http://127.0.0.1:{port}/_mock/touch?id=u_1000")])
        .output()
        .unwrap();
    assert!(touched.status.success());
    app.call(&root, "source.sync", json!({"id":source["id"]})).await.unwrap();
    let text = std::fs::read_to_string(&person_file).unwrap();
    assert!(text.contains("last_active_at: "), "{text}");
    let facts = app.call(&root, "source.person", json!({"slug":slug})).await.unwrap();
    let history = facts[0]["history"].as_array().unwrap();
    assert!(history.iter().any(|h| h["key"] == "last_active_at" && h["label"] == "Last active"), "{history:?}");

    // Bulk edits, CSV export and field history.
    let other = app.call(&root, "contact.create", json!({"Name":"Second"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    app.call(&root, "contact.bulk_update", json!({"slugs":[slug, other],"values":{"Stage":"In progress"}})).await.unwrap();
    assert!(app.call(&root, "contact.bulk_update", json!({"slugs":[slug],"values":{"Stage":"No such stage"}})).await.is_err());
    app.call(&root, "contact.bulk_follow_up", json!({"slugs":[slug, other],"days":3})).await.unwrap();
    let service = app.service(&root).unwrap();
    assert_eq!(service.contact(&other).await.unwrap().text("Stage"), "In progress");
    assert!(!service.contact(&other).await.unwrap().text("Follow up").is_empty());
    let csv_path = dir.path().join("export.csv");
    let exported = app.call(&root, "contacts.export", json!({"path":csv_path,"slugs":[slug]})).await.unwrap();
    assert_eq!(exported["exported"], 1);
    let csv = std::fs::read_to_string(&csv_path).unwrap();
    assert!(csv.starts_with('\u{feff}') && csv.contains("Phone") && csv.contains("+86 138 0000 0000") && !csv.contains("Second"));
    let history = app.call(&root, "contact.history", json!({"slug":slug})).await.unwrap();
    let items = history["items"].as_array().unwrap();
    assert!(items.iter().any(|h| h["field_title"] == "Stage" && h["after"] == "In progress"), "{items:?}");

    // Writing preferences: saved to preferences.md and given to the Agent with the workspace guide.
    app.call(&root, "preferences.add", json!({"note":"Skip the small talk; state the purpose up front"})).await.unwrap();
    let prefs = app.call(&root, "preferences.get", json!({})).await.unwrap();
    assert!(prefs["text"].as_str().unwrap().contains("state the purpose up front"));
    let guide = app.mcp(json!({"workspace":root,"method":"tools/call","params":{"name":"workspace.guide","arguments":{}}})).await.unwrap();
    let guide = &guide["structuredContent"];
    assert!(guide["writing_preferences"].as_str().unwrap().contains("state the purpose up front"), "preferences reach the agent");
    assert!(guide["playbooks"].as_array().unwrap().iter().any(|p| p["kind"] == "purpose" && !p["instructions"].as_str().unwrap().is_empty()));
    assert!(guide["fields"].as_array().unwrap().iter().any(|f| f["title"] == "Phone"));
}
