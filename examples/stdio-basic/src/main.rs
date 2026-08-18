use async_trait::async_trait;
use mcp_router::registry::{MCPExecutionResult, MCPTool, MCPToolExecutor};
use mcp_router::router::{Router, RouterResponse};
use mcp_router::MCPTool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use tokio::io::{AsyncBufReadExt, BufReader};

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
            // No tool in this example ever streams, but the match stays exhaustive
            // so the code is honest about what exec_from_value can actually return.
            RouterResponse::Stream(_) => unreachable!("stdio-basic registers no streaming tools"),
        }
    }
}
