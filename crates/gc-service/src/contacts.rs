use crate::Service;
use serde::{Deserialize, Serialize};
use gc_core::config::Role;
use serde_json::{Map, Value, json};

pub const TABLE: &str = "contacts.ktable";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    pub id: i64,
    pub values: Map<String, Value>,
}
impl Row {
    pub fn text(&self, key: &str) -> &str {
        self.values.get(key).and_then(Value::as_str).unwrap_or("")
    }
}
impl Service {
    /// Supply stable identities to contacts created directly in the table editor.
    /// Adds the follow-up date and keep-in-touch cadence to workspaces created before them.
    pub async fn ensure_follow_up(&self) -> Result<(), String> {
        let table = self.table("contacts").await?;
        for (title, kind) in [("Follow up", "date"), ("Keep in touch", "number")] {
            if !table["fields"].as_array().into_iter().flatten().any(|f| f["title"] == title) {
                self.docs
                    .execute(TABLE, "table.fields.create", json!({"tableId":table["id"],"title":title,"type":kind}), "system:contacts")
                    .await?;
            }
        }
        Ok(())
    }

    /// Days until the next follow-up: the contact's keep-in-touch cadence, else `default`.
    pub async fn follow_up_days(&self, slug: &str, default: i64) -> i64 {
        self.contact(slug)
            .await
            .ok()
            .and_then(|r| r.values.get("Keep in touch").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))))
            .map(|d| d.round() as i64)
            .filter(|d| *d > 0)
            .unwrap_or(default)
    }

    /// Schedules (or clears, with `None`) the next follow-up date for a contact.
    pub async fn set_follow_up(&self, slug: &str, at: Option<chrono::DateTime<chrono::Utc>>) -> Result<(), String> {
        self.ensure_follow_up().await?;
        let row = self.contact(slug).await?;
        let value = at.map(|t| json!(t.format("%Y-%m-%d").to_string())).unwrap_or(Value::Null);
        self.write_row("contacts", Some(row.id), json!({"Follow up": value}).as_object().ok_or("follow up")?, "system:followup")
            .await?;
        Ok(())
    }

    /// Adds channel kinds introduced after a workspace was created (e.g. WhatsApp, LinkedIn) to
    /// its select fields: contacts.Primary channel, channels.Kind and interactions.Channel.
    /// Existing choices and values are kept; only missing choices are appended.
    pub async fn ensure_channel_choices(&self) -> Result<(), String> {
        let answer = self.docs.execute(TABLE, "table.tables.list", json!({}), "system:contacts").await?;
        let tables = answer["result"]["items"].as_array().cloned().unwrap_or_default();
        let mut updates = Vec::new();
        for (table, title, wanted) in gc_core::schema::channel_choices() {
            let Some(table) = tables.iter().find(|t| t["name"] == table || t["title"] == table) else { continue };
            let Some(field) = table["fields"].as_array().into_iter().flatten().find(|f| f["title"] == title) else { continue };
            if !field["type"].as_str().unwrap_or_default().ends_with("select") {
                continue;
            }
            let existing = field["options"]["choices"].as_array().cloned().unwrap_or_default();
            if let Some(choices) = gc_core::schema::merge_choices(&existing, wanted) {
                let mut options = if field["options"].is_object() { field["options"].clone() } else { json!({}) };
                options["choices"] = json!(choices);
                updates.push(gc_docs::Step {
                    command: "table.fields.update".into(),
                    params: json!({"tableId": table["id"], "fieldId": field["id"], "options": options}),
                });
            }
        }
        if !updates.is_empty() {
            let _guard = self.mutations.lock().await;
            self.docs.run(TABLE, updates, "system:contacts").await?;
        }
        Ok(())
    }

    pub async fn reconcile_contacts(&self) -> Result<(), String> {
        self.ensure_follow_up().await?;
        self.ensure_channel_choices().await?;
        let _guard = self.mutations.lock().await;
        let rows = self.rows("contacts").await?;
        let mut slugs: std::collections::HashSet<String> = rows
            .iter()
            .map(|r| r.text("Slug").to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        let initial = self.workspace_config()?.initial_stage().to_owned();
        for row in rows.iter().filter(|r| r.text("Slug").is_empty()) {
            let base = slugify(if row.text("Name").trim().is_empty() {
                "contact"
            } else {
                row.text("Name")
            });
            let mut slug = base.clone();
            let mut suffix = 2;
            while slugs.contains(&slug) {
                slug = format!("{base}-{suffix}");
                suffix += 1;
            }
            slugs.insert(slug.clone());
            let mut values = Map::new();
            values.insert("Slug".into(), json!(slug));
            if row.text("Stage").is_empty() {
                values.insert("Stage".into(), json!(initial));
            }
            self.write_row("contacts", Some(row.id), &values, "system:contacts")
                .await?;
        }
        Ok(())
    }
    pub async fn table(&self, name: &str) -> Result<Value, String> {
        let answer = self
            .docs
            .execute(TABLE, "table.tables.list", json!({}), "system:contacts")
            .await?;
        answer["result"]["items"]
            .as_array()
            .and_then(|tables| {
                tables
                    .iter()
                    .find(|t| t["name"] == name || t["title"] == name)
            })
            .cloned()
            .ok_or_else(|| format!("Table not found: {name}"))
    }
    pub async fn rows(&self, name: &str) -> Result<Vec<Row>, String> {
        let table = self.table(name).await?;
        let mut rows = Vec::new();
        let mut offset = 0;
        loop {
            let answer = self
                .docs
                .execute(
                    TABLE,
                    "table.records.list",
                    json!({"tableId":table["id"], "limit":500,"offset":offset}),
                    "system:contacts",
                )
                .await?;
            let items = answer["result"]["items"]
                .as_array()
                .ok_or("records.list did not return items")?;
            for row in items {
                let mut values = Map::new();
                for field in table["fields"].as_array().ok_or("table has no fields")? {
                    if let (Some(title), Some(key)) =
                        (field["title"].as_str(), field["name"].as_str())
                    {
                        values.insert(title.into(), row["values"][key].clone());
                    }
                }
                rows.push(Row {
                    id: row["id"].as_i64().ok_or("record id")?,
                    values,
                });
            }
            offset += items.len();
            if items.is_empty()
                || offset >= answer["result"]["total"].as_u64().unwrap_or(offset as u64) as usize
            {
                break;
            }
        }
        Ok(rows)
    }
    pub async fn write_row(
        &self,
        table_name: &str,
        id: Option<i64>,
        values: &Map<String, Value>,
        origin: &str,
    ) -> Result<Value, String> {
        let table = self.table(table_name).await?;
        let mut mapped = Map::new();
        for (title, value) in values {
            let field = table["fields"]
                .as_array()
                .and_then(|fields| fields.iter().find(|f| f["title"] == *title))
                .ok_or_else(|| format!("Unknown field: {title}"))?;
            mapped.insert(
                field["name"].as_str().ok_or("field name")?.into(),
                value.clone(),
            );
        }
        let (command, params) = record_step(&table["id"], id, mapped);
        let answer = self.docs.execute(TABLE, command, params, origin).await?;
        Ok(answer["result"].clone())
    }
    /// Creates or updates many rows of one table in a single kit run (one load and save).
    pub async fn write_rows(
        &self,
        table_name: &str,
        rows: Vec<(Option<i64>, Map<String, Value>)>,
        origin: &str,
    ) -> Result<Vec<Value>, String> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let table = self.table(table_name).await?;
        let fields = table["fields"].as_array().cloned().unwrap_or_default();
        let mut steps = Vec::new();
        for (id, values) in rows {
            let mut mapped = Map::new();
            for (title, value) in values {
                let field = fields
                    .iter()
                    .find(|f| f["title"] == title.as_str())
                    .ok_or_else(|| format!("Unknown field: {title}"))?;
                mapped.insert(field["name"].as_str().ok_or("field name")?.into(), value);
            }
            let (command, params) = record_step(&table["id"], id, mapped);
            steps.push(gc_docs::Step { command: command.into(), params });
        }
        // Like write_row, answer each step's record (`result`), not the kit envelope.
        Ok(self
            .docs
            .run(TABLE, steps, origin)
            .await?
            .into_iter()
            .map(|answer| answer["result"].clone())
            .collect())
    }
    /// The contact table's fields for the profile editor. `system` fields are host-managed.
    pub async fn contact_fields(&self) -> Result<Value, String> {
        let table = self.table("contacts").await?;
        let system = ["Slug", "Last contacted", "Last reply", "AI summary", "Stage", "Follow up", "Keep in touch"];
        Ok(json!(table["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| {
                let title = f["title"].as_str().unwrap_or_default();
                json!({
                    "title": title,
                    "type": f["type"],
                    "choices": f["options"]["choices"].as_array().map(|c| c.iter().map(|v| v.as_str().map(str::to_owned).unwrap_or_else(|| v["name"].as_str().unwrap_or_default().to_owned())).collect::<Vec<_>>()).unwrap_or_default(),
                    "system": system.contains(&title),
                })
            })
            .collect::<Vec<_>>()))
    }

    /// The contact row's change history from the table kit, with field names shown as titles.
    pub async fn contact_history(&self, slug: &str) -> Result<Value, String> {
        let row = self.contact(slug).await?;
        let table = self.table("contacts").await?;
        // History items name fields by id.
        let titles: std::collections::HashMap<i64, String> = table["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| Some((f["id"].as_i64()?, f["title"].as_str()?.to_owned())))
            .collect();
        let answer = self
            .docs
            .execute(TABLE, "table.records.history", json!({"tableId":table["id"],"recordId":row.id,"limit":100}), "system:contacts")
            .await?;
        let mut items = answer["result"]["items"].as_array().cloned().unwrap_or_default();
        for item in &mut items {
            if let Some(title) = item["field_id"].as_i64().and_then(|id| titles.get(&id)) {
                item["field_title"] = json!(title);
            }
        }
        Ok(json!({"items": items}))
    }

    /// Adds a profile field to every contact (a column of the contacts table).
    pub async fn contact_field_add(&self, title: &str, kind: &str, choices: &[String]) -> Result<Value, String> {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 40 {
            return Err("Field name can't be empty and must be at most 40 characters.".into());
        }
        if !["text", "long_text", "number", "url", "date", "checkbox", "single_select", "multi_select"].contains(&kind) {
            return Err("Unsupported field type.".into());
        }
        let _guard = self.mutations.lock().await;
        let table = self.table("contacts").await?;
        if table["fields"].as_array().into_iter().flatten().any(|f| f["title"].as_str().is_some_and(|t| t.eq_ignore_ascii_case(title))) {
            return Err(format!("A field named “{title}” already exists."));
        }
        let mut field = json!({"tableId": table["id"], "title": title, "type": kind});
        if kind.ends_with("select") {
            field["options"] = json!({"choices": choices});
        }
        self.docs.execute(TABLE, "table.fields.create", field, "system:contacts").await?;
        Ok(json!({"title": title}))
    }

    /// Adds an option to a select field so a new value can be stored.
    pub async fn contact_field_choice(&self, title: &str, choice: &str) -> Result<Value, String> {
        let choice = choice.trim();
        if choice.is_empty() {
            return Err("Options can't be empty.".into());
        }
        let _guard = self.mutations.lock().await;
        let table = self.table("contacts").await?;
        let field = table["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|f| f["title"] == title)
            .ok_or_else(|| format!("Unknown field: {title}"))?
            .clone();
        if !field["type"].as_str().unwrap_or_default().ends_with("select") {
            return Err("Only select fields can have options.".into());
        }
        let mut options = field["options"].clone();
        if !options.is_object() {
            options = json!({});
        }
        let mut choices = options["choices"].as_array().cloned().unwrap_or_default();
        let exists = choices.iter().any(|c| c.as_str() == Some(choice) || c["name"].as_str() == Some(choice));
        if !exists {
            choices.push(if choices.first().is_some_and(Value::is_object) { json!({"name": choice}) } else { json!(choice) });
            options["choices"] = json!(choices);
            self.docs
                .execute(TABLE, "table.fields.update", json!({"tableId": table["id"], "fieldId": field["id"], "options": options}), "system:contacts")
                .await?;
        }
        Ok(json!({"choice": choice}))
    }

    pub async fn contact(&self, slug: &str) -> Result<Row, String> {
        self.rows("contacts")
            .await?
            .into_iter()
            .find(|r| r.text("Slug") == slug)
            .ok_or_else(|| format!("Contact not found: {slug}"))
    }
    /// The contacts.Language value to store for `language`: "Chinese" / "English" (also accepting
    /// the legacy Chinese label and language codes). Workspaces created by older versions offer
    /// only the legacy Chinese label as a choice, so Chinese is written that way there to keep one
    /// value per language. Unknown values are kept as given.
    pub async fn language_value(&self, language: &str) -> String {
        let Some(canonical) = gc_core::schema::language(language) else { return language.to_owned() };
        if canonical != "Chinese" {
            return canonical.to_owned();
        }
        let Ok(answer) = self.docs.execute(TABLE, "table.tables.list", json!({}), "system:contacts").await else {
            return canonical.to_owned();
        };
        let choices: Vec<String> = answer["result"]["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| t["name"] == "contacts" || t["title"] == "contacts")
            .flat_map(|t| t["fields"].as_array().cloned().unwrap_or_default())
            .filter(|f| f["title"] == "Language")
            .flat_map(|f| f["options"]["choices"].as_array().cloned().unwrap_or_default())
            .filter_map(|c| c.as_str().or_else(|| c["name"].as_str()).map(str::to_owned))
            .collect();
        if choices.iter().any(|c| c == gc_core::schema::LEGACY_CHINESE) && !choices.iter().any(|c| c == canonical) {
            gc_core::schema::LEGACY_CHINESE.to_owned()
        } else {
            canonical.to_owned()
        }
    }

    pub async fn create_contact(&self, values: Map<String, Value>) -> Result<String, String> {
        let _guard = self.mutations.lock().await;
        let name = values
            .get("Name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if name.is_empty() {
            return Err("Enter a name.".into());
        }
        let base = slugify(name);
        let rows = self.rows("contacts").await?;
        let mut slug = base.clone();
        let mut suffix = 2;
        while rows.iter().any(|r| r.text("Slug") == slug) {
            slug = format!("{base}-{suffix}");
            suffix += 1;
        }
        let mut values = values;
        values.insert("Slug".into(), json!(slug));
        values
            .entry("Stage")
            .or_insert(json!(self.workspace_config()?.initial_stage()));
        // New contacts default to English; values are stored as the workspace's Language
        // choices name them (see `language_value`).
        let language = values.get("Language").and_then(Value::as_str).unwrap_or("English").to_owned();
        values.insert("Language".into(), json!(self.language_value(&language).await));
        self.write_row("contacts", None, &values, "system:contacts")
            .await?;
        self.ensure_dossier(
            &slug,
            values["Name"].as_str().unwrap_or(""),
            values
                .get("Affiliation")
                .and_then(Value::as_str)
                .unwrap_or(""),
            values.get("Hook").and_then(Value::as_str).unwrap_or(""),
            values.get("Campaign").and_then(Value::as_str).unwrap_or(""),
        )
        .await?;
        Ok(slug)
    }
    pub async fn update_contact(
        &self,
        slug: &str,
        values: Map<String, Value>,
        agent: bool,
    ) -> Result<(), String> {
        let _guard = self.mutations.lock().await;
        if values.contains_key("Slug") {
            return Err("The Slug is managed by the app and doesn't change on rename.".into());
        }
        if agent
            && values
                .keys()
                .any(|k| !matches!(k.as_str(), "AI summary" | "Next action" | "Stage"))
        {
            return Err("The Agent can only change the summary, next action and stage.".into());
        }
        if values
            .get("Name")
            .and_then(Value::as_str)
            .is_some_and(|n| n.trim().is_empty())
        {
            return Err("Name can't be empty.".into());
        }
        if let Some(stage) = values.get("Stage").and_then(Value::as_str) {
            let config = self.workspace_config()?;
            if config.role(stage).is_none() {
                return Err(format!("Stage must be one of: {}", config.labels().join(", ")));
            }
        }
        let row = self.contact(slug).await?;
        self.write_row(
            "contacts",
            Some(row.id),
            &values,
            if agent {
                "system:agent"
            } else {
                "system:contacts"
            },
        )
        .await?;
        drop(_guard);
        if !agent
            && let Some(email) = values
                .get("Email")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
        {
            self.add_channel(slug, "email", email, "Edited by user", false)
                .await?;
        }
        Ok(())
    }
    pub async fn add_channel(
        &self,
        slug: &str,
        kind: &str,
        value: &str,
        source: &str,
        verified: bool,
    ) -> Result<bool, String> {
        let _guard = self.mutations.lock().await;
        self.contact(slug).await?;
        if value.trim().is_empty() || source.trim().is_empty() {
            return Err("Channel value and source can't be empty.".into());
        }
        if !gc_core::channels::is_kind(kind) {
            return Err("Invalid channel.".into());
        }
        let rows = self.rows("channels").await?;
        if rows.iter().any(|r| {
            r.text("Contact") == slug
                && r.text("Kind") == kind
                && r.text("Value").eq_ignore_ascii_case(value.trim())
        }) {
            return Ok(false);
        }
        self.write_row("channels",None,json!({"Contact":slug,"Kind":kind,"Value":value.trim(),"Source":source,"Verified":verified}).as_object().ok_or("channel")?,"system:contacts").await?;
        Ok(true)
    }
    pub async fn detail(&self, slug: &str) -> Result<Value, String> {
        let contact = self.contact(slug).await?;
        let channels: Vec<_> = self
            .rows("channels")
            .await?
            .into_iter()
            .filter(|r| r.text("Contact") == slug)
            .collect();
        let mut timeline: Vec<_> = self
            .rows("interactions")
            .await?
            .into_iter()
            .filter(|r| r.text("Contact") == slug)
            .collect();
        timeline.sort_by(|a, b| b.text("At").cmp(a.text("At")));
        let dossier_path = format!("people/{slug}.md");
        self.ensure_dossier(
            slug,
            contact.text("Name"),
            contact.text("Affiliation"),
            contact.text("Hook"),
            contact.text("Campaign"),
        )
        .await?;
        let dossier = self.read_note(&dossier_path).await?;
        let drafts = self.drafts(Some(slug)).await?;
        Ok(
            json!({"contact":contact,"channels":channels,"timeline":timeline,"dossier":dossier,"dossierPath":dossier_path,"drafts":drafts}),
        )
    }
    // The arguments correspond directly to the eight interaction columns.
    #[allow(clippy::too_many_arguments)]
    pub async fn interaction(
        &self,
        slug: &str,
        channel: &str,
        kind: &str,
        direction: &str,
        title: &str,
        file: &str,
        message_id: &str,
        at: &str,
    ) -> Result<(), String> {
        let _guard = self.mutations.lock().await;
        if !message_id.is_empty()
            && self
                .rows("interactions")
                .await?
                .iter()
                .any(|r| r.text("Contact") == slug && r.text("Message-ID") == message_id)
        {
            return Ok(());
        }
        self.write_row("interactions",None,json!({"Contact":slug,"Channel":channel,"Kind":kind,"Direction":direction,"Title":title,"File":file,"Message-ID":message_id,"At":at}).as_object().ok_or("interaction")?,"system:timeline").await?;
        if direction == "in" || direction == "out" {
            let row = self.contact(slug).await?;
            let key = if direction == "in" {
                "Last reply"
            } else {
                "Last contacted"
            };
            if row.text(key).is_empty() || row.text(key) < at {
                self.write_row(
                    "contacts",
                    Some(row.id),
                    json!({key:at,"Stage":self.stage_for(if direction=="in" {Role::Active} else {Role::Waiting})?})
                        .as_object()
                        .ok_or("timeline")?,
                    "system:timeline",
                )
                .await?;
            }
        }
        Ok(())
    }
}
pub fn slugify(name: &str) -> String {
    let slug = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        format!("contact-{}", uuid::Uuid::new_v4())
    } else {
        slug.chars().take(90).collect()
    }
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

impl Service {
    /// Email addresses known for a contact: the Email field, email channels and the
    /// emails of linked source identities, lower-cased and de-duplicated.
    pub async fn known_addresses(&self, slug: &str) -> Result<Vec<String>, String> {
        let contact = self.contact(slug).await?;
        let mut out: Vec<String> = Vec::new();
        let mut add = |email: &str| {
            let email = email.trim().to_lowercase();
            if email.contains('@') && !out.contains(&email) {
                out.push(email);
            }
        };
        add(contact.text("Email"));
        for row in self.rows("channels").await? {
            if row.text("Contact") == slug && row.text("Kind") == "email" {
                add(row.text("Value"));
            }
        }
        if let Ok(identities) = self.rows("identities").await {
            for row in identities.iter().filter(|r| r.text("Contact") == slug && matches!(r.text("Match"), "auto" | "confirmed") && r.values.get("Deleted") != Some(&json!(true))) {
                let attrs: Value = serde_json::from_str(row.text("Attributes")).unwrap_or(Value::Null);
                for pair in attrs["_emails"].as_array().into_iter().flatten() {
                    if let Some(email) = pair[0].as_str() {
                        add(email);
                    }
                }
            }
        }
        Ok(out)
    }

    /// Refuses an email to an address that is not known for the contact, so a typo or an
    /// invented address (for example in an Agent's draft) is caught before sending.
    pub async fn recipient_guard(&self, slug: &str, to: &str) -> Result<(), String> {
        let address = to
            .rsplit_once('<')
            .and_then(|(_, rest)| rest.split_once('>'))
            .map(|(a, _)| a)
            .unwrap_or(to)
            .trim()
            .to_lowercase();
        let known = self.known_addresses(slug).await?;
        if known.contains(&address) {
            return Ok(());
        }
        Err(if known.is_empty() {
            format!("This contact has no known email yet, so {address} can't be confirmed as theirs. Add an email to the profile or channels first, or confirm before sending.")
        } else {
            format!("Recipient {address} is not a known email for this contact ({}). Use a known email, or confirm before sending.", known.join(", "))
        })
    }
}

/// An update of an existing record, or a create (which takes no `recordId`).
fn record_step(table: &Value, id: Option<i64>, values: Map<String, Value>) -> (&'static str, Value) {
    match id {
        Some(id) => ("table.records.update", json!({"tableId": table, "recordId": id, "values": values})),
        None => ("table.records.create", json!({"tableId": table, "values": values})),
    }
}
