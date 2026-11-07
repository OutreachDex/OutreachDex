#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use gc_core::FileRead;
use gc_service::app::App;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::{Emitter, Manager, State};
#[tauri::command]
fn workspace_open(
    path: String,
    create: bool,
    template: Option<String>,
    state: State<'_, App>,
) -> Result<Value, String> {
    state.open_with(Path::new(&path), create, template.as_deref())
}
/// Opens (or on first launch creates) the default workspace, `~/Documents/OutreachDex`.
#[tauri::command]
fn workspace_open_default(state: State<'_, App>) -> Result<Value, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("The home folder isn't known.")?;
    state.open_default(&home)
}
/// Writes text to the system clipboard. The WebView's navigator.clipboard is refused by the
/// platform outside a direct user gesture, so copies go through the host.
#[tauri::command]
fn clipboard_write(text: String) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .map_err(|e| format!("Couldn't write to the clipboard: {e}"))
}
#[tauri::command]
async fn app_call(
    root: String,
    action: String,
    args: Value,
    state: State<'_, App>,
) -> Result<Value, String> {
    state.inner().clone().call(&root, &action, args).await
}
#[tauri::command]
fn file_read(
    root: String,
    path: String,
    offset: Option<usize>,
    length: Option<usize>,
    state: State<'_, App>,
) -> Result<FileRead, String> {
    state
        .service(&root)?
        .docs
        .workspace
        .read(&path, offset, length)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn file_stamp(root: String, path: String, state: State<'_, App>) -> Result<Option<String>, String> {
    state
        .service(&root)?
        .docs
        .workspace
        .stamp(&path)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn file_write(
    root: String,
    path: String,
    bytes: Vec<u8>,
    mode: String,
    expected: Option<String>,
    state: State<'_, App>,
) -> Result<String, String> {
    let result = state
        .service(&root)?
        .docs
        .workspace
        .write(&path, &bytes, &mode, expected.as_deref())
        .map_err(|e| e.to_string())?;
    (state.notify)("workspace-changed", json!({"root":root,"path":path}));
    Ok(result)
}
#[tauri::command]
async fn document_register(
    root: String,
    path: String,
    session: String,
    state: State<'_, App>,
) -> Result<(), String> {
    state.service(&root)?.docs.register(&path, &session).await
}
#[tauri::command]
fn document_unregister(
    root: String,
    path: String,
    session: String,
    state: State<'_, App>,
) -> Result<(), String> {
    state.service(&root)?.docs.unregister(&path, &session);
    Ok(())
}
#[tauri::command]
fn document_reply(
    root: String,
    id: String,
    session: String,
    result: Value,
    error: Option<String>,
    state: State<'_, App>,
) -> Result<(), String> {
    state.service(&root)?.docs.reply(
        &id,
        &session,
        match error {
            Some(e) => Err(e),
            None => Ok(result),
        },
    )
}
#[tauri::command]
async fn generate(root: String, request: Value, state: State<'_, App>) -> Result<Value, String> {
    state.inner().clone().generate(&root, request).await
}
#[tauri::command]
fn resolve_link(
    root: String,
    source: String,
    target: String,
    state: State<'_, App>,
) -> Result<String, String> {
    let service = state.service(&root)?;
    if Path::new(&target).is_absolute() || target.contains('\\') {
        return Err("Links must point inside the workspace.".into());
    }
    let mut parts: Vec<String> = source.split('/').map(str::to_owned).collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop().ok_or("Link points outside the workspace")?;
            }
            _ => parts.push(part.into()),
        }
    }
    let mut path = parts.join("/");
    if Path::new(&path).extension().is_none() {
        path.push_str(".md");
    }
    let file = service
        .docs
        .workspace
        .file(&path)
        .map_err(|e| e.to_string())?;
    if !file.is_file() {
        return Err("The linked file doesn't exist.".into());
    }
    Ok(path)
}
#[tauri::command]
fn open_external(
    root: String,
    url: Option<String>,
    path: Option<String>,
    state: State<'_, App>,
) -> Result<(), String> {
    let target = if let Some(url) = url {
        if !["https://", "http://", "mailto:"]
            .iter()
            .any(|prefix| url.starts_with(prefix))
        {
            return Err("This link isn't supported.".into());
        }
        url
    } else {
        let path = path.ok_or("Missing path")?;
        let file = state
            .service(&root)?
            .docs
            .workspace
            .file(&path)
            .map_err(|e| e.to_string())?;
        if !matches!(
            file.extension().and_then(|e| e.to_str()),
            Some("pdf" | "png" | "jpg" | "jpeg" | "txt" | "csv" | "webp" | "gif")
        ) {
            return Err("Open this attachment from the workspace folder.".into());
        }
        file.to_string_lossy().into_owned()
    };
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("explorer");
        c
    };
    command.arg(target).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
