//! Generic contact import from CSV / TSV with a column → field mapping.
use crate::Service;
use serde_json::{Map, Value, json};
use std::path::Path;

const MAX_BYTES: u64 = 20 * 1024 * 1024;
/// Fields an import may set. Slug, AI summary and contact timestamps stay host-managed.
const CORE_FIELDS: &[&str] = &[
    "Name",
    "Native name",
    "Affiliation",
    "Campaign",
    "Stage",
    "Language",
    "Email",
    "Homepage",
    "X",
    "Instagram",
    "Hook",
    "Next action",
    "Notes",
];
/// Fields that are also contact channels.
const CHANNELS: &[(&str, &str)] = &[
    ("Email", "email"),
    ("Homepage", "homepage"),
    ("X", "x"),
    ("Instagram", "instagram"),
    ("WhatsApp", "whatsapp"),
    ("TikTok", "tiktok"),
    ("Facebook", "facebook"),
    ("LinkedIn", "linkedin"),
];
/// Import targets that only become channels: the contacts table has no column for them.
const CHANNEL_ONLY: &[&str] = &["WhatsApp", "TikTok", "Facebook", "LinkedIn"];

/// Column headers recognized for each field, compared case-insensitively. Chinese headers are
/// recognized too, since many spreadsheets people import are in Chinese.
fn synonyms(field: &str) -> &'static [&'static str] {
    match field {
        "Name" => &["name", "full name", "contact", "contact name", "姓名", "名字", "联系人"],
        "Native name" => &["native name", "local name", "original name", "chinese name", "中文名", "本名"],
        "Affiliation" => &["affiliation", "company", "organization", "organisation", "institution", "employer", "单位", "公司", "机构", "身份"],
        "Campaign" => &["campaign", "project", "项目", "活动"],
        "Stage" => &["stage", "status", "阶段", "状态"],
        "Language" => &["language", "lang", "语言"],
        "Email" => &["email", "e-mail", "email address", "mail", "邮箱", "电子邮件", "邮件"],
        "Homepage" => &["homepage", "website", "url", "site", "web", "主页", "网站", "个人主页"],
        "X" => &["x", "twitter", "x / twitter"],
        "Instagram" => &["instagram", "ig"],
        "WhatsApp" => &["whatsapp", "whats app"],
        "TikTok" => &["tiktok", "tik tok"],
        "Facebook" => &["facebook", "fb", "messenger", "facebook messenger", "facebook / messenger"],
        "LinkedIn" => &["linkedin", "linked in", "领英"],
        "Hook" => &["hook", "why", "reason", "why contact", "切入点", "为什么找他", "为什么联系"],
        "Next action" => &["next action", "next step", "下一步"],
        "Notes" => &["notes", "note", "remark", "remarks", "comments", "备注", "说明"],
        _ => &[],
    }
}

struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

fn read_table(path: &Path) -> Result<Table, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
    if meta.len() > MAX_BYTES {
        return Err("The file is over 20 MB. Split it before importing.".into());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
    let text = String::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes).to_vec())
        .map_err(|_| "The file must be a UTF-8 CSV or TSV (in Excel, save as \"CSV UTF-8\").".to_string())?;
    let first = text.lines().next().unwrap_or("");
    let delimiter = [b'\t', b',', b';']
        .into_iter()
        .max_by_key(|d| first.matches(*d as char).count())
        .unwrap_or(b',');
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| format!("Couldn't read the header row: {e}"))?
        .iter()
        .map(|h| h.trim().to_owned())
        .collect();
    if headers.iter().all(String::is_empty) {
        return Err("The first row of the file must contain column names.".into());
    }
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| format!("Row {} is malformed: {e}", rows.len() + 2))?;
        let row: Vec<String> = record.iter().map(|c| c.trim().to_owned()).collect();
        if row.iter().any(|c| !c.is_empty()) {
            rows.push(row);
        }
    }
    Ok(Table { headers, rows })
}

impl Service {
    fn import_fields(&self) -> Result<Vec<String>, String> {
        let config = self.workspace_config()?;
        let mut fields: Vec<String> = CORE_FIELDS.iter().map(|s| (*s).to_owned()).collect();
        fields.extend(config.fields.iter().map(|f| f.title.clone()));
        // A custom field with the same name (e.g. "LinkedIn") stays a contact field.
        for channel in CHANNEL_ONLY {
            if !fields.iter().any(|f| f.eq_ignore_ascii_case(channel)) {
                fields.push((*channel).to_owned());
            }
        }
        Ok(fields)
    }

