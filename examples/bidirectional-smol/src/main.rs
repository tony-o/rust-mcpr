use async_trait::async_trait;
use mcp_router::registry::{
    MCPExecutionResult, MCPExecutionResultStream, MCPTool, MCPToolExecutor,
};
use mcp_router::router::{Router, RouterResponse, RouterStream};
use mcp_router::{MCPTool, SinkExt, StreamExt, stream_channel};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use smol::io::{AsyncBufReadExt, BufReader};
use std::io::Write;

#[derive(MCPTool, Deserialize, Serialize)]
#[meta(
    name = "summarizeViaSampling",
    title = "Summarize via Sampling",
    description = "Asks the client's own LLM to summarize a block of text mid-call"
)]
struct SummarizeViaSampling {
    text: String,
}

#[async_trait]
impl MCPToolExecutor for SummarizeViaSampling {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let (mut out_tx, out_rx) = stream_channel::<Value>(16);
        let (in_tx, mut in_rx) = stream_channel::<Value>(16);
        let text = self.text.clone();

        // smol::spawn returns a Task that is cancelled if dropped without .detach() --
        // unlike tokio's JoinHandle, which keeps the task running regardless. Without
        // .detach() here this background work silently never happens.
        smol::spawn(async move {
            let request_id = "summarize-1";
            out_tx
                .send(json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "method": "sampling/createMessage",
                    "params": {
                        "messages": [{
                            "role": "user",
                            "content": {
                                "type": "text",
                                "text": format!("Summarize this in one sentence: {}", text)
                            }
                        }],
                        "maxTokens": 100
                    }
                }))
                .await
                .ok();

            let reply = loop {
                match in_rx.next().await {
                    Some(v) if v.get("id") == Some(&json!(request_id)) => break v,
                    Some(_) => continue,
                    None => return,
                }
            };

            let summary = reply["result"]["content"]["text"]
                .as_str()
                .unwrap_or("(client gave no summary)")
                .to_string();

            out_tx
                .send(json!({"content": [{"type": "text", "text": summary}]}))
                .await
                .ok();
        })
        .detach();

        (
            vec![MCPExecutionResult::STREAM(MCPExecutionResultStream {
                receiver: out_rx,
                sender: in_tx,
            })],
            None,
        )
    }
}

fn print_line(v: &Value) {
    println!("{}", serde_json::to_string(v).expect("valid json in, valid json out"));
    std::io::stdout().flush().ok();
}

fn main() {
    smol::block_on(async {
        let router = Router::new().build();

        let stdin = smol::Unblock::new(std::io::stdin());
        let mut reader = BufReader::new(stdin).lines();

        // The sender half of whatever tool call is currently mid-stream, if any. A line
        // from stdin with no "method" is a reply to that tool's own server-initiated
        // request, not a new client request, and gets routed in here instead.
        let mut pending_sender = None;

        while let Some(Ok(line)) = reader.next().await {
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };

            if v.get("method").is_none() {
                if let Some(sender) = pending_sender.as_mut() {
                    let sender: &mut mcp_router::router::RouterStreamSender = sender;
                    sender.send(v).await;
                }
                continue;
            }

            match router.exec_from_value(v).await {
                RouterResponse::Value(v) if v.is_null() => {}
                RouterResponse::Value(v) => print_line(&v),
                RouterResponse::Stream(s) => {
                    let RouterStream {
                        mut receiver,
                        sender,
                    } = s;
                    pending_sender = Some(sender);
                    // Must .detach() -- see comment above on the same gotcha.
                    smol::spawn(async move {
                        while let Some(item) = receiver.next().await {
                            print_line(&item);
                        }
                    })
                    .detach();
                }
            }
        }
    });
}
