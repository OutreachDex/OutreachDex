//! First launch opens a workspace without asking: `~/Documents/OutreachDex`, created on demand
//! and reopened afterwards. A same-named folder that isn't a workspace is never touched.
use gc_service::app::App;
use std::{path::Path, sync::Arc};

fn app(data: &Path) -> App {
    App::new(data, Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"), std::env::current_exe().unwrap(), Arc::new(|_, _| {})).unwrap()
}

#[test]
fn default_workspace_is_created_once_and_reused() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let documents = home.join("Documents");
    std::fs::create_dir_all(documents.join("OutreachDex")).unwrap();
    std::fs::write(documents.join("OutreachDex/.DS_Store"), b"").unwrap();

    let first = app(&dir.path().join("data1"));
    let root = first.open_default(&home).unwrap()["root"].as_str().unwrap().to_owned();
    assert!(root.ends_with("Documents/OutreachDex"), "an empty folder (hidden files only) is used: {root}");
    assert!(Path::new(&root).join("contacts.ktable").is_file());
    assert!(Path::new(&root).join("globalcontact.toml").is_file());
    drop(first); // releases the workspace lock, like quitting the app

    let again = app(&dir.path().join("data2"));
    assert_eq!(again.open_default(&home).unwrap()["root"], root.as_str(), "the existing workspace is reopened");
    drop(again);

    // Someone's own "OutreachDex" folder with files in it stays as it was.
    let other = dir.path().join("home2");
    std::fs::create_dir_all(other.join("Documents/OutreachDex")).unwrap();
    std::fs::write(other.join("Documents/OutreachDex/notes.txt"), b"mine").unwrap();
    let third = app(&dir.path().join("data3"));
    let root = third.open_default(&other).unwrap()["root"].as_str().unwrap().to_owned();
    assert!(root.ends_with("Documents/OutreachDex 2"), "{root}");
    assert_eq!(std::fs::read_dir(other.join("Documents/OutreachDex")).unwrap().count(), 1, "nothing was added to the user's folder");
}
