//! Agent drafts keep their original wording; drafts made stale by a newer reply need an
//! explicit acknowledgement. Uses manual "mark sent" on a WeChat draft: nothing is sent.
use gc_service::app::App;
use serde_json::json;
use std::{path::Path, sync::Arc};

#[tokio::test]
async fn ai_original_and_stale_acknowledgement() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(
        &dir.path().join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    let root = app.open(&dir.path().join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    let slug = app.call(&root, "contact.create", json!({"Name":"Stale Person"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    let service = app.service(&root).unwrap();

    // The agent writes; a person edits; the original stays for comparison.
    let draft = service
        .draft_save(&json!({"contact_id":slug,"channel":"wechat","body":"First version written by AI"}), true)
        .await
        .unwrap();
    let path = draft["path"].as_str().unwrap().to_owned();
    let edited = app
        .call(&root, "draft.save", json!({"contact_id":slug,"channel":"wechat","body":"My edited version","path":path}))
        .await
        .unwrap();
    assert_eq!(edited["metadata"]["ai_original"], "First version written by AI");
    assert_eq!(edited["body"], "My edited version");
    let manual = app.call(&root, "draft.save", json!({"contact_id":slug,"channel":"wechat","body":"Written by hand"})).await.unwrap();
    assert!(manual["metadata"].get("ai_original").is_none(), "manual drafts have no AI original");

    // The person replies after the draft was created: marking it sent needs acknowledgement.
    app.call(&root, "draft.status", json!({"path":path,"status":"approved"})).await.unwrap();
    let later = chrono::Utc::now() + chrono::Duration::minutes(5);
    service
        .update_contact(&slug, json!({"Last reply": later.to_rfc3339()}).as_object().cloned().unwrap(), false)
        .await
        .unwrap();
    let refused = app.call(&root, "draft.mark_sent", json!({"path":path})).await.unwrap_err();
    assert!(refused.contains("wrote again"), "{refused}");
    // Paused sending refuses everything, including manual "mark sent".
    let mut config = app.config();
    config["sendingPaused"] = json!(true);
    app.call(&root, "settings.save", config.clone()).await.unwrap();
    assert!(app.call(&root, "draft.mark_sent", json!({"path":path,"acknowledge_stale":true})).await.unwrap_err().contains("paused"));
    config["sendingPaused"] = json!(false);
    app.call(&root, "settings.save", config).await.unwrap();
    app.call(&root, "draft.mark_sent", json!({"path":path,"acknowledge_stale":true})).await.unwrap();
    // Sending schedules the next follow-up.
    let follow = service.contact(&slug).await.unwrap();
    assert!(!follow.text("Follow up").is_empty(), "follow-up scheduled after sending");
    // A keep-in-touch cadence decides when the next follow-up falls.
    app.call(&root, "contact.update", json!({"slug":slug,"values":{"Keep in touch":30}})).await.unwrap();
    let cadence = app.call(&root, "draft.save", json!({"contact_id":slug,"channel":"wechat","body":"Routine check-in"})).await.unwrap();
    let cadence = cadence["path"].as_str().unwrap().to_owned();
    app.call(&root, "draft.status", json!({"path":cadence,"status":"approved"})).await.unwrap();
    app.call(&root, "draft.mark_sent", json!({"path":cadence,"acknowledge_stale":true})).await.unwrap();
    let expected = (chrono::Utc::now() + chrono::Duration::days(30)).format("%Y-%m-%d").to_string();
    assert!(service.contact(&slug).await.unwrap().text("Follow up").starts_with(&expected));
    app.call(&root, "contact.follow_up", json!({"slug":slug,"days":null})).await.unwrap();
    assert!(service.contact(&slug).await.unwrap().text("Follow up").is_empty());
    // Unreplaced template placeholders block even an approved, acknowledged draft.
    let templated = app.call(&root, "draft.save", json!({"contact_id":slug,"channel":"wechat","body":"Hi {{name}}, may I ask you something?"})).await.unwrap();
    let templated = templated["path"].as_str().unwrap().to_owned();
    app.call(&root, "draft.status", json!({"path":templated,"status":"approved"})).await.unwrap();
    let blocked = app.call(&root, "draft.mark_sent", json!({"path":templated,"acknowledge_stale":true})).await.unwrap_err();
    assert!(blocked.contains("placeholder"), "{blocked}");
    let after = app.call(&root, "draft.list", json!({"slug":slug})).await.unwrap();
    assert!(after.as_array().unwrap().iter().any(|d| d["path"] == path.as_str() && d["metadata"]["status"] == "sent"));
}

/// An email can only go to an address known for the contact (Email field, email channels,
/// linked source emails) unless the person explicitly confirms; checked before any SMTP use.
#[tokio::test]
async fn recipient_must_be_a_known_address() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(
        &dir.path().join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    let root = app.open(&dir.path().join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    let slug = app.call(&root, "contact.create", json!({"Name":"Known Person","Email":"Known@Example.com"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    let service = app.service(&root).unwrap();
    service.add_channel(&slug, "email", "work@example.org", "test", false).await.unwrap();
    let known = app.call(&root, "contact.addresses", json!({"slug":slug})).await.unwrap();
    assert_eq!(known, json!(["known@example.com", "work@example.org"]));

    let send = |to: &str, ack: bool| {
        let (app, root, slug, to) = (app.clone(), root.clone(), slug.clone(), to.to_owned());
        async move {
            let draft = app.call(&root, "draft.save", json!({"contact_id":slug,"channel":"email","to":to,"subject":"Hi","body":"Hello"})).await.unwrap();
            let path = draft["path"].as_str().unwrap().to_owned();
            app.call(&root, "draft.status", json!({"path":path,"status":"approved"})).await.unwrap();
            app.call(&root, "draft.send", json!({"path":path,"connection":"none","acknowledge_recipient":ack})).await.unwrap_err()
        }
    };
    let refused = send("typo@exampel.com", false).await;
    assert!(refused.contains("is not a known email for this contact") && refused.contains("known@example.com"), "{refused}");
    // Known addresses (any case, with a display name) pass the check and stop at the missing SMTP account.
    for to in ["KNOWN@example.com", "Known Person <work@example.org>"] {
        let later = send(to, false).await;
        assert!(later.contains("SMTP"), "{to}: {later}");
    }
    // An explicit confirmation lets an unknown address through the check.
    assert!(send("typo@exampel.com", true).await.contains("SMTP"));
    let lonely = app.call(&root, "contact.create", json!({"Name":"No Email"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    assert!(service.recipient_guard(&lonely, "a@b.co").await.unwrap_err().contains("has no known email yet"));
}
