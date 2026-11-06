use gc_service::app::App;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};
fn app() -> (tempfile::TempDir, App, String) {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(
        &dir.path().join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    let info = app.open(&dir.path().join("work"), true).unwrap();
    let root = info["root"].as_str().unwrap().to_owned();
    (dir, app, root)
}
async fn contact(app: &App, root: &str) -> String {
    app.call(
        root,
        "contact.create",
        json!({"Name":"Test Expert","Email":"expert@example.test"}),
    )
    .await
    .unwrap()["slug"]
        .as_str()
        .unwrap()
        .into()
}
#[tokio::test]
async fn approval_is_bound_to_saved_bytes_and_smtp_delivery_is_idempotent() {
    let (_dir, app, root) = app();
    let slug = contact(&app, &root).await;
    let service = app.service(&root).unwrap();
    let draft=app.call(&root,"draft.save",json!({"contact_id":slug,"channel":"email","to":"expert@example.test","subject":"Review","body":"Thanks for the reply.","in_reply_to":"<incoming@example.test>"})).await.unwrap();
    let path = draft["path"].as_str().unwrap();
    service.draft_status(path, "approved").await.unwrap();
    assert!(
        app.send(&root, path, "missing")
            .await
            .unwrap_err()
            .contains("approve")
    );
    app.call(
        &root,
        "draft.status",
        json!({"path":path,"status":"approved"}),
    )
    .await
    .unwrap();
    service
        .docs
        .execute(
            path,
            "property.set",
            json!({"key":"subject","value":"Changed"}),
            "system:test",
        )
        .await
        .unwrap();
    assert!(
        app.send(&root, path, "missing")
            .await
            .unwrap_err()
            .contains("approve")
    );
    app.call(
        &root,
        "draft.status",
        json!({"path":path,"status":"approved"}),
    )
    .await
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut io = BufReader::new(stream);
        io.get_mut().write_all(b"220 fixture\r\n").await.unwrap();
        let mut body = String::new();
        loop {
            let mut line = String::new();
            if io.read_line(&mut line).await.unwrap() == 0 {
                break;
            }
            let response = if line.starts_with("EHLO") {
                "250-fixture\r\n250 8BITMIME\r\n"
            } else if line.starts_with("DATA") {
                io.get_mut().write_all(b"354 continue\r\n").await.unwrap();
                loop {
                    let mut data = String::new();
                    io.read_line(&mut data).await.unwrap();
                    if data == ".\r\n" {
                        break;
                    }
                    body.push_str(&data);
                }
                "250 queued\r\n"
            } else if line.starts_with("QUIT") {
                io.get_mut().write_all(b"221 bye\r\n").await.unwrap();
                break;
            } else {
                "250 ok\r\n"
            };
            io.get_mut().write_all(response.as_bytes()).await.unwrap();
        }
        body
    });
    let connection=app.save_mail(&root,json!({"kind":"smtp","input":{"name":"Fixture","password":"test","settings":{"host":"127.0.0.1","port":port,"username":"test","from":"me@example.test","tlsMode":"plain"}}})).await.unwrap();
    let sent = app
        .send(&root, path, connection["id"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(sent["status"], "sent");
    let payload = server.await.unwrap();
    assert!(payload.contains("In-Reply-To: <incoming@example.test>"));
    assert!(payload.contains("Subject: Changed"));
    assert!(payload.contains("Thanks for the reply."));
    assert!(
        app.send(&root, path, connection["id"].as_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(service.contact(&slug).await.unwrap().text("Stage"), "Sent");
    assert_eq!(service.rows("interactions").await.unwrap().len(), 1);
}
#[tokio::test]
async fn mcp_uses_supplier_catalog_and_has_no_approval_or_sending_route() {
    let (_dir, app, root) = app();
    let slug = contact(&app, &root).await;
    let catalog = app.table_catalog().unwrap();
    let list = app
        .mcp(json!({"workspace":root,"method":"tools/list"}))
        .await
        .unwrap();
    for command in catalog["commands"].as_array().unwrap() {
        let tool = list["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == command["tool"])
            .unwrap();
        assert_eq!(tool["inputSchema"], command["inputSchema"]);
        assert_eq!(tool["outputSchema"], command["outputSchema"]);
    }
    for name in ["draft.send", "draft.status", "settings.save"] {
        assert!(app.mcp(json!({"workspace":root,"method":"tools/call","params":{"name":name,"arguments":{}}})).await.is_err());
    }
    let call = |name: &str, args: Value| json!({"workspace":root,"method":"tools/call","params":{"name":name,"arguments":args}});
    assert!(
        app.mcp(call(
            "dossier.write_section",
            json!({"slug":slug,"section":"notes","content":"Overwrite"})
        ))
        .await
        .is_err()
    );
    let result = app
        .mcp(call("table_table_tables_list", json!({})))
        .await
        .unwrap();
    assert!(result.to_string().contains("contacts"));
    let model = app
        .mcp(call("table_model_describe", json!({})))
        .await
        .unwrap();
    assert!(!model["structuredContent"].is_null());
    let check = app
        .mcp(call("table_document_check", json!({})))
        .await
        .unwrap();
    assert!(!check["structuredContent"].is_null());
}

struct FixtureMailbox;
impl gc_mail::imap::MailboxFetcher for FixtureMailbox {
    fn test<'a>(
        &'a self,
        _: &'a gc_mail::types::ConnectionConfig,
        _: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), gc_mail::imap::SyncError>> + Send + 'a>,
    > {
        Box::pin(async { Ok(()) })
    }
    fn fetch<'a>(
        &'a self,
        _: &'a gc_mail::types::ConnectionConfig,
        _: &'a str,
        cursor: gc_mail::imap::Cursor,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<gc_mail::imap::FetchBatch, gc_mail::imap::SyncError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            Ok(gc_mail::imap::FetchBatch {
                uid_validity: 1,
                total: if cursor.uid < 1 { 1 } else { 0 },
                messages: if cursor.uid < 1 {
                    vec![gc_mail::imap::FetchedMessage{uid:1,internal_date:Some(gc_core::now_millis()),raw:b"From: Expert <expert@example.test>\r\nTo: me@example.test\r\nSubject: Paper reply\r\nMessage-ID: <reply@example.test>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\nUseful findings.\r\n--x\r\nContent-Type: text/plain\r\nContent-Disposition: attachment; filename=review.txt\r\n\r\nReview attachment\r\n--x--\r\n".to_vec()}]
                } else {
                    vec![]
                },
            })
        })
    }
}
#[tokio::test]
async fn mail_sync_matches_channels_preserves_local_edits_and_advances_cursor() {
    let (_dir, app, root) = app();
    let slug = contact(&app, &root).await;
    let service = app.service(&root).unwrap();
    service
        .add_channel(&slug, "email", "expert@example.test", "fixture", true)
        .await
        .unwrap();
    let store = gc_mail::store::Store {
        db: app.db.clone(),
        secrets: app.secrets.clone(),
    };
    let mail = gc_mail::service::Service::new(
        store.clone(),
        Arc::new(FixtureMailbox),
        gc_mail::workspace::WorkspaceWriter {
            root: root.clone().into(),
            docs: service.docs.clone(),
        },
    );
    let workflow=mail.create(gc_service::app::user_id(&root),serde_json::from_value(json!({"name":"Fixture","password":"secret","connection":{"host":"fixture.invalid","port":993,"tls_mode":"tls","username":"me","mailbox":"INBOX"},"target":{"table_path":"mail/email.ktable","content_folder":"mail/messages","attachment_folder":"mail/attachments"},"schedule":{"enabled":false,"interval_minutes":15},"include_attachments":true})).unwrap()).await.unwrap();
    app.mail_services
        .lock()
        .unwrap()
        .insert(root.clone(), mail.clone());
    let tool = |workspace: &str, name: &str, arguments: Value| json!({"workspace":workspace,"method":"tools/call","params":{"name":name,"arguments":arguments}});
    let accounts = app
        .mcp(tool(&root, "mail.accounts", json!({})))
        .await
        .unwrap();
    assert_eq!(accounts["structuredContent"]["imap"][0]["id"], workflow.id);
    assert!(!accounts.to_string().contains("secret"));
    let result = app
        .mcp(tool(&root, "mail.sync", json!({"id":workflow.id})))
        .await
        .unwrap();
    assert_eq!(result["structuredContent"]["imported"], 1);
    let other = app
        .open(&_dir.path().join("other-mail-workspace"), true)
        .unwrap()["root"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        app.mcp(tool(&other, "mail.accounts", json!({})))
            .await
            .unwrap()["structuredContent"]["imap"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        app.mcp(tool(&other, "mail.sync", json!({"id":workflow.id})))
            .await
            .is_err()
    );
    let detail = service.detail(&slug).await.unwrap();
    assert_eq!(detail["timeline"].as_array().unwrap().len(), 1);
    assert_eq!(detail["contact"]["values"]["Stage"], "Replied");
    let path = detail["timeline"][0]["values"]["File"].as_str().unwrap();
    let before = service.read_note(path).await.unwrap();
    assert!(before.contains("Useful findings."));
    assert!(before.contains("review.txt"));
    service
        .docs
        .execute(
            path,
            "text.replace",
            json!({"from":0,"to":0,"text":"Human annotation\n"}),
            "system:test",
        )
        .await
        .unwrap();
    let second = mail
        .sync(gc_service::app::user_id(&root), &workflow.id, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.imported, 0);
    app.match_mail(&root).await.unwrap();
    assert_eq!(service.rows("interactions").await.unwrap().len(), 1);
    assert!(
        service
            .read_note(path)
            .await
            .unwrap()
            .starts_with("Human annotation")
    );
    assert_eq!(
        store
            .get(gc_service::app::user_id(&root), &workflow.id)
            .unwrap()
            .unwrap()
            .cursor_uid,
        1
    );
}

#[tokio::test]
async fn social_sent_requires_approval_and_updates_followup_dates() {
    let (_dir, app, root) = app();
    let slug = contact(&app, &root).await;
    let draft = app
        .call(
            &root,
            "draft.save",
            json!({"contact_id":slug,"channel":"x","to":"@expert","body":"A short introduction."}),
        )
        .await
        .unwrap();
    let path = draft["path"].as_str().unwrap();
    assert!(
        app.call(&root, "draft.mark_sent", json!({"path":path}))
            .await
            .is_err()
    );
    app.call(
        &root,
        "draft.status",
        json!({"path":path,"status":"approved"}),
    )
    .await
    .unwrap();
    app.call(&root, "draft.mark_sent", json!({"path":path}))
        .await
        .unwrap();
    app.call(&root, "draft.mark_sent", json!({"path":path}))
        .await
        .unwrap();
    let service = app.service(&root).unwrap();
    assert_eq!(service.rows("interactions").await.unwrap().len(), 1);
    let contact = service.contact(&slug).await.unwrap();
    assert_eq!(contact.text("Stage"), "Sent");
    assert!(!contact.text("Last contacted").is_empty());
}
#[test]
fn a_second_host_cannot_write_an_open_workspace() {
    let (_dir, first, root) = app();
    let data = tempfile::tempdir().unwrap();
    let second = App::new(
        data.path(),
        first.resources.clone(),
        first.executable.clone(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    assert!(second.open(Path::new(&root), false).is_err());
    assert!(first.open(Path::new(&root), false).is_ok());
    drop(first);
    assert!(second.open(Path::new(&root), false).is_ok());
}

#[tokio::test]
async fn rows_created_in_the_editor_gain_stable_contact_identities() {
    let (_dir, app, root) = app();
    let service = app.service(&root).unwrap();
    service
        .write_row(
            "contacts",
            None,
            json!({"Name":"Editor Contact"}).as_object().unwrap(),
            "agent",
        )
        .await
        .unwrap();
    let first = app.call(&root, "contacts.list", json!({})).await.unwrap();
    assert_eq!(first[0]["values"]["Slug"], "editor-contact");
    let next = app.call(&root, "contacts.list", json!({})).await.unwrap();
    assert_eq!(first, next);
    assert!(service.detail("editor-contact").await.is_ok());
}

/// Workspaces and spreadsheets from Chinese-language use keep working: Chinese CSV headers map
/// to fields, "中文" in a Language column means Chinese, and a workspace whose Language field
/// was created with the legacy Chinese label keeps one value for Chinese.
#[tokio::test]
async fn chinese_imports_and_legacy_language_choices() {
    let (dir, app, root) = app();
    let csv = dir.path().join("people.csv");
    std::fs::write(&csv, "姓名,邮箱,公司,语言\nLin Wei,lin@example.test,Northwind,中文\nAda Stone,ada@example.test,Acme,English\n").unwrap();
    let preview = app.call(&root, "import.preview", json!({"path":csv})).await.unwrap();
    assert_eq!(preview["mapping"], json!(["Name", "Email", "Affiliation", "Language"]), "{preview}");
    app.call(&root, "import.run", json!({"path":csv,"mapping":preview["mapping"]})).await.unwrap();
    let service = app.service(&root).unwrap();
    let rows = service.rows("contacts").await.unwrap();
    let language = |name: &str| rows.iter().find(|r| r.text("Name") == name).unwrap().text("Language").to_owned();
    assert_eq!((language("Lin Wei").as_str(), language("Ada Stone").as_str()), ("Chinese", "English"));
    assert_eq!(service.language_value("中文").await, "Chinese");

    // An older workspace offers only the legacy label: Chinese is written that way there.
    let tables = service.docs.execute("contacts.ktable", "table.tables.list", json!({}), "system:test").await.unwrap();
    let contacts = tables["result"]["items"].as_array().unwrap().iter().find(|t| t["name"] == "contacts").unwrap().clone();
    let field = contacts["fields"].as_array().unwrap().iter().find(|f| f["title"] == "Language").unwrap().clone();
    let mut options = field["options"].clone();
    options["choices"] = json!(["中文", "English"]);
    service
        .docs
        .execute("contacts.ktable", "table.fields.update", json!({"tableId":contacts["id"],"fieldId":field["id"],"options":options}), "system:test")
        .await
        .unwrap();
    assert_eq!(service.language_value("Chinese").await, "中文");
    assert_eq!(service.language_value("English").await, "English");
    let slug = app.call(&root, "contact.create", json!({"Name":"Legacy Chinese","Language":"Chinese"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    assert_eq!(service.contact(&slug).await.unwrap().text("Language"), "中文");
    // New contacts default to English.
    let slug = app.call(&root, "contact.create", json!({"Name":"Legacy Default"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    assert_eq!(service.contact(&slug).await.unwrap().text("Language"), "English");
}
