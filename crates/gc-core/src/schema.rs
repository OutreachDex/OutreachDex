//! Initial application schema, expressed directly in the kit's own commands.
use crate::channels;
use crate::config::{Role, WorkspaceConfig};
use table_sdk::{KitDocument, Origin, TableDocument};
use serde_json::{Value, json};

/// AI summary prompt for workspaces whose configuration gives none.
pub const DEFAULT_SUMMARY_PROMPT: &str = "Based on this contact's profile, summarize their recent situation, possible hooks and anything to keep in mind. Output a single paragraph only.";

/// Choices of contacts.Language. Workspaces created by older versions use the legacy Chinese
/// label for Chinese (`LEGACY_CHINESE`); `language` accepts both.
pub const LANGUAGES: &[&str] = &["Chinese", "English"];
pub const LEGACY_CHINESE: &str = "中文";

/// The canonical Language ("Chinese" / "English") for a stored or requested value, accepting
/// the legacy Chinese label and language codes. `None` for anything else.
pub fn language(value: &str) -> Option<&'static str> {
    let v = value.trim();
    if v == LEGACY_CHINESE || v.eq_ignore_ascii_case("chinese") || v.eq_ignore_ascii_case("zh") || v.to_ascii_lowercase().starts_with("zh-") {
        Some("Chinese")
    } else if v.eq_ignore_ascii_case("english") || v.eq_ignore_ascii_case("en") || v.to_ascii_lowercase().starts_with("en-") {
        Some("English")
    } else {
        None
    }
}

/// The contacts table: core fields the app relies on, plus the workspace's configured stages,
/// campaign choices, summary prompt and extra fields.
pub fn contacts_document(config: &WorkspaceConfig) -> Result<TableDocument, String> {
    let mut doc = TableDocument::create("contacts.ktable", 0, 0)?;
    let origin = Origin::system("system:workspace");
    let starter = doc.execute("table.tables.list", json!({}))?["result"]["items"].clone();
    let summary_prompt = if config.contacts.summary_prompt.trim().is_empty() {
        DEFAULT_SUMMARY_PROMPT
    } else {
        config.contacts.summary_prompt.as_str()
    };
    let mut tables = [
        (
            "contacts",
            vec![
                field("Name", "text"),
                field("Native name", "text"),
                field("Slug", "text"),
                field("Affiliation", "text"),
                select_owned("Campaign", &config.contacts.campaigns),
                select_owned(
                    "Stage",
                    &config.labels().into_iter().map(str::to_owned).collect::<Vec<_>>(),
                ),
                select("Language", LANGUAGES),
                select("Primary channel", channels::PRIMARY),
                field("Email", "text"),
                field("Homepage", "url"),
                field("X", "text"),
                field("Instagram", "text"),
                field("Verified", "checkbox"),
                field("Hook", "long_text"),
                json!({"title": "AI summary", "type": "long_text", "options": {"ai_config": {
                    "type": "text_generation", "prompt": summary_prompt
                }}}),
                field("Next action", "long_text"),
                field("Follow up", "date"),
                field("Keep in touch", "number"),
                field("Last contacted", "datetime"),
                field("Last reply", "datetime"),
                field("Notes", "long_text"),
            ],
        ),
        (
            "channels",
            vec![
                field("Contact", "text"),
                select("Kind", channels::KINDS),
                field("Value", "text"),
                field("Source", "text"),
                field("Verified", "checkbox"),
            ],
        ),
        ("identities", identities_fields()),
        (
            "interactions",
            vec![
                field("Contact", "text"),
                field("At", "datetime"),
                select("Direction", &["in", "out", "note"]),
                select("Channel", channels::MESSAGE),
                select("Kind", &["message", "post", "intake", "draft", "analysis"]),
                field("Title", "text"),
                field("File", "text"),
                field("Message-ID", "text"),
            ],
        ),
    ];
    let core: Vec<String> = tables[0].1.iter().filter_map(|f| f["title"].as_str().map(str::to_owned)).collect();
    for extra in &config.fields {
        if core.contains(&extra.title) {
            return Err(format!("Field {} in globalcontact.toml has the same name as a built-in field", extra.title));
        }
        tables[0].1.push(if extra.kind == "single_select" || extra.kind == "multi_select" {
            json!({"title": extra.title, "type": extra.kind, "options": {"choices": extra.options}})
        } else {
            json!({"title": extra.title, "type": extra.kind})
        });
    }
    for (name, fields) in tables {
        let mut primary = fields[0].clone();
        primary["primary"] = json!(true);
        let table = doc.execute_as(
            "table.tables.create",
            json!({
                "name": name, "title": name, "fields": [primary],
                "views": [{"title": "All", "type": "grid"}]
            }),
            &origin,
        )?;
        for mut field in fields.into_iter().skip(1) {
            field["tableId"] = table["result"]["id"].clone();
            doc.execute_as("table.fields.create", field, &origin)?;
        }
    }
    let current = doc.execute("table.tables.list", json!({}))?;
    let contacts = current["result"]["items"]
        .as_array()
        .and_then(|a| a.iter().find(|t| t["name"] == "contacts"))
        .ok_or("contacts table missing")?;
    let id = contacts["id"].clone();
    let fields = contacts["fields"]
        .as_array()
        .ok_or("contact fields missing")?;
    let field_id = |name: &str| {
        fields
            .iter()
            .find(|f| f["title"] == name)
            .map(|f| f["id"].clone())
            .ok_or_else(|| format!("Missing field {name}"))
    };
    let stage = field_id("Stage")?;
    let reply = field_id("Last reply")?;
    let view = doc.execute_as(
        "table.views.create",
        json!({"tableId":id,"title":"Awaiting reply","type":"grid"}),
        &origin,
    )?;
    let view_id = view["result"]["id"].clone();
    doc.execute_as(
        "table.views.putGroups",
        json!({"tableId":id,"viewId":view_id,"items":[{"field_id":stage,"direction":"asc"}]}),
        &origin,
    )?;
    doc.execute_as(
        "table.views.putSorts",
        json!({"tableId":id,"viewId":view_id,"items":[{"field_id":reply,"direction":"desc"}]}),
        &origin,
    )?;
    let pending: Vec<Value> = config
        .stages
        .iter()
        .filter(|s| matches!(s.role, Role::Waiting | Role::Active))
        .map(|s| json!({"condition":{"field_id":stage,"operator":"is","value":s.label}}))
        .collect();
    doc.execute_as("table.views.putFilterTree",json!({"tableId":id,"viewId":view_id,"tree":{"conjunction":"or","children":pending}}),&origin)?;
    // KitDocument::create supplies a sample table. Keep only our application tables.
    for table in starter
        .as_array()
        .ok_or("tables.list did not return items")?
    {
        doc.execute_as(
            "table.tables.delete",
            json!({"tableId": table["id"]}),
            &origin,
        )?;
    }
    Ok(doc)
}

