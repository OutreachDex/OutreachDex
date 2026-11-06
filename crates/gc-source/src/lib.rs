//! GCSP/1 client: protocol types, a read-only HTTP client and the conformance checker.
//! The protocol is specified in docs/gcsp.md; schemas are in spec/gcsp/v1/.
pub mod check;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::time::Duration;

pub const PROTOCOL: &str = "gcsp/1";
pub const MAX_LIMIT: u32 = 500;
/// GCSP identity kinds (docs/gcsp.md, spec/gcsp/v1/person.schema.json). Messaging kinds match
/// gc_core::channels; `phone` is a GCSP-only kind.
const IDENTITY_KINDS: &[&str] = &[
    "email", "x", "github", "instagram", "wechat", "whatsapp", "tiktok", "facebook", "linkedin", "phone", "homepage",
    "other",
];

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("The source rejected the credentials ({0}). Check the token and header.")]
    Auth(u16),
    #[error("The saved sync position is no longer valid; the next sync starts from the beginning.")]
    InvalidCursor,
    #[error("The source is rate limiting requests. Try again in {0} seconds.")]
    RateLimited(u64),
    #[error("The source returned HTTP {0}: {1}")]
    Http(u16, String),
    #[error("Couldn't connect to the source: {0}")]
    Network(String),
    #[error("The source response doesn't conform to GCSP/1: {0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    /// BCP 47 language most of the source's people use (e.g. "en"), when they have no `locale`.
    #[serde(default)]
    pub default_locale: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttributeSpec {
    pub key: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub pii: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventType {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub protocol: String,
    pub source: SourceInfo,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub attributes: Vec<AttributeSpec>,
    #[serde(default)]
    pub event_types: Vec<EventType>,
}

impl Manifest {
    pub fn can(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol != PROTOCOL {
            return Err(format!("protocol should be \"{PROTOCOL}\" but is \"{}\"", self.protocol));
        }
        let id = &self.source.id;
        if id.is_empty()
            || id.len() > 63
            || !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            || id.starts_with('-')
        {
            return Err("source.id may contain only lowercase letters, digits and -, length 1–63".into());
        }
        if self.source.name.trim().is_empty() {
            return Err("source.name must not be empty".into());
        }
        if !self.can("people") {
            return Err("capabilities must include \"people\"".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Identity {
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Person {
    pub external_id: String,
    pub updated_at: String,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub identities: Vec<Identity>,
    #[serde(default)]
    pub attributes: Map<String, Value>,
    #[serde(default)]
    pub url: Option<String>,
    /// BCP 47 language the person uses (e.g. "en-US", "zh-CN"), if the source knows it.
    #[serde(default)]
    pub locale: Option<String>,
    /// Tombstones only: why the person was removed. `opted_out` means they asked not to
    /// be contacted, so OutreachDex adds their addresses to the do-not-contact list.
    #[serde(default)]
    pub reason: Option<String>,
}

impl Person {
    pub fn emails(&self) -> impl Iterator<Item = &Identity> {
        self.identities.iter().filter(|i| i.kind == "email")
    }
    /// Keeps only attributes the manifest declares (privacy minimization, docs/gcsp.md §3).
    pub fn declared_attributes(&self, manifest: &Manifest) -> Map<String, Value> {
        self.attributes
            .iter()
            .filter(|(k, _)| manifest.attributes.iter().any(|a| &a.key == *k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub external_id: String,
    #[serde(default)]
    pub person_external_id: String,
    #[serde(default)]
    pub occurred_at: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body_markdown: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub attributes: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub items: Vec<Value>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

fn rfc3339(value: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(value).is_ok()
}

fn flat(map: &Map<String, Value>) -> bool {
    map.values().all(|v| !v.is_object() && !v.is_array())
}

/// Parses and validates one person item.
pub fn parse_person(value: &Value) -> Result<Person, String> {
    let person: Person = serde_json::from_value(value.clone()).map_err(|e| format!("Invalid person fields: {e}"))?;
    if person.external_id.trim().is_empty() || person.external_id.len() > 200 {
        return Err("person.external_id must be non-empty and at most 200 characters".into());
    }
    if !rfc3339(&person.updated_at) {
        return Err(format!("updated_at of person {} is not an RFC 3339 time", person.external_id));
    }
    if person.deleted {
        return Ok(person);
    }
    if let Some(identity) = person.identities.iter().find(|i| !IDENTITY_KINDS.contains(&i.kind.as_str()) || i.value.trim().is_empty()) {
        return Err(format!("Identity {}:{} of person {} is invalid", identity.kind, identity.value, person.external_id));
    }
    if !flat(&person.attributes) {
        return Err(format!("attributes of person {} must be flat scalar values", person.external_id));
    }
    Ok(person)
}

/// Parses and validates one event item.
pub fn parse_event(value: &Value) -> Result<Event, String> {
    let event: Event = serde_json::from_value(value.clone()).map_err(|e| format!("Invalid event fields: {e}"))?;
    if event.external_id.trim().is_empty() || event.external_id.len() > 200 {
        return Err("event.external_id must be non-empty and at most 200 characters".into());
    }
    if event.deleted {
        return Ok(event);
    }
    if event.person_external_id.trim().is_empty() {
        return Err(format!("event {} is missing person_external_id", event.external_id));
    }
    if !rfc3339(&event.occurred_at) {
        return Err(format!("occurred_at of event {} is not an RFC 3339 time", event.external_id));
    }
    if event.kind.is_empty()
        || event.kind.len() > 64
        || !event.kind.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.-".contains(c))
    {
        return Err(format!("type of event {} is invalid", event.external_id));
    }
    if event.updated_at.as_deref().is_some_and(|t| !rfc3339(t)) {
        return Err(format!("updated_at of event {} is not an RFC 3339 time", event.external_id));
    }
    if !flat(&event.attributes) {
        return Err(format!("attributes of event {} must be flat scalar values", event.external_id));
    }
    Ok(event)
}

/// A configured GCSP/1 endpoint.
#[derive(Clone)]
pub struct Client {
    base: String,
    header: String,
    token: String,
    http: reqwest::Client,
}

/// `https://` everywhere; plain `http://` only for this machine (local development).
pub fn normalize_base(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(url).map_err(|_| "Enter the full URL, e.g. https://admin.example.com/gcsp".to_string())?;
    let local = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1" | "[::1]"));
    match parsed.scheme() {
        "https" => {}
        "http" if local => {}
        "http" => return Err("To protect the token, the source URL must use https:// (http://127.0.0.1 is allowed for local testing).".into()),
        _ => return Err("The source URL must start with https://.".into()),
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("The source URL can't contain ? or # parts.".into());
    }
    Ok(url.to_owned())
}

impl Client {
    pub fn new(base: &str, header: &str, token: &str) -> Result<Self, String> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let header = if header.trim().is_empty() { "Authorization" } else { header.trim() };
        reqwest::header::HeaderName::from_bytes(header.as_bytes()).map_err(|_| "Invalid header name.".to_string())?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("OutreachDex-GCSP/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { base: normalize_base(base)?, header: header.to_owned(), token: token.to_owned(), http })
    }

    /// The same endpoint without credentials, used to confirm that the source enforces auth.
    pub fn anonymous(&self) -> Self {
        Self { token: String::new(), ..self.clone() }
    }

    fn credential(&self) -> Option<String> {
        if self.token.is_empty() {
            None
        } else if self.header.eq_ignore_ascii_case("authorization") && !self.token.contains(' ') {
            Some(format!("Bearer {}", self.token))
        } else {
            Some(self.token.clone())
        }
    }

    /// One GET with the GCSP error contract. A 429 within 60 seconds is retried once.
    pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, SourceError> {
        for attempt in 0..2 {
            let mut request = self.http.get(format!("{}{path}", self.base)).query(query).header("Accept", "application/json");
            if let Some(value) = self.credential() {
                request = request.header(self.header.as_str(), value);
            }
            let response = request.send().await.map_err(|e| {
                SourceError::Network(if e.is_timeout() { "Request timed out".into() } else { e.without_url().to_string() })
            })?;
            let status = response.status().as_u16();
            let retry = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(5);
            let text = response.text().await.map_err(|e| SourceError::Network(e.to_string()))?;
            let body: Option<Value> = serde_json::from_str(&text).ok();
            let message = || {
                body.as_ref()
                    .and_then(|b| b["error"]["message"].as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| text.chars().take(200).collect())
            };
            match status {
                200..=299 => return body.ok_or_else(|| SourceError::Invalid(format!("{path} did not return JSON"))),
                401 | 403 => return Err(SourceError::Auth(status)),
                400 if body.as_ref().is_some_and(|b| b["error"]["code"] == "invalid_cursor") => {
                    return Err(SourceError::InvalidCursor);
                }
                429 if attempt == 0 && retry <= 60 => tokio::time::sleep(Duration::from_secs(retry)).await,
                429 => return Err(SourceError::RateLimited(retry)),
                _ => return Err(SourceError::Http(status, message())),
            }
        }
        Err(SourceError::RateLimited(60))
    }

    pub async fn manifest(&self) -> Result<Manifest, SourceError> {
        let value = self.get("/manifest", &[]).await?;
        let manifest: Manifest = serde_json::from_value(value).map_err(|e| SourceError::Invalid(format!("manifest: {e}")))?;
        manifest.validate().map_err(SourceError::Invalid)?;
        Ok(manifest)
    }

    pub async fn page(&self, stream: &str, cursor: Option<&str>, limit: u32) -> Result<Page, SourceError> {
        let mut query = vec![("limit", limit.clamp(1, MAX_LIMIT).to_string())];
        if let Some(cursor) = cursor.filter(|c| !c.is_empty()) {
            query.push(("cursor", cursor.to_owned()));
        }
        let value = self.get(&format!("/{stream}"), &query).await?;
        let items = value["items"]
            .as_array()
            .cloned()
            .ok_or_else(|| SourceError::Invalid(format!("/{stream} is missing the items array")))?;
        let has_more = value["has_more"]
            .as_bool()
            .ok_or_else(|| SourceError::Invalid(format!("/{stream} is missing has_more")))?;
        let next_cursor = match &value["next_cursor"] {
            Value::String(s) => Some(s.clone()),
            Value::Null => None,
            _ => return Err(SourceError::Invalid(format!("next_cursor of /{stream} should be a string"))),
        };
        if has_more && next_cursor.is_none() {
            return Err(SourceError::Invalid(format!("/{stream} must return next_cursor when has_more is true")));
        }
        Ok(Page { items, next_cursor, has_more })
    }

    pub async fn person(&self, external_id: &str) -> Result<Person, SourceError> {
        let encoded: String = external_id
            .bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect();
        parse_person(&self.get(&format!("/people/{encoded}"), &[]).await?).map_err(SourceError::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_items() {
        let person = parse_person(&json!({"external_id":"u1","updated_at":"2026-11-01T08:00:00Z","identities":[{"kind":"email","value":"a@b.c","verified":true}],"attributes":{"plan":"pro","x":1}})).unwrap();
        assert_eq!(person.emails().count(), 1);
        assert!(parse_person(&json!({"external_id":"u1","updated_at":"yesterday"})).is_err());
        assert!(parse_person(&json!({"external_id":"u1","updated_at":"2026-11-01T08:00:00Z","attributes":{"x":{"nested":1}}})).is_err());
        assert!(parse_person(&json!({"external_id":"u1","updated_at":"2026-11-01T08:00:00Z","deleted":true})).unwrap().deleted);
        assert!(parse_event(&json!({"external_id":"e1","person_external_id":"u1","occurred_at":"2026-11-01T08:00:00Z","type":"feedback"})).is_ok());
        assert!(parse_event(&json!({"external_id":"e1","person_external_id":"u1","occurred_at":"2026-11-01T08:00:00Z","type":"Bad Type"})).is_err());
        assert!(parse_event(&json!({"external_id":"e1","deleted":true})).is_ok());
    }

    #[test]
    fn keeps_only_declared_attributes() {
        let manifest: Manifest = serde_json::from_value(json!({"protocol":"gcsp/1","source":{"id":"s","name":"S"},"capabilities":["people"],"attributes":[{"key":"plan"}]})).unwrap();
        let person = parse_person(&json!({"external_id":"u1","updated_at":"2026-11-01T08:00:00Z","attributes":{"plan":"pro","secret":1}})).unwrap();
        let kept = person.declared_attributes(&manifest);
        assert!(kept.contains_key("plan") && !kept.contains_key("secret"));
    }

    #[test]
    fn requires_https_except_locally() {
        assert!(normalize_base("https://a.example.com/gcsp/").unwrap().ends_with("/gcsp"));
        assert!(normalize_base("http://127.0.0.1:8787").is_ok());
        assert!(normalize_base("http://a.example.com").is_err());
        assert!(normalize_base("ftp://a.example.com").is_err());
        assert!(normalize_base("https://a.example.com/?x=1").is_err());
    }

    #[test]
    fn manifest_rules() {
        let mut manifest: Manifest = serde_json::from_value(json!({"protocol":"gcsp/1","source":{"id":"thumbnail-snap","name":"T"},"capabilities":["people","events"]})).unwrap();
        assert!(manifest.validate().is_ok());
        manifest.source.id = "Bad_ID".into();
        assert!(manifest.validate().is_err());
    }
}
