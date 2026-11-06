//! GCSP/1 sources: connection records, incremental sync into the workspace, identity matching.
//!
//! People become rows in the `identities` table (docs/gcsp.md §8). A verified email equal to a
//! contact's email links automatically; unverified matches wait for a person; everyone else stays
//! unlinked until a person links or creates a contact. Events become evidence files under
//! `sources/<source>/` and, for linked people, timeline entries.
use crate::{Service, app::App, contacts::Row, contacts::now};
use gc_source::{Client, Event, Manifest, Person, SourceError, parse_event, parse_person};
use rusqlite::{OptionalExtension, params};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

const IDENTITIES: &str = "identities";
const PAGE: u32 = 200;
/// Upper bound per run so one sync never holds the workspace for too long; the cursor resumes.
const MAX_PAGES: usize = 50;
const LINKED: &[&str] = &["auto", "confirmed"];

struct Record {
    key: String,
    name: String,
    base_url: String,
    header: String,
    secret: String,
    manifest: Manifest,
}

#[derive(Default)]
struct Counts {
    people: i64,
    events: i64,
    linked: i64,
    pending: i64,
    skipped: i64,
}

fn safe(id: &str) -> String {
    let cleaned: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .take(80)
        .collect();
    if cleaned.trim_matches('-').is_empty() { "event".into() } else { cleaned }
}

fn yaml(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

impl Service {
    /// Adds the identities table to workspaces created before sources existed.
    pub async fn ensure_identities(&self) -> Result<(), String> {
        if self.table(IDENTITIES).await.is_ok() {
            return Ok(());
        }
        let _guard = self.mutations.lock().await;
        let fields = gc_core::schema::identities_fields();
        let mut primary = fields[0].clone();
        primary["primary"] = json!(true);
        let created = self
            .docs
            .execute(
                crate::contacts::TABLE,
                "table.tables.create",
                json!({"name":IDENTITIES,"title":IDENTITIES,"fields":[primary],"views":[{"title":"All","type":"grid"}]}),
                "system:sources",
            )
            .await?;
        for mut field in fields.into_iter().skip(1) {
            field["tableId"] = created["result"]["id"].clone();
            self.docs
                .execute(crate::contacts::TABLE, "table.fields.create", field, "system:sources")
                .await?;
        }
        Ok(())
    }

    /// Lowercased email → contact slugs, from the Email column and email channels.
    async fn email_index(&self) -> Result<HashMap<String, Vec<String>>, String> {
        let mut index: HashMap<String, Vec<String>> = HashMap::new();
        let mut add = |email: &str, slug: &str| {
            let email = email.trim().to_lowercase();
            if !email.is_empty() && !slug.is_empty() {
                let slugs = index.entry(email).or_default();
                if !slugs.iter().any(|s| s == slug) {
                    slugs.push(slug.to_owned());
                }
            }
        };
        for row in self.rows("contacts").await? {
            add(row.text("Email"), row.text("Slug"));
        }
        for row in self.rows("channels").await? {
            if row.text("Kind") == "email" {
                add(row.text("Value"), row.text("Contact"));
            }
        }
        Ok(index)
    }

    /// Copies a linked person's contact details into the contact's channels.
    async fn adopt_identities(&self, slug: &str, person_emails: &[(String, bool)], source: &str) -> Result<(), String> {
        for (email, verified) in person_emails {
            self.add_channel(slug, "email", email, &format!("Source: {source}"), *verified).await?;
        }
        Ok(())
    }

    fn evidence_path(key: &str, event: &Event) -> String {
        let at = chrono::DateTime::parse_from_rfc3339(&event.occurred_at)
            .map(|t| t.format("%Y/%m").to_string())
            .unwrap_or_else(|_| "undated".into());
        format!("sources/{key}/{at}/{}.md", safe(&event.external_id))
    }

    fn write_evidence(&self, path: &str, record_name: &str, key: &str, event: &Event, label: &str) -> Result<(), String> {
        let mut text = format!(
            "---\nsource: {}\nsource_name: {}\nexternal_id: {}\nperson_external_id: {}\ntype: {}\noccurred_at: {}\n",
            yaml(key),
            yaml(record_name),
            yaml(&event.external_id),
            yaml(&event.person_external_id),
            yaml(&event.kind),
            yaml(&event.occurred_at),
        );
        if let Some(url) = event.url.as_deref().filter(|u| !u.is_empty()) {
            text.push_str(&format!("url: {}\n", yaml(url)));
        }
        if event.deleted {
            text.push_str("deleted: true\n");
        }
        text.push_str("---\n\n");
        let title = event.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| label.to_owned());
        text.push_str(&format!("# {}\n\n", title.replace('\n', " ")));
        text.push_str(&format!("Source: {record_name} · {label} · {}\n\n", event.occurred_at));
        if event.deleted {
            text.push_str("(The source deleted this event.)\n");
        } else {
            if !event.attributes.is_empty() {
                for (k, v) in &event.attributes {
                    let shown = v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string());
                    text.push_str(&format!("- {k}: {shown}\n"));
                }
                text.push('\n');
            }
            if let Some(body) = event.body_markdown.as_deref().filter(|b| !b.trim().is_empty()) {
                text.push_str(body.trim());
                text.push('\n');
            }
        }
        let workspace = &self.docs.workspace;
        let stamp = workspace.stamp(path).map_err(|e| e.to_string())?;
        workspace
            .write(path, text.as_bytes(), if stamp.is_some() { "save" } else { "create" }, stamp.as_deref())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// After a person is linked, adds timeline entries for events already stored as evidence.
    async fn backfill(&self, key: &str, external_id: &str, slug: &str) -> Result<usize, String> {
        let dir = self.docs.workspace.file(&format!("sources/{key}")).map_err(|e| e.to_string())?;
        let mut files = Vec::new();
        collect(&dir, &mut files);
        let mut added = 0;
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else { continue };
            let field = |name: &str| {
                text.lines()
                    .take_while(|l| !l.starts_with("# "))
                    .find_map(|l| l.strip_prefix(&format!("{name}: ")))
                    .and_then(|v| serde_json::from_str::<String>(v).ok())
            };
            if field("person_external_id").as_deref() != Some(external_id) || text.contains("\ndeleted: true\n") {
                continue;
            }
            let Ok(relative) = file.strip_prefix(self.docs.workspace.root()) else { continue };
            let title = text.lines().find_map(|l| l.strip_prefix("# ")).unwrap_or("Source event").to_owned();
            let name = field("source_name").unwrap_or_else(|| key.to_owned());
            self.interaction(
                slug,
                "other",
                "intake",
                "note",
                &format!("{name} · {title}"),
                &relative.to_string_lossy(),
                &format!("gcsp:{key}:{}", field("external_id").unwrap_or_default()),
                &field("occurred_at").unwrap_or_else(now),
            )
            .await?;
            added += 1;
        }
        Ok(added)
    }
}

fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

impl App {
    fn source_record(&self, root: &str, id: &str) -> Result<Record, String> {
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT id,source_key,name,base_url,header,secret,manifest FROM sources WHERE id=?1 AND workspace=?2",
            params![id, root],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Source not found. It may have been deleted.".to_string())
        .and_then(|(_, key, name, base_url, header, secret, manifest)| {
            Ok(Record {
                key,
                name,
                base_url,
                header,
                secret,
                manifest: serde_json::from_str(&manifest).map_err(|e| e.to_string())?,
            })
        })
    }

    fn source_client(&self, record: &Record) -> Result<Client, String> {
        let token = self.secrets.open(&record.secret)?.get("token").cloned().unwrap_or_default();
        Client::new(&record.base_url, &record.header, &token)
    }

    pub async fn source_check(&self, args: &Value) -> Result<Value, String> {
        let client = Client::new(
            args["url"].as_str().unwrap_or(""),
            args["header"].as_str().unwrap_or(""),
            args["token"].as_str().unwrap_or(""),
        )?;
        let report = gc_source::check::run(&client).await;
        Ok(json!({"passed": report.passed(), "findings": report.findings, "manifest": report.manifest, "sample": report.sample}))
    }

    pub async fn source_add(&self, root: &str, args: &Value) -> Result<Value, String> {
        let url = args["url"].as_str().unwrap_or("");
        let header = args["header"].as_str().filter(|h| !h.trim().is_empty()).unwrap_or("Authorization").trim();
        let token = args["token"].as_str().unwrap_or("");
        let client = Client::new(url, header, token)?;
        let manifest = client.manifest().await.map_err(|e| e.to_string())?;
        // Confirm that people can be read with these credentials before saving them.
        client.page("people", None, 1).await.map_err(|e| e.to_string())?;
        self.service(root)?.ensure_identities().await?;
        let mut bag = gc_connect::secret::SecretBag::new();
        bag.insert("token".into(), token.into());
        let sealed = self.secrets.seal(&bag)?;
        let id = uuid::Uuid::new_v4().to_string();
        let interval = args["interval"].as_i64().unwrap_or(60).clamp(15, 24 * 60);
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let exists: Option<String> = conn
            .query_row("SELECT id FROM sources WHERE workspace=?1 AND source_key=?2", params![root, manifest.source.id], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        if exists.is_some() {
            return Err(format!("This workspace is already connected to source {} ({}).", manifest.source.name, manifest.source.id));
        }
        conn.execute(
            // The connect flow runs the first sync; the scheduler takes over after one interval.
            "INSERT INTO sources(id,workspace,source_key,name,base_url,header,secret,manifest,interval_minutes,next_sync_at,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10 + ?9 * 60000,?10)",
            params![
                id,
                root,
                manifest.source.id,
                manifest.source.name,
                gc_source::normalize_base(url)?,
                header,
                sealed,
                serde_json::to_string(&manifest).map_err(|e| e.to_string())?,
                interval,
                gc_core::now_millis()
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(json!({"id": id, "key": manifest.source.id, "name": manifest.source.name}))
    }

    pub fn source_delete(&self, root: &str, id: &str) -> Result<Value, String> {
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let removed = conn
            .execute("DELETE FROM sources WHERE id=?1 AND workspace=?2", params![id, root])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM source_cursors WHERE source=?1", params![id]).map_err(|e| e.to_string())?;
        Ok(json!({"removed": removed > 0}))
    }

    pub fn source_reset(&self, root: &str, id: &str) -> Result<Value, String> {
        self.source_record(root, id)?;
        self.db
            .conn()
            .map_err(|e| e.to_string())?
            .execute("DELETE FROM source_cursors WHERE source=?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(json!({"reset": true}))
    }

    pub async fn source_list(&self, root: &str) -> Result<Value, String> {
        let service = self.service(root)?;
        let identities = service.rows(IDENTITIES).await.unwrap_or_default();
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id,source_key,name,base_url,header,manifest,interval_minutes,next_sync_at,created_at FROM sources WHERE workspace=?1 ORDER BY created_at")
            .map_err(|e| e.to_string())?;
        let sources = stmt
            .query_map(params![root], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "key": r.get::<_, String>(1)?,
                    "name": r.get::<_, String>(2)?,
                    "url": r.get::<_, String>(3)?,
                    "header": r.get::<_, String>(4)?,
                    "manifest": serde_json::from_str::<Value>(&r.get::<_, String>(5)?).unwrap_or(Value::Null),
                    "interval": r.get::<_, i64>(6)?,
                    "next_sync_at": r.get::<_, i64>(7)?,
                    "created_at": r.get::<_, i64>(8)?,
                }))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for mut source in sources {
            let key = source["key"].as_str().unwrap_or_default().to_owned();
            let count = |m: &str| identities.iter().filter(|r| r.text("Source") == key && r.text("Match") == m && r.values.get("Deleted") != Some(&json!(true))).count();
            source["language"] = json!(self.source_language(root, &key));
            // Normalized: settings saved by older versions use the legacy Chinese label.
            source["language_setting"] = self
                .setting(&format!("source-language:{root}:{key}"))
                .as_str()
                .and_then(gc_core::schema::language)
                .map_or(Value::Null, |l| json!(l));
            source["counts"] = json!({
                "linked": count("auto") + count("confirmed"),
                "pending": count("pending"),
                "unlinked": count("unlinked"),
            });
            source["last_run"] = conn
                .query_row(
                    "SELECT status,people,events,linked,pending,skipped,error,started_at,finished_at FROM source_runs WHERE source=?1 ORDER BY started_at DESC LIMIT 1",
                    params![source["id"].as_str()],
                    |r| {
                        Ok(json!({"status":r.get::<_,String>(0)?,"people":r.get::<_,i64>(1)?,"events":r.get::<_,i64>(2)?,"linked":r.get::<_,i64>(3)?,"pending":r.get::<_,i64>(4)?,"skipped":r.get::<_,i64>(5)?,"error":r.get::<_,String>(6)?,"started_at":r.get::<_,i64>(7)?,"finished_at":r.get::<_,Option<i64>>(8)?}))
                    },
                )
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or(Value::Null);
            result.push(source);
        }
        Ok(json!(result))
    }

    fn cursor(&self, id: &str, stream: &str) -> Result<Option<String>, String> {
        self.db
            .conn()
            .map_err(|e| e.to_string())?
            .query_row("SELECT cursor FROM source_cursors WHERE source=?1 AND stream=?2", params![id, stream], |r| r.get(0))
            .optional()
            .map_err(|e| e.to_string())
    }

    fn save_cursor(&self, id: &str, stream: &str, cursor: Option<&str>) -> Result<(), String> {
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        match cursor {
            Some(c) => conn.execute(
                "INSERT INTO source_cursors(source,stream,cursor) VALUES(?1,?2,?3) ON CONFLICT(source,stream) DO UPDATE SET cursor=excluded.cursor",
                params![id, stream, c],
            ),
            None => conn.execute("DELETE FROM source_cursors WHERE source=?1 AND stream=?2", params![id, stream]),
        }
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    /// Pulls new people and events since the saved cursors. Progress is kept per page, so an
    /// interrupted run resumes where it stopped.
    pub async fn source_sync(&self, root: &str, id: &str) -> Result<Value, String> {
        let lock_key = format!("source:{id}");
        {
            let mut running = self.running.lock().map_err(|e| e.to_string())?;
            if running.contains_key(&lock_key) {
                return Err("This source is syncing.".into());
            }
            running.insert(lock_key.clone(), true);
        }
        let run = uuid::Uuid::new_v4().to_string();
        let started = gc_core::now_millis();
        let _ = self.db.conn().map(|c| {
            c.execute("INSERT INTO source_runs(id,source,status,started_at) VALUES(?1,?2,'running',?3)", params![run, id, started])
        });
        let mut counts = Counts::default();
        let result = self.sync_inner(root, id, &mut counts).await;
        self.running.lock().unwrap_or_else(|e| e.into_inner()).remove(&lock_key);
        let (status, error) = match &result {
            Ok(()) => ("completed", String::new()),
            Err(e) => ("failed", e.clone()),
        };
        let interval: i64 = self
            .db
            .conn()
            .ok()
            .and_then(|c| c.query_row("SELECT interval_minutes FROM sources WHERE id=?1", params![id], |r| r.get(0)).ok())
            .unwrap_or(60);
        if let Ok(conn) = self.db.conn() {
            let _ = conn.execute(
                "UPDATE source_runs SET status=?2,people=?3,events=?4,linked=?5,pending=?6,skipped=?7,error=?8,finished_at=?9 WHERE id=?1",
                params![run, status, counts.people, counts.events, counts.linked, counts.pending, counts.skipped, error, gc_core::now_millis()],
            );
            let _ = conn.execute("UPDATE sources SET next_sync_at=?2 WHERE id=?1", params![id, gc_core::now_millis() + interval * 60000]);
        }
        (self.notify)("workspace-changed", json!({"root":root}));
        result?;
        Ok(json!({"people":counts.people,"events":counts.events,"linked":counts.linked,"pending":counts.pending,"skipped":counts.skipped}))
    }

    async fn sync_inner(&self, root: &str, id: &str, counts: &mut Counts) -> Result<(), String> {
        let mut record = self.source_record(root, id)?;
        let client = self.source_client(&record)?;
        let service = self.service(root)?;
        service.ensure_identities().await?;
        // Refresh the manifest: the attribute whitelist and labels may have changed.
        let manifest = client.manifest().await.map_err(|e| e.to_string())?;
        if manifest.source.id != record.key {
            return Err(format!("The source ID changed from {} to {}. Remove the source and connect it again.", record.key, manifest.source.id));
        }
        self.db
            .conn()
            .map_err(|e| e.to_string())?
            .execute(
                "UPDATE sources SET manifest=?2,name=?3 WHERE id=?1",
                params![id, serde_json::to_string(&manifest).map_err(|e| e.to_string())?, manifest.source.name],
            )
            .map_err(|e| e.to_string())?;
        record.name = manifest.source.name.clone();
        record.manifest = manifest;

        let mut identities: HashMap<String, Row> = service
            .rows(IDENTITIES)
            .await?
            .into_iter()
            .filter(|r| r.text("Source") == record.key)
            .map(|r| (r.text("External ID").to_owned(), r))
            .collect();
        let mut emails = service.email_index().await?;
        let mut cursor = self.cursor(id, "people")?;
        for _ in 0..MAX_PAGES {
            let page = self.fetch(&client, id, "people", cursor.as_deref()).await?;
            // Decide every person of the page, then write all identity rows in one kit run.
            let mut plans = Vec::new();
            for item in &page.items {
                match parse_person(item) {
                    Ok(person) => {
                        counts.people += 1;
                        if let Some(plan) = plan_person(&record, &identities, &mut emails, &person) {
                            plans.push(plan);
                        }
                    }
                    Err(_) => counts.skipped += 1,
                }
            }
            let written = service
                .write_rows(IDENTITIES, plans.iter().map(|p| (p.row, p.values.clone())).collect(), "system:sources")
                .await?;
            for plan in &plans {
                match &plan.purge {
                    Some(purge) => service.purge_source_person(&record.key, &plan.external_id, purge).await?,
                    None => service.write_person_file(&record.name, &record.key, plan)?,
                }
            }
            for (plan, answer) in plans.into_iter().zip(written) {
                let row = answer["id"].as_i64().or(plan.row).ok_or("The identity record returned no id after writing")?;
                let mut values = identities.get(&plan.external_id).map(|r| r.values.clone()).unwrap_or_default();
                values.extend(plan.values);
                identities.insert(plan.external_id.clone(), Row { id: row, values });
                if let Some((slug, person_emails)) = plan.linked {
                    service.adopt_identities(&slug, &person_emails, &record.name).await?;
                    service.backfill(&record.key, &plan.external_id, &slug).await?;
                }
            }
            if !self.advance(id, "people", &page, &mut cursor)? {
                break;
            }
        }
        let live = || identities.values().filter(|r| r.values.get("Deleted") != Some(&json!(true)));
        counts.linked = live().filter(|r| LINKED.contains(&r.text("Match"))).count() as i64;
        counts.pending = live().filter(|r| r.text("Match") == "pending").count() as i64;

        if !record.manifest.can("events") {
            return Ok(());
        }
        let labels: HashMap<&str, &str> = record
            .manifest
            .event_types
            .iter()
            .map(|t| (t.kind.as_str(), t.label.as_deref().unwrap_or(t.kind.as_str())))
            .collect();
        let linked: HashMap<&str, &str> = identities
            .iter()
            .filter(|(_, r)| LINKED.contains(&r.text("Match")))
            .map(|(k, r)| (k.as_str(), r.text("Contact")))
            .collect();
        // Timeline entries already present, so updated events do not add duplicates.
        let mut seen: std::collections::HashSet<(String, String)> = service
            .rows("interactions")
            .await?
            .into_iter()
            .filter(|r| r.text("Message-ID").starts_with("gcsp:"))
            .map(|r| (r.text("Contact").to_owned(), r.text("Message-ID").to_owned()))
            .collect();
        let mut cursor = self.cursor(id, "events")?;
        for _ in 0..MAX_PAGES {
            let page = self.fetch(&client, id, "events", cursor.as_deref()).await?;
            // Evidence files first, then the page's new timeline rows in one kit run.
            let mut timeline = Vec::new();
            for item in &page.items {
                match parse_event(item) {
                    Ok(event) => {
                        let label = labels.get(event.kind.as_str()).copied().unwrap_or(event.kind.as_str()).to_owned();
                        let path = Service::evidence_path(&record.key, &event);
                        service.write_evidence(&path, &record.name, &record.key, &event, &label)?;
                        counts.events += 1;
                        let slug = linked.get(event.person_external_id.as_str()).copied().unwrap_or("");
                        let message_id = format!("gcsp:{}:{}", record.key, event.external_id);
                        if !slug.is_empty() && !event.deleted && seen.insert((slug.to_owned(), message_id.clone())) {
                            let title = event.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or(label);
                            timeline.push((
                                None,
                                json!({"Contact":slug,"Channel":"other","Kind":"intake","Direction":"note","Title":format!("{} · {title}", record.name),"File":path,"Message-ID":message_id,"At":event.occurred_at})
                                    .as_object()
                                    .cloned()
                                    .unwrap_or_default(),
                            ));
                        }
                    }
                    Err(_) => counts.skipped += 1,
                }
            }
            service.write_rows("interactions", timeline, "system:sources").await?;
            if !self.advance(id, "events", &page, &mut cursor)? {
                break;
            }
        }
        Ok(())
    }

    async fn fetch(&self, client: &Client, id: &str, stream: &str, cursor: Option<&str>) -> Result<gc_source::Page, String> {
        match client.page(stream, cursor, PAGE).await {
            Ok(page) => Ok(page),
            Err(SourceError::InvalidCursor) => {
                self.save_cursor(id, stream, None)?;
                Err(SourceError::InvalidCursor.to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Saves the page's cursor once its items are stored; returns whether another page follows.
    fn advance(&self, id: &str, stream: &str, page: &gc_source::Page, cursor: &mut Option<String>) -> Result<bool, String> {
        if page.next_cursor.is_some() {
            self.save_cursor(id, stream, page.next_cursor.as_deref())?;
        }
        let more = page.has_more && page.next_cursor != *cursor;
        *cursor = page.next_cursor.clone();
        Ok(more)
    }

    /// Link, confirm, reject or create a contact for one external identity.
    pub async fn identity_decide(&self, root: &str, args: &Value) -> Result<Value, String> {
        let service = self.service(root)?;
        let key = args["source"].as_str().ok_or("Missing source")?;
        let external = args["external_id"].as_str().ok_or("Missing external ID")?;
        let action = args["action"].as_str().unwrap_or("");
        let row = service
            .rows(IDENTITIES)
            .await?
            .into_iter()
            .find(|r| r.text("Source") == key && r.text("External ID") == external)
            .ok_or("This source identity wasn't found. Sync again.")?;
        let emails: Vec<(String, bool)> = serde_json::from_str::<Value>(row.text("Attributes"))
            .ok()
            .and_then(|v| serde_json::from_value(v["_emails"].clone()).ok())
            .unwrap_or_default();
        let source_name = self
            .db
            .conn()
            .ok()
            .and_then(|c| c.query_row("SELECT name FROM sources WHERE workspace=?1 AND source_key=?2", params![root, key], |r| r.get::<_, String>(0)).ok())
            .unwrap_or_else(|| key.to_owned());
        let slug = match action {
            "reject" => {
                write_identity(&service, Some(row.id), json!({"Match":"rejected"})).await?;
                return Ok(json!({"match":"rejected"}));
            }
            "confirm" => row.text("Contact").to_owned(),
            "link" => {
                let slug = args["slug"].as_str().ok_or("Choose a contact")?;
                service.contact(slug).await?;
                slug.to_owned()
            }
            "create" => {
                let name = if row.text("Name").trim().is_empty() { external } else { row.text("Name") };
                let mut values = Map::new();
                values.insert("Name".into(), json!(name));
                // Drafts follow the contact's language: the person's own locale, else the
                // source's language setting, else what the source declares for its people.
                let locale = serde_json::from_str::<Value>(row.text("Attributes")).ok().and_then(|v| v["_locale"].as_str().map(str::to_owned));
                if let Some(language) = locale.as_deref().map(language_of).or_else(|| self.source_language(root, key)) {
                    values.insert("Language".into(), json!(language));
                }
                if let Some((email, _)) = emails.iter().find(|(_, v)| *v).or(emails.first()) {
                    values.insert("Email".into(), json!(email));
                }
                service.create_contact(values).await?
            }
            _ => return Err("Unknown action.".into()),
        };
        if slug.is_empty() {
            return Err("This identity has no candidate contact yet. Choose who to link it to.".into());
        }
        write_identity(&service, Some(row.id), json!({"Match":"confirmed","Contact":slug})).await?;
        service.adopt_identities(&slug, &emails, &source_name).await?;
        let added = service.backfill(key, external, &slug).await?;
        Ok(json!({"match":"confirmed","contact":slug,"events":added}))
    }

    /// External identities for a page: one source's people, filtered by match state.
    pub async fn source_people(&self, root: &str, args: &Value) -> Result<Value, String> {
        let service = self.service(root)?;
        let key = args["source"].as_str();
        let filter = args["match"].as_str();
        let rows = service.rows(IDENTITIES).await.unwrap_or_default();
        Ok(json!(rows
            .into_iter()
            .filter(|r| key.is_none_or(|k| r.text("Source") == k))
            .filter(|r| filter.is_none_or(|m| r.text("Match") == m))
            .filter(|r| r.values.get("Deleted") != Some(&json!(true)))
            .map(identity_json)
            .collect::<Vec<_>>()))
    }

    /// Each connected source's name and manifest, by source key.
    fn manifests(&self, root: &str) -> Result<HashMap<String, (String, Manifest)>, String> {
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT source_key,name,manifest FROM sources WHERE workspace=?1").map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![root], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter_map(|(k, n, m)| serde_json::from_str(&m).ok().map(|m| (k, (n, m))))
            .collect();
        Ok(rows)
    }

    /// One source person as shown to people and the Agent: labeled facts and attribute history.
    fn describe_identity(service: &Service, row: Row, manifests: &HashMap<String, (String, Manifest)>) -> Value {
        let mut item = identity_json(row);
        let key = item["source"].as_str().unwrap_or_default().to_owned();
        item["history"] = json!(service.person_history(&key, item["external_id"].as_str().unwrap_or_default()));
        if let Some((name, manifest)) = manifests.get(&key) {
            item["source_name"] = json!(name);
            let labels: HashMap<&str, &str> = manifest.attributes.iter().map(|a| (a.key.as_str(), a.label.as_deref().unwrap_or(a.key.as_str()))).collect();
            for entry in item["history"].as_array_mut().into_iter().flatten() {
                if let Some(label) = entry["key"].as_str().and_then(|k| labels.get(k)) {
                    entry["label"] = json!(label);
                }
            }
            let attributes = item["attributes"].as_object().cloned().unwrap_or_default();
            item["facts"] = json!(manifest
                .attributes
                .iter()
                .filter_map(|a| attributes.get(&a.key).filter(|v| !v.is_null()).map(|v| json!({"key":a.key,"label":a.label.clone().unwrap_or(a.key.clone()),"type":a.kind,"pii":a.pii,"value":v})))
                .collect::<Vec<_>>());
        }
        item
    }

    /// Source facts for one contact: linked identities with labeled attributes, and suggestions.
    pub async fn source_person(&self, root: &str, slug: &str) -> Result<Value, String> {
        let service = self.service(root)?;
        let manifests = self.manifests(root)?;
        let rows = service.rows(IDENTITIES).await.unwrap_or_default();
        let items: Vec<Value> = rows
            .into_iter()
            .filter(|r| r.text("Contact") == slug && r.values.get("Deleted") != Some(&json!(true)))
            .filter(|r| LINKED.contains(&r.text("Match")) || r.text("Match") == "pending")
            .map(|r| Self::describe_identity(&service, r, &manifests))
            .collect();
        Ok(json!(items))
    }

    /// Any source person, linked or not: facts, attribute history and their events (newest first).
    pub async fn source_identity(&self, root: &str, source: &str, external_id: &str) -> Result<Value, String> {
        let service = self.service(root)?;
        let manifests = self.manifests(root)?;
        let row = service
            .rows(IDENTITIES)
            .await?
            .into_iter()
            .find(|r| r.text("Source") == source && r.text("External ID") == external_id && r.values.get("Deleted") != Some(&json!(true)))
            .ok_or("This person isn't in the source, or was deleted.")?;
        let mut item = Self::describe_identity(&service, row, &manifests);
        let labels: HashMap<String, String> = manifests
            .get(source)
            .map(|(_, m)| m.event_types.iter().map(|t| (t.kind.clone(), t.label.clone().unwrap_or(t.kind.clone()))).collect())
            .unwrap_or_default();
        let mut events = service.person_events(source, external_id);
        for event in &mut events {
            if let Some(label) = event["type"].as_str().and_then(|k| labels.get(k)) {
                event["label"] = json!(label);
            }
        }
        item["events"] = json!(events);
        Ok(item)
    }

    /// Contact language for people created from a source: the user's setting for that source,
    /// else the manifest's default_locale. None leaves the workspace default.
    pub fn source_language(&self, root: &str, key: &str) -> Option<&'static str> {
        // Settings saved by older versions use the legacy Chinese label; both are accepted.
        if let Some(language) = self.setting(&format!("source-language:{root}:{key}")).as_str().and_then(gc_core::schema::language) {
            return Some(language);
        }
        let manifest: String = self
            .db
            .conn()
            .ok()?
            .query_row("SELECT manifest FROM sources WHERE workspace=?1 AND source_key=?2", params![root, key], |r| r.get(0))
            .ok()?;
        let manifest: Manifest = serde_json::from_str(&manifest).ok()?;
        manifest.source.default_locale.as_deref().filter(|l| !l.trim().is_empty()).map(language_of)
    }

    /// Searches source people (linked or not) by name, email or attribute value, for the Agent.
    pub async fn source_search(&self, root: &str, args: &Value) -> Result<Value, String> {
        let service = self.service(root)?;
        let source = args["source"].as_str().filter(|s| !s.is_empty());
        let state = args["match"].as_str().filter(|s| !s.is_empty());
        let query = args["query"].as_str().unwrap_or("").trim().to_lowercase();
        let limit = args["limit"].as_u64().unwrap_or(50).clamp(1, 200) as usize;
        let matched: Vec<Value> = service
            .rows(IDENTITIES)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.values.get("Deleted") != Some(&json!(true)))
            .filter(|r| source.is_none_or(|k| r.text("Source") == k))
            .filter(|r| state.is_none_or(|m| r.text("Match") == m || (m == "linked" && LINKED.contains(&r.text("Match")))))
            .map(identity_json)
            .filter(|p| query.is_empty() || p.to_string().to_lowercase().contains(&query))
            .collect();
        let total = matched.len();
        Ok(json!({"total": total, "items": matched.into_iter().take(limit).collect::<Vec<_>>()}))
    }

    /// Syncs every source of `root` whose interval has elapsed.
    pub async fn sync_due_sources(&self, root: &str) {
        let due: Vec<String> = self
            .db
            .conn()
            .ok()
            .and_then(|c| {
                let mut stmt = c.prepare("SELECT id FROM sources WHERE workspace=?1 AND next_sync_at<=?2").ok()?;
                let ids = stmt.query_map(params![root, gc_core::now_millis()], |r| r.get(0)).ok()?.filter_map(Result::ok).collect();
                Some(ids)
            })
            .unwrap_or_default();
        for id in due {
            if let Err(error) = self.source_sync(root, &id).await {
                (self.notify)("background-error", json!({"message": format!("Source sync failed: {error}")}));
            }
        }
    }
}

fn identity_json(r: Row) -> Value {
    let attributes: Value = serde_json::from_str(r.text("Attributes")).unwrap_or(json!({}));
    let emails = attributes.get("_emails").cloned().unwrap_or(json!([]));
    let mut shown = attributes;
    if let Some(map) = shown.as_object_mut() {
        map.remove("_emails");
    }
    json!({
        "row": r.id,
        "name": r.text("Name"),
        "source": r.text("Source"),
        "external_id": r.text("External ID"),
        "contact": r.text("Contact"),
        "match": r.text("Match"),
        "emails": emails,
        "attributes": shown,
        "url": r.text("URL"),
        "updated_at": r.text("Updated at"),
        "synced_at": r.text("Synced at"),
    })
}

async fn write_identity(service: &Service, id: Option<i64>, values: Value) -> Result<Value, String> {
    service
        .write_row(IDENTITIES, id, values.as_object().ok_or("identity values")?, "system:sources")
        .await
}

struct Plan {
    external_id: String,
    row: Option<i64>,
    values: Map<String, Value>,
    /// Declared attributes that changed since the last sync: (key, before, after).
    changes: Vec<(String, Value, Value)>,
    attributes: Map<String, Value>,
    name: String,
    fresh: bool,
    /// Set when this sync newly links the person automatically: contact slug and emails.
    linked: Option<(String, Vec<(String, bool)>)>,
    /// Set for a tombstone of a known person: what to remove locally.
    purge: Option<Purge>,
}

/// A removed source person: their stored data is dropped; if they opted out, their
/// addresses join the do-not-contact list and a linked contact moves to that stage.
struct Purge {
    emails: Vec<String>,
    opted_out: bool,
    contact: Option<String>,
}

/// Decides one person's identity row and match state, without writing.
fn plan_person(
    record: &Record,
    identities: &HashMap<String, Row>,
    emails: &mut HashMap<String, Vec<String>>,
    person: &Person,
) -> Option<Plan> {
    let existing = identities.get(&person.external_id);
    if person.deleted {
        let row = existing?;
        let emails = serde_json::from_str::<Value>(row.text("Attributes"))
            .ok()
            .and_then(|v| serde_json::from_value::<Vec<(String, bool)>>(v["_emails"].clone()).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|(e, _)| e)
            .collect();
        let contact = Some(row.text("Contact").to_owned()).filter(|c| !c.is_empty() && LINKED.contains(&row.text("Match")));
        return Some(Plan {
            external_id: person.external_id.clone(),
            row: Some(row.id),
            // The source removed this person: keep only the fact that they existed here.
            values: json!({"Deleted": true, "Name": "", "Attributes": "{}", "Updated at": person.updated_at, "Synced at": now()}).as_object().cloned().unwrap_or_default(),
            linked: None,
            changes: Vec::new(),
            attributes: Map::new(),
            name: String::new(),
            fresh: false,
            purge: Some(Purge { emails, opted_out: person.reason.as_deref() == Some("opted_out"), contact }),
        });
    }
    let person_emails: Vec<(String, bool)> = person.emails().map(|e| (e.value.trim().to_lowercase(), e.verified)).collect();
    let declared = person.declared_attributes(&record.manifest);
    let before: Map<String, Value> = existing
        .and_then(|r| serde_json::from_str::<Value>(r.text("Attributes")).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    let mut changes = Vec::new();
    if existing.is_some() {
        let keys: std::collections::BTreeSet<&String> = declared.keys().chain(before.keys().filter(|k| *k != "_emails")).collect();
        for key in keys {
            let old = before.get(key.as_str()).cloned().unwrap_or(Value::Null);
            let new = declared.get(key.as_str()).cloned().unwrap_or(Value::Null);
            if old != new {
                changes.push((key.clone(), old, new));
            }
        }
    }
    let mut attributes = declared.clone();
    attributes.insert("_emails".into(), json!(person_emails));
    if let Some(locale) = person.locale.as_deref().filter(|l| !l.trim().is_empty()) {
        attributes.insert("_locale".into(), json!(locale.trim()));
    }
    let previous = existing.map(|r| r.text("Match").to_owned()).unwrap_or_default();
    let previous_contact = existing.map(|r| r.text("Contact").to_owned()).unwrap_or_default();
    // A person's decision (confirmed / rejected) and earlier automatic links stand.
    let (state, contact) = if matches!(previous.as_str(), "confirmed" | "rejected" | "auto") {
        (previous.clone(), previous_contact)
    } else {
        let mut verified: Vec<&String> = Vec::new();
        let mut any: Vec<&String> = Vec::new();
        for (email, is_verified) in &person_emails {
            for slug in emails.get(email).into_iter().flatten() {
                if *is_verified && !verified.contains(&slug) {
                    verified.push(slug);
                }
                if !any.contains(&slug) {
                    any.push(slug);
                }
            }
        }
        if verified.len() == 1 {
            ("auto".to_owned(), verified[0].clone())
        } else if let Some(first) = any.first() {
            ("pending".to_owned(), (*first).clone())
        } else {
            ("unlinked".to_owned(), String::new())
        }
    };
    let linked = (state == "auto" && previous != "auto").then(|| {
        for (email, _) in &person_emails {
            let slugs = emails.entry(email.clone()).or_default();
            if !slugs.contains(&contact) {
                slugs.push(contact.clone());
            }
        }
        (contact.clone(), person_emails.clone())
    });
    let values = json!({
        "Name": person.display_name.clone().unwrap_or_default(),
        "Source": record.key,
        "External ID": person.external_id,
        "Contact": contact,
        "Match": state,
        "Emails": person_emails.iter().map(|(e, v)| if *v { format!("{e} ✓") } else { e.clone() }).collect::<Vec<_>>().join(", "),
        "Attributes": Value::Object(attributes).to_string(),
        "URL": person.url.clone().unwrap_or_default(),
        "Updated at": person.updated_at,
        "Synced at": now(),
        "Deleted": false,
    });
    Some(Plan {
        external_id: person.external_id.clone(),
        row: existing.map(|r| r.id),
        values: values.as_object().cloned().unwrap_or_default(),
        linked,
        changes,
        attributes: declared,
        name: person.display_name.clone().unwrap_or_default(),
        fresh: existing.is_none(),
        purge: None,
    })
}

fn shown(value: &Value) -> String {
    match value {
        Value::Null => "(empty)".into(),
        Value::String(s) => s.replace('\n', " "),
        other => other.to_string(),
    }
}

impl Service {
    fn person_path(key: &str, external_id: &str) -> String {
        format!("sources/{key}/people/{}.md", safe(external_id))
    }

    /// Keeps one file per source person: current declared attributes and an append-only log of
    /// attribute changes, so the workspace remembers how a user's situation evolved.
    fn write_person_file(&self, record_name: &str, key: &str, plan: &Plan) -> Result<(), String> {
        if !plan.fresh && plan.changes.is_empty() {
            return Ok(());
        }
        let path = Self::person_path(key, &plan.external_id);
        let workspace = &self.docs.workspace;
        let stamp = workspace.stamp(&path).map_err(|e| e.to_string())?;
        let previous = if stamp.is_some() { std::fs::read_to_string(workspace.file(&path).map_err(|e| e.to_string())?).unwrap_or_default() } else { String::new() };
        let mut history: Vec<String> = history_section(&previous)
            .map(|s| s.lines().filter(|l| l.starts_with("- ")).map(str::to_owned).collect())
            .unwrap_or_default();
        let at = now();
        if plan.fresh && history.is_empty() {
            history.push(format!("- {at} · First sync"));
        }
        for (k, from, to) in &plan.changes {
            history.push(format!("- {at} · {k}: {} → {}", shown(from), shown(to)));
        }
        let mut text = format!(
            "---\nsource: {}\nsource_name: {}\nexternal_id: {}\n---\n\n# {}\n\n## Current attributes\n\n",
            yaml(key),
            yaml(record_name),
            yaml(&plan.external_id),
            if plan.name.is_empty() { &plan.external_id } else { &plan.name }
        );
        for (k, v) in &plan.attributes {
            text.push_str(&format!("- {k}: {}\n", shown(v)));
        }
        text.push_str(&format!("\n{HISTORY}\n\n"));
        for line in &history {
            text.push_str(line);
            text.push('\n');
        }
        workspace
            .write(&path, text.as_bytes(), if stamp.is_some() { "save" } else { "create" }, stamp.as_deref())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Removes what was stored about a person the source deleted: their history file and
    /// event evidence (reduced to a stub). If they opted out, their addresses join
    /// suppression.txt and a linked contact moves to the do-not-contact stage.
    async fn purge_source_person(&self, key: &str, external_id: &str, purge: &Purge) -> Result<(), String> {
        let workspace = &self.docs.workspace;
        if let Ok(path) = workspace.file(&Self::person_path(key, external_id))
            && path.is_file()
        {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        let dir = workspace.file(&format!("sources/{key}")).map_err(|e| e.to_string())?;
        let mut files = Vec::new();
        collect(&dir, &mut files);
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else { continue };
            let head: Vec<&str> = text.lines().take_while(|l| !l.starts_with("# ")).collect();
            let field = |name: &str| head.iter().find_map(|l| l.strip_prefix(&format!("{name}: "))).and_then(|v| serde_json::from_str::<String>(v).ok());
            if field("person_external_id").as_deref() != Some(external_id) || head.contains(&"deleted: true") {
                continue;
            }
            let Ok(relative) = file.strip_prefix(workspace.root()) else { continue };
            let relative = relative.to_string_lossy().into_owned();
            let stub = format!(
                "---\nsource: {}\nexternal_id: {}\nperson_external_id: {}\ndeleted: true\n---\n\n# Deleted\n\nThe source deleted this person's data.\n",
                yaml(key),
                yaml(&field("external_id").unwrap_or_default()),
                yaml(external_id)
            );
            let stamp = workspace.stamp(&relative).map_err(|e| e.to_string())?;
            workspace.write(&relative, stub.as_bytes(), "save", stamp.as_deref()).map_err(|e| e.to_string())?;
        }
        if purge.opted_out {
            for email in &purge.emails {
                gc_core::lists::suppress(workspace, email, "source: they chose not to be contacted").map_err(|e| e.to_string())?;
            }
            if let Some(slug) = &purge.contact {
                let config = self.workspace_config()?;
                if let Some(stage) = config.stages.iter().find(|s| s.role == gc_core::config::Role::Suppressed)
                    && self.contact(slug).await.is_ok()
                {
                    self.update_contact(slug, json!({"Stage": stage.label}).as_object().cloned().unwrap_or_default(), false).await?;
                }
            }
        }
        Ok(())
    }

    /// Events stored as evidence for one source person, newest first (at most 100).
    pub fn person_events(&self, key: &str, external_id: &str) -> Vec<Value> {
        let Ok(dir) = self.docs.workspace.file(&format!("sources/{key}")) else { return Vec::new() };
        let mut files = Vec::new();
        collect(&dir, &mut files);
        let mut events: Vec<Value> = files
            .iter()
            .filter_map(|file| {
                let text = std::fs::read_to_string(file).ok()?;
                let head: Vec<&str> = text.lines().take_while(|l| !l.starts_with("# ")).collect();
                let field = |name: &str| head.iter().find_map(|l| l.strip_prefix(&format!("{name}: "))).and_then(|v| serde_json::from_str::<String>(v).ok());
                if field("person_external_id").as_deref() != Some(external_id) || head.contains(&"deleted: true") {
                    return None;
                }
                let path = file.strip_prefix(self.docs.workspace.root()).ok()?.to_string_lossy().into_owned();
                let title = text.lines().find_map(|l| l.strip_prefix("# ")).unwrap_or("").to_owned();
                Some(json!({"external_id": field("external_id"), "type": field("type"), "occurred_at": field("occurred_at"), "title": title, "path": path}))
            })
            .collect();
        events.sort_by(|a, b| b["occurred_at"].as_str().cmp(&a["occurred_at"].as_str()));
        events.truncate(100);
        events
    }

    /// Attribute changes recorded for one source person, oldest first.
    pub fn person_history(&self, key: &str, external_id: &str) -> Vec<Value> {
        let Ok(path) = self.docs.workspace.file(&Self::person_path(key, external_id)) else { return Vec::new() };
        let text = std::fs::read_to_string(path).unwrap_or_default();
        history_section(&text)
            .map(|s| {
                s.lines()
                    .filter_map(|l| l.strip_prefix("- "))
                    .filter_map(|l| {
                        let (at, rest) = l.split_once(" · ")?;
                        Some(match split_key(rest).and_then(|(k, v)| v.split_once(" → ").map(|(f, t)| (k, f, t))) {
                            Some((key, from, to)) => json!({"at": at, "key": key, "from": from, "to": to}),
                            None => json!({"at": at, "note": rest}),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Heading of the attribute-change log in a source person file.
const HISTORY: &str = "## History";
/// The same heading in person files written by older versions.
const LEGACY_HISTORY: &str = "## 变化记录";

/// The text after a person file's change-log heading (current or legacy).
fn history_section(text: &str) -> Option<&str> {
    text.split_once(HISTORY).or_else(|| text.split_once(LEGACY_HISTORY)).map(|(_, rest)| rest)
}

/// Splits `key: value` at the first separator: ": ", or the full-width colon that older versions
/// wrote.
fn split_key(line: &str) -> Option<(&str, &str)> {
    let ascii = line.find(": ").map(|i| (i, 2));
    let legacy = line.find('：').map(|i| (i, '：'.len_utf8()));
    let (at, len) = match (ascii, legacy) {
        (Some(a), Some(b)) => if a.0 < b.0 { a } else { b },
        (a, b) => a.or(b)?,
    };
    Some((&line[..at], &line[at + len..]))
}

/// The contact Language for a BCP 47 locale: Chinese for zh-*, English otherwise.
fn language_of(locale: &str) -> &'static str {
    if locale.trim().to_lowercase().starts_with("zh") { "Chinese" } else { "English" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn person_history_reads_current_and_legacy_files() {
        let current = "# A\n\n## Current attributes\n\n- plan: pro\n\n## History\n\n- 2026-11-01 · plan: free → pro\n";
        assert!(history_section(current).unwrap().contains("plan: free → pro"));
        // Files written by older versions use a Chinese heading and full-width colons.
        let legacy = "# A\n\n## 当前属性\n\n- plan：pro\n\n## 变化记录\n\n- 2026-11-01 · plan：free → pro\n";
        assert!(history_section(legacy).unwrap().contains("plan：free → pro"));
        assert_eq!(split_key("plan：free → pro"), Some(("plan", "free → pro")));
        assert_eq!(split_key("note: a：b"), Some(("note", "a：b")));
        assert_eq!(split_key("First sync"), None);
        assert_eq!(language_of("zh-CN"), "Chinese");
        assert_eq!(language_of("fr"), "English");
    }
}
