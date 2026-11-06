//! Synced mail: an automatic reply is not a reply, an opt-out suppresses the person, and a
//! bounce suppresses the failed address. Fixture mailbox only; nothing is sent.
use gc_service::app::App;
use serde_json::json;
use std::{path::Path, sync::Arc};

struct Fixture;
fn message(uid: u32, raw: &str) -> gc_mail::imap::FetchedMessage {
    gc_mail::imap::FetchedMessage { uid, internal_date: Some(gc_core::now_millis()), raw: raw.replace('\n', "\r\n").into_bytes() }
}
impl gc_mail::imap::MailboxFetcher for Fixture {
    fn test<'a>(&'a self, _: &'a gc_mail::types::ConnectionConfig, _: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), gc_mail::imap::SyncError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn fetch<'a>(
        &'a self,
        _: &'a gc_mail::types::ConnectionConfig,
        _: &'a str,
        cursor: gc_mail::imap::Cursor,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<gc_mail::imap::FetchBatch, gc_mail::imap::SyncError>> + Send + 'a>> {
        Box::pin(async move {
            let messages = if cursor.uid < 1 {
                vec![
                    message(1, "From: Away <away@example.test>\nTo: me@example.test\nSubject: Automatic reply: Hello\nMessage-ID: <auto@example.test>\nContent-Type: text/plain; charset=UTF-8\n\nI am out of office until Monday.\n"),
                    message(2, "From: Done <done@example.test>\nTo: me@example.test\nSubject: Re: Hello\nMessage-ID: <stop@example.test>\nContent-Type: text/plain; charset=UTF-8\n\nPlease unsubscribe me, thanks.\n"),
                    message(3, "From: Mail Delivery System <MAILER-DAEMON@mx.example.test>\nTo: me@example.test\nSubject: Undelivered Mail Returned to Sender\nMessage-ID: <dsn@example.test>\nContent-Type: text/plain; charset=UTF-8\n\nThis is the mail system.\n\nFinal-Recipient: rfc822; gone@example.test\nAction: failed\nStatus: 5.1.1\n"),
                ]
            } else {
                vec![]
            };
            Ok(gc_mail::imap::FetchBatch { uid_validity: 1, total: messages.len(), messages })
        })
    }
}

#[tokio::test]
async fn auto_replies_opt_outs_and_bounces() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(&dir.path().join("data"), Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"), std::env::current_exe().unwrap(), Arc::new(|_, _| {})).unwrap();
    let root = app.open(&dir.path().join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    let service = app.service(&root).unwrap();
    let mut slugs = Vec::new();
    for (name, email) in [("Away", "away@example.test"), ("Done", "done@example.test"), ("Gone", "gone@example.test")] {
        let slug = app.call(&root, "contact.create", json!({"Name":name,"Email":email})).await.unwrap()["slug"].as_str().unwrap().to_owned();
        service.add_channel(&slug, "email", email, "fixture", true).await.unwrap();
        slugs.push(slug);
    }
    let pending_done = app.call(&root, "draft.save", json!({"contact_id":slugs[1],"channel":"email","subject":"Follow-up","body":"..."})).await.unwrap()["path"].as_str().unwrap().to_owned();
    let pending_gone = app.call(&root, "draft.save", json!({"contact_id":slugs[2],"channel":"email","subject":"Follow-up","body":"..."})).await.unwrap()["path"].as_str().unwrap().to_owned();

    let store = gc_mail::store::Store { db: app.db.clone(), secrets: app.secrets.clone() };
    let mail = gc_mail::service::Service::new(store, Arc::new(Fixture), gc_mail::workspace::WorkspaceWriter { root: root.clone().into(), docs: service.docs.clone() });
    let workflow = mail.create(gc_service::app::user_id(&root), serde_json::from_value(json!({"name":"Fixture","password":"secret","connection":{"host":"fixture.invalid","port":993,"tls_mode":"tls","username":"me","mailbox":"INBOX"},"target":{"table_path":"mail/email.ktable","content_folder":"mail/messages","attachment_folder":"mail/attachments"},"schedule":{"enabled":false,"interval_minutes":15},"include_attachments":false})).unwrap()).await.unwrap();
    mail.sync(gc_service::app::user_id(&root), &workflow.id, true).await.unwrap().unwrap();
    app.match_mail(&root).await.unwrap();

    // Automatic reply: recorded as a note; the person has not replied.
    let away = service.detail(&slugs[0]).await.unwrap();
    assert_ne!(away["contact"]["values"]["Stage"], "Replied");
    assert!(away["contact"]["values"]["Last reply"].is_null() || away["contact"]["values"]["Last reply"] == "");
    assert!(away["timeline"].as_array().unwrap().iter().any(|r| r["values"]["Title"].as_str().unwrap_or("").starts_with("Auto-reply")));
    // "until Monday" names no date: follow up in a week.
    let week = (chrono::Utc::now() + chrono::Duration::days(7)).format("%Y-%m-%d").to_string();
    assert!(away["contact"]["values"]["Follow up"].as_str().unwrap_or("").starts_with(&week), "{}", away["contact"]["values"]["Follow up"]);

    // Opt-out: do-not-contact stage, suppressed address, queued draft discarded, noted once.
    let done = service.detail(&slugs[1]).await.unwrap();
    assert_eq!(done["contact"]["values"]["Stage"], "Do not contact");
    let suppression = std::fs::read_to_string(Path::new(&root).join("suppression.txt")).unwrap();
    assert!(suppression.contains("done@example.test"));
    assert!(done["drafts"].as_array().unwrap().iter().any(|d| d["path"] == pending_done.as_str() && d["metadata"]["status"] == "discarded"));

    // Bounce: the failed address is suppressed and its draft discarded.
    assert!(suppression.contains("gone@example.test"));
    let gone = service.detail(&slugs[2]).await.unwrap();
    assert!(gone["drafts"].as_array().unwrap().iter().any(|d| d["path"] == pending_gone.as_str() && d["metadata"]["status"] == "discarded"));
    assert!(gone["timeline"].as_array().unwrap().iter().any(|r| r["values"]["Title"].as_str().unwrap_or("").starts_with("Bounced")));

    // Matching again changes nothing.
    let before = service.rows("interactions").await.unwrap().len();
    app.match_mail(&root).await.unwrap();
    assert_eq!(service.rows("interactions").await.unwrap().len(), before);
}