    /// Headers, a sample of rows and a suggested mapping (`column index → field`).
    pub fn import_preview(&self, path: &Path) -> Result<Value, String> {
        let table = read_table(path)?;
        let fields = self.import_fields()?;
        let mut used = std::collections::HashSet::new();
        let mapping: Vec<Value> = table
            .headers
            .iter()
            .map(|header| {
                let key = header.to_lowercase();
                let found = fields.iter().find(|f| {
                    !used.contains(f.as_str())
                        && (f.to_lowercase() == key || synonyms(f).contains(&key.as_str()))
                });
                match found {
                    Some(f) => {
                        used.insert(f.as_str());
                        json!(f)
                    }
                    None => Value::Null,
                }
            })
            .collect();
        Ok(json!({
            "file": path.file_name().map(|n| n.to_string_lossy().into_owned()),
            "headers": table.headers,
            "sample": table.rows.iter().take(5).collect::<Vec<_>>(),
            "total": table.rows.len(),
            "fields": fields,
            "mapping": mapping,
        }))
    }

    /// Imports rows. Existing contacts match by email, then by exact name, and get the mapped
    /// non-empty values; others are created. Email, homepage, X, Instagram, WhatsApp, TikTok,
    /// Facebook and LinkedIn become channels.
    pub async fn import_csv(&self, path: &Path, mapping: &[Option<String>]) -> Result<Value, String> {
        let table = read_table(path)?;
        let fields = self.import_fields()?;
        let config = self.workspace_config()?;
        if mapping.len() != table.headers.len() {
            return Err("The column mapping doesn't match the file's columns. Preview again.".into());
        }
        for field in mapping.iter().flatten() {
            if !fields.contains(field) {
                return Err(format!("Can't import into field {field}."));
            }
        }
        if mapping.iter().flatten().filter(|f| *f == "Name").count() != 1 {
            return Err("Map one column to “Name”.".into());
        }
        let source = format!(
            "Import: {}",
            path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        );
        let (mut created, mut updated, mut ignored) = (0, 0, Vec::<String>::new());
        // "Chinese" / "English" (or the legacy Chinese label) in the Language column, stored
        // the way this workspace's Language choices name them.
        let chinese = self.language_value("Chinese").await;
        let mut existing = self.rows("contacts").await?;
        for (index, row) in table.rows.iter().enumerate() {
            let mut values = Map::new();
            for (column, field) in mapping.iter().enumerate() {
                let (Some(field), Some(cell)) = (field, row.get(column)) else { continue };
                if cell.is_empty() {
                    continue;
                }
                if field == "Stage" && config.role(cell).is_none() {
                    ignored.push(format!("Row {}: stage “{cell}” isn't in the workspace configuration and was ignored", index + 2));
                    continue;
                }
                let cell = match (field.as_str(), gc_core::schema::language(cell)) {
                    ("Language", Some("Chinese")) => chinese.clone(),
                    ("Language", Some(language)) => language.to_owned(),
                    _ => cell.clone(),
                };
                values.insert(field.clone(), json!(cell));
            }
            // Channel-only columns are not contact fields: take them out before writing the row.
            let custom = |f: &str| config.fields.iter().any(|c| c.title.eq_ignore_ascii_case(f));
            let mut channel_values = Map::new();
            for field in CHANNEL_ONLY.iter().filter(|f| !custom(f)) {
                if let Some(value) = values.remove(*field) {
                    channel_values.insert((*field).to_owned(), value);
                }
            }
            let name = values.get("Name").and_then(Value::as_str).unwrap_or("").to_owned();
            if name.is_empty() {
                ignored.push(format!("Row {} has no name and was skipped", index + 2));
                continue;
            }
            let email = values.get("Email").and_then(Value::as_str).unwrap_or("").to_lowercase();
            let matched = existing
                .iter()
                .find(|r| !email.is_empty() && r.text("Email").to_lowercase() == email)
                .or_else(|| existing.iter().find(|r| r.text("Name") == name));
            let slug = match matched {
                Some(row) => {
                    let slug = row.text("Slug").to_owned();
                    self.update_contact(&slug, values.clone(), false).await?;
                    updated += 1;
                    slug
                }
                None => {
                    let slug = self.create_contact(values.clone()).await?;
                    created += 1;
                    existing = self.rows("contacts").await?;
                    slug
                }
            };
            for (field, kind) in CHANNELS {
                if let Some(value) = values.get(*field).or_else(|| channel_values.get(*field)).and_then(Value::as_str) {
                    self.add_channel(&slug, kind, value, &source, false).await?;
                }
            }
        }
        Ok(json!({"created": created, "updated": updated, "ignored": ignored, "total": table.rows.len()}))
    }
}
