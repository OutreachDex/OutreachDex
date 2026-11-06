//! GCSP/1 sync against the reference mock server (scripts/gcsp-mock.mjs). Needs `node` on PATH.
use gc_service::app::App;
use serde_json::{Value, json};
use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

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
        .args(["--port", &port.to_string(), "--token", "t0ken", "--page-size", "7"])
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

fn count(people: &Value, state: &str) -> usize {
    people.as_array().unwrap().iter().filter(|p| p["match"] == state).count()
}

#[tokio::test(flavor = "multi_thread")]
async fn syncs_people_events_and_identity_decisions() {
    let _server = mock(18787);
    let base = "http://127.0.0.1:18787";
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(&dir.path().join("data"), PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"), PathBuf::from("globalcontact"), Arc::new(|_, _| {})).unwrap();
    let root = app.open(&dir.path().join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    // One contact matches a verified source email, another an unverified one.
    let verified = app.call(&root, "contact.create", json!({"Name":"Nora Field","Email":"nora.field@university.example"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    let unverified = app.call(&root, "contact.create", json!({"Name":"Leo Grant","Email":"leo.grant@mail.example"})).await.unwrap()["slug"].as_str().unwrap().to_owned();

    let report = app.call(&root, "source.check", json!({"url":base,"token":"t0ken"})).await.unwrap();
    assert_eq!(report["passed"], true, "{report}");
    assert!(app.call(&root, "source.add", json!({"url":base,"token":"wrong"})).await.unwrap_err().contains("credentials"));
    let id = app.call(&root, "source.add", json!({"url":base,"token":"t0ken"})).await.unwrap()["id"].as_str().unwrap().to_owned();
    assert!(app.call(&root, "source.add", json!({"url":base,"token":"t0ken"})).await.is_err(), "same source twice");

    let first = app.call(&root, "source.sync", json!({"id":id})).await.unwrap();
    assert_eq!(first["people"], 23, "{first}");
    assert_eq!(first["events"], 74, "{first}");
    let people = app.call(&root, "source.people", json!({"source":"thumbnailsnap-mock"})).await.unwrap();
    assert_eq!(people.as_array().unwrap().len(), 22, "the tombstone is not listed");
    assert_eq!(count(&people, "auto"), 1);
    assert_eq!(count(&people, "pending"), 1);
    assert_eq!(count(&people, "unlinked"), 20);
    let auto = people.as_array().unwrap().iter().find(|p| p["match"] == "auto").unwrap();
    assert_eq!(auto["contact"], verified.as_str());
    assert!(auto["attributes"].get("internal_score").is_none(), "undeclared attributes are dropped");
    assert_eq!(auto["attributes"]["plan"], "creator");

    // The linked contact sees source facts and timeline entries; evidence files exist.
    let facts = app.call(&root, "source.person", json!({"slug":verified})).await.unwrap();
    assert!(facts[0]["facts"].as_array().unwrap().iter().any(|f| f["label"] == "Plan"));
    let detail = app.call(&root, "contact.get", json!({"slug":verified})).await.unwrap();
    let timeline = detail["timeline"].as_array().unwrap();
    assert!(timeline.iter().any(|r| r["values"]["Title"].as_str().unwrap_or("").contains("Signed up")), "{timeline:?}");
    let evidence = std::path::Path::new(&root).join("sources/thumbnailsnap-mock");
    assert!(evidence.is_dir());

    // Incremental: nothing new, nothing duplicated.
    let second = app.call(&root, "source.sync", json!({"id":id})).await.unwrap();
    assert_eq!(second["people"], 0, "{second}");
    let again = app.call(&root, "contact.get", json!({"slug":verified})).await.unwrap();
    assert_eq!(again["timeline"].as_array().unwrap().len(), timeline.len());

    // Decisions: reject the suggestion, create a contact for an unlinked person.
    let pending = people.as_array().unwrap().iter().find(|p| p["match"] == "pending").unwrap();
    assert_eq!(pending["contact"], unverified.as_str());
    app.call(&root, "identity.decide", json!({"source":"thumbnailsnap-mock","external_id":pending["external_id"],"action":"reject"})).await.unwrap();
    let unlinked = people.as_array().unwrap().iter().find(|p| p["match"] == "unlinked").unwrap();
    let created = app.call(&root, "identity.decide", json!({"source":"thumbnailsnap-mock","external_id":unlinked["external_id"],"action":"create"})).await.unwrap();
    assert!(created["events"].as_u64().unwrap() >= 2, "backfilled events: {created}");
    let slug = created["contact"].as_str().unwrap();
    let new_contact = app.call(&root, "contact.get", json!({"slug":slug})).await.unwrap();
    assert!(new_contact["channels"].as_array().unwrap().iter().any(|c| c["values"]["Kind"] == "email"));

    // A full resync keeps decisions.
    app.call(&root, "source.reset", json!({"id":id})).await.unwrap();
    app.call(&root, "source.sync", json!({"id":id})).await.unwrap();
    let after = app.call(&root, "source.people", json!({"source":"thumbnailsnap-mock"})).await.unwrap();
    assert_eq!(count(&after, "rejected"), 1);
    assert_eq!(count(&after, "confirmed"), 1);
    assert_eq!(after.as_array().unwrap().len(), 22);

    let list = app.call(&root, "source.list", json!({})).await.unwrap();
    assert_eq!(list[0]["counts"]["linked"], 2);
    assert_eq!(list[0]["last_run"]["status"], "completed");
    // The Agent can understand people who are not contacts yet: search, facts, history, events.
    let mcp = |name: &str, arguments: serde_json::Value| app.mcp(json!({"workspace":root,"method":"tools/call","params":{"name":name,"arguments":arguments}}));
    let found = mcp("source.people", json!({"match":"unlinked","query":"free","limit":3})).await.unwrap()["structuredContent"].clone();
    assert!(found["total"].as_u64().unwrap() > 3, "{found}");
    assert_eq!(found["items"].as_array().unwrap().len(), 3);
    assert!(found["items"].as_array().unwrap().iter().all(|p| p["match"] == "unlinked" && p["attributes"]["plan"] == "free"));
    let someone = found["items"][0]["external_id"].as_str().unwrap().to_owned();
    let detail = mcp("source.person", json!({"source":"thumbnailsnap-mock","external_id":someone})).await.unwrap()["structuredContent"].clone();
    assert!(detail["facts"].as_array().unwrap().iter().any(|f| f["label"] == "Plan"), "{detail}");
    let events = detail["events"].as_array().unwrap();
    assert!(!events.is_empty() && events.iter().all(|e| e["path"].as_str().unwrap().starts_with("sources/thumbnailsnap-mock/")));
    assert!(events.windows(2).all(|w| w[0]["occurred_at"].as_str() >= w[1]["occurred_at"].as_str()), "newest first");
    assert!(events.iter().any(|e| e["label"].is_string()), "event labels from the manifest");
    let evidence_text = mcp("document.read", json!({"path":events[0]["path"]})).await.unwrap();
    assert!(evidence_text["structuredContent"]["text"].as_str().unwrap().contains(&someone));
    assert!(mcp("source.person", json!({"source":"thumbnailsnap-mock","external_id":"nobody"})).await.is_err());
    assert_eq!(app.call(&root, "source.identity", json!({"source":"thumbnailsnap-mock","external_id":someone})).await.unwrap()["external_id"], someone.as_str());

    // New contacts from the source get its language: the person's own locale first, then the
    // source's setting, then the manifest's default_locale ("en" for the mock).
    let unlinked_now = app.call(&root, "source.people", json!({"source":"thumbnailsnap-mock","match":"unlinked"})).await.unwrap();
    let pick = |name: &str| unlinked_now.as_array().unwrap().iter().find(|p| p["name"] == name).map(|p| p["external_id"].as_str().unwrap().to_owned());
    let language_of = |slug: String| {
        let app = app.clone();
        let root = root.clone();
        async move { app.service(&root).unwrap().contact(&slug).await.unwrap().text("Language").to_owned() }
    };
    let decide = |external: String| app.call(&root, "identity.decide", json!({"source":"thumbnailsnap-mock","external_id":external,"action":"create"}));
    let sources_now = app.call(&root, "source.list", json!({})).await.unwrap();
    assert_eq!(sources_now[0]["language"], "English", "{}", sources_now[0]);
    let english = decide(pick("Sato Kenji").unwrap()).await.unwrap()["contact"].as_str().unwrap().to_owned();
    assert_eq!(language_of(english).await, "English");
    let chinese = decide(pick("Zhou Yining").unwrap()).await.unwrap()["contact"].as_str().unwrap().to_owned();
    assert_eq!(language_of(chinese).await, "Chinese", "the person's own locale wins");
    app.call(&root, "source.language", json!({"source":"thumbnailsnap-mock","language":"Chinese"})).await.unwrap();
    let set = decide(pick("Rita Gomez").unwrap()).await.unwrap()["contact"].as_str().unwrap().to_owned();
    assert_eq!(language_of(set).await, "Chinese", "the source setting overrides the manifest default");
    // Older versions sent and stored the legacy Chinese label; it means the same setting.
    app.call(&root, "source.language", json!({"source":"thumbnailsnap-mock","language":"中文"})).await.unwrap();
    let listed = app.call(&root, "source.list", json!({})).await.unwrap();
    assert_eq!((listed[0]["language"].as_str(), listed[0]["language_setting"].as_str()), (Some("Chinese"), Some("Chinese")), "{}", listed[0]);
    app.call(&root, "source.language", json!({"source":"thumbnailsnap-mock","language":"auto"})).await.unwrap();
    assert_eq!(app.call(&root, "source.list", json!({})).await.unwrap()[0]["language"], "English");

    // The person opts out at the source: their stored data is purged, their address joins the
    // do-not-contact list and the linked contact moves to the do-not-contact stage.
    let opted = auto["external_id"].as_str().unwrap().to_owned();
    let person_file = std::path::Path::new(&root).join(format!("sources/thumbnailsnap-mock/people/{opted}.md"));
    assert!(person_file.is_file());
    let optout = std::process::Command::new("curl").args(["-s", "-X", "POST", &format!("{base}/_mock/optout?id={opted}")]).output().unwrap();
    assert!(optout.status.success());
    let third = app.call(&root, "source.sync", json!({"id":id})).await.unwrap();
    assert_eq!(third["people"], 1, "{third}");
    assert!(!person_file.exists(), "the person's history file is removed");
    let mut files = Vec::new();
    let mut stack = vec![evidence.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            if entry.path().is_dir() { stack.push(entry.path()) } else { files.push(entry.path()) }
        }
    }
    let theirs: Vec<String> = files
        .iter()
        .map(|f| std::fs::read_to_string(f).unwrap())
        .filter(|t| t.contains(&format!("person_external_id: \"{opted}\"")))
        .collect();
    assert!(!theirs.is_empty());
    assert!(theirs.iter().all(|t| t.contains("deleted: true") && t.contains("The source deleted this person's data")), "evidence reduced to stubs: {theirs:?}");
    let suppressed = std::fs::read_to_string(std::path::Path::new(&root).join("suppression.txt")).unwrap();
    assert!(suppressed.contains("nora.field@university.example"), "{suppressed}");
    let stage = app.service(&root).unwrap().contact(&verified).await.unwrap().text("Stage").to_owned();
    let config = app.service(&root).unwrap().workspace_config().unwrap();
    assert_eq!(config.role(&stage), Some(gc_core::config::Role::Suppressed), "stage {stage}");
    assert!(app.call(&root, "source.person", json!({"slug":verified})).await.unwrap().as_array().unwrap().is_empty(), "no source facts remain");
    let remaining = app.call(&root, "source.people", json!({"source":"thumbnailsnap-mock"})).await.unwrap();
    assert!(!remaining.as_array().unwrap().iter().any(|p| p["external_id"] == opted.as_str()));

    app.call(&root, "source.delete", json!({"id":id})).await.unwrap();
    assert_eq!(app.call(&root, "source.list", json!({})).await.unwrap().as_array().unwrap().len(), 0);
}
