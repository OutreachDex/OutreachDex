//! Outreach lists (`lists/<id>.toml`) and their filters.
//!
//! A list selects people worth contacting from the workspace's contacts and, optionally, one
//! data source's people (including those not yet linked to a contact). Preparing a list drafts
//! a message per person for review; sending always stays a person's decision.
use crate::{Error, Workspace, config::Role};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;

pub const LIST_DIR: &str = "lists";
pub const SUPPRESSION_FILE: &str = "suppression.txt";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Filter {
    /// `contact.<Field>`, `role`, `attr.<key>`, `event.<type>`, `no_event.<type>`,
    /// `last_contacted`, `last_reply`.
    pub field: String,
    /// eq, neq, contains, gt, gte, lt, lte, in, empty, not_empty,
    /// within_days / older_than_days (events and dates), never (dates).
    pub op: String,
    /// TOML has no null: filters without a value omit it.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub value: Value,
    /// `event_count.<type>`: only events in the last `days` count (none: all time).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub days: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListSpec {
    #[serde(default, skip_serializing)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// A GCSP source key; its people (linked or not) are candidates as well as contacts.
    #[serde(default)]
    pub source: String,
    /// Purpose playbook used to prepare drafts (default: outreach).
    #[serde(default)]
    pub playbook: String,
    #[serde(default)]
    pub context: String,
    #[serde(default = "email")]
    pub channel: String,
    #[serde(default)]
    pub filters: Vec<Filter>,
}

fn email() -> String {
    "email".into()
}

/// One candidate: a contact, a source person, or both when linked.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Member {
    /// Stable key: `contact:<slug>` or `source:<key>:<external_id>`.
    pub key: String,
    pub name: String,
    pub email: String,
    pub contact: Option<String>,
    pub identity: Option<(String, String)>,
    #[serde(skip)]
    pub values: Map<String, Value>,
    pub role: Option<Role>,
    pub stage: String,
    pub attributes: Map<String, Value>,
    /// (type, occurred_at RFC 3339)
    #[serde(skip)]
    pub events: Vec<(String, String)>,
    pub suppressed: bool,
    /// Human-readable reasons this member matched, for "why now".
    pub evidence: Vec<String>,
}

fn now_ms() -> i64 {
    crate::now_millis()
}

fn time(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value).ok().map(|t| t.timestamp_millis())
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
}

fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn compare(actual: &Value, op: &str, expected: &Value) -> bool {
    let empty = actual.is_null() || text(actual).trim().is_empty();
    match op {
        "empty" => empty,
        "not_empty" => !empty,
        "eq" => !empty && text(actual).eq_ignore_ascii_case(&text(expected)),
        "neq" => empty || !text(actual).eq_ignore_ascii_case(&text(expected)),
        "contains" => text(actual).to_lowercase().contains(&text(expected).to_lowercase()),
        "in" => expected.as_array().is_some_and(|items| items.iter().any(|e| text(actual).eq_ignore_ascii_case(&text(e)))),
        "gt" | "gte" | "lt" | "lte" => match (number(actual), number(expected)) {
            (Some(a), Some(b)) => match op {
                "gt" => a > b,
                "gte" => a >= b,
                "lt" => a < b,
                _ => a <= b,
            },
            _ => false,
        },
        _ => false,
    }
}

fn days(value: &Value) -> i64 {
    number(value).unwrap_or(30.0).max(0.0) as i64
}

impl Filter {
    /// Whether `member` satisfies the filter; on success, a short reason.
    pub fn check(&self, member: &Member, roles: &dyn Fn(&str) -> Option<Role>) -> Option<String> {
        self.check_labeled(member, roles, &|k: &str| k.to_owned())
    }