fn resources() -> PathBuf {
    std::env::var_os("GC_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("resources"))
}
async fn cli(args: &[String]) -> Result<(), String> {
    if args[1] == "source" && args.get(2).is_some_and(|a| a == "check") {
        // globalcontact source check <base-url> [--token T] [--header H]
        let flag = |name: &str| args.windows(2).find(|a| a[0] == name).map(|a| a[1].clone()).unwrap_or_default();
        let url = args.get(3).filter(|a| !a.starts_with("--")).ok_or("Usage: globalcontact source check <base-url> [--token T] [--header H]")?;
        let client = gc_source::Client::new(url, &flag("--header"), &flag("--token"))?;
        let report = gc_source::check::run(&client).await;
        for f in &report.findings {
            let mark = match f.level {
                gc_source::check::Level::Ok => "✓",
                gc_source::check::Level::Warn => "!",
                gc_source::check::Level::Fail => "✗",
            };
            println!("{mark} {}: {}", f.name, f.detail);
        }
        return if report.passed() { Ok(()) } else { Err("Check failed".into()) };
    }
    let workspace = args
        .windows(2)
        .find(|a| a[0] == "--workspace")
        .map(|a| a[1].clone())
        .ok_or("--workspace <directory> is required")?;
    if args[1] == "mcp" {
        return gc_mcp::serve(&workspace).await;
    }
    if args[1] == "import" {
        // globalcontact import <file.csv> --workspace <dir> [--template <id>]
        let file = args.get(2).filter(|a| !a.starts_with("--")).ok_or("CSV / TSV file required")?;
        let template = args.windows(2).find(|a| a[0] == "--template").map(|a| a[1].as_str());
        let data = tempfile::tempdir().map_err(|e| e.to_string())?;
        let app = App::new(
            data.path(),
            resources(),
            std::env::current_exe().map_err(|e| e.to_string())?,
            Arc::new(|_, _| {}),
        )?;
        let info = app.open_with(
            Path::new(&workspace),
            !Path::new(&workspace).join("contacts.ktable").exists(),
            template,
        )?;
        let root = info["root"].as_str().ok_or("root")?;
        let preview = app.call(root, "import.preview", json!({"path": file})).await?;
        let output = app
            .call(root, "import.run", json!({"path": file, "mapping": preview["mapping"]}))
            .await?;
        println!("{output}");
        return Ok(());
    }
    Err("Unknown command".into())
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|a| a == "mcp" || a == "import" || a == "source")
    {
        let result = tokio::runtime::Runtime::new()
            .map_err(|e| e.to_string())
            .and_then(|rt| rt.block_on(cli(&args)));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            let notify = Arc::new(move |event: &str, value: Value| {
                let _ = handle.emit(event, value);
            });
            let bundled = app.path().resource_dir()?.join("resources");
            let resource = if bundled.join("kits").is_dir() {
                bundled
            } else {
                resources()
            };
            let service = App::new(
                &app.path().app_data_dir()?,
                resource,
                std::env::current_exe()?,
                notify,
            )
            .map_err(std::io::Error::other)?;
            if let Some(path) = args
                .windows(2)
                .find(|a| a[0] == "--workspace")
                .map(|a| &a[1])
            {
                service
                    .open(Path::new(path), false)
                    .map_err(std::io::Error::other)?;
            }
            let background = service.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = background.start_bridge().await {
                    (background.notify)("background-error", json!({"message":error}));
                }
                let mut ticks = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    ticks.tick().await;
                    background.scheduler().await;
                }
            });
            app.manage(service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            resolve_link,
            open_external,
            workspace_open,
            workspace_open_default,
            clipboard_write,
            app_call,
            file_read,
            file_stamp,
            file_write,
            document_register,
            document_unregister,
            document_reply,
            generate
        ])
        .build(tauri::generate_context!())
        .expect("failed to build OutreachDex")
        .run(|handle, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                handle.state::<App>().terminals.shutdown();
            }
        });
}
