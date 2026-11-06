//! Per-contact Agent memory: an append-only log in `memory/<slug>.md` of what the Agent learned
//! about a person and what it changed for them. Every new analysis reads it first, so the
//! understanding of a person accumulates across sessions instead of starting over.
use crate::Service;
use serde_json::{Value, json};

/// Kinds of entries and their labels in the log; `change` is written by the host for every MCP
/// write, the rest by the Agent. Logs written by older versions carry Chinese labels; they are
/// kept as written (the host never parses labels back).
pub const KINDS: &[(&str, &str)] = &[
    ("fact", "Fact"),
    ("insight", "Insight"),
    ("preference", "Preference"),
    ("commitment", "Commitment"),
    ("question", "Question"),
    ("change", "Change"),
];
const READ_LIMIT: usize = 16000;

pub fn path(slug: &str) -> String {
    format!("memory/{slug}.md")
}

impl Service {
    /// Appends one dated entry. Text is stored as data; it never becomes an instruction.
    pub async fn memory_append(&self, slug: &str, kind: &str, text: &str, origin: &str) -> Result<Value, String> {
        let label = KINDS.iter().find(|(k, _)| *k == kind).map(|(_, l)| *l).ok_or("Unknown entry type.")?;
        let text = text.trim();
        if text.is_empty() {
            return Err("The entry is empty.".into());
        }
        let contact = self.contact(slug).await?;
        let text: String = text.chars().take(2000).collect();
        let at = chrono::Local::now().format("%Y-%m-%d %H:%M");
        let entry = format!("- {at} · {label} · {origin}\n  {}\n", text.replace('\n', "\n  "));
        let path = path(slug);
        let _guard = self.mutations.lock().await;
        if self.docs.workspace.stamp(&path).map_err(|e| e.to_string())?.is_some() {
            // Through the document service, so an open editor shows the new entry live.
            let current = self.read_note(&path).await?;
            let next = format!("{}\n{entry}", current.trim_end());
            self.docs
                .execute(&path, "text.replace", json!({"from":0,"to":current.chars().count(),"expected":current,"text":next}), "system:memory")
                .await?;
        } else {
            let head = format!("# Agent log · {}\n\nAppended in time order: what the Agent learned and what it changed. Later analyses read this first.\n\n", contact.text("Name"));
            self.docs.workspace.put_new(&path, format!("{head}{entry}").as_bytes()).map_err(|e| e.to_string())?;
        }
        Ok(json!({"path":path}))
    }

    /// The log, newest part first when long, so prompts stay bounded.
    pub async fn memory_read(&self, slug: &str) -> Result<Value, String> {
        let path = path(slug);
        if self.docs.workspace.stamp(&path).map_err(|e| e.to_string())?.is_none() {
            return Ok(json!({"path":path,"text":"","truncated":false}));
        }
        let text = self.read_note(&path).await?;
        let count = text.chars().count();
        let truncated = count > READ_LIMIT;
        let text = if truncated { text.chars().skip(count - READ_LIMIT).collect() } else { text };
        Ok(json!({"path":path,"text":text,"truncated":truncated}))
    }
}
