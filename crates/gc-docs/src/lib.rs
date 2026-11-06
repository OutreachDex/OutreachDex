//! Document ownership and transport only. Commands and results belong to the kit.
pub mod parse;
use gc_core::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Mutex as AsyncMutex, oneshot};

pub type Notify = Arc<dyn Fn(&str, Value) + Send + Sync>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step {
    pub command: String,
    pub params: Value,
}
#[derive(Default)]
struct Registry {
    open: HashMap<String, String>,
    pending: HashMap<String, (String, oneshot::Sender<Result<Value, String>>)>,
    locks: HashMap<String, Arc<AsyncMutex<()>>>,
}
/// Opened tables by path, with the file version they were opened from. Decoding a table is the
/// expensive part of every native command, and most commands are reads.
type TableCache = Arc<Mutex<HashMap<String, (String, table_sdk::TableDocument)>>>;

#[derive(Clone)]
pub struct Docs {
    pub workspace: Workspace,
    registry: Arc<Mutex<Registry>>,
    notify: Notify,
    tables: TableCache,
}
impl Docs {
    pub fn new(workspace: Workspace, notify: Notify) -> Self {
        Self {
            workspace,
            registry: Arc::default(),
            notify,
            tables: Arc::default(),
        }
    }
    fn key(&self, path: &str) -> String {
        format!("{}:{path}", self.workspace.root().display())
    }
    pub async fn register(&self, path: &str, session: &str) -> Result<(), String> {
        self.workspace.file(path).map_err(|e| e.to_string())?;
        let lock = self.lock(path);
        let _guard = lock.lock().await;
        let mut registry = self.registry.lock().map_err(|e| e.to_string())?;
        if registry.open.contains_key(path) {
            return Err("The document is already open in an editor.".into());
        }
        registry.open.insert(path.into(), session.into());
        Ok(())
    }
    pub fn unregister(&self, path: &str, session: &str) {
        let mut registry = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        if registry.open.get(path).is_some_and(|s| s == session) {
            registry.open.remove(path);
        }
        let ids: Vec<_> = registry
            .pending
            .iter()
            .filter(|(_, (s, _))| s == session)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some((_, sender)) = registry.pending.remove(&id) {
                let _ = sender.send(Err("The editor was closed. Try again.".into()));
            }
        }
    }
    pub fn reply(
        &self,
        id: &str,
        session: &str,
        result: Result<Value, String>,
    ) -> Result<(), String> {
        let mut registry = self.registry.lock().map_err(|e| e.to_string())?;
        if registry.pending.get(id).is_some_and(|(s, _)| s == session) {
            if let Some((_, sender)) = registry.pending.remove(id) {
                let _ = sender.send(result);
            }
            Ok(())
        } else {
            Err("The document request has expired.".into())
        }
    }
    pub fn is_open(&self, path: &str) -> bool {
        self.registry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .open
            .contains_key(path)
    }
    pub fn lock(&self, path: &str) -> Arc<AsyncMutex<()>> {
        self.registry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .locks
            .entry(self.key(path))
            .or_default()
            .clone()
    }
    pub async fn execute(
        &self,
        path: &str,
        command: &str,
        params: Value,
        origin: &str,
    ) -> Result<Value, String> {
        let mut values = self
            .run(
                path,
                vec![Step {
                    command: command.into(),
                    params,
                }],
                origin,
            )
            .await?;
        values.pop().ok_or("kit did not return a result".into())
    }
    pub async fn run(
        &self,
        path: &str,
        mut steps: Vec<Step>,
        origin: &str,
    ) -> Result<Vec<Value>, String> {
        self.workspace.file(path).map_err(|e| e.to_string())?;
        let lock = self.lock(path);
        let _guard = lock.lock().await;
        if path.ends_with(".ktable") && origin.starts_with("system:") {
            // Only commands whose published schema takes `asOrigin` (the mutating ones);
            // The table refuses parameters a command does not declare.
            for step in &mut steps {
                if takes_as_origin(&step.command) {
                    step.params[table_sdk::AS_ORIGIN_PARAM] = json!(origin);
                }
            }
        }
        let session = self
            .registry
            .lock()
            .map_err(|e| e.to_string())?
            .open
            .get(path)
            .cloned();
        if let Some(session) = session {
            let id = uuid::Uuid::new_v4().to_string();
            let (sender, receiver) = oneshot::channel();
            self.registry
                .lock()
                .map_err(|e| e.to_string())?
                .pending
                .insert(id.clone(), (session.clone(), sender));
            (self.notify)(
                "document-request",
                json!({"id":id,"session":session,"path":path,"root":self.workspace.root(),"steps":steps}),
            );
            let result = tokio::time::timeout(Duration::from_secs(60), receiver).await;
            self.registry
                .lock()
                .map_err(|e| e.to_string())?
                .pending
                .remove(&id);
            let response = result
                .map_err(|_| "The editor isn't responding. The original file was kept; reopen the document and try again.".to_string())?
                .map_err(|e| e.to_string())??;
            if response["error"].is_object() {
                return Err(response["error"].to_string());
            }
            let results = response["completed"]
                .as_array()
                .ok_or("invalid ai/run response")?;
            let mut output = Vec::new();
            for result in results {
                if result.get("error").is_some_and(|v| !v.is_null()) {
                    return Err(result["error"].to_string());
                }
                output.push(
                    result
                        .get("result")
                        .cloned()
                        .unwrap_or_else(|| result.clone()),
                );
            }
            if output.len() != steps.len() {
                return Err(format!(
                    "kit returned {} results for {} commands",
                    output.len(),
                    steps.len()
                ));
            }
            if results.iter().any(|r| r["changed"] == true) {
                (self.notify)("workspace-changed", json!({"path":path}));
            }
            return Ok(output);
        }
        let workspace = self.workspace.clone();
        let path = path.to_owned();
        let origin = origin.to_owned();
        let tables = self.tables.clone();
        let output = tokio::task::spawn_blocking(move || {
            if path.ends_with(".ktable") {
                use table_sdk::{KitDocument, Origin, TableDocument};
                // Reuse the decoded table while the file is unchanged; any other writer (an
                // editor, another process) changes the version and forces a fresh open.
                let version = workspace.stamp(&path).map_err(|e| e.to_string())?.ok_or("File not found")?;
                let cached = tables
                    .lock()
                    .map_err(|e| e.to_string())?
                    .remove(&path)
                    .filter(|(v, _)| *v == version)
                    .map(|(_, doc)| doc);
                let mut doc = match cached {
                    Some(doc) => doc,
                    None => {
                        let file = workspace.read(&path, None, None).map_err(|e| e.to_string())?;
                        TableDocument::open(&path, &file.bytes)?
                    }
                };
                let mut output = Vec::new();
                // On an error the partly changed document is dropped, never cached.
                for step in steps {
                    output.push(doc.execute_as(
                        &step.command,
                        step.params,
                        &if origin.starts_with("system:") {
                            Origin::system(&origin)
                        } else {
                            Origin::agent(&origin)
                        },
                    )?);
                }
                let changed = doc.dirty();
                let version = if changed {
                    let bytes = doc.save(None)?;
                    workspace
                        .write(&path, &bytes, "save", Some(&version))
                        .map_err(|e| e.to_string())?
                } else {
                    version
                };
                tables.lock().map_err(|e| e.to_string())?.insert(path, (version, doc));
                return Ok::<_, String>((output, changed));
            }
            let file = workspace
                .read(&path, None, None)
                .map_err(|e| e.to_string())?;
            let mut output = Vec::new();
            let bytes = {
                use note_sdk::{KitDocument, KitSessionDocument, NoteDocument, Origin};
                let mut doc = NoteDocument::open(&path, &file.bytes)?;
                for step in steps {
                    output.push(doc.execute_as(
                        &step.command,
                        step.params,
                        &if origin.starts_with("system:") {
                            Origin::system(&origin)
                        } else {
                            Origin::agent(&origin)
                        },
                    )?);
                }
                if doc.dirty() {
                    Some(doc.save(None)?)
                } else {
                    None
                }
            };
            let changed = bytes.is_some();
            if let Some(bytes) = bytes {
                workspace
                    .write(&path, &bytes, "save", Some(&file.version))
                    .map_err(|e| e.to_string())?;
            }
            Ok::<_, String>((output, changed))
        })
        .await
        .map_err(|e| e.to_string())??;
        if output.1 {
            (self.notify)("workspace-changed", json!({}));
        }
        Ok(output.0)
    }
    pub fn create_table(&self, path: &str) -> Result<(), String> {
        use table_sdk::{KitDocument, TableDocument};
        let doc = TableDocument::create(path, 0, 0)?;
        self.workspace
            .put_new(path, &doc.save(None)?)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
impl Docs {
    /// Agent commands on closed files use the supplier's headless executable.
    pub async fn headless(
        &self,
        path: &str,
        command: &str,
        params: Value,
        executable: &std::path::Path,
    ) -> Result<Value, String> {
        let lock = self.lock(path);
        let guard = lock.lock().await;
        if self.is_open(path) {
            drop(guard);
            return self.execute(path, command, params, "agent").await;
        }
        let file = self.workspace.file(path).map_err(|e| e.to_string())?;
        let mut options = kit_host::LaunchOptions::new(executable);
        options.workspace_roots = vec![self.workspace.root().to_owned()];
        options.stderr = kit_host::KitStderr::Inherit;
        let kit = kit_host::HeadlessKit::launch(options)
            .await
            .map_err(|e| e.to_string())?;
        let limit = Duration::from_secs(60);
        let result=async{
            let opened=kit.request("document.open",json!({"uri":kit_host::file_uri(&file)}),limit).await.map_err(|e|e.to_string())?;
            let executed=kit.request("commands.execute",json!({"documentId":opened["documentId"],"command":command,"params":params,"origin":"agent"}),limit).await.map_err(|e|e.to_string())?;
            if executed["changed"]==true {kit.request("document.save",json!({"documentId":opened["documentId"]}),limit).await.map_err(|e|e.to_string())?;(self.notify)("workspace-changed",json!({"path":path}));}
            Ok::<_,String>(executed["result"].clone())
        }.await;
        kit.shutdown(Duration::from_secs(2)).await;
        result
    }
}
impl Docs {
    pub async fn headless_method(
        &self,
        path: &str,
        method: &str,
        executable: &std::path::Path,
    ) -> Result<Value, String> {
        let lock = self.lock(path);
        let _guard = lock.lock().await;
        let session = self
            .registry
            .lock()
            .map_err(|e| e.to_string())?
            .open
            .get(path)
            .cloned();
        if let Some(session) = session {
            let id = uuid::Uuid::new_v4().to_string();
            let (sender, receiver) = oneshot::channel();
            self.registry
                .lock()
                .map_err(|e| e.to_string())?
                .pending
                .insert(id.clone(), (session.clone(), sender));
            (self.notify)(
                "document-request",
                json!({"id":id,"session":session,"path":path,"root":self.workspace.root(),"method":method}),
            );
            let result = tokio::time::timeout(Duration::from_secs(60), receiver).await;
            self.registry
                .lock()
                .map_err(|e| e.to_string())?
                .pending
                .remove(&id);
            return result
                .map_err(|_| "Editor request timed out".to_string())?
                .map_err(|e| e.to_string())?;
        }
        let mut options = kit_host::LaunchOptions::new(executable);
        options.workspace_roots = vec![self.workspace.root().to_owned()];
        options.stderr = kit_host::KitStderr::Inherit;
        let kit = kit_host::HeadlessKit::launch(options)
            .await
            .map_err(|e| e.to_string())?;
        let timeout = Duration::from_secs(60);
        let result = async {
            let params = {
                let file = self.workspace.file(path).map_err(|e| e.to_string())?;
                let opened = kit
                    .request(
                        "document.open",
                        json!({"uri":kit_host::file_uri(&file)}),
                        timeout,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                json!({"documentId":opened["documentId"]})
            };
            kit.request(method, params, timeout)
                .await
                .map_err(|e| e.to_string())
        }
        .await;
        kit.shutdown(Duration::from_secs(2)).await;
        result
    }
}

/// Whether a table command declares `asOrigin` (writes as a host feature), per the kit's own
/// command schema.
fn takes_as_origin(command: &str) -> bool {
    table_sdk::agent::input_json_schema(command)
        .is_some_and(|schema| schema["properties"].get(table_sdk::AS_ORIGIN_PARAM).is_some())
}

#[cfg(test)]
mod origin_tests {
    #[test]
    fn as_origin_only_on_mutating_commands() {
        assert!(super::takes_as_origin("table.records.create"));
        assert!(!super::takes_as_origin("table.tables.list"));
        assert!(!super::takes_as_origin("table.records.list"));
        assert!(!super::takes_as_origin("table.undo"));
    }
}
