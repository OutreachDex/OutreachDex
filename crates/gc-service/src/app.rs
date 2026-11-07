use crate::{Service, contacts::now, notes::metadata};
use gc_connect::SecretStore;
use gc_core::Workspace;
use gc_docs::{Docs, Notify};
use gc_store::Db;
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

#[derive(Clone)]
pub struct App {
    pub db: Db,
    pub secrets: SecretStore,
    pub terminals: crate::terminal::Terminals,
    pub notify: Notify,
    pub resources: PathBuf,
    pub executable: PathBuf,
    pub mail_services: Arc<Mutex<HashMap<String, gc_mail::service::Service>>>,
    pub services: Arc<Mutex<HashMap<String, Service>>>,
    workspace_locks: Arc<Mutex<HashMap<String, std::fs::File>>>,
    pub endpoint: Arc<Mutex<String>>,
    pub token: String,
    pub running: Arc<Mutex<HashMap<String, bool>>>,
    pub ui_context: Arc<Mutex<HashMap<String, Value>>>,
    pub send_lock: Arc<tokio::sync::Mutex<()>>,
}
pub fn user_id(root: &str) -> i64 {
    let hash = Sha256::digest(root.as_bytes());
    let mut data = [0; 8];
    data.copy_from_slice(&hash[..8]);
    i64::from_le_bytes(data) & i64::MAX
}
fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args[key].as_str().ok_or_else(|| format!("Missing {key}"))
}
impl App {
    pub fn new(
        data: &Path,
        resources: PathBuf,
        executable: PathBuf,
        notify: Notify,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(data).map_err(|e| e.to_string())?;
        let db = Db::open(&data.join("globalcontact.sqlite").to_string_lossy())
            .map_err(|e| e.to_string())?;
        db.conn()
            .map_err(|e| e.to_string())?
            .execute(
                "UPDATE agent_sessions SET status='interrupted' WHERE status='running'",
                [],
            )
            .map_err(|e| e.to_string())?;
        db.conn()
            .map_err(|e| e.to_string())?
            .execute(
                "UPDATE terminals SET status='interrupted' WHERE status='running'",
                [],
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            db,
            secrets: SecretStore::with_key_path(data.join("secret.key")),
            terminals: Default::default(),
            notify,
            resources,
            executable,
            mail_services: Arc::default(),
            services: Arc::default(),
            workspace_locks: Arc::default(),
            endpoint: Arc::default(),
            token: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            running: Arc::default(),
            ui_context: Arc::default(),
            send_lock: Arc::default(),
        })
    }
    pub fn open(&self, path: &Path, create: bool) -> Result<Value, String> {
        self.open_with(path, create, None)
    }
    /// Opens a workspace, or creates one from `template` (default: blank).
    pub fn open_with(&self, path: &Path, create: bool, template: Option<&str>) -> Result<Value, String> {
        if create {
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        }
        let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
        let key = canonical.to_string_lossy().into_owned();
        if !create
            && self
                .services
                .lock()
                .map_err(|e| e.to_string())?
                .contains_key(&key)
        {
            self.setting_set("last_workspace", json!(key))?;
            return Ok(json!({"root":key}));
        }
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(canonical.join(".globalcontact.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|_| {
            "This workspace is already open in another OutreachDex process. Close it there to continue.".to_string()
        })?;
        let workspace = if create {
            Workspace::create_with(path, template.unwrap_or("blank"))
        } else {
            Workspace::open(path)
        }
        .map_err(|e| e.to_string())?;
        // Validates the configuration; generates one for workspaces made before it existed.
        gc_core::config::WorkspaceConfig::load(&workspace).map_err(|e| e.to_string())?;
        let root = workspace.root().to_string_lossy().into_owned();
        self.services
            .lock()
            .map_err(|e| e.to_string())?
            .entry(root.clone())
            .or_insert_with(|| {
                Service::new(
                    Docs::new(workspace, self.notify.clone()),
                    self.notify.clone(),
                )
            });
        self.workspace_locks
            .lock()
            .map_err(|e| e.to_string())?
            .insert(root.clone(), lock);
        self.setting_set("last_workspace", json!(root))?;
        Ok(json!({"root":root}))
    }
    pub fn service(&self, root: &str) -> Result<Service, String> {
        self.services
            .lock()
            .map_err(|e| e.to_string())?
            .get(root)
            .cloned()
            .ok_or("Open a workspace first.".into())
    }
    pub fn setting(&self, key: &str) -> Value {
        self.db
            .conn()
            .ok()
            .and_then(|c| {
                c.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                    r.get::<_, String>(0)
                })
                .optional()
                .ok()
                .flatten()
            })
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null)
    }
    pub fn setting_set(&self, key: &str, value: Value) -> Result<(), String> {
        self.db.conn().map_err(|e|e.to_string())?.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value.to_string()]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn config(&self) -> Value {
        let mut value = self.setting("config");
        if !value.is_object() {
            value = json!({});
        }
        if value["defaultAgent"].is_null() {
            value["defaultAgent"] = json!("codex");
        }
        if value["followupDays"].is_null() {
            value["followupDays"] = json!(7);
        }
        value
    }
    /// The user's own Claude Code and Codex CLIs (the app bundles neither): the configured path,
    /// or a standard install location.
    pub fn detect(&self) -> Value {
        let config = self.config();
        let mut result = json!({});
        for kind in ["claude", "codex"] {
            let program = crate::terminal::cli_program(kind, config["terminal"][kind].as_str());
            result[kind] = json!({"available":program.is_some(),"program":program});
        }
        result
    }
    pub(crate) async fn current_context(&self, root: &str) -> Result<Value, String> {
        let service = self.service(root)?;
        let mut context = self
            .ui_context
            .lock()
            .map_err(|e| e.to_string())?
            .get(root)
            .cloned()
            .unwrap_or(json!({"contact":null,"document":null}));
        context["workspace"] = json!(root);
        context["name"] = if let Some(slug) = context["contact"].as_str() {
            json!(service.contact(slug).await?.text("Name"))
        } else {
            Value::Null
        };
        let selected: Vec<String> = context["selection"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_owned)).collect();
        if !selected.is_empty() {
            // Names help the Agent confirm whom "these people" means; slugs stay authoritative.
            let rows = service.rows("contacts").await?;
            context["selection"] = json!(
                selected
                    .iter()
                    .take(50)
                    .map(|slug| json!({"slug":slug,"name":rows.iter().find(|r| r.text("Slug") == slug).map(|r| r.text("Name")).unwrap_or("")}))
                    .collect::<Vec<_>>()
            );
            context["selection_count"] = json!(selected.len());
        }
        Ok(context)
    }
    pub(crate) fn mail(&self, root: &str) -> Result<gc_mail::service::Service, String> {
        let service = self.service(root)?;
        let mut cache = self.mail_services.lock().map_err(|e| e.to_string())?;
        Ok(cache
            .entry(root.to_owned())
            .or_insert_with(|| {
                gc_mail::service::Service::new(
                    gc_mail::store::Store {
                        db: self.db.clone(),
                        secrets: self.secrets.clone(),
                    },
                    Arc::new(gc_mail::imap::ImapFetcher),
                    gc_mail::workspace::WorkspaceWriter {
                        root: service.docs.workspace.root().to_owned(),
                        docs: service.docs,
                    },
                )
            })
            .clone())
    }
    pub async fn call(&self, root: &str, action: &str, args: Value) -> Result<Value, String> {
        match action {
            "bootstrap" => {
                return Ok(
                    json!({"lastWorkspace":self.setting("last_workspace"),"config":self.config(),"agents":self.detect(),"templates":gc_core::config::TEMPLATES.iter().map(|t| json!({"id":t.id,"label":t.label,"description":t.description})).collect::<Vec<_>>()}),
                );
            }
            "settings.get" => return Ok(json!({"config":self.config(),"agents":self.detect()})),
            "settings.save" => {
                self.setting_set("config", args)?;
                return Ok(self.config());
            }
            "terminal.running" => return Ok(json!(self.terminals.running())),
            _ => {}
        }
        let service = self.service(root)?;
        if action == "context.set" {
            // UI-only state: MCP can read this selection but cannot change it.
            let contact = args["contact"].as_str().filter(|s| !s.is_empty());
            let document = args["document"].as_str().filter(|s| !s.is_empty());
            if let Some(path) = document {
                service
                    .docs
                    .workspace
                    .file(path)
                    .map_err(|e| e.to_string())?;
            }
            // What else is on screen: the page, an open list or draft, and people picked in bulk.
            let selection: Vec<&str> = args["selection"].as_array().into_iter().flatten().filter_map(Value::as_str).take(200).collect();
            let context = json!({
                "contact": contact,
                "document": document,
                "view": args["view"].as_str().filter(|s| !s.is_empty()),
                "list": args["list"].as_str().filter(|s| !s.is_empty()),
                "draft": args["draft"].as_str().filter(|s| s.starts_with("drafts/")),
                "selection": selection,
            });
            self.ui_context
                .lock()
                .map_err(|e| e.to_string())?
                .insert(root.to_owned(), context.clone());
            return Ok(context);
        }
        if action == "context.get" {
            return self.current_context(root).await;
        }
        if action.starts_with("terminal.") {
            return self.terminal_call(root, action, args).await;
        }
        let result = match action {
            "contacts.list" => {
                service.reconcile_contacts().await?;
                json!(service.rows("contacts").await?)
            }
            "contact.get" => service.detail(text(&args, "slug")?).await?,
            "contact.create" => {
                json!({"slug":service.create_contact(args.as_object().cloned().ok_or("contact values")?).await?})
            }
            "contact.update" => {
                service
                    .update_contact(
                        text(&args, "slug")?,
                        args["values"].as_object().cloned().ok_or("values")?,
                        false,
                    )
                    .await?;
                json!({})
            }
            "channel.add" => {
                json!({"created":service.add_channel(text(&args,"slug")?,text(&args,"kind")?,text(&args,"value")?,text(&args,"source")?,args["verified"]==true).await?})
            }
            "import.preview" => service.import_preview(Path::new(text(&args, "path")?))?,
            "chat.preview" => service.chat_preview(&args).await?,
            "chat.import" => service.chat_import(&args).await?,
            "import.run" => {
                let mapping: Vec<Option<String>> = serde_json::from_value(args["mapping"].clone())
                    .map_err(|_| "Invalid column mapping.")?;
                service.import_csv(Path::new(text(&args, "path")?), &mapping).await?
            }
            "contact.fields" => service.contact_fields().await?,
            "contact.bulk_update" => {
                // The same validation as single edits (Stage must be a configured stage).
                let values = args["values"].as_object().cloned().ok_or("Missing fields to change")?;
                let slugs: Vec<String> = args["slugs"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_owned)).collect();
                for slug in &slugs {
                    service.update_contact(slug, values.clone(), false).await?;
                }
                json!({"updated": slugs.len()})
            }
            "contact.bulk_follow_up" => {
                let at = args["days"].as_i64().map(|d| chrono::Utc::now() + chrono::Duration::days(d.clamp(0, 3650)));
                let slugs: Vec<String> = args["slugs"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_owned)).collect();
                for slug in &slugs {
                    service.set_follow_up(slug, at).await?;
                }
                json!({"updated": slugs.len()})
            }
            "contacts.export" => {
                // Writes a UTF-8 CSV (with BOM for spreadsheet apps) to a path the person chose.
                let path = text(&args, "path")?;
                let wanted: Option<std::collections::HashSet<String>> = args["slugs"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect());
                let fields: Vec<String> = service.contact_fields().await?.as_array().into_iter().flatten().filter_map(|f| f["title"].as_str().map(str::to_owned)).collect();
                let rows: Vec<crate::contacts::Row> = service.rows("contacts").await?.into_iter().filter(|r| wanted.as_ref().is_none_or(|w| w.contains(r.text("Slug")))).collect();
                let mut writer = csv::Writer::from_writer(vec![0xEF, 0xBB, 0xBF]);
                writer.write_record(&fields).map_err(|e| e.to_string())?;
                for row in &rows {
                    let record: Vec<String> = fields
                        .iter()
                        .map(|f| match row.values.get(f) {
                            None | Some(Value::Null) => String::new(),
                            Some(Value::String(s)) => s.clone(),
                            Some(Value::Array(items)) => items.iter().map(|v| v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string())).collect::<Vec<_>>().join(", "),
                            Some(other) => other.to_string(),
                        })
                        .collect();
                    writer.write_record(&record).map_err(|e| e.to_string())?;
                }
                let bytes = writer.into_inner().map_err(|e| e.to_string())?;
                std::fs::write(path, bytes).map_err(|e| format!("Couldn't write the file: {e}"))?;
                json!({"exported": rows.len(), "path": path})
            }
            "contact.history" => service.contact_history(text(&args, "slug")?).await?,
            "contact.follow_up" => {
                // days: number of days from now; null clears the reminder.
                let at = args["days"].as_i64().map(|d| chrono::Utc::now() + chrono::Duration::days(d.clamp(0, 3650)));
                let at = match args["date"].as_str() {
                    Some(date) => Some(
                        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                            .map_err(|_| "Date must be YYYY-MM-DD")?
                            .and_hms_opt(9, 0, 0)
                            .ok_or("Invalid date")?
                            .and_utc(),
                    ),
                    None => at,
                };
                service.set_follow_up(text(&args, "slug")?, at).await?;
                json!({})
            }
            "contact.merge_preview" => service.merge_preview(text(&args, "winner")?, text(&args, "loser")?).await?,
            "contact.merge" => self.contact_merge(root, text(&args, "winner")?, text(&args, "loser")?).await?,
            "preferences.get" => json!({"text": service.preferences_full()}),
            "contact.addresses" => json!(service.known_addresses(text(&args, "slug")?).await?),
            "source.language" => {
                // "Chinese" / "English" for new contacts from this source (the legacy Chinese label
                // is accepted too and stored as "Chinese"); anything else means automatic.
                let key = text(&args, "source")?;
                let language = args["language"].as_str().and_then(gc_core::schema::language);
                self.setting_set(&format!("source-language:{root}:{key}"), language.map(|l| json!(l)).unwrap_or(Value::Null))?;
                json!({"language": self.source_language(root, key)})
            }
            "source.identity" => self.source_identity(root, text(&args, "source")?, text(&args, "external_id")?).await?,
            "memory.get" => service.memory_read(text(&args, "slug")?).await?,
            "agent.analysis" => self.agent_analysis(root, text(&args, "slug")?).await?,
            "agent.recent" => self.agent_recent(root, args["limit"].as_i64().unwrap_or(8))?,
            "agent.activity" => self.activity_list(root, args["session"].as_str(), args["limit"].as_i64().unwrap_or(100))?,
            "preferences.save" => {
                service.preferences_save(args["text"].as_str().unwrap_or(""))?;
                json!({})
            }
            "preferences.add" => {
                service.preferences_add(text(&args, "note")?)?;
                json!({})
            }
            "contact.field_add" => {
                let choices: Vec<String> = args["choices"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_owned)).collect();
                service.contact_field_add(text(&args, "title")?, text(&args, "type")?, &choices).await?
            }
            "contact.field_choice" => service.contact_field_choice(text(&args, "title")?, text(&args, "choice")?).await?,
            "list.all" => self.lists_all(root).await?,
            "list.get" => self.list_get(root, text(&args, "id")?).await?,
            "list.save" => self.list_save(root, &args)?,
            "list.preview" => self.list_preview(root, &args).await?,
            "list.delete" => self.list_delete(root, text(&args, "id")?)?,
            "list.prepare" => self.list_prepare(root, text(&args, "id")?, &args).await?,
            "list.cancel" => self.list_cancel(root, text(&args, "id")?)?,
            "list.skip" => self.list_skip(root, text(&args, "id")?, text(&args, "member")?)?,
            "list.suppress" => self.list_suppress(root, text(&args, "id")?, text(&args, "member")?).await?,
            "source.check" => self.source_check(&args).await?,
            "source.add" => self.source_add(root, &args).await?,
            "source.list" => self.source_list(root).await?,
            "source.sync" => self.source_sync(root, text(&args, "id")?).await?,
            "source.reset" => self.source_reset(root, text(&args, "id")?)?,
            "source.delete" => self.source_delete(root, text(&args, "id")?)?,
            "source.people" => self.source_people(root, &args).await?,
            "source.person" => self.source_person(root, text(&args, "slug")?).await?,
            "identity.decide" => self.identity_decide(root, &args).await?,
            "workspace.config" => {
                let config = service.workspace_config()?;
                let playbooks = gc_core::config::playbooks(&service.docs.workspace).map_err(|e| e.to_string())?;
                json!({"config": config, "playbooks": playbooks})
            }
            "note.read" => json!({"text":service.read_note(text(&args,"path")?).await?}),
            "documents.list" => json!(service.documents()?),
            "intake.add" => {
                let slug = text(&args, "slug")?;
                let path = service
                    .intake(
                        slug,
                        text(&args, "channel")?,
                        text(&args, "title")?,
                        text(&args, "body")?,
                    )
                    .await?;
                json!({"path":path})
            }
            "draft.list" => json!(service.drafts(args["slug"].as_str()).await?),
            "draft.save" => {
                let value = service.draft_save(&args, false).await?;
                self.clear_approval(root, value["path"].as_str().unwrap_or(""))?;
                value
            }
            "draft.status" => {
                let path = text(&args, "path")?;
                let status = text(&args, "status")?;
                service.draft_status(path, status).await?;
                if status == "approved" {
                    let file = service
                        .docs
                        .workspace
                        .read(path, None, None)
                        .map_err(|e| e.to_string())?;
                    self.db.conn().map_err(|e|e.to_string())?.execute("INSERT INTO approvals(workspace,path,hash) VALUES(?1,?2,?3) ON CONFLICT(workspace,path) DO UPDATE SET hash=excluded.hash",params![root,path,file.version]).map_err(|e|e.to_string())?;
                } else {
                    self.clear_approval(root, path)?;
                }
                json!({})
            }
            "draft.send" => {
                self.send_checked(root, text(&args, "path")?, text(&args, "connection")?, args["acknowledge_stale"] == true, args["acknowledge_recipient"] == true)
                    .await?
            }
            "draft.mark_sent" => {
                let path = text(&args, "path")?;
                if !path.starts_with("drafts/") {
                    return Err("Invalid draft path.".into());
                }
                let note = service.read_note(path).await?;
                let meta = metadata(&note);
                if meta["channel"] == "email" {
                    return Err("Mail must be recorded through the send action.".into());
                }
                if meta["status"] == "sent" {
                    return Ok(json!({}));
                }
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
                let version = service
                    .docs
                    .workspace
                    .stamp(path)
                    .map_err(|e| e.to_string())?;
                if meta["status"] != "approved" || approved.is_none() || approved != version {
                    return Err("Save and approve the current version first.".into());
                }
                self.outreach_guard(root, &service, &meta).await?;
                self.stale_guard(&service, &meta, args["acknowledge_stale"] == true).await?;
                App::placeholder_guard(crate::notes::body(&note))?;
                service
                    .docs
                    .execute(
                        path,
                        "property.set",
                        json!({"key":"status","value":"sent"}),
                        "system:drafts",
                    )
                    .await?;
                self.record_outreach(root, path, &meta)?;
                self.schedule_follow_up(&service, &meta).await?;
                service
                    .interaction(
                        text(&meta, "contact_id")?,
                        text(&meta, "channel")?,
                        "message",
                        "out",
                        meta["subject"].as_str().unwrap_or("Sent manually"),
                        path,
                        &format!("manual:{path}"),
                        &now(),
                    )
                    .await?;
                service
                    .update_contact(
                        text(&meta, "contact_id")?,
                        json!({"Last contacted":now(),"Stage":service.stage_for(gc_core::config::Role::Waiting)?})
                            .as_object()
                            .cloned()
                            .unwrap_or_default(),
                        false,
                    )
                    .await?;
                self.clear_approval(root, path)?;
                json!({})
            }
            "mail.messages" => json!(self.mail_rows(root).await?),
            "mail.assign" => {
                let slug = text(&args, "slug")?;
                service
                    .add_channel(slug, "email", text(&args, "email")?, "Matched by user from incoming mail", true)
                    .await?;
                self.match_mail(root).await?;
                json!({})
            }
            "mail.accounts" => {
                let smtp = gc_connect::store::Store {
                    db: self.db.clone(),
                    secrets: self.secrets.clone(),
                };
                let conn = self.db.conn().map_err(|e| e.to_string())?;
                json!({"imap":self.mail(root)?.list(user_id(root)).await.map_err(|e|e.message())?,"smtp":smtp.list(&conn,user_id(root)).map_err(|e|e.to_string())?.iter().map(|c|c.view(0)).collect::<Vec<_>>()})
            }
            "mail.save" => self.save_mail(root, args).await?,
            "mail.test" => {
                let id = text(&args, "id")?;
                if args["kind"] == "imap" {
                    json!(
                        self.mail(root)?
                            .test(user_id(root), id)
                            .await
                            .map_err(|e| e.message())?
                    )
                } else {
                    let store = gc_connect::store::Store {
                        db: self.db.clone(),
                        secrets: self.secrets.clone(),
                    };
                    let conn = self.db.conn().map_err(|e| e.to_string())?;
                    let row = store
                        .get(&conn, user_id(root), id)
                        .map_err(|e| e.to_string())?
                        .ok_or("SMTP connection missing")?;
                    let secrets = store.open_secrets(&row)?;
                    json!(gc_connect::channel::test(&row, &secrets).await)
                }
            }
            "mail.delete" => {
                if args["kind"] == "imap" {
                    self.mail(root)?
                        .delete(user_id(root), text(&args, "id")?)
                        .await
                        .map_err(|e| e.message())?;
                } else {
                    let store = gc_connect::store::Store {
                        db: self.db.clone(),
                        secrets: self.secrets.clone(),
                    };
                    store
                        .delete(
                            &*self.db.conn().map_err(|e| e.to_string())?,
                            user_id(root),
                            text(&args, "id")?,
                        )
                        .map_err(|e| e.to_string())?;
                }
                json!({})
            }
            "mail.sync" => {
                let result = self
                    .mail(root)?
                    .sync(user_id(root), text(&args, "id")?, false)
                    .await
                    .map_err(|(e, _)| e)?
                    .ok_or("No incoming mailbox found for this workspace.")?;
                self.match_mail(root).await?;
                json!(result)
            }
            _ => return Err(format!("Unknown action: {action}")),
        };
        if !matches!(
            action,
            "contacts.list"
                | "contact.get"
                | "workspace.config"
                | "import.preview"
                | "chat.preview"
                | "source.check"
                | "contact.fields"
                | "contact.history"
                | "contact.merge_preview"
                | "preferences.get"
                | "memory.get"
                | "contact.addresses"
                | "source.identity"
                | "agent.activity"
                | "agent.analysis"
                | "agent.recent"
                | "list.all"
                | "list.get"
                | "list.preview"
                | "source.list"
                | "source.people"
                | "source.person"
                | "draft.list"
                | "note.read"
                | "documents.list"
                | "mail.accounts"
        ) {
            (self.notify)("workspace-changed", json!({"root":root}));
        }
        Ok(result)
    }
    pub(crate) fn clear_approval(&self, root: &str, path: &str) -> Result<(), String> {
        self.db
            .conn()
            .map_err(|e| e.to_string())?
            .execute(
                "DELETE FROM approvals WHERE workspace=?1 AND path=?2",
                params![root, path],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub async fn start_bridge(&self) -> Result<(), String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| e.to_string())?;
        *self.endpoint.lock().map_err(|e| e.to_string())? = listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string();
        let app = self.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let app = app.clone();
                tokio::spawn(async move {
                    let (read, mut write) = stream.into_split();
                    let mut lines = BufReader::new(read).lines();
                    if let Ok(Ok(Some(line))) =
                        tokio::time::timeout(std::time::Duration::from_secs(10), lines.next_line())
                            .await
                    {
                        if line.len() > 8 * 1024 * 1024 {
                            return;
                        }
                        let input: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                        if input["token"] != app.token {
                            return;
                        }
                        let response = match app.mcp(input["request"].clone()).await {
                            Ok(result) => json!({"result":result}),
                            Err(error) => json!({"error":error}),
                        };
                        let _ = write.write_all(format!("{response}\n").as_bytes()).await;
                    }
                });
            }
        });
        Ok(())
    }
}
pub fn find_program(name: &str) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_owned());
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|p| p.is_file())
    })
}
