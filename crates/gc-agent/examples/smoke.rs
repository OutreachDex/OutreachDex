//! Check a real installed adapter without giving it tools or workspace content.
use gc_agent::{Agents, Request};
use serde_json::json;
use std::{sync::Arc, time::Duration};
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let kind = args.get(1).expect("claude or codex");
    let program = args.get(2).expect("adapter path");
    let agents = Agents::default();
    let deny = agents.clone();
    let result=tokio::time::timeout(Duration::from_secs(90),agents.run(Request{id:"smoke".into(),program:program.into(),args:vec![],workspace:std::env::temp_dir(),prompt:"Reply with exactly READY. Do not read or write files, access the network, or invoke tools.".into(),mcp:json!([]),generation:kind=="claude"},Arc::new(move|event,value|{if event=="agent-permission"{let _=deny.permission(value["id"].as_str().unwrap_or(""),None);}}))).await;
    match result {
        Ok(Ok(text)) => println!("{text}"),
        Ok(Err(error)) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
        Err(_) => {
            agents.cancel("smoke");
            eprintln!("Agent did not finish within 90 seconds");
            std::process::exit(2)
        }
    }
}
