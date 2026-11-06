//! Workspace configuration (`globalcontact.toml`) and analysis playbooks (`playbooks/*.md`).
//!
//! Business specifics live in the workspace, not in code: stage names, extra contact fields,
//! campaign choices, the AI summary prompt, analysis purposes and business contexts. Code only
//! relies on stage *roles*.
use crate::{Error, Workspace};
use serde::{Deserialize, Serialize};
use std::fs;

pub const CONFIG_FILE: &str = "globalcontact.toml";
pub const PLAYBOOK_DIR: &str = "playbooks";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Not contacted yet.
    Open,
    /// We wrote; waiting for their reply.
    Waiting,
    /// They replied, or work is in progress.
    Active,
    /// Paused or resolved.
    Closed,
    /// Do not contact.
    Suppressed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stage {
    pub label: String,
    pub role: Role,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ContactSettings {
    #[serde(default)]
    pub campaigns: Vec<String>,
    #[serde(default)]
    pub summary_prompt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutreachSettings {
    /// Most list messages sent per day (email sends and manual "mark sent" together).
    #[serde(default = "default_daily_limit")]
    pub daily_limit: u32,
}

fn default_daily_limit() -> u32 {
    20
}

impl Default for OutreachSettings {
    fn default() -> Self {
        Self { daily_limit: default_daily_limit() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FieldSpec {
    pub title: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub version: u32,
    #[serde(default)]
    pub template: String,
    pub stages: Vec<Stage>,
    #[serde(default)]
    pub contacts: ContactSettings,
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
    #[serde(default)]
    pub outreach: OutreachSettings,
}

pub struct Template {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    config: &'static str,
    playbooks: &'static [(&'static str, &'static str)],
}

const COMMON: &[(&str, &str)] = &[
    ("understand.md", include_str!("../templates/common/playbooks/understand.md")),
    ("feedback.md", include_str!("../templates/common/playbooks/feedback.md")),
    ("outreach.md", include_str!("../templates/common/playbooks/outreach.md")),
    ("followup.md", include_str!("../templates/common/playbooks/followup.md")),
    ("question.md", include_str!("../templates/common/playbooks/question.md")),
    ("general.md", include_str!("../templates/common/playbooks/general.md")),
];

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "blank",
        label: "Blank",
        description: "General stages and analysis purposes for any one-to-one relationship.",
        config: include_str!("../templates/blank/globalcontact.toml"),
        playbooks: &[],
    },
    Template {
        id: "expert-outreach",
        label: "Expert outreach",
        description: "Research, first contact and follow-up with academics or industry experts.",
        config: include_str!("../templates/expert-outreach/globalcontact.toml"),
        playbooks: &[],
    },
    Template {
        id: "user-relationships",
        label: "Product user relationships",
        description: "Understand product users' goals and feedback, and follow up on issues and check-ins.",
        config: include_str!("../templates/user-relationships/globalcontact.toml"),
        playbooks: &[(
            "product-users.md",
            include_str!("../templates/user-relationships/playbooks/product-users.md"),
        )],
    },
];

pub fn template(id: &str) -> Result<&'static Template, Error> {
    TEMPLATES
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| Error::Invalid(format!("Unknown workspace template: {id}")))
}

impl Template {
    pub fn config(&self) -> WorkspaceConfig {
        WorkspaceConfig::parse(self.config).expect("bundled template config is valid")
    }
}

impl WorkspaceConfig {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let config: Self = toml::from_str(text)
            .map_err(|e| Error::Invalid(format!("{CONFIG_FILE} is invalid: {e}")))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.stages.is_empty() {
            return Err(Error::Invalid(format!("{CONFIG_FILE} needs at least one stage.")));
        }
        for role in [Role::Open, Role::Waiting, Role::Active] {
            if !self.stages.iter().any(|s| s.role == role) {
                return Err(Error::Invalid(format!(
                    "{CONFIG_FILE} needs at least one stage with role = \"{}\".",
                    serde_json::to_value(role).unwrap().as_str().unwrap_or_default()
                )));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for stage in &self.stages {
            if stage.label.trim().is_empty() || !seen.insert(stage.label.as_str()) {
                return Err(Error::Invalid(format!("Stage names in {CONFIG_FILE} can't be empty or repeated.")));
            }
        }
        Ok(())
    }

    /// Reads the workspace configuration. Workspaces created before configuration existed get
    /// the legacy one, whose (Chinese) stage labels match what those versions wrote into their
    /// table.
    pub fn load(workspace: &Workspace) -> Result<Self, Error> {
        match fs::read_to_string(workspace.file(CONFIG_FILE)?) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                write_new(workspace, CONFIG_FILE, LEGACY_CONFIG)?;
                install_playbooks(workspace, &[])?;
                Self::parse(LEGACY_CONFIG)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// The stage new contacts start in.
    pub fn initial_stage(&self) -> &str {
        self.first(Role::Open)
    }

    /// The first stage with `role`; validation guarantees open, waiting and active exist.
    pub fn first(&self, role: Role) -> &str {
        self.stages
            .iter()
            .find(|s| s.role == role)
            .or_else(|| self.stages.first())
            .map(|s| s.label.as_str())
            .unwrap_or_default()
    }

    pub fn role(&self, label: &str) -> Option<Role> {
        self.stages.iter().find(|s| s.label == label).map(|s| s.role)
    }

    /// Paused, resolved or do-not-contact: no follow-up reminders or automatic analysis.
    pub fn is_inactive(&self, label: &str) -> bool {
        matches!(self.role(label), Some(Role::Closed | Role::Suppressed))
    }

    pub fn labels(&self) -> Vec<&str> {
        self.stages.iter().map(|s| s.label.as_str()).collect()
    }
}

/// Configuration for workspaces created before `globalcontact.toml` existed: the expert-outreach
/// stages as those versions named them, without extra fields (older tables do not have them).
const LEGACY_CONFIG: &str = include_str!("../templates/legacy/globalcontact.toml");

/// Writes the template's configuration and playbooks into a new workspace.
pub fn install(workspace: &Workspace, id: &str) -> Result<WorkspaceConfig, Error> {
    let template = template(id)?;
    write_new(workspace, CONFIG_FILE, template.config)?;
    install_playbooks(workspace, template.playbooks)?;
    Ok(template.config())
}

fn install_playbooks(workspace: &Workspace, extra: &[(&str, &str)]) -> Result<(), Error> {
    fs::create_dir_all(workspace.file(PLAYBOOK_DIR)?)?;
    for (name, text) in COMMON.iter().chain(extra) {
        let path = format!("{PLAYBOOK_DIR}/{name}");
        write_new(workspace, &path, text)?;
    }
    Ok(())
}

/// Creates `path` unless it exists. The editor write path only accepts documents, so the
/// configuration goes through the checked path resolver and an atomic write directly.
fn write_new(workspace: &Workspace, path: &str, text: &str) -> Result<(), Error> {
    let target = workspace.file(path)?;
    if target.symlink_metadata().is_ok() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::fs::write_atomic(&target, text.as_bytes())?;
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct Playbook {
    pub id: String,
    pub label: String,
    /// `purpose` (what to do) or `context` (business background).
    pub kind: String,
    pub order: i64,
    pub creates_drafts: bool,
    pub requires_prompt: bool,
    pub path: String,
    #[serde(skip)]
    pub instructions: String,
}

/// Playbooks in the workspace, ordered for display. Purposes fall back to the bundled ones when
/// the workspace defines none, so analysis always has at least one purpose.
pub fn playbooks(workspace: &Workspace) -> Result<Vec<Playbook>, Error> {
    let mut items = Vec::new();
    let dir = workspace.file(PLAYBOOK_DIR)?;
    if let Ok(entries) = fs::read_dir(&dir) {
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".md") && !n.starts_with('.'))
            .collect();
        names.sort();
        for name in names {
            let path = format!("{PLAYBOOK_DIR}/{name}");
            let text = fs::read_to_string(workspace.file(&path)?)?;
            if let Some(playbook) = parse_playbook(&path, &text)
                && !items.iter().any(|p: &Playbook| p.id == playbook.id)
            {
                items.push(playbook);
            }
        }
    }
    if !items.iter().any(|p| p.kind == "purpose") {
        items.extend(
            COMMON
                .iter()
                .filter_map(|(name, text)| parse_playbook(&format!("{PLAYBOOK_DIR}/{name}"), text))
                .filter(|p| p.kind == "purpose"),
        );
    }
    items.sort_by(|a, b| a.kind.cmp(&b.kind).reverse().then(a.order.cmp(&b.order)).then(a.label.cmp(&b.label)));
    Ok(items)
}

/// Parses `---` front matter (flat `key: value` pairs) followed by the instructions.
pub fn parse_playbook(path: &str, text: &str) -> Option<Playbook> {
    let rest = text.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    let (head, body) = (&rest[..end], &rest[end + 4..]);
    let mut meta = std::collections::HashMap::new();
    for line in head.lines() {
        if let Some((key, value)) = line.split_once(':') {
            meta.insert(key.trim().to_owned(), value.trim().trim_matches('"').to_owned());
        }
    }
    let stem = path.rsplit('/').next()?.trim_end_matches(".md");
    let id = meta.get("id").filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| stem.to_owned());
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || id == "contact" {
        return None;
    }
    let kind = meta.get("kind").map(String::as_str).unwrap_or("purpose");
    if kind != "purpose" && kind != "context" {
        return None;
    }
    let flag = |key: &str| meta.get(key).is_some_and(|v| v == "true");
    let instructions = body.trim().to_owned();
    if instructions.is_empty() {
        return None;
    }
    Some(Playbook {
        label: meta.get("label").filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| id.clone()),
        id,
        kind: kind.into(),
        order: meta.get("order").and_then(|v| v.parse().ok()).unwrap_or(100),
        creates_drafts: flag("creates_drafts"),
        requires_prompt: flag("requires_prompt"),
        path: path.into(),
        instructions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_templates_parse() {
        for t in TEMPLATES {
            let config = t.config();
            assert!(!config.initial_stage().is_empty(), "{}", t.id);
        }
        for (name, text) in COMMON {
            assert!(parse_playbook(name, text).is_some(), "{name}");
        }
        let legacy = WorkspaceConfig::parse(LEGACY_CONFIG).unwrap();
        assert_eq!(legacy.template, "legacy");
        assert!(legacy.fields.is_empty());
    }

    #[test]
    fn roles_drive_stage_choice() {
        let config = template("blank").unwrap().config();
        assert_eq!(config.first(Role::Waiting), "Sent");
        assert_eq!(config.first(Role::Active), "Replied");
        assert!(config.is_inactive("Do not contact"));
        assert!(!config.is_inactive("Replied"));
    }

    #[test]
    fn rejects_configs_without_required_roles() {
        let text = "version = 1\nstages = [{ label = \"A\", role = \"open\" }]\n";
        assert!(WorkspaceConfig::parse(text).is_err());
    }

    #[test]
    fn playbook_front_matter() {
        let p = parse_playbook("playbooks/x.md", "---\nlabel: Test\nkind: context\norder: 3\n---\nDo it.\n").unwrap();
        assert_eq!((p.id.as_str(), p.label.as_str(), p.kind.as_str(), p.order), ("x", "Test", "context", 3));
        assert!(parse_playbook("playbooks/bad.md", "no front matter").is_none());
        assert!(parse_playbook("playbooks/contact.md", "---\nkind: context\n---\nx").is_none());
    }
}
