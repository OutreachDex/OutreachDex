use gc_core::{Error, Workspace};
use note_sdk::{KitDocument as NoteKitDocument, NoteDocument};
use table_sdk::TableDocument;
use serde_json::json;

#[test]
fn schema_and_expert_survive_native_save_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = Workspace::create(dir.path()).unwrap();
    let file = workspace.read("contacts.ktable", None, None).unwrap();
    let mut doc = TableDocument::open("contacts.ktable", &file.bytes).unwrap();
    let result = doc.execute("table.tables.list", json!({})).unwrap();
    let tables = result["result"]["items"].as_array().unwrap();
    assert_eq!(tables.len(), 4);
    // The blank template adds no fields beyond the core ones.
    for (title, count) in [("contacts", 21), ("channels", 5), ("identities", 11), ("interactions", 8)] {
        let table = tables.iter().find(|t| t["title"] == title).unwrap();
        assert_eq!(table["fields"].as_array().unwrap().len(), count);
    }
    assert!(dir.path().join("globalcontact.toml").is_file());
    assert!(dir.path().join("playbooks/understand.md").is_file());
    let contacts = tables.iter().find(|t| t["title"] == "contacts").unwrap();
    let table_id = contacts["id"].clone();
    let fields = contacts["fields"].as_array().unwrap();
    let name = fields.iter().find(|f| f["title"] == "Name").unwrap()["name"]
        .as_str()
        .unwrap();
    let summary = fields.iter().find(|f| f["title"] == "AI summary").unwrap();
    assert_eq!(summary["options"]["ai_config"]["type"], "text_generation");
    assert!(
        !doc.can_undo(),
        "schema initialization is not a person's edit"
    );
    doc.execute(
        "table.records.create",
        json!({"tableId": table_id, "values": {name: "Elena Park"}}),
    )
    .unwrap();
    let bytes = doc.save(None).unwrap();
    workspace
        .write("contacts.ktable", &bytes, "save", Some(&file.version))
        .unwrap();
    let reopened = Workspace::open(dir.path()).unwrap();
    let bytes = reopened.read("contacts.ktable", None, None).unwrap().bytes;
    let mut doc = TableDocument::open("contacts.ktable", &bytes).unwrap();
    let rows = doc
        .execute("table.records.list", json!({"tableId": table_id}))
        .unwrap();
    assert!(rows.to_string().contains("Elena Park"));
    assert!(Workspace::create(dir.path()).is_err());
    assert_eq!(
        reopened.read("contacts.ktable", None, None).unwrap().bytes,
        bytes
    );
}

#[test]
fn file_versions_ranges_and_scope_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = Workspace::create(dir.path()).unwrap();
    let file = workspace.read("contacts.ktable", None, None).unwrap();
    let range = workspace.read("contacts.ktable", Some(2), Some(5)).unwrap();
    assert_eq!(range.bytes, file.bytes[2..7]);
    assert_eq!(range.size, file.size);
    assert_eq!(range.version, file.version);
    assert!(matches!(
        workspace.write("contacts.ktable", &file.bytes, "save", Some("stale")),
        Err(Error::Conflict)
    ));
    assert!(
        workspace
            .write("contacts.ktable", b"broken", "save", Some(&file.version))
            .is_err()
    );
    assert!(
        workspace
            .write(
                "contacts.ktable",
                &file.bytes,
                "export",
                Some(&file.version)
            )
            .is_err()
    );
    assert!(workspace.read("../contacts.ktable", None, None).is_err());
    assert_eq!(
        workspace.read("contacts.ktable", None, None).unwrap().bytes,
        file.bytes
    );
    #[cfg(unix)]
    {
        let other = tempfile::tempdir().unwrap();
        let external = other.path().join("external.ktable");
        std::fs::write(&external, &file.bytes).unwrap();
        std::fs::remove_file(dir.path().join("contacts.ktable")).unwrap();
        std::os::unix::fs::symlink(external, dir.path().join("contacts.ktable")).unwrap();
        assert!(workspace.read("contacts.ktable", None, None).is_err());
        assert!(
            workspace
                .write("contacts.ktable", &file.bytes, "save", Some(&file.version))
                .is_err()
        );
    }
}

#[test]
fn note_sdk_native_roundtrip_preserves_the_file() {
    let bytes = b"---\r\ncontact_id: elena-park\r\n---\r\n\r\n# Elena Park\r\n";
    let doc = NoteDocument::open("elena-park.md", bytes).unwrap();
    assert_eq!(doc.save(None).unwrap(), bytes);
}

#[test]
fn templates_add_configured_fields_and_stages() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = Workspace::create_with(dir.path(), "user-relationships").unwrap();
    let file = workspace.read("contacts.ktable", None, None).unwrap();
    let mut doc = TableDocument::open("contacts.ktable", &file.bytes).unwrap();
    let result = doc.execute("table.tables.list", json!({})).unwrap();
    let contacts = result["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["title"] == "contacts")
        .unwrap()
        .clone();
    let fields = contacts["fields"].as_array().unwrap();
    assert!(fields.iter().any(|f| f["title"] == "Plan"));
    assert!(fields.iter().any(|f| f["title"] == "Open issue"));
    let stage = fields.iter().find(|f| f["title"] == "Stage").unwrap();
    assert!(stage.to_string().contains("New user"));
    let config = gc_core::config::WorkspaceConfig::load(&workspace).unwrap();
    assert_eq!(config.initial_stage(), "New user");
    let playbooks = gc_core::config::playbooks(&workspace).unwrap();
    assert!(playbooks.iter().any(|p| p.id == "product-users" && p.kind == "context"));
    assert!(Workspace::create_with(tempfile::tempdir().unwrap().path(), "nope").is_err());
}

#[test]
fn legacy_workspace_gets_a_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = Workspace::create(dir.path()).unwrap();
    std::fs::remove_file(dir.path().join("globalcontact.toml")).unwrap();
    std::fs::remove_dir_all(dir.path().join("playbooks")).unwrap();
    let config = gc_core::config::WorkspaceConfig::load(&workspace).unwrap();
    // Pre-configuration workspaces were created with Chinese stage labels; the generated
    // configuration keeps them so it matches the existing table.
    assert_eq!(config.initial_stage(), "待调研");
    assert!(config.is_inactive("不再联系"));
    let text = std::fs::read_to_string(dir.path().join("globalcontact.toml")).unwrap();
    assert!(text.contains("template = \"legacy\"") && !text.contains("[[fields]]"));
    assert!(dir.path().join("playbooks/question.md").is_file());
}
