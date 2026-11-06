//! Outreach lists: select people from contacts and data sources, let the Agent prepare one
//! reviewed draft per person through MCP, and guard sending (do-not-contact list, daily limit).
//! Nothing here sends: drafts wait for a person's approval and explicit send, as everywhere else.
use crate::{Service, app::App, contacts::Row, notes::metadata};
use gc_core::config::Role;
use gc_core::lists::{self, ListSpec, Member};
use rusqlite::params;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

const LINKED: &[&str] = &["auto", "confirmed"];

/// Events per (source, external id), read from the evidence files written by source sync.
fn event_index(service: &Service, source: &str) -> HashMap<String, Vec<(String, String)>> {
    let mut index: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let Ok(dir) = service.docs.workspace.file(&format!("sources/{source}")) else { return index };
    let mut stack = vec![dir];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(entry.path());
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
            let head: Vec<&str> = text.lines().take_while(|l| !l.starts_with("# ")).collect();
            if head.contains(&"deleted: true") {
                continue;
            }
            let field = |name: &str| {
                head.iter()
                    .find_map(|l| l.strip_prefix(&format!("{name}: ")))
                    .and_then(|v| serde_json::from_str::<String>(v).ok())
                    .unwrap_or_default()
            };
            let person = field("person_external_id");
            if !person.is_empty() {
                index.entry(person).or_default().push((field("type"), field("occurred_at")));
            }
        }
    }
    index
}

fn attributes(row: &Row) -> (Map<String, Value>, Vec<String>) {
    let mut attrs: Map<String, Value> = serde_json::from_str::<Value>(row.text("Attributes"))
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    let emails = attrs
        .remove("_emails")
        .and_then(|v| serde_json::from_value::<Vec<(String, bool)>>(v).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|(e, _)| e)
        .collect();
    (attrs, emails)
}

fn start_of_day() -> i64 {
    use chrono::{Local, TimeZone};
    let today = Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap_or_default();
    Local.from_local_datetime(&today).earliest().map(|t| t.timestamp_millis()).unwrap_or(0)
}

