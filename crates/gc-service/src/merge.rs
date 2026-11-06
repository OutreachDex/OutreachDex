//! Merging two contacts that are the same person. The winner keeps its identity; the loser's
//! channels, timeline, source identities, drafts, intake notes, dossier text, list state and
//! analysis history move to it, empty fields are filled, and the loser row is removed.
use crate::{
    Service,
    app::App,
    contacts::{Row, TABLE, now},
    notes::{body, metadata},
};
use rusqlite::params;
use serde_json::{Map, Value, json};

/// Fields the host manages; never copied across.
const MANAGED: &[&str] = &["Slug", "Name", "AI summary"];

fn time(v: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(v).map(|t| t.timestamp_millis()).unwrap_or(0)
}

fn empty(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.trim().is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        _ => false,
    }
}

impl Service {
    fn files_under(&self, dir: &str) -> Vec<String> {
        let Ok(base) = self.docs.workspace.file(dir) else { return Vec::new() };
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&base) else { return out };
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_file()) {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.ends_with(".md") && !name.starts_with('.') {
                    out.push(format!("{dir}/{name}"));
                }
            }
        }
        out.sort();
        out
    }

    /// What a merge would move, for the confirmation dialog.
    pub async fn merge_preview(&self, winner: &str, loser: &str) -> Result<Value, String> {
        if winner == loser {
            return Err("Can't merge a contact with itself.".into());
        }
        let (w, l) = (self.contact(winner).await?, self.contact(loser).await?);
        let count = |rows: &[Row], field: &str| rows.iter().filter(|r| r.text(field) == loser).count();
        let channels = self.rows("channels").await?;
        let interactions = self.rows("interactions").await?;
        let identities = self.rows("identities").await.unwrap_or_default();
        let fill: Vec<String> = l
            .values
            .iter()
            .filter(|(k, v)| !MANAGED.contains(&k.as_str()) && !empty(Some(v)) && empty(w.values.get(k.as_str())))
            .map(|(k, _)| k.clone())
            .collect();
        Ok(json!({
            "winner": w.text("Name"),
            "loser": l.text("Name"),
            "channels": count(&channels, "Contact"),
            "interactions": count(&interactions, "Contact"),
            "identities": count(&identities, "Contact"),
            "drafts": self.files_under(&format!("drafts/{loser}")).len(),
            "intake": self.files_under(&format!("intake/{loser}")).len(),
            "fill": fill,
        }))
    }

    /// Moves one Markdown file, rewriting `contact_id` in its front matter.
    async fn move_note(&self, from: &str, to: &str, winner: &str) -> Result<(), String> {
        let text = self.read_note(from).await?;
        let rewritten = text
            .lines()
            .map(|l| if l.starts_with("contact_id:") { format!("contact_id: {}", json!(winner)) } else { l.to_owned() })
            .collect::<Vec<_>>()
            .join("\n")
            + if text.ends_with('\n') { "\n" } else { "" };
        let workspace = &self.docs.workspace;
        let mut target = to.to_owned();
        let mut n = 2;
        while workspace.stamp(&target).map_err(|e| e.to_string())?.is_some() {
            target = to.replace(".md", &format!("-{n}.md"));
            n += 1;
        }
        workspace.put_new(&target, rewritten.as_bytes()).map_err(|e| e.to_string())?;
        std::fs::remove_file(workspace.file(from).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl App {
    pub async fn contact_merge(&self, root: &str, winner: &str, loser: &str) -> Result<Value, String> {
        let service = self.service(root)?;
        let preview = service.merge_preview(winner, loser).await?;
        let (w, l) = (service.contact(winner).await?, service.contact(loser).await?);

        // 1. Fields: fill empty ones; keep the latest contact dates.
        let mut values = Map::new();
        for (k, v) in &l.values {
            if MANAGED.contains(&k.as_str()) || empty(Some(v)) {
                continue;
            }
            if k == "Last contacted" || k == "Last reply" {
                if time(v.as_str().unwrap_or("")) > time(w.text(k)) {
                    values.insert(k.clone(), v.clone());
                }
            } else if empty(w.values.get(k.as_str())) {
                values.insert(k.clone(), v.clone());
            }
        }
        if !values.is_empty() {
            service.write_row("contacts", Some(w.id), &values, "system:merge").await?;
        }

        // 2. Files: drafts and intake notes move under the winner; remember new paths.
        let mut moved: Vec<(String, String)> = Vec::new();
        for folder in ["drafts", "intake"] {
            for from in service.files_under(&format!("{folder}/{loser}")) {
                let name = from.rsplit('/').next().unwrap_or("note.md");
                let to = format!("{folder}/{winner}/{name}");
                service.move_note(&from, &to, winner).await?;
                moved.push((from, to));
            }
        }
        let renamed = |path: &str| moved.iter().find(|(f, _)| f == path).map(|(_, t)| t.clone());

        // 3. Rows that point at the loser: channels (deduplicated), timeline, identities.
        let channels = service.rows("channels").await?;
        let mut channel_updates = Vec::new();
        let mut channel_deletes = Vec::new();
        for c in channels.iter().filter(|c| c.text("Contact") == loser) {
            let duplicate = channels.iter().any(|o| {
                o.text("Contact") == winner && o.text("Kind") == c.text("Kind") && o.text("Value").eq_ignore_ascii_case(c.text("Value"))
            });
            if duplicate {
                channel_deletes.push(c.id);
            } else {
                channel_updates.push((Some(c.id), json!({"Contact": winner}).as_object().cloned().unwrap_or_default()));
            }
        }
        service.write_rows("channels", channel_updates, "system:merge").await?;
        let interactions: Vec<(Option<i64>, Map<String, Value>)> = service
            .rows("interactions")
            .await?
            .into_iter()
            .filter(|r| r.text("Contact") == loser)
            .map(|r| {
                let mut v = json!({"Contact": winner}).as_object().cloned().unwrap_or_default();
                if let Some(path) = renamed(r.text("File")) {
                    v.insert("File".into(), json!(path));
                }
                (Some(r.id), v)
            })
            .collect();
        service.write_rows("interactions", interactions, "system:merge").await?;
        let identities: Vec<(Option<i64>, Map<String, Value>)> = service
            .rows("identities")
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.text("Contact") == loser)
            .map(|r| (Some(r.id), json!({"Contact": winner}).as_object().cloned().unwrap_or_default()))
            .collect();
        if !identities.is_empty() {
            service.write_rows("identities", identities, "system:merge").await?;
        }

        // 4. Dossier: the loser's text is appended to the winner's, under a dated heading.
        let (wd, ld) = (format!("people/{winner}.md"), format!("people/{loser}.md"));
        if service.docs.workspace.stamp(&ld).map_err(|e| e.to_string())?.is_some() {
            let loser_text = service.read_note(&ld).await?;
            let extra = body(&loser_text).trim();
            if !extra.is_empty() {
                let current = service.read_note(&wd).await?;
                let merged = format!("{}\n\n## Merged from {} ({})\n\n{}\n", current.trim_end(), l.text("Name"), &now()[..10], extra);
                service
                    .docs
                    .execute(&wd, "text.replace", json!({"from":0,"to":current.chars().count(),"expected":current,"text":merged}), "system:merge")
                    .await?;
            }
            std::fs::remove_file(service.docs.workspace.file(&ld).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }

        // 4b. Agent memory: the loser's log is appended to the winner's, oldest entries kept.
        let lm = crate::memory::path(loser);
        if service.docs.workspace.stamp(&lm).map_err(|e| e.to_string())?.is_some() {
            let entries: String = service.read_note(&lm).await?.lines().skip_while(|l| !l.starts_with("- ")).collect::<Vec<_>>().join("\n");
            if !entries.trim().is_empty() {
                service.memory_append(winner, "change", &format!("Merged the log of {}:\n{entries}", l.text("Name")), "System").await?;
            }
            std::fs::remove_file(service.docs.workspace.file(&lm).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }

        // 5. Local state: list items, analysis history; approvals for moved drafts are dropped
        //    (the moved files must be approved again before sending).
        {
            let conn = self.db.conn().map_err(|e| e.to_string())?;
            conn.execute("UPDATE list_items SET contact=?3 WHERE workspace=?1 AND contact=?2", params![root, loser, winner]).map_err(|e| e.to_string())?;
            conn.execute("UPDATE agent_sessions SET contact=?3 WHERE workspace=?1 AND contact=?2", params![root, loser, winner]).map_err(|e| e.to_string())?;
            for (from, _) in &moved {
                conn.execute("DELETE FROM approvals WHERE workspace=?1 AND path=?2", params![root, from]).map_err(|e| e.to_string())?;
            }
        }
        for (_, to) in &moved {
            if to.starts_with("drafts/") && metadata(&service.read_note(to).await?)["status"] == "approved" {
                service.docs.execute(to, "property.set", json!({"key":"status","value":"draft"}), "system:merge").await?;
            }
        }

        // 6. Remove duplicate channels and the loser row.
        let channel_table = service.table("channels").await?;
        let contacts_table = service.table("contacts").await?;
        let mut steps: Vec<gc_docs::Step> = channel_deletes
            .into_iter()
            .map(|id| gc_docs::Step { command: "table.records.delete".into(), params: json!({"tableId":channel_table["id"],"recordId":id}) })
            .collect();
        steps.push(gc_docs::Step { command: "table.records.delete".into(), params: json!({"tableId":contacts_table["id"],"recordId":l.id}) });
        service.docs.run(TABLE, steps, "system:merge").await?;
        (self.notify)("workspace-changed", json!({"root":root}));
        Ok(json!({"merged": loser, "into": winner, "moved": preview}))
    }
}
