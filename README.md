# mcp-router - an MCP router and serializer

this library is for people who don't want to be experts in MCP dances, and inspired by the lack of cohesion/poor documentation in current MCP libraries.

there are a few modes this library was intended to be used in:

- see no evil, hear no evil, speak (see: ez mode below)
- see no evil, hear, and speak mode (see: medium difficulty below)

## ez mode

this mode is for mostly fully managed users who just want to bootstrap an MCP server and don't really need a lot of edge case management. you can likely
implement everything you need to do in this mode. see below in this readme for a further discussion as to why you'd want to go to a less managed level.

### ez mode tools

```rust
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
            "example={},optional_example={:?}",
            self.example,
            self.optional_example,
        ))]
    }
}

/* somewhere else in a transport mechanism, shown here as a rocket route */
#[post("/mcp", format = "json", data = "<body>")]
pub async fn mcp(body: Json<Value>) -> Json<Value> {
    /* you might put this default router into a fairing or whatever your HTTP framework's analog */
    Json(mcp_router::router::Router::new().exec_from_value(body.into_inner()))
}
```

### ez mode resources

```rust
use mcp_router::registry::{
    MCPResource, MCPResourceExecutor, MCPResourceResult,
};
use serde::{Deserialize, Serialize};

#[derive(MCPResource, Deserialize, Serialize)]
#[meta(title = "HandSaws", description = "Let's the LLM know what handsaws you can use the FingerSaw on")]
struct HandSaw {
    dsn: udsn::DSN, /* this is optional and the only field this struct can ever get populated with
                     * by way of the MCP spec. you can either use the DSN as a resource template
                     * or you need to manually list your resources per the spec.
                     * this struct member can be omitted safely if it's not needed in the resource
                     * execution
                     */
}

impl MCPResourceExecutor for HandSaw {
    fn execute(&self) -> Vec<MCPResourceResult> {
        /* self here is a HandSaw, you can do whatever you need to in here */
        if self.dsn.protocol == "file" {
            /* serve files */
        } else if self.dsn.protocol == "git" {
            /* do something with git */
        }

        vec![MCPResourceResult::new("file:///example".to_string(), "example file".to_string())]
    }

    fn serves(dsn: &udsn::DSN) -> bool {
        dsn.protocol == "git" || dsn.protocol == "file"
    }

    fn is_template() -> bool {
        true
    }
}

/* somewhere else in a transport mechanism, shown here as a rocket route.
 * the same route and router is able to handle everything, you don't need
 * to do any manual RPC handling for MCP methods
 */
#[post("/mcp", format = "json", data = "<body>")]
pub async fn mcp(body: Json<Value>) -> Json<Value> {
    /* you might put this default router into a fairing or whatever your HTTP framework's analog */
    Json(mcp_router::router::Router::new().exec_from_value(body.into_inner()))
}
```

## medium difficulty

medium difficulty is where you might end up if you are generating resources or have specific routing requirements. good use cases for this
are:

- you want multiple MCPs handled in one transport mechanism
  - eg one router for http transport @ /v1/mcp and another @ /v2/mcp
- you have static resources (not templates) you want to list in the router initialization handshake
  - eg you only want to mcp to know of files in a directory at server startup
  - you're generating access/commands from a config file and will handle them programmatically
  - you are willing to implement both traits needed manually rather than using a resource template

example resource:

```rust
use mcp_router::registry::{
    FromArgResult, MCPMeta, MCPResource, MCPResourceExecutor, MCPResourceResult,
};
use serde_json::Value;

#[derive(serde::Deserialize)]
pub struct ManualResource {
    dsn: udsn::DSN,
}

impl MCPResourceExecutor for ManualResource {
    fn execute(&self) -> Vec<MCPResourceResult> {
        println!("dsn executor called: {}", self.dsn.to_string());
        vec![]
    }

    fn serves(dsn: &udsn::DSN) -> bool {
        /* this is only called when is_template is true
         * the DSN must match exactly if is_template is false
         */
        dsn.protocol == "manual-resource"
    }

    fn is_template() -> bool {
        true
    }
}

impl MCPResource for ManualResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new()
            .name("meta_example")
            .uri("manual-resource:///")
            .build()]
    }
    fn params() -> Value {
        Value::Null
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Resource(Box::new(s)),
            Err(e) => {
                /* handle your error here */
                FromArgResult::Error(e.to_string())
            }
        }
    }
}

/* elsewhere in your registry initializer */

pub fn init_router() {
    use std::collections::HashMap;
    let registry = registry::Registry::new_from(HashMap::new(), HashMap::new());
    registry.register_resource_adapter::<ManualResource>("manual-resource://{some path}");
    let router = Router::default().registry(&registry).build();

    /* do whatever you need to do with your router here */
}
```

## TODO

- document MCPExecutionResult
- fix tool documentation
- organize all of this
- make better docs
- talk about transport or make examples of them, really stick it to 'em
