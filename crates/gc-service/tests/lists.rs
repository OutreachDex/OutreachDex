//! Outreach lists against the GCSP mock, prepared through the Agent's MCP tools. Nothing is sent:
//! every send below must be refused by the outreach guard before any connection is opened.
use gc_service::app::App;
use serde_json::{Value, json};
use std::{path::Path, path::PathBuf, process::Stdio, sync::Arc, time::Duration};

struct Mock(std::process::Child);
impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn mock(port: u16) -> Mock {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gcsp-mock.mjs");
    let child = std::process::Command::new("node")
        .arg(script)
        .args(["--port", &port.to_string(), "--token", "t0ken"])
        .stdout(Stdio::null())
        .spawn()
        .expect("node is required for the GCSP mock");
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Mock(child)
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_select_prepare_and_guard_sending() {
    let _server = mock(18788);
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(
        &dir.path().join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    let root = app.open(&dir.path().join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    let source = app.call(&root, "source.add", json!({"url":"http://127.0.0.1:18788","token":"t0ken"})).await.unwrap();
    app.call(&root, "source.sync", json!({"id":source["id"]})).await.unwrap();

    // Free plan, no credits left, ran out recently enough, never bought, not yet contacted.
    let saved = app
        .call(
            &root,
            "list.save",
            json!({"list":{
                "name":"Free credits used up, not paid",
                "description":"Find out where they got stuck and whether they need help",
                "source":"thumbnailsnap-mock",
                "channel":"email",
                "filters":[
                    {"field":"attr.plan","op":"eq","value":"free"},
                    {"field":"attr.credits_balance","op":"lte","value":0},
                    {"field":"event.credits_exhausted","op":"within_days","value":3650},
                    {"field":"no_event.purchase","op":"is"},
                    {"field":"last_contacted","op":"never"}
                ]}}),
        )
        .await
        .unwrap();
    let id = saved["id"].as_str().unwrap().to_owned();
    assert!(std::path::Path::new(&root).join(format!("lists/{id}.toml")).is_file());
    let list = app.call(&root, "list.get", json!({"id":id})).await.unwrap();
    let members = list["members"].as_array().unwrap().clone();
    assert!(!members.is_empty(), "{list}");
    for m in &members {
        assert_eq!(m["attributes"]["plan"], "free");
        assert_eq!(m["status"], "new");
        assert_eq!(m["evidence"].as_array().unwrap().len(), 5);
    }
    let all = app.call(&root, "list.all", json!({})).await.unwrap();
    assert_eq!(all[0]["counts"]["matched"], members.len());

    // Prepare two people: the app only queues them; the Agent does the work through MCP.
    let keys: Vec<Value> = members.iter().take(2).map(|m| m["key"].clone()).collect();
    assert_eq!(app.call(&root, "list.prepare", json!({"id":id,"members":keys})).await.unwrap()["queued"], 2);
    let mcp = |name: &str, arguments: Value| app.mcp(json!({"workspace":root,"method":"tools/call","params":{"name":name,"arguments":arguments}}));
    let seen = mcp("list.get", json!({"id":id})).await.unwrap();
    let queued = seen["structuredContent"]["members"].as_array().unwrap().iter().filter(|m| m["status"] == "queued").count();
    assert_eq!(queued, 2);
    for key in &keys {
        // An unlinked source person becomes a contact first; the draft is tagged with the list.
        let saved = mcp("list.draft_save", json!({"list":id,"member":key,"subject":"Out of credits?","body":"I saw you used up your credits and wanted to ask where you got stuck."})).await.unwrap();
        let path = saved["structuredContent"]["path"].as_str().unwrap().to_owned();
        let note = app.service(&root).unwrap().read_note(&path).await.unwrap();
        assert!(note.contains(&id), "{note}");
    }
    let after = app.call(&root, "list.get", json!({"id":id})).await.unwrap();
    let prepared: Vec<&Value> = after["members"].as_array().unwrap().iter().filter(|m| keys.contains(&m["key"])).collect();
    assert!(prepared.iter().all(|m| m["status"] == "ready" && m["draft_status"] == "draft"), "{after}");
    assert!(prepared.iter().all(|m| m["contact"].as_str().is_some_and(|c| !c.is_empty())));
    // Every change for a person lands in their Agent memory.
    let memory = app.call(&root, "memory.get", json!({"slug":prepared[0]["contact"]})).await.unwrap();
    assert!(memory["text"].as_str().unwrap().contains("Saved draft for review"), "{memory}");
    if let Some(third) = members.get(2) {
        mcp("list.decline", json!({"list":id,"member":third["key"],"reason":"Just signed up, hasn't used it yet"})).await.unwrap();
        let list = app.call(&root, "list.get", json!({"id":id})).await.unwrap();
        let declined = list["members"].as_array().unwrap().iter().find(|m| m["key"] == third["key"]).unwrap().clone();
        assert_eq!(declined["status"], "no_draft");
        assert_eq!(declined["error"], "Just signed up, hasn't used it yet");
        assert_eq!(app.call(&root, "list.prepare", json!({"id":id,"members":[third["key"]]})).await.unwrap()["queued"], 1, "no_draft may retry");
    }

    // A list draft for the first prepared person, approved by a person.
    let slug = prepared[0]["contact"].as_str().unwrap().to_owned();
    let draft = app
        .call(&root, "draft.save", json!({"contact_id":slug,"channel":"email","subject":"Hello","body":"Hi"}))
        .await
        .unwrap();
    let path = draft["path"].as_str().unwrap().to_owned();
    app.service(&root).unwrap().docs.execute(&path, "property.set", json!({"key":"list","value":id}), "system:lists").await.unwrap();
    // Editing keeps the host-managed list marker.
    let edited = app
        .call(&root, "draft.save", json!({"contact_id":slug,"channel":"email","subject":"Hello","body":"Hi, edited","path":path}))
        .await
        .unwrap();
    assert_eq!(edited["metadata"]["list"], id.as_str());
    app.call(&root, "draft.status", json!({"path":path,"status":"approved"})).await.unwrap();

    // Do not contact: the address joins suppression.txt and sending is refused before SMTP.
    let suppressed = app.call(&root, "list.suppress", json!({"id":id,"member":prepared[0]["key"]})).await.unwrap();
    assert_eq!(suppressed["status"], "suppressed");
    assert!(std::fs::read_to_string(Path::new(&root).join("suppression.txt")).unwrap().contains('@'));
    let refused = app.call(&root, "draft.send", json!({"path":path,"connection":"none"})).await.unwrap_err();
    assert!(refused.contains("do-not-contact") || refused.contains("Do not contact") || refused.contains("approve"), "{refused}");
    let list = app.call(&root, "list.get", json!({"id":id})).await.unwrap();
    assert!(!list["members"].as_array().unwrap().iter().any(|m| m["key"] == prepared[0]["key"] && m["status"] == "new"), "suppressed people never match again");

    // Daily limit 0: a manual-channel list message cannot be marked sent.
    let config_path = Path::new(&root).join("globalcontact.toml");
    let text = std::fs::read_to_string(&config_path).unwrap().replace("daily_limit = 20", "daily_limit = 0");
    std::fs::write(&config_path, text).unwrap();
    let other = prepared[1]["contact"].as_str().unwrap().to_owned();
    let wechat = app
        .call(&root, "draft.save", json!({"contact_id":other,"channel":"wechat","subject":"","body":"Hi"}))
        .await
        .unwrap();
    let wechat_path = wechat["path"].as_str().unwrap().to_owned();
    app.service(&root).unwrap().docs.execute(&wechat_path, "property.set", json!({"key":"list","value":id}), "system:lists").await.unwrap();
    app.call(&root, "draft.status", json!({"path":wechat_path,"status":"approved"})).await.unwrap();
    let limited = app.call(&root, "draft.mark_sent", json!({"path":wechat_path})).await.unwrap_err();
    assert!(limited.contains("daily limit"), "{limited}");

    app.call(&root, "list.skip", json!({"id":id,"member":members.last().unwrap()["key"]})).await.unwrap();
    app.call(&root, "list.delete", json!({"id":id})).await.unwrap();
    assert_eq!(app.call(&root, "list.all", json!({})).await.unwrap().as_array().unwrap().len(), 0);
}
