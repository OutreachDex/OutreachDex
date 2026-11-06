//! Pure parsers for table command results.
use serde_json::{Map, Value};
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInfo {
    pub id: String,
    /// The engine's key for the field's values.
    pub name: String,
    /// What people call it: its title, or its name without one.
    pub title: String,
    pub field_type: String,
    pub primary: bool,
    /// The field as the kit answered it.
    pub raw: Value,
}

/// A table in a file.
#[derive(Debug, Clone, PartialEq)]
pub struct TableInfo {
    pub id: String,
    pub name: String,
    pub title: String,
    pub fields: Vec<FieldInfo>,
    /// The table's own metadata, which host features may keep things in (form drafts).
    pub meta: Value,
}

impl TableInfo {
    /// Values keyed by field id, name, or title, rekeyed by the field's name (the engine's key);
    /// keys no field has are left out.
    pub fn values_by_name(&self, values: &Map<String, Value>) -> Map<String, Value> {
        let mut out = Map::new();
        for (key, value) in values {
            let field = self
                .fields
                .iter()
                .find(|field| field.id == *key)
                .or_else(|| self.fields.iter().find(|field| field.name == *key))
                .or_else(|| self.fields.iter().find(|field| field.title == *key));
            if let Some(field) = field {
                out.insert(field.name.clone(), value.clone());
            }
        }
        out
    }
}

/// A record's id, values (keyed by field name), and creation time.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordInfo {
    pub id: String,
    pub values: Map<String, Value>,
    pub created_at: i64,
}
pub fn field_info(field: &Value) -> FieldInfo {
    let name = text(field, "name");
    FieldInfo {
        id: id_text(&field["id"]),
        title: display(text(field, "title"), name.clone()),
        name,
        field_type: text(field, "type"),
        primary: field["is_primary"] == true,
        raw: field.clone(),
    }
}

pub fn table_info(table: &Value) -> TableInfo {
    let name = text(table, "name");
    TableInfo {
        id: id_text(&table["id"]),
        title: display(text(table, "title"), name.clone()),
        name,
        fields: table["fields"]
            .as_array()
            .map(|fields| fields.iter().map(field_info).collect())
            .unwrap_or_default(),
        meta: table.get("meta").cloned().unwrap_or(Value::Null),
    }
}

pub fn record_info(record: &Value) -> RecordInfo {
    RecordInfo {
        id: id_text(&record["id"]),
        values: record["values"].as_object().cloned().unwrap_or_default(),
        created_at: record["created_at"].as_i64().unwrap_or_default(),
    }
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}
fn id_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
fn display(title: String, name: String) -> String {
    if title.is_empty() { name } else { title }
}