impl App {
    fn spec(&self, root: &str, id: &str) -> Result<ListSpec, String> {
        let service = self.service(root)?;
        lists::load(&service.docs.workspace)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|l| l.id == id)
            .ok_or_else(|| "List not found.".to_string())
    }

    /// Candidates for a list, before filtering.
    async fn candidates(&self, service: &Service, spec: &ListSpec) -> Result<Vec<Member>, String> {
        let config = service.workspace_config()?;
        let suppressed: HashSet<String> = lists::suppression(&service.docs.workspace).map_err(|e| e.to_string())?.into_iter().collect();
        let contacts: HashMap<String, Row> = service.rows("contacts").await?.into_iter().map(|r| (r.text("Slug").to_owned(), r)).collect();
        let identities = service.rows("identities").await.unwrap_or_default();
        let contact_member = |row: &Row| {
            let email = row.text("Email").trim().to_lowercase();
            Member {
                key: format!("contact:{}", row.text("Slug")),
                name: row.text("Name").to_owned(),
                email: email.clone(),
                contact: Some(row.text("Slug").to_owned()),
                values: row.values.clone(),
                role: config.role(row.text("Stage")),
                stage: row.text("Stage").to_owned(),
                suppressed: !email.is_empty() && suppressed.contains(&email),
                ..Default::default()
            }
        };
        let mut members = Vec::new();
        if spec.source.is_empty() {
            // Contacts, with attributes and events from any linked identity.
            let mut linked: HashMap<&str, Vec<&Row>> = HashMap::new();
            for row in identities.iter().filter(|r| LINKED.contains(&r.text("Match")) && r.values.get("Deleted") != Some(&json!(true))) {
                linked.entry(row.text("Contact")).or_default().push(row);
            }
            let mut events_by_source: HashMap<String, HashMap<String, Vec<(String, String)>>> = HashMap::new();
            for row in contacts.values() {
                let mut member = contact_member(row);
                for identity in linked.get(row.text("Slug")).into_iter().flatten() {
                    let (attrs, emails) = attributes(identity);
                    member.attributes.extend(attrs);
                    member.suppressed |= emails.iter().any(|e| suppressed.contains(e));
                    let source = identity.text("Source").to_owned();
                    let events = events_by_source.entry(source.clone()).or_insert_with(|| event_index(service, &source));
                    member.events.extend(events.get(identity.text("External ID")).cloned().unwrap_or_default());
                }
                members.push(member);
            }
        } else {
            let events = event_index(service, &spec.source);
            for row in identities.iter().filter(|r| r.text("Source") == spec.source && r.values.get("Deleted") != Some(&json!(true))) {
                let (attrs, emails) = attributes(row);
                let linked = LINKED.contains(&row.text("Match")).then(|| contacts.get(row.text("Contact"))).flatten();
                let mut member = match linked {
                    Some(contact) => contact_member(contact),
                    None => Member {
                        key: format!("source:{}:{}", spec.source, row.text("External ID")),
                        name: row.text("Name").to_owned(),
                        email: emails.first().cloned().unwrap_or_default(),
                        ..Default::default()
                    },
                };
                // Key source lists by the source identity, which stays stable when the person
                // is linked to (or becomes) a contact during preparation.
                member.key = format!("source:{}:{}", spec.source, row.text("External ID"));
                member.identity = Some((spec.source.clone(), row.text("External ID").to_owned()));
                if member.name.is_empty() {
                    member.name = row.text("Name").to_owned();
                }
                member.attributes = attrs;
                member.suppressed |= emails.iter().any(|e| suppressed.contains(e));
                member.events = events.get(row.text("External ID")).cloned().unwrap_or_default();
                members.push(member);
            }
        }
        Ok(members)
    }

    async fn selected(&self, root: &str, spec: &ListSpec) -> Result<Vec<Member>, String> {
        let service = self.service(root)?;
        let config = service.workspace_config()?;
        let candidates = self.candidates(&service, spec).await?;
        // Display names for attribute keys and event types, from the source manifests.
        let mut labels: HashMap<String, String> = HashMap::new();
        if let Ok(list) = self.source_list(root).await {
            for source in list.as_array().into_iter().flatten() {
                for a in source["manifest"]["attributes"].as_array().into_iter().flatten() {
                    if let (Some(k), Some(l)) = (a["key"].as_str(), a["label"].as_str()) {
                        labels.insert(k.to_owned(), l.to_owned());
                    }
                }
                for t in source["manifest"]["event_types"].as_array().into_iter().flatten() {
                    if let (Some(k), Some(l)) = (t["type"].as_str(), t["label"].as_str()) {
                        labels.insert(k.to_owned(), l.to_owned());
                    }
                }
            }
        }
        Ok(spec.select_labeled(candidates, &|stage: &str| config.role(stage), &|k: &str| labels.get(k).cloned().unwrap_or_else(|| k.to_owned())))
    }

    fn items(&self, root: &str, list: &str) -> Result<HashMap<String, Value>, String> {
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT member,name,contact,status,draft,evidence,error,updated_at FROM list_items WHERE workspace=?1 AND list=?2")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![root, list], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    json!({"name":r.get::<_,String>(1)?,"contact":r.get::<_,String>(2)?,"status":r.get::<_,String>(3)?,"draft":r.get::<_,String>(4)?,"evidence":serde_json::from_str::<Value>(&r.get::<_,String>(5)?).unwrap_or(json!([])),"error":r.get::<_,String>(6)?,"updated_at":r.get::<_,i64>(7)?}),
                ))
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    fn set_item(&self, root: &str, list: &str, member: &str, values: Value) -> Result<(), String> {
        let conn = self.db.conn().map_err(|e| e.to_string())?;
        let get = |k: &str| values.get(k).cloned();
        conn.execute(
            "INSERT INTO list_items(workspace,list,member,name,contact,status,draft,evidence,error,updated_at) VALUES(?1,?2,?3,COALESCE(?4,''),COALESCE(?5,''),?6,COALESCE(?7,''),COALESCE(?8,'[]'),COALESCE(?9,''),?10)
             ON CONFLICT(workspace,list,member) DO UPDATE SET name=COALESCE(?4,name),contact=COALESCE(?5,contact),status=?6,draft=COALESCE(?7,draft),evidence=COALESCE(?8,evidence),error=COALESCE(?9,error),updated_at=?10",
            params![
                root,
                list,
                member,
                get("name").and_then(|v| v.as_str().map(str::to_owned)),
                get("contact").and_then(|v| v.as_str().map(str::to_owned)),
                get("status").and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_else(|| "queued".into()),
                get("draft").and_then(|v| v.as_str().map(str::to_owned)),
                get("evidence").map(|v| v.to_string()),
                get("error").and_then(|v| v.as_str().map(str::to_owned)),
                gc_core::now_millis()
            ],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    pub async fn lists_all(&self, root: &str) -> Result<Value, String> {
        let service = self.service(root)?;
        let specs = lists::load(&service.docs.workspace).map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for spec in specs {
            let matched = self.selected(root, &spec).await.map(|m| m.len()).unwrap_or(0);
            let items = self.items(root, &spec.id)?;
            let count = |s: &str| items.values().filter(|i| i["status"] == s).count();
            let mut value = serde_json::to_value(&spec).map_err(|e| e.to_string())?;
            value["id"] = json!(spec.id);
            value["counts"] = json!({"matched":matched,"queued":count("queued")+count("preparing"),"ready":count("ready"),"skipped":count("skipped")});
            result.push(value);
        }
        Ok(json!(result))
    }

    /// The list with its current members, preparation status and draft state.
    pub async fn list_get(&self, root: &str, id: &str) -> Result<Value, String> {
        let spec = self.spec(root, id)?;
        let service = self.service(root)?;
        let members = self.selected(root, &spec).await?;
        let items = self.items(root, id)?;
        let drafts: HashMap<String, Value> = service
            .drafts(None)
            .await?
            .into_iter()
            .filter_map(|d| d["path"].as_str().map(|p| (p.to_owned(), d["metadata"].clone())))
            .collect();
        // A draft tagged with this list counts for its contact even if preparation did not write it
        // (written by hand or in the Agent terminal). The newest open one wins.
        let mut tagged: HashMap<String, String> = HashMap::new();
        for (path, meta) in &drafts {
            if meta["list"] == id && !matches!(meta["status"].as_str(), Some("discarded")) {
                let contact = meta["contact_id"].as_str().unwrap_or_default().to_owned();
                if tagged.get(&contact).is_none_or(|p| p < path) {
                    tagged.insert(contact, path.clone());
                }
            }
        }
        let mut out: Vec<Value> = members
            .iter()
            .map(|m| {
                let mut item = items.get(&m.key).cloned().unwrap_or(json!({}));
                let contact = item["contact"].as_str().filter(|c| !c.is_empty()).map(str::to_owned).or(m.contact.clone()).unwrap_or_default();
                if matches!(item["status"].as_str().unwrap_or("new"), "new" | "no_draft" | "failed")
                    && let Some(path) = tagged.get(&contact)
                {
                    item["status"] = json!("ready");
                    item["draft"] = json!(path);
                }
                let draft = item["draft"].as_str().unwrap_or("");
                json!({
                    "key": m.key,
                    "name": m.name,
                    "email": m.email,
                    "contact": item["contact"].as_str().filter(|c| !c.is_empty()).map(str::to_owned).or(m.contact.clone()),
                    "stage": m.stage,
                    "attributes": m.attributes,
                    "evidence": m.evidence,
                    "status": item["status"].as_str().unwrap_or("new"),
                    "draft": draft,
                    "draft_status": drafts.get(draft).map(|d| d["status"].clone()).unwrap_or(Value::Null),
                    "error": item["error"],
                })
            })
            .collect();
        // People handled earlier who no longer match stay visible with their outcome.
        let current: HashSet<&str> = members.iter().map(|m| m.key.as_str()).collect();
        for (key, item) in &items {
            if !current.contains(key.as_str()) && item["status"] != "queued" {
                let draft = item["draft"].as_str().unwrap_or("");
                out.push(json!({"key":key,"name":item["name"],"contact":item["contact"],"evidence":item["evidence"],"status":item["status"],"draft":draft,"draft_status":drafts.get(draft).map(|d| d["status"].clone()).unwrap_or(Value::Null),"error":item["error"],"left":true}));
            }
        }
        let limit = service.workspace_config()?.outreach.daily_limit;
        let mut value = serde_json::to_value(&spec).map_err(|e| e.to_string())?;
        value["id"] = json!(spec.id);
        Ok(json!({"list": value, "members": out, "daily_limit": limit, "sent_today": self.sent_today(root)?}))
    }

    pub fn list_save(&self, root: &str, args: &Value) -> Result<Value, String> {
        let service = self.service(root)?;
        let mut spec: ListSpec = serde_json::from_value(args["list"].clone()).map_err(|e| format!("Invalid list: {e}"))?;
        spec.id = args["id"].as_str().unwrap_or("").to_owned();
        if spec.id.is_empty() {
            let base: String = crate::contacts::slugify(&spec.name).chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-').collect();
            let base = base.trim_matches('-').to_owned();
            let base = if base.is_empty() { "list".to_owned() } else { base };
            let existing: HashSet<String> = lists::load(&service.docs.workspace).map_err(|e| e.to_string())?.into_iter().map(|l| l.id).collect();
            let mut id = base.clone();
            let mut n = 2;
            while existing.contains(&id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            spec.id = id;
        }
        lists::save(&service.docs.workspace, &spec).map_err(|e| e.to_string())?;
        Ok(json!({"id": spec.id}))
    }

    /// Evaluates an unsaved list definition: how many match, and the first few.
    pub async fn list_preview(&self, root: &str, args: &Value) -> Result<Value, String> {
        let mut spec: ListSpec = serde_json::from_value(args["list"].clone()).map_err(|e| format!("Invalid list: {e}"))?;
        spec.id = "preview".into();
        if spec.name.trim().is_empty() {
            spec.name = "preview".into();
        }
        spec.validate().map_err(|e| e.to_string())?;
        let members = self.selected(root, &spec).await?;
        Ok(json!({
            "count": members.len(),
            "sample": members.iter().take(8).map(|m| json!({"name":m.name,"email":m.email,"evidence":m.evidence})).collect::<Vec<_>>(),
        }))
    }

    pub fn list_delete(&self, root: &str, id: &str) -> Result<Value, String> {
        lists::delete(&self.service(root)?.docs.workspace, id).map_err(|e| e.to_string())?;
        self.db
            .conn()
            .map_err(|e| e.to_string())?
            .execute("DELETE FROM list_items WHERE workspace=?1 AND list=?2", params![root, id])
            .map_err(|e| e.to_string())?;
        Ok(json!({"deleted": true}))
    }

    /// Marks members for the Agent to prepare (see the `list` terminal task). Already prepared
    /// members are skipped unless `again` is set.
    pub async fn list_prepare(&self, root: &str, id: &str, args: &Value) -> Result<Value, String> {
        let spec = self.spec(root, id)?;
        let members = self.selected(root, &spec).await?;
        let wanted: Option<HashSet<String>> = args["members"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect());
        let items = self.items(root, id)?;
        let again = args["again"] == true;
        let mut queued = 0;
        for m in members.iter().filter(|m| wanted.as_ref().is_none_or(|w| w.contains(&m.key))) {
            let status = items.get(&m.key).and_then(|i| i["status"].as_str()).unwrap_or("new");
            if !again && !matches!(status, "new" | "failed" | "no_draft") {
                continue;
            }
            self.set_item(root, id, &m.key, json!({"name":m.name,"contact":m.contact.clone().unwrap_or_default(),"status":"queued","evidence":m.evidence,"error":"","draft":""}))?;
            queued += 1;
        }
        Ok(json!({"queued": queued}))
    }

    pub fn list_cancel(&self, root: &str, id: &str) -> Result<Value, String> {
        let removed = self
            .db
            .conn()
            .map_err(|e| e.to_string())?
            .execute("DELETE FROM list_items WHERE workspace=?1 AND list=?2 AND status='queued'", params![root, id])
            .map_err(|e| e.to_string())?;
        Ok(json!({"cancelled": removed}))
    }

    pub fn list_skip(&self, root: &str, id: &str, member: &str) -> Result<Value, String> {
        self.set_item(root, id, member, json!({"status":"skipped"}))?;
        Ok(json!({"status":"skipped"}))
    }

    /// Never contact this person again: their addresses join suppression.txt, a linked contact
    /// moves to the do-not-contact stage, and any unsent list draft is discarded.
    pub async fn list_suppress(&self, root: &str, id: &str, member: &str) -> Result<Value, String> {
        let spec = self.spec(root, id)?;
        let service = self.service(root)?;
        let candidates = self.candidates(&service, &spec).await?;
        let found = candidates.iter().find(|m| m.key == member).ok_or("This person is no longer a list candidate.")?;
        let mut addresses: Vec<String> = Vec::new();
        if !found.email.is_empty() {
            addresses.push(found.email.clone());
        }
        if let Some((source, external)) = &found.identity
            && let Some(row) = service.rows("identities").await?.into_iter().find(|r| r.text("Source") == source && r.text("External ID") == external)
        {
            addresses.extend(attributes(&row).1);
        }
        if addresses.is_empty() {
            return Err("This person has no address to record. Change their stage to Do not contact in the contact instead.".into());
        }
        for address in &addresses {
            lists::suppress(&service.docs.workspace, address, &format!("list {}", spec.name)).map_err(|e| e.to_string())?;
        }
        let config = service.workspace_config()?;
        let item = self.items(root, id)?.get(member).cloned().unwrap_or(json!({}));
        let contact = item["contact"].as_str().filter(|c| !c.is_empty()).map(str::to_owned).or(found.contact.clone());
        if let Some(slug) = contact
            && let Some(stage) = config.stages.iter().find(|s| s.role == Role::Suppressed)
        {
            service.update_contact(&slug, json!({"Stage": stage.label}).as_object().cloned().unwrap_or_default(), false).await?;
        }
        if let Some(draft) = item["draft"].as_str().filter(|d| !d.is_empty()) {
            let meta = metadata(&service.read_note(draft).await?);
            if !matches!(meta["status"].as_str(), Some("sent" | "sending" | "uncertain")) {
                service.docs.execute(draft, "property.set", json!({"key":"status","value":"discarded"}), "system:lists").await?;
            }
        }
        self.set_item(root, id, member, json!({"status":"suppressed"}))?;
        Ok(json!({"status":"suppressed","addresses":addresses.len()}))
    }

    /// The list member's contact, created from its source identity on first use.
    async fn member_contact(&self, root: &str, id: &str, key: &str) -> Result<(ListSpec, String), String> {
        let spec = self.spec(root, id)?;
        let members = self.selected(root, &spec).await?;
        let Some(member) = members.into_iter().find(|m| m.key == key) else {
            // No longer matches (contacted meanwhile, suppressed, data changed).
            self.set_item(root, id, key, json!({"status":"skipped","error":"No longer matches the list"}))?;
            return Err("This person no longer matches the list and was skipped.".into());
        };
        let slug = match &member.contact {
            Some(slug) => slug.clone(),
            None => {
                let (source, external) = member.identity.clone().ok_or("List member has no contact")?;
                let created = self.identity_decide(root, &json!({"source":source,"external_id":external,"action":"create"})).await?;
                created["contact"].as_str().ok_or("Couldn't create the contact")?.to_owned()
            }
        };
        self.set_item(root, id, key, json!({"name":member.name,"contact":slug,"status":"preparing","evidence":member.evidence}))?;
        Ok((spec, slug))
    }

    /// Saves the Agent's draft for one list member, tagged with the list so the daily limit and
    /// review queue apply. Never sends.
    pub async fn list_draft_save(&self, root: &str, args: &Value) -> Result<Value, String> {
        let id = args["list"].as_str().ok_or("list required")?;
        let key = args["member"].as_str().ok_or("member required")?;
        let (spec, slug) = self.member_contact(root, id, key).await?;
        let service = self.service(root)?;
        let mut draft = json!({"contact_id":slug,"channel":args["channel"].as_str().unwrap_or(&spec.channel),"body":args["body"],"created_by":args["created_by"].as_str().unwrap_or("agent")});
        for k in ["to", "subject"] {
            if args[k].is_string() {
                draft[k] = args[k].clone();
            }
        }
        let saved = service.draft_save(&draft, true).await?;
        let path = saved["path"].as_str().ok_or("Couldn't save the draft")?;
        service.docs.execute(path, "property.set", json!({"key":"list","value":id}), "system:lists").await?;
        self.set_item(root, id, key, json!({"status":"ready","draft":path,"error":""}))?;
        Ok(json!({"path":path,"contact":slug}))
    }

    /// Records that the Agent judged this member not worth contacting now, with its reason.
    pub async fn list_decline(&self, root: &str, id: &str, key: &str, reason: &str) -> Result<Value, String> {
        self.spec(root, id)?;
        self.set_item(root, id, key, json!({"status":"no_draft","error":reason}))?;
        Ok(json!({"status":"no_draft"}))
    }

    fn sent_today(&self, root: &str) -> Result<i64, String> {
        self.db
            .conn()
            .map_err(|e| e.to_string())?
            .query_row(
                "SELECT COUNT(*) FROM outreach_sends WHERE workspace=?1 AND at>=?2",
                params![root, start_of_day()],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }

    /// Refuses a body that still has template placeholders such as {{name}} or [Name], including
    /// the Chinese ones Chinese-language templates use.
    pub fn placeholder_guard(body: &str) -> Result<(), String> {
        let re = regex::Regex::new(r"(?i)\{\{[^}]*\}\}|\[(姓名|名字|公司|name|first ?name|last ?name|full ?name|company)\]").expect("valid regex");
        if let Some(m) = re.find(body) {
            return Err(format!("The message still has an unreplaced placeholder “{}”. Edit it and approve again.", m.as_str()));
        }
        Ok(())
    }

    /// Refuses a send to a do-not-contact address or contact, and list messages over the daily limit.
    pub async fn outreach_guard(&self, root: &str, service: &Service, meta: &Value) -> Result<(), String> {
        if self.config()["sendingPaused"] == true {
            return Err("All sending is paused. Resume it in Settings → Assistant.".into());
        }
        let config = service.workspace_config()?;
        let suppressed = lists::suppression(&service.docs.workspace).map_err(|e| e.to_string())?;
        if let Some(to) = meta["to"].as_str() {
            let lower = to.to_lowercase();
            if suppressed.iter().any(|a| lower.contains(a.as_str())) {
                return Err("The recipient is on the do-not-contact list (suppression.txt) and can't be sent to.".into());
            }
        }
        if let Some(slug) = meta["contact_id"].as_str()
            && let Ok(contact) = service.contact(slug).await
            && config.role(contact.text("Stage")) == Some(Role::Suppressed)
        {
            return Err("This contact's stage is Do not contact; it can't be sent to.".into());
        }
        if meta["list"].as_str().is_some_and(|l| !l.is_empty()) {
            let sent = self.sent_today(root)?;
            if sent >= config.outreach.daily_limit as i64 {
                return Err(format!("{sent} list messages were sent today, reaching the daily limit of {}. Adjust it under [outreach] in globalcontact.toml.", config.outreach.daily_limit));
            }
        }
        Ok(())
    }

    /// A draft is stale when the person wrote again after it was created. Sending one requires
    /// an explicit acknowledgement that the newer message was read.
    pub async fn stale_guard(&self, service: &Service, meta: &Value, acknowledged: bool) -> Result<(), String> {
        if acknowledged {
            return Ok(());
        }
        let Some(slug) = meta["contact_id"].as_str() else { return Ok(()) };
        let Ok(contact) = service.contact(slug).await else { return Ok(()) };
        let parse = |s: &str| chrono::DateTime::parse_from_rfc3339(s).ok().map(|t| t.timestamp_millis());
        if let (Some(reply), Some(created)) = (parse(contact.text("Last reply")), meta["created_at"].as_str().and_then(parse))
            && reply > created
        {
            return Err("They wrote again after this draft was created. Read the latest message and make sure the draft still fits before sending.".into());
        }
        Ok(())
    }

    /// After a message goes out, the contact is due for a follow-up in the configured days.
    pub async fn schedule_follow_up(&self, service: &Service, meta: &Value) -> Result<(), String> {
        let Some(slug) = meta["contact_id"].as_str() else { return Ok(()) };
        let fallback = self.config()["followupDays"].as_i64().unwrap_or(7).max(1);
        let days = service.follow_up_days(slug, fallback).await;
        service.set_follow_up(slug, Some(chrono::Utc::now() + chrono::Duration::days(days))).await
    }

    pub fn record_outreach(&self, root: &str, path: &str, meta: &Value) -> Result<(), String> {
        if meta["list"].as_str().is_some_and(|l| !l.is_empty()) {
            self.db
                .conn()
                .map_err(|e| e.to_string())?
                .execute("INSERT OR IGNORE INTO outreach_sends(workspace,path,at) VALUES(?1,?2,?3)", params![root, path, gc_core::now_millis()])
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
