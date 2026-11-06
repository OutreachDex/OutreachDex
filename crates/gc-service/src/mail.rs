use crate::{
    app::{App, user_id},
    contacts::now,
    notes::{body, metadata},
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
impl App {
    pub async fn save_mail(&self, root: &str, args: Value) -> Result<Value, String> {
        if args["kind"] == "imap" {
            let mut input = args["input"].clone();
            if input["target"].is_null() {
                input["target"] = json!({"table_path":"mail/email.ktable","content_folder":"mail/messages","attachment_folder":"mail/attachments"});
            }
            let input = serde_json::from_value(input).map_err(|e| e.to_string())?;
            let mail = self.mail(root)?;
            return if let Some(id) = args["id"].as_str() {
                Ok(json!(
                    mail.update(user_id(root), id, input)
                        .await
                        .map_err(|e| e.message())?
                ))
            } else {
                Ok(json!(
                    mail.create(user_id(root), input)
                        .await
                        .map_err(|e| e.message())?
                ))
            };
        }
        let store = gc_connect::store::Store {
            db: self.db.clone(),
            secrets: self.secrets.clone(),
        };
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let settings = args["input"]["settings"]
            .as_object()
            .cloned()
            .ok_or("SMTP settings required")?;
        let mut secrets = gc_connect::SecretBag::new();
        if let Some(password) = args["input"]["password"].as_str().filter(|s| !s.is_empty()) {
            secrets.insert("password".into(), password.into());
        }
        let validation_secrets = if secrets.is_empty() {
            if let Some(id) = args["id"].as_str() {
                store.open_secrets(
                    &store
                        .get(&conn, user_id(root), id)
                        .map_err(|e| e.to_string())?
                        .ok_or("Connection missing")?,
                )?
            } else {
                secrets.clone()
            }
        } else {
            secrets.clone()
        };
        gc_connect::channel::validate(&settings, &validation_secrets)?;
        let input = gc_connect::store::UpsertInput {
            channel: "email_smtp".into(),
            name: args["input"]["name"].as_str().unwrap_or("SMTP").into(),
            settings: Some(settings),
            secrets,
        };
        let row = if let Some(id) = args["id"].as_str() {
            store
                .update(&conn, user_id(root), id, input)?
                .ok_or("Connection missing")?
        } else {
            store.create(&conn, user_id(root), input)?
        };
        Ok(json!(row.view(0)))
    }
    pub async fn send(&self, root: &str, path: &str, connection: &str) -> Result<Value, String> {
        self.send_checked(root, path, connection, false, false).await
    }
    /// `acknowledged`: the person saw that the contact wrote after this draft was created.
    /// `unknown_recipient_ok`: the person confirmed sending to an address not known for this contact.
    pub async fn send_checked(&self, root: &str, path: &str, connection: &str, acknowledged: bool, unknown_recipient_ok: bool) -> Result<Value, String> {
        let _guard = self.send_lock.lock().await;
        if !path.starts_with("drafts/") || !path.ends_with(".md") {
            return Err("Invalid draft.".into());
        }
        let service = self.service(root)?;
        let text = service.read_note(path).await?;
        let meta = metadata(&text);
        let version = service
            .docs
            .workspace
            .read(path, None, None)
            .map_err(|e| e.to_string())?
            .version;
        let approved: Option<String> = self
            .db
            .conn()
            .map_err(|e| e.to_string())?
            .query_row(
                "SELECT hash FROM approvals WHERE workspace=?1 AND path=?2",
                params![root, path],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if meta["status"] != "approved" || approved.as_deref() != Some(version.as_str()) {
            return Err("Save and approve the current version of the draft first. Edits need approval again.".into());
        }
        if meta["channel"] != "email" {
            return Err("Copy this and send it manually on this channel.".into());
        }
        let to = meta["to"].as_str().ok_or("No recipient")?;
        gc_connect::smtp::parse_address(to).map_err(|e| e.to_string())?;
        let slug = meta["contact_id"].as_str().ok_or("Draft has no contact")?;
        service.contact(slug).await?;
        if !unknown_recipient_ok {
            service.recipient_guard(slug, to).await?;
        }
        // Do-not-contact and the daily list limit apply before any connection is opened.
        self.outreach_guard(root, &service, &meta).await?;
        self.stale_guard(&service, &meta, acknowledged).await?;
        App::placeholder_guard(body(&text))?;
        let store = gc_connect::store::Store {
            db: self.db.clone(),
            secrets: self.secrets.clone(),
        };
        let row = store
            .get(
                &*self.db.conn().map_err(|e| e.to_string())?,
                user_id(root),
                connection,
            )
            .map_err(|e| e.to_string())?
            .ok_or("Choose an SMTP account")?;
        let secrets = store.open_secrets(&row)?;
        gc_connect::channel::validate(&row.settings, &secrets)?;
        let message_id = format!("<{}@globalcontact.local>", uuid::Uuid::new_v4());
        let message = gc_connect::smtp::Message {
            to: to.into(),
            subject: meta["subject"].as_str().unwrap_or("").into(),
            body: body(&text).into(),
            message_id: message_id.clone(),
            in_reply_to: meta["in_reply_to"].as_str().unwrap_or("").into(),
        };
        self.db.conn().map_err(|e|e.to_string())?.execute("INSERT INTO sent_receipts(workspace,path,message_id,status,at) VALUES(?1,?2,?3,'sending',?4)",params![root,path,message_id,gc_core::now_millis()]).map_err(|_|"This draft already has a send record and can't be sent again.".to_string())?;
        self.clear_approval(root, path)?;
        service
            .docs
            .execute(
                path,
                "property.set",
                json!({"key":"status","value":"sending"}),
                "system:smtp",
            )
            .await?;
        let delivered = gc_connect::channel::deliver(&row, &secrets, &message).await;
        let status = if delivered.is_ok() {
            "sent"
        } else {
            "uncertain"
        };
        self.db
            .conn()
            .map_err(|e| e.to_string())?
            .execute(
                "UPDATE sent_receipts SET status=?3 WHERE workspace=?1 AND path=?2",
                params![root, path, status],
            )
            .map_err(|e| e.to_string())?;
        service
            .docs
            .execute(
                path,
                "property.set",
                json!({"key":"status","value":status}),
                "system:smtp",
            )
            .await?;
        delivered
            .map_err(|e| format!("Sending wasn't confirmed: {e}. Check your mailbox; this draft is locked to avoid sending it twice."))?;
        self.record_outreach(root, path, &meta)?;
        self.schedule_follow_up(&service, &meta).await?;
        service
            .interaction(
                slug,
                "email",
                "message",
                "out",
                &message.subject,
                path,
                &message_id,
                &now(),
            )
            .await?;
        service
            .update_contact(
                slug,
                json!({"Last contacted":now(),"Stage":service.stage_for(gc_core::config::Role::Waiting)?})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
                false,
            )
            .await?;
        Ok(json!({"message_id":message_id,"status":"sent"}))
    }
    pub async fn mail_rows(&self, root: &str) -> Result<Vec<Value>, String> {
        let service = self.service(root)?;
        let path = "mail/email.ktable";
        if service
            .docs
            .workspace
            .stamp(path)
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Ok(vec![]);
        }
        let tables = service
            .docs
            .execute(path, "table.tables.list", json!({}), "system:mail")
            .await?;
        let table = tables["result"]["items"]
            .as_array()
            .and_then(|a| a.iter().find(|t| t["name"] == "email_messages"))
            .ok_or("Mail table missing")?;
        let mut rows = vec![];
        loop {
            let response = service
                .docs
                .execute(
                    path,
                    "table.records.list",
                    json!({"tableId":table["id"],"limit":500,"offset":rows.len()}),
                    "system:mail",
                )
                .await?;
            let items = response["result"]["items"]
                .as_array()
                .ok_or("Mail record missing")?;
            rows.extend(items.iter().map(|r| r["values"].clone()));
            if items.is_empty()
                || rows.len() >= response["result"]["total"].as_u64().unwrap_or(0) as usize
            {
                break;
            }
        }
        Ok(rows)
    }
    pub async fn match_mail(&self, root: &str) -> Result<(), String> {
        let service = self.service(root)?;
        let channels = service.rows("channels").await?;
        let existing = service.rows("interactions").await?;
        let re = regex::Regex::new(r"(?i)[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9.-]+\.[a-z]{2,}")
            .map_err(|e| e.to_string())?;
        for row in self.mail_rows(root).await? {
            let from = row["from"].as_str().unwrap_or("");
            let to = row["to"].as_str().unwrap_or("");
            let from_addresses: Vec<_> = re
                .find_iter(from)
                .map(|m| m.as_str().to_lowercase())
                .collect();
            let to_addresses: Vec<_> = re
                .find_iter(to)
                .map(|m| m.as_str().to_lowercase())
                .collect();
            let path = row["document"].as_str().unwrap_or("");
            let message_id = row["message_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(path);
            let subject = row["subject"].as_str().unwrap_or("");
            let mut signal = crate::signals::classify(subject, from, row["preview"].as_str().unwrap_or(""));
            if matches!(signal, crate::signals::Signal::Bounce(_)) {
                // Failed recipients are in the full delivery report, not the preview.
                if let Ok(text) = service.read_note(path).await {
                    signal = crate::signals::classify(subject, from, &text);
                }
                if let crate::signals::Signal::Bounce(failed) = &signal {
                    for address in failed {
                        gc_core::lists::suppress(&service.docs.workspace, address, "bounced").map_err(|e| e.to_string())?;
                        service.discard_pending_drafts(None, Some(address)).await?;
                        for channel in channels.iter().filter(|c| c.text("Kind") == "email" && c.text("Value").eq_ignore_ascii_case(address)) {
                            let slug = channel.text("Contact");
                            let id = format!("bounce:{message_id}");
                            if !existing.iter().any(|r| r.text("Contact") == slug && r.text("Message-ID") == id) {
                                service
                                    .interaction(slug, "email", "message", "note", &format!("Bounced: {address} is undeliverable and was added to the do-not-contact list"), path, &id, row["received_at"].as_str().unwrap_or(""))
                                    .await?;
                            }
                        }
                    }
                    continue;
                }
            }
            for channel in channels.iter().filter(|c| c.text("Kind") == "email") {
                let address = channel.text("Value").to_lowercase();
                let incoming = from_addresses.contains(&address);
                if !incoming && !to_addresses.contains(&address) {
                    continue;
                }
                let slug = channel.text("Contact");
                if existing
                    .iter()
                    .any(|r| r.text("Contact") == slug && r.text("Message-ID") == message_id)
                {
                    continue;
                }
                let at = row["received_at"].as_str().unwrap_or("");
                if incoming && signal == crate::signals::Signal::AutoReply {
                    // Recorded, but an automatic reply is not the person answering. Follow up
                    // after the return date it mentions, or in a week.
                    service
                        .interaction(slug, "email", "message", "note", &format!("Auto-reply: {subject}"), path, message_id, at)
                        .await?;
                    // Only the subject and message text: the stored file also carries headers
                    // such as the Date, which is not a return date.
                    let today = chrono::Utc::now().date_naive();
                    let back = crate::signals::return_date(&format!("{subject}\n{}", row["preview"].as_str().unwrap_or("")), today)
                        .filter(|d| *d >= today)
                        .map(|d| d.succ_opt().unwrap_or(d))
                        .unwrap_or(today + chrono::Duration::days(7));
                    service.set_follow_up(slug, back.and_hms_opt(9, 0, 0).map(|t| t.and_utc())).await?;
                    continue;
                }
                service
                    .interaction(
                        slug,
                        "email",
                        "message",
                        if incoming { "in" } else { "out" },
                        row["subject"].as_str().unwrap_or(""),
                        path,
                        message_id,
                        at,
                    )
                    .await?;
                let current = service.contact(slug).await?;
                let field = if incoming {
                    "Last reply"
                } else {
                    "Last contacted"
                };
                if at > current.text(field) {
                    service
                        .update_contact(
                            slug,
                            json!({field:at,"Stage":service.stage_for(if incoming {gc_core::config::Role::Active} else {gc_core::config::Role::Waiting})?})
                                .as_object()
                                .cloned()
                                .unwrap_or_default(),
                            false,
                        )
                        .await?;
                }
                if incoming && signal == crate::signals::Signal::Normal {
                    // They answered: the reminder to chase them is no longer needed.
                    service.set_follow_up(slug, None).await?;
                }
                if incoming && signal == crate::signals::Signal::Unsubscribe {
                    // The person asked not to be contacted: do-not-contact list, stage, and no
                    // queued drafts. Recorded once, with the new interaction.
                    for address in &from_addresses {
                        gc_core::lists::suppress(&service.docs.workspace, address, "asked not to be contacted").map_err(|e| e.to_string())?;
                    }
                    let config = service.workspace_config()?;
                    if let Some(stage) = config.stages.iter().find(|s| s.role == gc_core::config::Role::Suppressed) {
                        service
                            .update_contact(slug, json!({"Stage": stage.label}).as_object().cloned().unwrap_or_default(), false)
                            .await?;
                    }
                    let discarded = service.discard_pending_drafts(Some(slug), None).await?;
                    service
                        .interaction(
                            slug,
                            "email",
                            "message",
                            "note",
                            &format!("Asked not to be contacted: added to the do-not-contact list{}", if discarded > 0 { format!("; discarded {discarded} pending {}", if discarded == 1 { "draft" } else { "drafts" }) } else { String::new() }),
                            path,
                            &format!("optout:{message_id}"),
                            at,
                        )
                        .await?;
                    continue;
                }
            }
        }
        Ok(())
    }
    pub async fn scheduler(&self) {
        let roots: Vec<String> = self
            .services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect();
        let store = gc_mail::store::Store {
            db: self.db.clone(),
            secrets: self.secrets.clone(),
        };
        for root in roots {
            if let Ok(workflows) = store.list_due(gc_core::now_millis()) {
                for w in workflows
                    .into_iter()
                    .filter(|w| w.user_id == user_id(&root))
                {
                    if let Ok(mail) = self.mail(&root) {
                        if let Err((error, _)) = mail.sync(user_id(&root), &w.id, false).await {
                            (self.notify)("background-error", json!({"message":error}));
                        }
                        let _ = self.match_mail(&root).await;
                    }
                }
            }
            self.sync_due_sources(&root).await;

        }
    }
}
