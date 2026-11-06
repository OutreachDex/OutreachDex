use agent_client_protocol::schema::{ProtocolVersion, v1::*};
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, ConnectionTo};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::oneshot;
pub type Notify = Arc<dyn Fn(&str, Value) + Send + Sync>;
type Permission = (String, oneshot::Sender<Option<String>>);
#[derive(Clone, Default)]
pub struct Agents {
    permissions: Arc<Mutex<HashMap<String, Permission>>>,
    cancelled: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}
#[derive(Clone)]
pub struct Request {
    pub id: String,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub workspace: PathBuf,
    pub prompt: String,
    pub mcp: Value,
    pub generation: bool,
}
impl Agents {
    pub fn permission(&self, id: &str, option: Option<String>) -> Result<(), String> {
        self.permissions
            .lock()
            .map_err(|e| e.to_string())?
            .remove(id)
            .ok_or("The permission request has expired.")?
            .1
            .send(option)
            .map_err(|_| "The session has ended.".into())
    }
    pub fn cancel(&self, id: &str) {
        if let Some(sender) = self
            .cancelled
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
        {
            let _ = sender.send(());
        }
    }
    pub async fn run(&self, request: Request, notify: Notify) -> Result<String, String> {
        let (done, receiver) = oneshot::channel();
        let (cancel, cancelled) = oneshot::channel();
        self.cancelled
            .lock()
            .map_err(|e| e.to_string())?
            .insert(request.id.clone(), cancel);
        let agents = self.clone();
        let id = request.id.clone();
        std::thread::spawn(move || {
            let result=tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e|e.to_string()).and_then(|rt|rt.block_on(tokio::task::LocalSet::new().run_until(async{
                tokio::select!{result=agents.session(request,notify)=>result,_=cancelled=>Err("Analysis cancelled.".into())}
            })));
            let _ = done.send(result);
        });
        let result = receiver.await.map_err(|e| e.to_string())?;
        self.cancelled
            .lock()
            .map_err(|e| e.to_string())?
            .remove(&id);
        self.permissions
            .lock()
            .map_err(|e| e.to_string())?
            .retain(|_, (session, _)| session != &id);
        result
    }
    async fn session(&self, request: Request, notify: Notify) -> Result<String, String> {
        let agent = AcpAgent::new(AcpAgentConfig::new(&request.program).args(request.args.clone()));
        let output = Arc::new(Mutex::new(String::new()));
        let shared = output.clone();
        let session_id = request.id.clone();
        let announce = notify.clone();
        let permissions = self.permissions.clone();
        let permission_id = request.id.clone();
        let generation = request.generation;
        let work=agent_client_protocol::Client.builder()
            .on_receive_notification(async move |notification:SessionNotification,_cx|{
                let data=serde_json::to_value(&notification.update).unwrap_or(Value::Null);
                if data["sessionUpdate"]=="agent_message_chunk" && let Some(text)=data["content"]["text"].as_str(){shared.lock().unwrap_or_else(|e|e.into_inner()).push_str(text);}
                announce("agent-update",json!({"id":session_id,"update":data}));Ok(())
            },agent_client_protocol::on_receive_notification!())
            .on_receive_request(async move |question:RequestPermissionRequest,responder,_connection|{
                let id=uuid::Uuid::new_v4().to_string();
                let choice=if generation {None}else{
                    let (sender,receiver)=oneshot::channel();
                    permissions.lock().unwrap_or_else(|e|e.into_inner()).insert(id.clone(),(permission_id.clone(),sender));
                    notify("agent-permission",json!({"id":id,"session":permission_id,"request":question}));
                    let answer=tokio::time::timeout(Duration::from_secs(600),receiver).await.ok().and_then(Result::ok).flatten();
                    permissions.lock().unwrap_or_else(|e|e.into_inner()).remove(&id);
                    answer
                };
                let outcome=choice.and_then(|choice|question.options.iter().find(|o|o.option_id.to_string()==choice).map(|o|o.option_id.clone())).map(|id|RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id))).unwrap_or(RequestPermissionOutcome::Cancelled);
                responder.respond(RequestPermissionResponse::new(outcome))
            },agent_client_protocol::on_receive_request!())
            .connect_with(agent,|connection:ConnectionTo<Agent>|async move{
                connection.send_request(InitializeRequest::new(ProtocolVersion::V1)).block_task().await?;
                let mut params=json!({"cwd":request.workspace,"mcpServers":if request.generation{json!([])}else{request.mcp}});
                if request.generation {params["_meta"]=json!({"claudeCode":{"options":{"tools":[],"allowedTools":[],"disallowedTools":["*"],"settingSources":[],"mcpServers":{},"strictMcpConfig":true}}});}
                let params:NewSessionRequest=serde_json::from_value(params).map_err(|e|agent_client_protocol::Error::internal_error().data(e.to_string()))?;
                let session=connection.send_request(params).block_task().await?;
                connection.send_request(PromptRequest::new(session.session_id,vec![ContentBlock::Text(TextContent::new(request.prompt))])).block_task().await?;
                Ok(())
            });
        tokio::time::timeout(Duration::from_secs(1800), work)
            .await
            .map_err(|_| "Analysis timed out.".to_string())?
            .map_err(|e| e.to_string())?;
        let result = output.lock().map_err(|e| e.to_string())?.clone();
        Ok(result)
    }
}