    /// Like `check`; `label` turns attribute keys and event types into display names.
    pub fn check_labeled(&self, member: &Member, roles: &dyn Fn(&str) -> Option<Role>, label: &dyn Fn(&str) -> String) -> Option<String> {
        let field = self.field.as_str();
        if let Some(name) = field.strip_prefix("contact.") {
            let actual = member.values.get(name).cloned().unwrap_or(Value::Null);
            return compare(&actual, &self.op, &self.value).then(|| format!("{name}: {}", text(&actual)));
        }
        if let Some(key) = field.strip_prefix("attr.") {
            let actual = member.attributes.get(key).cloned().unwrap_or(Value::Null);
            return compare(&actual, &self.op, &self.value).then(|| format!("{} = {}", label(key), text(&actual)));
        }
        if field == "role" {
            let role = roles(&member.stage).map(|r| serde_json::to_value(r).unwrap_or_default()).unwrap_or(Value::Null);
            return compare(&role, if self.op == "eq" || self.op == "in" { &self.op } else { "in" }, &self.value)
                .then(|| format!("Stage: {}", if member.stage.is_empty() { "Not contacted" } else { &member.stage }));
        }
        if let Some(kind) = field.strip_prefix("event.") {
            let name = label(kind);
            let window = days(&self.value) * 86_400_000;
            let latest = member
                .events
                .iter()
                .filter(|(t, _)| t == kind)
                .filter_map(|(_, at)| time(at))
                .max();
            return match (self.op.as_str(), latest) {
                ("within_days", Some(at)) if now_ms() - at <= window => Some(format!("“{name}” in the last {} days", days(&self.value))),
                ("older_than_days", Some(at)) if now_ms() - at > window => Some(format!("“{name}” more than {} days ago", days(&self.value))),
                ("exists", Some(_)) | ("not_empty", Some(_)) => Some(format!("Has “{name}”")),
                _ => None,
            };
        }
        if let Some(kind) = field.strip_prefix("event_count.") {
            let since = self.days.filter(|d| *d > 0).map(|d| now_ms() - d * 86_400_000);
            let count = member
                .events
                .iter()
                .filter(|(t, at)| t == kind && since.is_none_or(|s| time(at).is_some_and(|a| a >= s)))
                .count();
            let target = number(&self.value).unwrap_or(1.0);
            let hit = match self.op.as_str() {
                "gt" => count as f64 > target,
                "lte" => count as f64 <= target,
                "lt" => (count as f64) < target,
                "eq" => (count as f64 - target).abs() < f64::EPSILON,
                _ => count as f64 >= target,
            };
            let window = self.days.filter(|d| *d > 0).map(|d| format!(" in the last {d} days")).unwrap_or_default();
            return hit.then(|| format!("“{}” {count} {}{window}", label(kind), if count == 1 { "time" } else { "times" }));
        }
        if let Some(kind) = field.strip_prefix("no_event.") {
            return (!member.events.iter().any(|(t, _)| t == kind)).then(|| format!("No “{}”", label(kind)));
        }
        if field == "last_contacted" || field == "last_reply" {
            let column = if field == "last_contacted" { "Last contacted" } else { "Last reply" };
            let at = member.values.get(column).and_then(Value::as_str).and_then(time);
            let label = if field == "last_contacted" { "Last contacted" } else { "Last reply" };
            return match (self.op.as_str(), at) {
                ("never", None) => Some(format!("{label}: never")),
                ("older_than_days", None) => Some(format!("{label}: never")),
                ("older_than_days", Some(t)) if now_ms() - t > days(&self.value) * 86_400_000 => {
                    Some(format!("{label} more than {} days ago", days(&self.value)))
                }
                ("within_days", Some(t)) if now_ms() - t <= days(&self.value) * 86_400_000 => {
                    Some(format!("{label} within {} days", days(&self.value)))
                }
                _ => None,
            };
        }
        None
    }
}

impl ListSpec {
    pub fn parse(id: &str, text: &str) -> Result<Self, Error> {
        let mut spec: Self = toml::from_str(text).map_err(|e| Error::Invalid(format!("List {id} is invalid: {e}")))?;
        spec.id = id.to_owned();
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.name.trim().is_empty() {
            return Err(Error::Invalid("The list needs a name.".into()));
        }
        if !crate::channels::is_message(&self.channel) {
            return Err(Error::Invalid("Invalid list channel.".into()));
        }
        for f in &self.filters {
            let known = f.field.starts_with("contact.")
                || f.field.starts_with("attr.")
                || f.field.starts_with("event.")
                || f.field.starts_with("no_event.")
                || f.field.starts_with("event_count.")
                || matches!(f.field.as_str(), "role" | "last_contacted" | "last_reply");
            if !known {
                return Err(Error::Invalid(format!("Unknown filter field: {}", f.field)));
            }
        }
        Ok(())
    }

