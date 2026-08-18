use async_trait::async_trait;
use mcp_router::registry::{
    MCPExecutionResult, MCPExecutionResultStream, MCPTool, MCPToolExecutor,
};
use mcp_router::router::{Router, RouterResponse, RouterStream};
use mcp_router::{MCPTool, SinkExt, StreamExt, stream_channel};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Write;
use tokio::io::{AsyncBufReadExt, BufReader};

/// A perfectly ordinary tool, registered alongside a streaming one, to show that
/// non-streaming tools need no special handling just because a sibling tool streams.
#[derive(MCPTool, Deserialize, Serialize)]
#[meta(
    name = "reverseString",
    title = "Reverse String",
    description = "Reverses the characters of the given string"
)]
struct ReverseString {
    text: String,
}

#[async_trait]
impl MCPToolExecutor for ReverseString {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let reversed: String = self.text.chars().rev().collect();
        (vec![MCPExecutionResult::TEXT(reversed.into())], None)
    }
}

/// Simulates a slow job: counts from 1 to n, reporting progress after every step,
/// then answers with the final count. One-way -- unlike bidirectional-tokio's
/// sampling tool, nothing here ever expects a reply back from the client, so the
/// paired reply receiver is just created and dropped.
#[derive(MCPTool, Deserialize, Serialize)]
#[meta(
    name = "countTo",
    title = "Count To",
    description = "Counts up to n, emitting a progress notification at each step"
)]
struct CountTo {
    n: u32,
}

#[async_trait]
impl MCPToolExecutor for CountTo {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let (mut out_tx, out_rx) = stream_channel::<Value>(16);
        let (in_tx, _in_rx) = stream_channel::<Value>(16);
        let n = self.n;

        tokio::spawn(async move {
            for i in 1..=n {
                let percent = (i * 100) / n.max(1);
                out_tx
                    .send(json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/progress",
                        "params": {"progress": i, "total": n, "percent": percent}
                    }))
                    .await
                    .ok();
            }
            out_tx.send(json!({"countedTo": n})).await.ok();
        });

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

#[tokio::main]
async fn main() {
    let router = Router::new().build();

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        match router.exec_from_value(v).await {
            RouterResponse::Value(v) if v.is_null() => {}
            RouterResponse::Value(v) => print_line(&v),
            RouterResponse::Stream(RouterStream { mut receiver, .. }) => {
                // countTo never expects a reply, so the stream's own reply sender
                // is discarded here instead of stashed for routing stdin into it.
                tokio::spawn(async move {
                    while let Some(item) = receiver.next().await {
                        print_line(&item);
                    }
                });
            }
        }
    }
}
