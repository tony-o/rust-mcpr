A transport-agnostic router for the [Model Context Protocol](https://modelcontextprotocol.io).

`mcp-router` implements the JSON-RPC dispatch, pagination, batching, and content-block
serialization that every MCP server needs, so you only have to write the tools, resources,
and prompts themselves. It never picks a transport (HTTP, stdio, whatever you're already
using) and never picks an async runtime (it has no runtime dependency of its own — tokio,
async-std, and smol are all first-class).

# Quick start

Define a tool with `#[derive(MCPTool)]`, implement [`registry::MCPToolExecutor`], then hand
any JSON-RPC request straight to [`router::Router::exec_from_value`]:

```
use async_trait::async_trait;
use mcp_router::MCPTool;
use mcp_router::registry::{MCPExecutionResult, MCPTool as _, MCPToolExecutor};
use mcp_router::router::{Router, RouterResponse};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(MCPTool, Deserialize, Serialize)]
#[meta(name = "echo", title = "Echo", description = "echoes its input back")]
struct Echo {
    text: String,
}

#[async_trait]
impl MCPToolExecutor for Echo {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        (vec![MCPExecutionResult::TEXT(self.text.clone().into())], None)
    }
}

# #[tokio::main(flavor = "current_thread")]
# async fn main() {
let router = Router::new().build();
let response = router
    .exec_from_value(json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "echo", "arguments": {"text": "hello"}}
    }))
    .await;
assert!(matches!(response, RouterResponse::Value(_)));
# }
```

# Where to go next

- [`registry`] — the traits and content types you implement to define tools, resources, and
  prompts ([`registry::MCPTool`]/[`registry::MCPToolExecutor`],
  [`registry::MCPResource`]/[`registry::MCPResourceExecutor`],
  [`registry::MCPPrompt`]/[`registry::MCPPromptExecutor`], and the [`registry::MCPExecutionResult`]
  content-block enum), plus the [`registry::Registry`] you can build by hand instead of relying
  on the derive macros' global auto-registration.
- [`router`] — [`router::Router`], the single entry point ([`router::Router::exec_from_value`])
  that turns a raw JSON-RPC value into a [`router::RouterResponse`], and the streaming types
  ([`router::RouterStream`]/[`router::RouterStreamSender`]) behind `MCPExecutionResult::STREAM`.
- The crate's `README.md` (rendered on the repository/crates.io page) walks through pagination,
  transports, ez-mode vs. manual-mode registration, and the full streaming story
  (progress notifications, cancellation, `sampling/createMessage`-style round trips, and batch
  elevation) in far more depth than doc comments comfortably can.
- The `examples/` directory in the repository has full, runnable servers: stdio transports
  (with and without streaming), an HTTP server per major framework (Rocket, Axum, Actix-web,
  Warp), and the same bidirectional streaming demo ported across tokio, async-std, and smol.
