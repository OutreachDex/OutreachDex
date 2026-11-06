//! Conformance checks for a GCSP/1 source (`globalcontact source check`, and the connect wizard).
use crate::{Client, Manifest, SourceError, parse_event, parse_person};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub name: String,
    pub level: Level,
    pub detail: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub manifest: Option<Manifest>,
    /// The first few people, for the connect wizard's preview.
    pub sample: Vec<Value>,
}

impl Report {
    fn add(&mut self, name: &str, level: Level, detail: impl Into<String>) {
        self.findings.push(Finding { name: name.into(), level, detail: detail.into() });
    }
    pub fn passed(&self) -> bool {
        self.manifest.is_some() && !self.findings.iter().any(|f| f.level == Level::Fail)
    }
}

const PAGES: usize = 3;
const LIMIT: u32 = 5;

pub async fn run(client: &Client) -> Report {
    let mut report = Report::default();
    let manifest = match client.manifest().await {
        Ok(m) => {
            report.add("manifest", Level::Ok, format!("{} ({}), capabilities: {}", m.source.name, m.source.id, m.capabilities.join(", ")));
            m
        }
        Err(e) => {
            report.add("manifest", Level::Fail, e.to_string());
            return report;
        }
    };
    match client.anonymous().manifest().await {
        Err(SourceError::Auth(_)) => report.add("authentication", Level::Ok, "Requests without credentials are rejected"),
        Ok(_) => report.add("authentication", Level::Warn, "The manifest can be read without credentials; if people and other endpoints are open too, add authentication"),
        Err(e) => report.add("authentication", Level::Warn, format!("Couldn't confirm authentication behavior: {e}")),
    }
    let declared: HashSet<&str> = manifest.attributes.iter().map(|a| a.key.as_str()).collect();
    let first_person = stream(client, &mut report, "people", &declared).await;
    if manifest.can("events") {
        stream(client, &mut report, "events", &declared).await;
    } else {
        report.add("events", Level::Ok, "events capability not declared; skipped");
    }
    if manifest.can("person_detail") {
        match first_person {
            Some(person) => match client.person(person["external_id"].as_str().unwrap_or_default()).await {
                Ok(p) if Some(p.external_id.as_str()) == person["external_id"].as_str() => {
                    report.add("person_detail", Level::Ok, format!("/people/{} returns the same person", p.external_id))
                }
                Ok(p) => report.add("person_detail", Level::Fail, format!("Requested {} but got {}", person["external_id"], p.external_id)),
                Err(e) => report.add("person_detail", Level::Fail, e.to_string()),
            },
            None => report.add("person_detail", Level::Warn, "No people available to check the detail endpoint"),
        }
    }
    match client.page("people", Some("gcsp-check-invalid-cursor"), 1).await {
        Err(SourceError::InvalidCursor) => report.add("invalid cursor", Level::Ok, "Returns 400 invalid_cursor"),
        Err(e) => report.add("invalid cursor", Level::Warn, format!("Should return 400 with error.code = invalid_cursor; got: {e}")),
        Ok(_) => report.add("invalid cursor", Level::Warn, "An unrecognized cursor should return 400 invalid_cursor, not normal data"),
    }
    report.sample = report.sample.iter().take(5).cloned().collect();
    report.manifest = Some(manifest);
    report
}

/// Pages through up to `PAGES` pages of one stream; returns the first live item.
async fn stream(client: &Client, report: &mut Report, name: &str, declared: &HashSet<&str>) -> Option<Value> {
    let mut cursor: Option<String> = None;
    let mut seen = HashSet::new();
    let mut first_ids = Vec::new();
    let mut first_live = None;
    let (mut total, mut invalid, mut undeclared) = (0usize, Vec::new(), HashSet::new());
    let mut last_time = String::new();
    let mut unordered = false;
    for index in 0..PAGES {
        let page = match client.page(name, cursor.as_deref(), LIMIT).await {
            Ok(p) => p,
            Err(e) => {
                report.add(name, Level::Fail, format!("Page {}: {e}", index + 1));
                return first_live;
            }
        };
        if page.items.len() > LIMIT as usize {
            report.add(&format!("{name} paging"), Level::Warn, format!("Requested limit={LIMIT} but got {} items", page.items.len()));
        }
        for item in &page.items {
            total += 1;
            let parsed = if name == "people" {
                parse_person(item).map(|p| (p.external_id.clone(), p.updated_at.clone(), p.deleted, p.attributes))
            } else {
                parse_event(item).map(|e| {
                    let time = e.updated_at.clone().unwrap_or(e.occurred_at.clone());
                    (e.external_id.clone(), time, e.deleted, Default::default())
                })
            };
            match parsed {
                Ok((id, time, deleted, attributes)) => {
                    if index == 0 {
                        first_ids.push(id.clone());
                    }
                    seen.insert(id);
                    if !time.is_empty() && time < last_time {
                        unordered = true;
                    }
                    last_time = time;
                    undeclared.extend(attributes.keys().filter(|k| !declared.contains(k.as_str())).cloned());
                    if !deleted && first_live.is_none() {
                        first_live = Some(item.clone());
                    }
                    if name == "people" && !deleted {
                        report.sample.push(item.clone());
                    }
                }
                Err(e) => invalid.push(e),
            }
        }
        if !page.has_more {
            if page.next_cursor.is_none() {
                report.add(&format!("{name} incremental"), Level::Warn, "The last page returned no next_cursor, so clients must sync from the start every time");
            } else {
                report.add(&format!("{name} incremental"), Level::Ok, "The last page returned a next_cursor");
            }
            break;
        }
        if page.next_cursor == cursor {
            report.add(&format!("{name} paging"), Level::Fail, "has_more is true but next_cursor did not advance; clients would repeat the same page forever");
            return first_live;
        }
        cursor = page.next_cursor;
    }
    if invalid.is_empty() {
        report.add(name, Level::Ok, format!("Checked {total} items; fields are well formed"));
    } else {
        report.add(name, Level::Fail, format!("{} / {total} items are invalid, e.g.: {}", invalid.len(), invalid[0]));
    }
    if unordered {
        report.add(&format!("{name} order"), Level::Warn, "Items are not in ascending update order; incremental sync may miss data");
    }
    if name == "people" && !undeclared.is_empty() {
        let mut keys: Vec<_> = undeclared.into_iter().collect();
        keys.sort();
        report.add(
            "attribute allowlist",
            Level::Warn,
            format!("These attributes are not declared in the manifest and OutreachDex drops them: {}", keys.join(", ")),
        );
    }
    match client.page(name, None, LIMIT).await {
        Ok(again) => {
            let ids: Vec<String> = again.items.iter().filter_map(|i| i["external_id"].as_str().map(str::to_owned)).collect();
            if ids == first_ids {
                report.add(&format!("{name} stability"), Level::Ok, "Repeated requests for the first page return the same result");
            } else {
                report.add(&format!("{name} stability"), Level::Warn, "Repeated requests for the first page return different results; use a stable sort order");
            }
        }
        Err(e) => report.add(&format!("{name} stability"), Level::Warn, e.to_string()),
    }
    first_live
}
