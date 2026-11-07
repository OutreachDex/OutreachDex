//! Conversation imports from a WhatsApp chat export and a LinkedIn data archive: partners are
//! matched to contacts, messages land in day transcripts and one timeline row per day and
//! direction, and importing again only adds what is new. No account is signed in to.
use gc_core::config::Role;
use gc_service::app::App;
use serde_json::{Value, json};
use std::io::Write;
use std::{path::Path, sync::Arc};

fn app(dir: &Path) -> (App, String) {
    let app = App::new(
        &dir.join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    let root = app.open(&dir.join("work"), true).unwrap()["root"].as_str().unwrap().to_owned();
    (app, root)
}

async fn rows(app: &App, root: &str, table: &str) -> Vec<gc_service::contacts::Row> {
    app.service(root).unwrap().rows(table).await.unwrap()
}

#[tokio::test]
async fn whatsapp_export_imports_once_and_merges_longer_exports() {
    let dir = tempfile::tempdir().unwrap();
    let (app, root) = app(dir.path());
    let maya = app.call(&root, "contact.create", json!({"Name":"Maya Chen"})).await.unwrap()["slug"].as_str().unwrap().to_owned();

    let file = dir.path().join("WhatsApp Chat with Maya Chen.txt");
    let first = "\u{200e}[06/03/2024, 21:45:12] Maya Chen: \u{200e}Messages and calls are end-to-end encrypted.\n\
[06/03/2024, 21:45:12] Maya Chen: Hi! Saw your talk\n\
[06/03/2024, 21:50:00] Alex Rivera: Thanks Maya,\nhappy to chat next week\n\
[13/03/2024, 09:01:02] Alex Rivera: Does Tuesday work?\n";
    std::fs::write(&file, first).unwrap();

    let preview = app.call(&root, "chat.preview", json!({"kind":"whatsapp","path":file})).await.unwrap();
    assert_eq!(preview["me"], "Alex Rivera", "the partner is named in the file name");
    assert_eq!(preview["dateOrder"], "dmy");
    let conversation = &preview["conversations"][0];
    assert_eq!(conversation["match"]["slug"], maya.as_str());
    assert_eq!(conversation["match"]["by"], "name");
    assert_eq!(conversation["messages"], 3, "the encryption notice is not a message");
    assert_eq!(conversation["incoming"], 1);
    let key = conversation["key"].as_str().unwrap().to_owned();

    let import = |assign: Value| {
        let (app, root, file) = (&app, root.clone(), file.clone());
        async move { app.call(&root, "chat.import", json!({"kind":"whatsapp","path":file,"assign":assign})).await.unwrap() }
    };
    let result = import(json!({&key: &maya})).await;
    assert_eq!(result["added"], 3);
    assert_eq!(result["files"], 2, "one transcript per day");
    assert_eq!(result["timeline"], 3, "6 March in and out, 13 March out");

    let timeline: Vec<_> = rows(&app, &root, "interactions").await.into_iter().filter(|r| r.text("Channel") == "whatsapp").collect();
    assert_eq!(timeline.len(), 3);
    assert!(timeline.iter().all(|r| r.text("Contact") == maya && r.text("Kind") == "message"));
    let day = timeline.iter().find(|r| r.text("Direction") == "in").unwrap();
    assert!(day.text("Title").starts_with("WhatsApp: Hi! Saw your talk"), "{}", day.text("Title"));
    let transcript = std::fs::read_to_string(Path::new(&root).join(day.text("File"))).unwrap();
    assert!(transcript.contains("contact_id: \"maya-chen\""), "{transcript}");
    assert!(transcript.contains("**21:50 · Me**"));
    assert!(transcript.contains("happy to chat next week"));

    // We wrote last, so the contact waits for a reply.
    let config = app.service(&root).unwrap().workspace_config().unwrap();
    let contact = rows(&app, &root, "contacts").await.into_iter().find(|r| r.text("Slug") == maya).unwrap();
    assert!(contact.text("Last contacted").starts_with("2024-03-1"), "{}", contact.text("Last contacted"));
    assert!(contact.text("Last reply").starts_with("2024-03-0"));
    assert_eq!(config.role(contact.text("Stage")), Some(Role::Waiting));

    // The same export again changes nothing.
    let again = import(json!({&key: &maya})).await;
    assert_eq!((again["added"].as_u64(), again["files"].as_u64(), again["timeline"].as_u64()), (Some(0), Some(0), Some(0)));
    assert_eq!(again["contactsUpdated"], 0);

    // A later, longer export adds only the new reply, and the stage follows it.
    std::fs::write(&file, format!("{first}[13/03/2024, 10:15:00] Maya Chen: Tuesday is perfect\n")).unwrap();
    let longer = import(json!({&key: &maya})).await;
    assert_eq!(longer["added"], 1);
    assert_eq!(longer["files"], 1);
    assert_eq!(longer["timeline"], 1, "a new incoming row for 13 March");
    let contact = rows(&app, &root, "contacts").await.into_iter().find(|r| r.text("Slug") == maya).unwrap();
    assert_eq!(config.role(contact.text("Stage")), Some(Role::Active));
    let transcript = std::fs::read_to_string(Path::new(&root).join(longer_file(&app, &root).await)).unwrap();
    assert_eq!(transcript.matches("<!-- m:").count(), 2, "13 March: the question and the reply, once each");

    // An unassigned conversation is skipped.
    let skipped = import(json!({})).await;
    assert_eq!(skipped["conversations"], 0);
    assert_eq!(skipped["skipped"], 1);
}

async fn longer_file(app: &App, root: &str) -> String {
    rows(app, root, "interactions")
        .await
        .into_iter()
        .filter(|r| r.text("Channel") == "whatsapp")
        .map(|r| r.text("File").to_owned())
        .max()
        .unwrap()
}

#[tokio::test]
async fn whatsapp_needs_the_owner_when_the_file_name_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let (app, root) = app(dir.path());
    let file = dir.path().join("_chat.txt");
    std::fs::write(&file, "3/14/23, 9:05 PM - +44 7700 900123: hello\n3/14/23, 9:07 PM - Alex: hi there\n").unwrap();
    let preview = app.call(&root, "chat.preview", json!({"kind":"whatsapp","path":file})).await.unwrap();
    assert!(preview["me"].is_null());
    assert_eq!(preview["conversations"].as_array().unwrap().len(), 0);
    assert_eq!(preview["meChoices"].as_array().unwrap().len(), 2);
    let error = app.call(&root, "chat.import", json!({"kind":"whatsapp","path":file,"assign":{}})).await.unwrap_err();
    assert!(error.contains("you"), "{error}");

    // Once the owner is chosen, a partner known by phone number matches its WhatsApp channel.
    let sam = app.call(&root, "contact.create", json!({"Name":"Sam Ortiz"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    let other = app.call(&root, "contact.create", json!({"Name":"Someone Else"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    app.call(&root, "channel.add", json!({"slug":other,"kind":"whatsapp","value":"+44 7700 900999","source":"test"})).await.unwrap();
    // Saved in national format: the trailing digits still match.
    app.call(&root, "channel.add", json!({"slug":sam,"kind":"whatsapp","value":"07700 900123","source":"test"})).await.unwrap();
    let preview = app.call(&root, "chat.preview", json!({"kind":"whatsapp","path":file,"me":"Alex"})).await.unwrap();
    assert_eq!(preview["conversations"][0]["match"]["slug"], sam.as_str());
    assert_eq!(preview["conversations"][0]["match"]["by"], "whatsapp");
    assert_eq!(preview["dateOrder"], "mdy");

    // Group chats are reported, not imported.
    std::fs::write(&file, "3/14/23, 9:05 PM - A: one\n3/14/23, 9:06 PM - B: two\n3/14/23, 9:07 PM - C: three\n").unwrap();
    let preview = app.call(&root, "chat.preview", json!({"kind":"whatsapp","path":file})).await.unwrap();
    assert_eq!(preview["groups"], 1);
}

fn linkedin_zip(path: &Path) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let files = [
        ("Basic_LinkedInDataExport/messages.csv", "CONVERSATION ID,CONVERSATION TITLE,FROM,SENDER PROFILE URL,TO,RECIPIENT PROFILE URLS,DATE,SUBJECT,CONTENT,FOLDER\n\
c1,,Alex Rivera,https://www.linkedin.com/in/alex-rivera,Maya Chen,https://www.linkedin.com/in/maya-chen,2024-03-05 18:22:10 UTC,,Loved the paper,INBOX\n\
c1,,Maya Chen,https://www.linkedin.com/in/maya-chen,Alex Rivera,https://www.linkedin.com/in/alex-rivera,2024-03-06 08:00:00 UTC,,Thanks!,INBOX\n\
c2,,Alex Rivera,https://www.linkedin.com/in/alex-rivera,Sam Ortiz,https://www.linkedin.com/in/sam-ortiz,2024-04-01 10:00:00 UTC,Intro,Hello Sam,INBOX\n\
c3,Team,Alex Rivera,https://www.linkedin.com/in/alex-rivera,\"Maya Chen,Sam Ortiz\",\"https://www.linkedin.com/in/maya-chen,https://www.linkedin.com/in/sam-ortiz\",2024-04-02 10:00:00 UTC,,All hands,INBOX\n"),
        ("Basic_LinkedInDataExport/Connections.csv", "Notes:\n\"Some emails are hidden.\"\n\nFirst Name,Last Name,URL,Email Address,Company,Position,Connected On\n\
Maya,Chen,https://www.linkedin.com/in/maya-chen,maya@example.org,Example Lab,Researcher,05 Mar 2024\n\
Sam,Ortiz,https://www.linkedin.com/in/sam-ortiz,,Ortiz & Co,Founder,01 Apr 2024\n"),
        ("Basic_LinkedInDataExport/Profile.csv", "First Name,Last Name,Headline\nAlex,Rivera,Builder\n"),
    ];
    for (name, text) in files {
        zip.start_file(name, options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

#[tokio::test]
async fn linkedin_archive_matches_by_email_and_creates_partners_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let (app, root) = app(dir.path());
    let maya = app.call(&root, "contact.create", json!({"Name":"Dr. M. Chen"})).await.unwrap()["slug"].as_str().unwrap().to_owned();
    app.call(&root, "channel.add", json!({"slug":maya,"kind":"email","value":"MAYA@example.org","source":"test"})).await.unwrap();
    // A do-not-contact person keeps that stage whatever the import finds.
    let config = app.service(&root).unwrap().workspace_config().unwrap();
    let suppressed = config.stages.iter().find(|s| s.role == Role::Suppressed).map(|s| s.label.clone());
    if let Some(stage) = &suppressed {
        app.call(&root, "contact.update", json!({"slug":maya,"values":{"Stage":stage}})).await.unwrap();
    }
    let archive = dir.path().join("Basic_LinkedInDataExport_11-06-2026.zip");
    linkedin_zip(&archive);

    let preview = app.call(&root, "chat.preview", json!({"kind":"linkedin","path":archive})).await.unwrap();
    assert_eq!(preview["me"], "linkedin.com/in/alex-rivera", "the owner comes from Profile.csv");
    assert_eq!(preview["groups"], 1);
    let conversations = preview["conversations"].as_array().unwrap();
    assert_eq!(conversations.len(), 2);
    let c1 = conversations.iter().find(|c| c["key"] == "c1").unwrap();
    assert_eq!(c1["match"]["slug"], maya.as_str());
    assert_eq!(c1["match"]["by"], "email", "Connections.csv gives Maya's email");
    let c2 = conversations.iter().find(|c| c["key"] == "c2").unwrap();
    assert!(c2["match"].is_null());
    assert_eq!(c2["partner"]["affiliation"], "Founder, Ortiz & Co");

    let result = app
        .call(&root, "chat.import", json!({"kind":"linkedin","path":archive,"assign":{"c1":maya,"c2":"new"}}))
        .await
        .unwrap();
    assert_eq!(result["conversations"], 2);
    assert_eq!(result["created"], 1);
    assert_eq!(result["added"], 3);

    let contacts = rows(&app, &root, "contacts").await;
    let sam = contacts.iter().find(|r| r.text("Name") == "Sam Ortiz").expect("Sam was created");
    assert_eq!(sam.text("Affiliation"), "Founder, Ortiz & Co");
    assert_eq!(config.role(sam.text("Stage")), Some(Role::Waiting), "we wrote, no reply yet");
    let maya_row = contacts.iter().find(|r| r.text("Slug") == maya).unwrap();
    assert!(maya_row.text("Last reply").starts_with("2024-03-06"));
    if let Some(stage) = &suppressed {
        assert_eq!(maya_row.text("Stage"), stage);
    }
    let channels = rows(&app, &root, "channels").await;
    assert!(channels.iter().any(|r| r.text("Contact") == maya && r.text("Kind") == "linkedin" && r.text("Value") == "https://www.linkedin.com/in/maya-chen"));
    assert!(channels.iter().any(|r| r.text("Contact") == sam.text("Slug") && r.text("Kind") == "linkedin"));

    // Next time, Sam matches by the LinkedIn channel the import added.
    let preview = app.call(&root, "chat.preview", json!({"kind":"linkedin","path":archive})).await.unwrap();
    let c2 = preview["conversations"].as_array().unwrap().iter().find(|c| c["key"] == "c2").unwrap().clone();
    assert_eq!(c2["match"]["by"], "linkedin");
    let again = app
        .call(&root, "chat.import", json!({"kind":"linkedin","path":archive,"assign":{"c1":maya,"c2":sam.text("Slug")}}))
        .await
        .unwrap();
    assert_eq!((again["added"].as_u64(), again["timeline"].as_u64(), again["created"].as_u64()), (Some(0), Some(0), Some(0)));
}