    /// Evaluates every filter (all must hold). Suppressed people never match.
    pub fn select(&self, members: Vec<Member>, roles: &dyn Fn(&str) -> Option<Role>) -> Vec<Member> {
        self.select_labeled(members, roles, &|k: &str| k.to_owned())
    }

    pub fn select_labeled(&self, members: Vec<Member>, roles: &dyn Fn(&str) -> Option<Role>, label: &dyn Fn(&str) -> String) -> Vec<Member> {
        members
            .into_iter()
            .filter(|m| !m.suppressed && !matches!(m.role, Some(Role::Suppressed)))
            .filter_map(|mut m| {
                let mut evidence = Vec::new();
                for filter in &self.filters {
                    evidence.push(filter.check_labeled(&m, roles, label)?);
                }
                m.evidence = evidence;
                Some(m)
            })
            .collect()
    }

    pub fn to_toml(&self) -> Result<String, Error> {
        toml::to_string_pretty(self).map_err(|e| Error::Invalid(e.to_string()))
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// All lists in the workspace, by file name.
pub fn load(workspace: &Workspace) -> Result<Vec<ListSpec>, Error> {
    let mut lists = Vec::new();
    let Ok(entries) = fs::read_dir(workspace.file(LIST_DIR)?) else { return Ok(lists) };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".toml"))
        .collect();
    names.sort();
    for name in names {
        let id = name.trim_end_matches(".toml");
        if !valid_id(id) {
            continue;
        }
        let text = fs::read_to_string(workspace.file(&format!("{LIST_DIR}/{name}"))?)?;
        lists.push(ListSpec::parse(id, &text)?);
    }
    Ok(lists)
}

pub fn save(workspace: &Workspace, spec: &ListSpec) -> Result<(), Error> {
    if !valid_id(&spec.id) {
        return Err(Error::Invalid("A list ID may contain only lowercase letters, digits and -.".into()));
    }
    spec.validate()?;
    let path = workspace.file(&format!("{LIST_DIR}/{}.toml", spec.id))?;
    fs::create_dir_all(path.parent().ok_or_else(|| Error::Invalid("Invalid path".into()))?)?;
    crate::fs::write_atomic(&path, spec.to_toml()?.as_bytes())?;
    Ok(())
}

