//! Messaging channels added after a workspace was created (WhatsApp, TikTok, Facebook,
//! LinkedIn): older workspaces gain the select choices without losing data, drafts and channels
//! accept the new kinds, and CSV columns import as channels. Nothing is sent.
use gc_service::{app::App, contacts::TABLE};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

async fn choices(app: &App, root: &str, table: &str, field: &str) -> Vec<String> {
    let table = app.service(root).unwrap().table(table).await.unwrap();
    let field = table["fields"].as_array().unwrap().iter().find(|f| f["title"] == field).unwrap().clone();
    field["options"]["choices"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn older_workspaces_gain_new_channel_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(
        &dir.path().join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    let root = app.open(&dir.path().join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    let slug = app.call(&root, "contact.create", json!({"Name":"Old Workspace Person"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    app.call(&root, "channel.add", json!({"slug":slug,"kind":"x","value":"@old","source":"test"})).await.unwrap();

    // Turn the select fields back into what older workspaces were created with.
    let service = app.service(&root).unwrap();
    let old: [(&str, &str, Value); 3] = [
        ("contacts", "Primary channel", json!(["Email", "X", "Instagram", "WeChat", "Other"])),
        ("channels", "Kind", json!(["email", "x", "instagram", "homepage", "scholar", "github", "wechat", "other", "custom"])),
        ("interactions", "Channel", json!(["email", "x", "instagram", "wechat", "other"])),
    ];
    for (table, title, list) in &old {
        let t = service.table(table).await.unwrap();
        let f = t["fields"].as_array().unwrap().iter().find(|f| f["title"] == *title).unwrap().clone();
        service
            .docs
            .execute(TABLE, "table.fields.update", json!({"tableId":t["id"],"fieldId":f["id"],"options":{"choices":list}}), "system:test")
            .await
            .unwrap();
    }
    assert!(!choices(&app, &root, "channels", "Kind").await.contains(&"whatsapp".to_owned()));

    // Listing contacts (what opening a workspace does) adds the missing choices.
    app.call(&root, "contacts.list", json!({})).await.unwrap();
    let kind = choices(&app, &root, "channels", "Kind").await;
    for k in ["whatsapp", "tiktok", "facebook", "linkedin", "custom"] {
        assert!(kind.contains(&k.to_owned()), "{k} missing from {kind:?}");
    }
    assert_eq!(kind[..8], ["email", "x", "instagram", "homepage", "scholar", "github", "wechat", "whatsapp"]);
    assert!(choices(&app, &root, "interactions", "Channel").await.contains(&"linkedin".to_owned()));
    assert!(choices(&app, &root, "contacts", "Primary channel").await.contains(&"LinkedIn".to_owned()));
    let before = choices(&app, &root, "channels", "Kind").await;
    app.call(&root, "contacts.list", json!({})).await.unwrap();
    assert_eq!(choices(&app, &root, "channels", "Kind").await, before, "upgrade is idempotent");

    // Existing values survive; the new kinds are accepted.
    app.call(&root, "channel.add", json!({"slug":slug,"kind":"whatsapp","value":"+1 555 0100","source":"test"})).await.unwrap();
    assert!(app.call(&root, "channel.add", json!({"slug":slug,"kind":"myspace","value":"a","source":"test"})).await.is_err());
    app.call(&root, "contact.update", json!({"slug":slug,"values":{"Primary channel":"LinkedIn"}})).await.unwrap();
    let draft = app.call(&root, "draft.save", json!({"contact_id":slug,"channel":"linkedin","body":"Hello"})).await.unwrap();
    assert!(draft["path"].as_str().unwrap().contains("-linkedin-"));
    let detail = app.call(&root, "contact.get", json!({"slug":slug})).await.unwrap();
    let kinds: Vec<&str> = detail["channels"].as_array().unwrap().iter().map(|c| c["values"]["Kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["x", "whatsapp"]);
    assert_eq!(detail["contact"]["values"]["Primary channel"], "LinkedIn");

    // CSV columns named after the new channels become channels, not contact fields.
    let csv = dir.path().join("people.csv");
    std::fs::write(&csv, "Name,WhatsApp,TikTok,Messenger,LinkedIn\nImported Person,+44 20 0000 0000,@tt,fb.me/ip,linkedin.com/in/ip\n").unwrap();
    let preview = app.call(&root, "import.preview", json!({"path":csv})).await.unwrap();
    assert_eq!(preview["mapping"], json!(["Name", "WhatsApp", "TikTok", "Facebook", "LinkedIn"]));
    let result = app.call(&root, "import.run", json!({"path":csv,"mapping":preview["mapping"]})).await.unwrap();
    assert_eq!(result["created"], 1);
    let rows = app.call(&root, "contacts.list", json!({})).await.unwrap();
    let imported = rows.as_array().unwrap().iter().find(|r| r["values"]["Name"] == "Imported Person").unwrap();
    let detail = app.call(&root, "contact.get", json!({"slug":imported["values"]["Slug"]})).await.unwrap();
    let mut kinds: Vec<&str> = detail["channels"].as_array().unwrap().iter().map(|c| c["values"]["Kind"].as_str().unwrap()).collect();
    kinds.sort();
    assert_eq!(kinds, ["facebook", "linkedin", "tiktok", "whatsapp"]);
}
