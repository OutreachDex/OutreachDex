//! Two records of one person become one, with nothing lost.
use gc_service::app::App;
use serde_json::json;
use std::{path::Path, sync::Arc};

#[tokio::test]
async fn merging_moves_everything_to_the_winner() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(&dir.path().join("data"), Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"), std::env::current_exe().unwrap(), Arc::new(|_, _| {})).unwrap();
    let root = app.open(&dir.path().join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    let service = app.service(&root).unwrap();
    let winner = app.call(&root, "contact.create", json!({"Name":"Lin Wei","Email":"lin@example.test"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    let loser = app.call(&root, "contact.create", json!({"Name":"林薇","Affiliation":"Northwind","Email":"lin.wei@work.test"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    service.add_channel(&winner, "email", "lin@example.test", "a", true).await.unwrap();
    service.add_channel(&loser, "email", "lin.wei@work.test", "b", true).await.unwrap();
    service.add_channel(&loser, "email", "LIN@example.test", "dup", false).await.unwrap();
    app.call(&root, "intake.add", json!({"slug":loser,"channel":"other","title":"Interview notes","body":"Wants to export PDF"})).await.unwrap();
    let draft = app.call(&root, "draft.save", json!({"contact_id":loser,"channel":"email","subject":"Follow-up","body":"Hello"})).await.unwrap();
    app.call(&root, "draft.status", json!({"path":draft["path"],"status":"approved"})).await.unwrap();

    let preview = app.call(&root, "contact.merge_preview", json!({"winner":winner,"loser":loser})).await.unwrap();
    assert_eq!(preview["drafts"], 1);
    assert_eq!(preview["intake"], 1);
    assert!(preview["fill"].as_array().unwrap().iter().any(|f| f == "Affiliation"));
    assert!(app.call(&root, "contact.merge", json!({"winner":winner,"loser":winner})).await.is_err());

    app.call(&root, "contact.merge", json!({"winner":winner,"loser":loser})).await.unwrap();
    assert!(service.contact(&loser).await.is_err(), "the duplicate is gone");
    let detail = app.call(&root, "contact.get", json!({"slug":winner})).await.unwrap();
    assert_eq!(detail["contact"]["values"]["Affiliation"], "Northwind", "empty fields are filled");
    assert_eq!(detail["contact"]["values"]["Email"], "lin@example.test", "the winner keeps its own values");
    let emails: Vec<String> = detail["channels"].as_array().unwrap().iter().map(|c| c["values"]["Value"].as_str().unwrap().to_lowercase()).collect();
    assert_eq!(emails.iter().filter(|e| *e == "lin@example.test").count(), 1, "duplicate channels collapse: {emails:?}");
    assert!(emails.contains(&"lin.wei@work.test".to_owned()));
    assert!(detail["timeline"].as_array().unwrap().iter().any(|r| r["values"]["Title"] == "Interview notes"));
    let file = detail["timeline"].as_array().unwrap().iter().find(|r| r["values"]["Title"] == "Interview notes").unwrap()["values"]["File"].as_str().unwrap().to_owned();
    assert!(file.starts_with(&format!("intake/{winner}/")), "{file}");
    assert!(service.read_note(&file).await.is_ok());
    let drafts = detail["drafts"].as_array().unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0]["metadata"]["contact_id"], winner.as_str());
    assert_eq!(drafts[0]["metadata"]["status"], "draft", "moved drafts need approval again");
    let dossier = service.read_note(&format!("people/{winner}.md")).await.unwrap();
    // The duplicate's Chinese name (test input) appears in the merge heading.
    assert!(dossier.contains("Merged from 林薇"));
    assert!(!Path::new(&root).join(format!("people/{loser}.md")).exists());
}