pub fn delete(workspace: &Workspace, id: &str) -> Result<(), Error> {
    if !valid_id(id) {
        return Err(Error::Invalid("Invalid list.".into()));
    }
    match fs::remove_file(workspace.file(&format!("{LIST_DIR}/{id}.toml"))?) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Lowercased addresses that must never be contacted (one per line, `#` comments).
pub fn suppression(workspace: &Workspace) -> Result<Vec<String>, Error> {
    match fs::read_to_string(workspace.file(SUPPRESSION_FILE)?) {
        Ok(text) => Ok(text
            .lines()
            .map(|l| l.split('#').next().unwrap_or("").trim().to_lowercase())
            .filter(|l| !l.is_empty())
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

pub fn suppress(workspace: &Workspace, address: &str, reason: &str) -> Result<(), Error> {
    let address = address.trim().to_lowercase();
    if address.is_empty() || address.contains(char::is_whitespace) {
        return Err(Error::Invalid("Invalid address.".into()));
    }
    if suppression(workspace)?.contains(&address) {
        return Ok(());
    }
    let path = workspace.file(SUPPRESSION_FILE)?;
    let mut text = fs::read_to_string(&path).unwrap_or_else(|_| "# Addresses never to contact, one per line. Lists and sending both skip them.\n".into());
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let reason = reason.replace(['\n', '#'], " ");
    text.push_str(&format!("{address}  # {reason} {}\n", crate::time::to_rfc3339(crate::now_millis())));
    crate::fs::write_atomic(&path, text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn member(attrs: Value, events: &[(&str, i64)], stage: &str) -> Member {
        Member {
            key: "m".into(),
            stage: stage.into(),
            attributes: attrs.as_object().cloned().unwrap_or_default(),
            events: events
                .iter()
                .map(|(t, days_ago)| (t.to_string(), crate::time::to_rfc3339(now_ms() - days_ago * 86_400_000)))
                .collect(),
            ..Default::default()
        }
    }

    fn roles(stage: &str) -> Option<Role> {
        match stage {
            "Sent" => Some(Role::Waiting),
            "Do not contact" => Some(Role::Suppressed),
            _ => Some(Role::Open),
        }
    }

    #[test]
    fn filters_attributes_events_and_roles() {
        let spec = ListSpec::parse(
            "free-exhausted",
            r#"
name = "Free credits used up, not paid"
source = "ts"
[[filters]]
field = "attr.plan"
op = "eq"
value = "free"
[[filters]]
field = "attr.credits_balance"
op = "lte"
value = 0
[[filters]]
field = "event.credits_exhausted"
op = "within_days"
value = 30
[[filters]]
field = "no_event.purchase"
op = "is"
[[filters]]
field = "role"
op = "in"
value = ["open"]
"#,
        )
        .unwrap();
        let candidates = vec![
            member(json!({"plan":"free","credits_balance":0}), &[("credits_exhausted", 3)], ""),
            member(json!({"plan":"free","credits_balance":"0"}), &[("credits_exhausted", 90)], ""),
            member(json!({"plan":"free","credits_balance":0}), &[("credits_exhausted", 3), ("purchase", 1)], ""),
            member(json!({"plan":"creator","credits_balance":0}), &[("credits_exhausted", 3)], ""),
            member(json!({"plan":"free","credits_balance":0}), &[("credits_exhausted", 3)], "Sent"),
            member(json!({"plan":"free","credits_balance":0}), &[("credits_exhausted", 3)], "Do not contact"),
        ];
        let mut suppressed = candidates[0].clone();
        suppressed.suppressed = true;
        let mut all = candidates;
        all.push(suppressed);
        let selected = spec.select(all, &roles);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].evidence.len(), 5);
        assert!(selected[0].evidence.iter().any(|e| e.contains("credits_exhausted")));
    }

    #[test]
    fn rejects_unknown_fields_and_round_trips() {
        assert!(ListSpec::parse("x", "name = \"x\"\n[[filters]]\nfield = \"password\"\nop = \"eq\"\n").is_err());
        let mut spec = ListSpec::parse("x", "name = \"X\"\nchannel = \"wechat\"\n").unwrap();
        spec.filters.push(Filter { field: "no_event.purchase".into(), op: "is".into(), value: Value::Null, days: None });
        let again = ListSpec::parse("x", &spec.to_toml().unwrap()).unwrap();
        assert_eq!(again.channel, "wechat");
        assert!(!valid_id("Bad Id") && valid_id("free-exhausted-2"));
    }

    #[test]
    fn event_counts_with_window() {
        let m = member(json!({}), &[("job_failed", 1), ("job_failed", 5), ("job_failed", 40)], "");
        let roles = |_: &str| Some(Role::Open);
        let recent = Filter { field: "event_count.job_failed".into(), op: "gte".into(), value: json!(2), days: Some(14) };
        assert_eq!(recent.check(&m, &roles).as_deref(), Some("“job_failed” 2 times in the last 14 days"));
        let strict = Filter { days: Some(3), ..recent.clone() };
        assert!(strict.check(&m, &roles).is_none());
        let ever = Filter { field: "event_count.purchase".into(), op: "eq".into(), value: json!(0), days: None };
        assert!(ever.check(&m, &roles).is_some());
        let spec = ListSpec::parse("c", "name = \"C\"\n[[filters]]\nfield = \"event_count.job_failed\"\nop = \"gte\"\nvalue = 2\ndays = 14\n").unwrap();
        assert_eq!(spec.filters[0].days, Some(14));
    }

    #[test]
    fn dates_and_contact_fields() {
        let mut m = member(json!({}), &[], "");
        m.values.insert("Campaign".into(), json!("Launch"));
        let roles = |_: &str| Some(Role::Open);
        let never = Filter { field: "last_contacted".into(), op: "never".into(), value: Value::Null, days: None };
        assert!(never.check(&m, &roles).is_some());
        let campaign = Filter { field: "contact.Campaign".into(), op: "eq".into(), value: json!("launch"), days: None };
        assert!(campaign.check(&m, &roles).is_some());
        m.values.insert("Last contacted".into(), json!(crate::time::to_rfc3339(now_ms() - 2 * 86_400_000)));
        assert!(never.check(&m, &roles).is_none());
        let stale = Filter { field: "last_contacted".into(), op: "older_than_days".into(), value: json!(7), days: None };
        assert!(stale.check(&m, &roles).is_none());
    }
}
