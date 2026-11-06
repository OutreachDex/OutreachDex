use gc_service::app::App;
use portable_pty::{CommandBuilder, PtySize};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};

fn setup(dir: &Path) -> (App, String) {
    let app = App::new(
        &dir.join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    let root = app.open(&dir.join("work"), true).unwrap()["root"]
        .as_str()
        .unwrap()
        .to_owned();
    (app, root)
}
async fn wait_output(app: &App, root: &str, id: &str, needle: &str) -> Value {
    for _ in 0..100 {
        let output = app
            .call(root, "terminal.read", json!({"id":id}))
            .await
            .unwrap();
        let bytes: Vec<u8> = output["chunks"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|c| serde_json::from_value::<Vec<u8>>(c["bytes"].clone()).unwrap())
            .collect();
        if String::from_utf8_lossy(&bytes).contains(needle) {
            return output;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    panic!("Missing PTY output: {needle}");
}

#[tokio::test]
async fn interactive_terminal_scopes_io_preserves_bytes_and_replays_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (app, root) = setup(dir.path());
    let other = app.open(&dir.path().join("other"), true).unwrap()["root"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args([
        "-c",
        "printf 'READY\\n'; read -r line; printf 'RESULT:%s\\n' \"$line\"; read -r line",
    ]);
    cmd.cwd(&root);
    let id = app
        .terminals
        .spawn(
            app.db.clone(),
            &root,
            "fixture",
            uuid::Uuid::new_v4().to_string(),
            cmd,
            PtySize {
                cols: 80,
                rows: 24,
                pixel_width: 0,
                pixel_height: 0,
            },
        )
        .unwrap();
    wait_output(&app, &root, &id, "READY").await;
    app.call(
        &root,
        "terminal.resize",
        json!({"id":id,"cols":95,"rows":32}),
    )
    .await
    .unwrap();
    for action in [
        "terminal.read",
        "terminal.write",
        "terminal.stop",
        "terminal.resize",
    ] {
        assert!(
            app.call(&other, action, json!({"id":id,"text":"wrong\n"}))
                .await
                .is_err(),
            "{action} crossed workspaces"
        );
    }
    app.call(&root, "terminal.write", json!({"id":id,"text":"中文 🐋\n"}))
        .await
        .unwrap();
    let output = wait_output(&app, &root, &id, "RESULT:中文 🐋").await;
    let last = output["chunks"].as_array().unwrap().last().unwrap()["seq"]
        .as_i64()
        .unwrap();
    assert!(
        app.call(&root, "terminal.read", json!({"id":id,"after":last}))
            .await
            .unwrap()["chunks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    app.call(&root, "terminal.stop", json!({"id":id}))
        .await
        .unwrap();
    for _ in 0..100 {
        if app.call(&root, "terminal.list", json!({})).await.unwrap()[0]["status"] != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert_ne!(
        app.call(&root, "terminal.list", json!({})).await.unwrap()[0]["status"],
        "running"
    );
    drop(app);
    let reopened = App::new(
        &dir.path().join("data"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources"),
        std::env::current_exe().unwrap(),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    reopened.open(Path::new(&root), false).unwrap();
    wait_output(&reopened, &root, &id, "RESULT:中文 🐋").await;
}

#[tokio::test]
async fn document_tools_reject_stale_text_and_protect_dossier_and_draft_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let (app, root) = setup(dir.path());
    let tool = |name: &str, args: Value| json!({"workspace":root,"method":"tools/call","params":{"name":name,"arguments":args}});
    app.mcp(tool(
        "document.create",
        json!({"path":"notes/feedback.md","text":"# User feedback\nImport failed."}),
    ))
    .await
    .unwrap();
    app.mcp(tool(
        "document.replace",
        json!({"path":"notes/feedback.md","expected":"Import failed.","text":"Waiting to confirm the import format."}),
    ))
    .await
    .unwrap();
    assert!(
        app.mcp(tool(
            "document.replace",
            json!({"path":"notes/feedback.md","expected":"Import failed.","text":"Old result"})
        ))
        .await
        .is_err()
    );
    for path in [
        "people/a.md",
        "drafts/a.md",
        "mail/a.md",
        "notes/../campaigns/a.md",
        "notes/.secret.md",
    ] {
        assert!(
            app.mcp(tool(
                "document.create",
                json!({"path":path,"text":"forbidden"})
            ))
            .await
            .is_err()
        );
    }
    let read = app
        .mcp(tool("document.read", json!({"path":"notes/feedback.md"})))
        .await
        .unwrap();
    assert!(
        read["structuredContent"]["text"]
            .as_str()
            .unwrap()
            .contains("Waiting to confirm the import format.")
    );
    assert_eq!(
        app.call(&root, "documents.list", json!({})).await.unwrap(),
        json!(["notes/feedback.md"])
    );
}

#[tokio::test]
async fn current_selection_is_live_workspace_scoped_and_read_only_for_agents() {
    let dir = tempfile::tempdir().unwrap();
    let (app, root) = setup(dir.path());
    let other = app.open(&dir.path().join("other"), true).unwrap()["root"]
        .as_str()
        .unwrap()
        .to_owned();
    let a = app
        .call(&root, "contact.create", json!({"Name":"Person A"}))
        .await
        .unwrap()["slug"]
        .clone();
    let b = app
        .call(&root, "contact.create", json!({"Name":"Person B"}))
        .await
        .unwrap()["slug"]
        .clone();
    let tool = |workspace: &str, name: &str, arguments: Value| json!({"workspace":workspace,"method":"tools/call","params":{"name":name,"arguments":arguments}});
    for (slug, name) in [(a.clone(), "Person A"), (b, "Person B"), (a, "Person A")] {
        app.call(
            &root,
            "context.set",
            json!({"contact":slug,"document":format!("people/{}.md",slug.as_str().unwrap())}),
        )
        .await
        .unwrap();
        let context = app
            .mcp(tool(&root, "context.get", json!({})))
            .await
            .unwrap()["structuredContent"]
            .clone();
        assert_eq!(context["contact"], slug);
        assert_eq!(context["name"], name);
        assert_eq!(context["workspace"], root);
        assert!(
            app.mcp(tool(&other, "context.get", json!({})))
                .await
                .unwrap()["structuredContent"]["contact"]
                .is_null()
        );
    }
    assert!(
        app.mcp(tool(&root, "context.set", json!({"contact":null})))
            .await
            .is_err()
    );
    assert!(
        app.call(&root, "context.set", json!({"document":"../secret.md"}))
            .await
            .is_err()
    );
    app.call(&root, "context.set", json!({})).await.unwrap();
    assert!(
        app.mcp(tool(&root, "context.get", json!({})))
            .await
            .unwrap()["structuredContent"]["contact"]
            .is_null()
    );
}
