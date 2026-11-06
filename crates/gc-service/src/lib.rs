pub mod contacts;
pub mod notes;
pub mod memory;
use gc_docs::{Docs, Notify};
use std::sync::Arc;
/// The workspace owner's writing and analysis preferences, read by every agent prompt.
pub const PREFERENCES: &str = "preferences.md";
use tokio::sync::Mutex;
#[derive(Clone)]
pub struct Service {
    pub docs: Docs,
    pub notify: Notify,
    pub mutations: Arc<Mutex<()>>,
}
impl Service {
    pub fn new(docs: Docs, notify: Notify) -> Self {
        Self {
            docs,
            notify,
            mutations: Arc::default(),
        }
    }
    /// The workspace's `globalcontact.toml` (stages by role, fields, summary prompt).
    pub fn workspace_config(&self) -> Result<gc_core::config::WorkspaceConfig, String> {
        gc_core::config::WorkspaceConfig::load(&self.docs.workspace).map_err(|e| e.to_string())
    }
    /// The workspace owner's writing preferences (`preferences.md`), trimmed for prompts.
    pub fn preferences(&self) -> String {
        let text = self
            .docs
            .workspace
            .file(PREFERENCES)
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        text.chars().take(4000).collect::<String>().trim().to_owned()
    }
    pub fn preferences_full(&self) -> String {
        self.docs
            .workspace
            .file(PREFERENCES)
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default()
    }
    pub fn preferences_save(&self, text: &str) -> Result<(), String> {
        if text.chars().count() > 20000 {
            return Err("Preferences are too long (20,000 characters max).".into());
        }
        let path = self.docs.workspace.file(PREFERENCES).map_err(|e| e.to_string())?;
        gc_core::fs::write_atomic(&path, text.as_bytes()).map_err(|e| e.to_string())
    }
    /// Appends one remembered preference as a dated bullet.
    pub fn preferences_add(&self, note: &str) -> Result<(), String> {
        let note = note.trim().replace('\n', " ");
        if note.is_empty() {
            return Err("Write the preference to remember.".into());
        }
        let mut text = self
            .docs
            .workspace
            .file(PREFERENCES)
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_else(|| "# Writing preferences\n\nThe assistant follows these preferences when drafting and analyzing. Edit or delete them any time.\n\n".into());
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!("- {note} ({})\n", chrono::Local::now().format("%Y-%m-%d")));
        self.preferences_save(&text)
    }
    pub fn stage_for(&self, role: gc_core::config::Role) -> Result<String, String> {
        Ok(self.workspace_config()?.first(role).to_owned())
    }
}
pub mod import;
pub mod activity;
pub mod agent;
pub mod app;
pub mod mail;
pub mod mcp;
pub mod lists;
pub mod merge;
pub mod signals;
pub mod sources;
pub mod terminal;
