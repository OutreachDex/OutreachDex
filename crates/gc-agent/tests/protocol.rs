use gc_agent::{Agents, Request};
use serde_json::json;
use std::{path::Path, sync::Arc, time::Duration};
fn request(mode: &str) -> Request {
    Request {
        id: mode.into(),
        program: "/usr/bin/python3".into(),
        args: vec![
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/mock_acp.py")
                .to_string_lossy()
                .into(),
            mode.into(),
        ],
        workspace: std::env::temp_dir(),
        prompt: "Test".into(),
        mcp: json!([]),
        generation: mode == "generation",
    }
}
#[tokio::test]
async fn streaming_permissions_and_generation_are_real_acp_frames() {
    for mode in ["allow", "deny", "generation"] {
        let agents = Agents::default();
        let answer = agents.clone();
        let option = mode.to_owned();
        let output = agents
            .run(
                request(mode),
                Arc::new(move |event, value| {
                    if event == "agent-permission" {
                        answer
                            .permission(value["id"].as_str().unwrap(), Some(option.clone()))
                            .unwrap();
                    }
                }),
            )
            .await
            .unwrap();
        assert_eq!(output, "Streaming complete.");
    }
}
#[tokio::test]
async fn cancellation_and_crashes_end_the_session() {
    let agents = Agents::default();
    let cancel = agents.clone();
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        agents.run(
            request("cancel"),
            Arc::new(move |event, _| {
                if event == "agent-update" {
                    cancel.cancel("cancel");
                }
            }),
        ),
    )
    .await
    .unwrap();
    assert!(output.unwrap_err().contains("cancelled"));
    assert!(
        agents
            .run(request("crash"), Arc::new(|_, _| {}))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cancellation_expires_an_unanswered_permission() {
    let agents = Agents::default();
    let cancel = agents.clone();
    let pending = Arc::new(std::sync::Mutex::new(String::new()));
    let saved = pending.clone();
    let result = agents
        .run(
            request("allow"),
            Arc::new(move |event, value| {
                if event == "agent-permission" {
                    *saved.lock().unwrap() = value["id"].as_str().unwrap().into();
                    cancel.cancel("allow");
                }
            }),
        )
        .await;
    assert!(result.unwrap_err().contains("cancelled"));
    assert!(
        agents
            .permission(&pending.lock().unwrap(), None)
            .unwrap_err()
            .contains("expired")
    );
}
