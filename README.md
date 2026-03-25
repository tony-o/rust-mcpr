# mcpr - an MCP router and serializer

this library is for people who don't want to be experts in MCP dances, and inspired by the lack of cohesion/poor documentation in current MCP libraries.

there are a few modes this library was intended to be used in:

- see no evil, hear no evil, speak mode
- see no evil, hear, and speak mode
- i need a lot of edge case management

## ez mode

this mode is for mostly fully managed users who just want to bootstrap an MCP server and don't really need a lot of edge case management. you can likely
implement everything you need to do in this mode. see below in this readme for a further discussion as to why you'd want to go to a less managed level.

### ez mode tools

```rust
use mcpr::macros::MCPTool;
use mcpr::registry::{
    MCPExecutionResult, MCPTool, MCPToolExecutor,
};
use serde::{Deserialize, Serialize};

#[derive(MCPTool, Deserialize, Serialize)]
#[meta(title = "FingerSaw", description = "Cuts off one of the user's fingers")]
struct FingerSaw {
    example: u32,
    optional_example: Option<u32>,
    /* anything JSON serializable works here */
}

impl MCPToolExecutor for FingerSaw {
    fn execute(&self) -> Vec<MCPExecutionResult> {
        /* self here is a FingerSaw, you can do whatever you need to in here */

        vec![MCPExecutionResult::TEXT(format!(
            "example={},optional_example={:?}"
            self.example,
            self.optional_example,
        ))]
    }
}

/* somewhere else in a transport mechanism, shown here as a rocket route */
#[post("/mcp", format = "json", data = "<body>")]
pub async mcp(body: Json<Value>) -> Json<Value> {
    Json(mcpr::router::Router::exec_from_value(body.into_inner()))
}
```

### ez mode resources

```rust
use mcpr::macros::MCPTool;
use mcpr::registry::{
    MCPExecutionResult, MCPTool, MCPToolExecutor,
};
use serde::{Deserialize, Serialize};

#[derive(MCPTool, Deserialize, Serialize)]
#[meta(title = "HandSaws", description = "Let's the LLM know what handsaws you can use the FingerSaw on")]
struct HandSaw {
    dsn: udsn::DSN, /* this is optional and the only field this struct can ever get populated with
                     * by way of the MCP spec. you can either use the DSN as a resource template
                     * or you need to manually list your resources per the spec.
                     */
}

impl MCPToolExecutor for HandSaw {
    fn execute(&self) -> Vec<MCPExecutionResult> {
        /* self here is a HandSaw, you can do whatever you need to in here */
        if self.dsn.protocol == "file" {
            /* serve files */
        } else if self.dsn.protocol == "git" {
            /* do something with git */
        }

        vec![MCPExecutionResult::TEXT("see the documentation below for more information about MCPExecutionResult"))]
    }

    fn serves(&self, dsn: &udsn::DSN) -> bool {
        dsn.protocol == "git" || dsn.protocol == "file"
    }
}

/* somewhere else in a transport mechanism, shown here as a rocket route.
 * the same route and router is able to handle everything, you don't need
 * to do any manual RPC handling for MCP methods
 */
#[post("/mcp", format = "json", data = "<body>")]
pub async mcp(body: Json<Value>) -> Json<Value> {
    Json(mcpr::router::Router::exec_from_value(body.into_inner()))
}
```

# TODO

- document MCPExecutionResult