/// Select fields whose choices are channel kinds: (table, field, choices). Workspaces created
/// with fewer kinds get the missing choices when opened.
pub fn channel_choices() -> [(&'static str, &'static str, &'static [&'static str]); 3] {
    [
        ("contacts", "Primary channel", channels::PRIMARY),
        ("channels", "Kind", channels::KINDS),
        ("interactions", "Channel", channels::MESSAGE),
    ]
}

/// `existing` choices plus any of `wanted` that are missing, inserted before a trailing
/// "other"; `None` when nothing is missing. Existing choices (strings or `{name}` objects, with
/// their order and any extra choices a person added) are kept as they are.
pub fn merge_choices(existing: &[Value], wanted: &[&str]) -> Option<Vec<Value>> {
    let name = |c: &Value| c.as_str().or_else(|| c["name"].as_str()).map(str::to_owned);
    let have: Vec<String> = existing.iter().filter_map(name).collect();
    let missing: Vec<&str> = wanted.iter().copied().filter(|w| !have.iter().any(|h| h == w)).collect();
    if missing.is_empty() {
        return None;
    }
    let objects = existing.first().is_some_and(Value::is_object);
    let mut merged = existing.to_vec();
    let at = have
        .iter()
        .position(|h| h.eq_ignore_ascii_case("other"))
        .unwrap_or(merged.len());
    for (i, kind) in missing.into_iter().enumerate() {
        merged.insert(at + i, if objects { json!({"name": kind}) } else { json!(kind) });
    }
    Some(merged)
}

/// External identities from GCSP sources (docs/gcsp.md §8), keyed by Source + External ID.
/// Match: auto (verified email), confirmed / rejected (a person decided), pending (suggested),
/// unlinked (no candidate).
pub fn identities_fields() -> Vec<Value> {
    vec![
        field("Name", "text"),
        field("Source", "text"),
        field("External ID", "text"),
        field("Contact", "text"),
        select("Match", &["auto", "confirmed", "pending", "rejected", "unlinked"]),
        field("Emails", "text"),
        field("Attributes", "long_text"),
        field("URL", "url"),
        field("Updated at", "datetime"),
        field("Synced at", "datetime"),
        field("Deleted", "checkbox"),
    ]
}

fn field(title: &str, kind: &str) -> Value {
    json!({"title": title, "type": kind})
}

fn select_owned(title: &str, choices: &[String]) -> Value {
    json!({"title": title, "type": "single_select", "options": {"choices": choices}})
}

fn select(title: &str, choices: &[&str]) -> Value {
    json!({"title": title, "type": "single_select", "options": {"choices": choices}})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_accepts_legacy_and_english_labels() {
        assert_eq!(language("中文"), Some("Chinese"));
        assert_eq!(language("Chinese"), Some("Chinese"));
        assert_eq!(language("zh-CN"), Some("Chinese"));
        assert_eq!(language("English"), Some("English"));
        assert_eq!(language("auto"), None);
        assert_eq!(LEGACY_CHINESE, "中文");
    }

    #[test]
    fn merge_choices_adds_missing_kinds_before_other() {
        let old = vec![json!("email"), json!("x"), json!("custom"), json!("other")];
        let merged = merge_choices(&old, &["email", "x", "whatsapp", "linkedin", "other"]).unwrap();
        assert_eq!(merged, vec![json!("email"), json!("x"), json!("custom"), json!("whatsapp"), json!("linkedin"), json!("other")]);
        assert!(merge_choices(&merged, &["email", "whatsapp"]).is_none());
        let objects = merge_choices(&[json!({"name":"Email","color":"blue"})], &["Email", "LinkedIn"]).unwrap();
        assert_eq!(objects, vec![json!({"name":"Email","color":"blue"}), json!({"name":"LinkedIn"})]);
    }
}
