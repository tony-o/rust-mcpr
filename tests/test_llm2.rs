use async_trait::async_trait;
use mcp_router::registry::{
    FromArgResult, Info, InfoType, MCPExecutionResult, MCPMeta, MCPPrompt, MCPPromptExecutor,
    MCPPromptMessage, MCPPromptResult, MCPResource, MCPResourceExecutor, MCPResourceResult,
    MCPTool, MCPToolExecutor, Registry,
};
use mcp_router::router::{Router, RouterResponse};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(serde::Deserialize)]
struct NoopTool {}

#[async_trait]
impl MCPToolExecutor for NoopTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        (vec![MCPExecutionResult::TEXT("noop".into())], None)
    }
}

impl MCPTool for NoopTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("noop").build()]
    }
    fn params() -> Value {
        json!({
            "name": "noop",
            "title": "Noop",
            "description": "does nothing",
            "inputSchema": {"type": "object", "properties": {}, "required": []}
        })
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Tool(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

#[derive(serde::Deserialize)]
struct ErrorWithDataTool {}

#[async_trait]
impl MCPToolExecutor for ErrorWithDataTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        (
            vec![MCPExecutionResult::ERROR((
                "boom".to_string(),
                Some(json!({"detail": "extra info"})),
            ))],
            None,
        )
    }
}

impl MCPTool for ErrorWithDataTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("errorWithData").build()]
    }
    fn params() -> Value {
        json!({
            "name": "errorWithData",
            "title": "Error With Data",
            "description": "always errors, carrying an error.data payload",
            "inputSchema": {"type": "object", "properties": {}, "required": []}
        })
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Tool(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

#[derive(serde::Deserialize)]
struct RawVariantsTool {}

#[async_trait]
impl MCPToolExecutor for RawVariantsTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        (
            vec![
                MCPExecutionResult::RAW(json!("a plain string")),
                MCPExecutionResult::RAW(json!(42)),
                MCPExecutionResult::RAW(json!([1, 2, 3])),
            ],
            None,
        )
    }
}

impl MCPTool for RawVariantsTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("rawVariants").build()]
    }
    fn params() -> Value {
        json!({
            "name": "rawVariants",
            "title": "Raw Variants",
            "description": "returns RAW content blocks that are not JSON objects",
            "inputSchema": {"type": "object", "properties": {}, "required": []}
        })
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Tool(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

#[derive(serde::Deserialize)]
struct MinimalResource {
    dsn: udsn::DSN,
}

#[async_trait]
impl MCPResourceExecutor for MinimalResource {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        (
            vec![MCPResourceResult::new(self.dsn.to_string(), "minimal".to_string()).build()],
            None,
        )
    }
    fn serves(_dsn: &udsn::DSN) -> bool {
        false
    }
    fn is_template() -> bool {
        false
    }
}

impl MCPResource for MinimalResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new()
            .name("minimal")
            .uri("minimal-resource://fixed")
            .build()]
    }
    fn params() -> Value {
        Value::Null
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Resource(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

macro_rules! define_ambiguous_template {
    ($ident:ident, $tag:expr) => {
        #[derive(serde::Deserialize)]
        struct $ident {
            dsn: udsn::DSN,
        }

        #[async_trait]
        impl MCPResourceExecutor for $ident {
            async fn execute(
                &self,
                _cursor: Option<String>,
            ) -> (Vec<MCPResourceResult>, Option<String>) {
                (
                    vec![MCPResourceResult::new(self.dsn.to_string(), $tag.to_string()).build()],
                    None,
                )
            }
            fn serves(dsn: &udsn::DSN) -> bool {
                dsn.protocol == "ambiguous"
            }
            fn is_template() -> bool {
                true
            }
        }

        impl MCPResource for $ident {
            fn get_executor(&self) -> &dyn MCPResourceExecutor {
                self
            }
            fn meta() -> Vec<MCPMeta> {
                vec![MCPMeta::new()
                    .name($tag)
                    .uri(&format!("ambiguous-{}://{{id}}", $tag))
                    .build()]
            }
            fn params() -> Value {
                Value::Null
            }
            fn from_args(v: &Value) -> FromArgResult {
                match serde_json::from_value::<Self>(v.clone()) {
                    Ok(s) => FromArgResult::Resource(Box::new(s)),
                    Err(e) => FromArgResult::Error(e.to_string()),
                }
            }
        }
    };
}

define_ambiguous_template!(TemplateA, "template-a");
define_ambiguous_template!(TemplateB, "template-b");

fn empty_registry() -> Registry {
    Registry::new_from(HashMap::new(), HashMap::new())
}

fn router_for(registry: &Registry) -> Router<'_> {
    Router::new().registry(registry).build()
}

fn router_for_paged(registry: &Registry, page_size: usize) -> Router<'_> {
    Router::new().registry(registry).page_size(page_size).build()
}

// A Request with an explicit "id": null is still a real request per JSON-RPC 2.0 (it MUST
// receive a response, with the same id echoed back) — this is distinct from an *absent* id,
// which makes it a Notification. May currently fail since serde collapses both cases to None.
#[tokio::test]
async fn explicit_id_null_differs_from_absent_id_per_spec() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":null,"method":"tools/list","params":{}}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert!(
        resp.get("id").is_some(),
        "a request with an explicit null id must still get a response with id present"
    );
    assert_eq!(resp["id"], Value::Null);
}

// notifications/cancelled is a real MCP notification method, sent without an id, and per
// JSON-RPC 2.0 must get no response. Only "notifications/initialized" is special-cased today,
// so this may currently fail the same way as any other method sent without an id.
#[tokio::test]
async fn notifications_cancelled_without_id_should_yield_no_response() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp, Value::Null);
}

#[tokio::test]
async fn batch_request_returns_array_of_responses_in_order() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!([
            {"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}},
            {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
        ]))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    let responses = resp.as_array().expect("batch input should yield an array of responses");
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["id"], json!(1));
    assert_eq!(responses[1]["id"], json!(2));
}

#[tokio::test]
async fn batch_request_omits_responses_for_notifications() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!([
            {"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}},
            {"jsonrpc":"2.0","method":"notifications/initialized"}
        ]))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    let responses = resp.as_array().expect("batch input should yield an array of responses");
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["id"], json!(1));
}

#[tokio::test]
async fn batch_request_all_notifications_yields_no_response() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!([
            {"jsonrpc":"2.0","method":"notifications/initialized"},
            {"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}
        ]))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp, Value::Null);
}

#[tokio::test]
async fn empty_batch_request_returns_single_invalid_request_error() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router.exec_from_value(json!([])).await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert!(resp.is_object(), "an empty batch must yield a single Response object, not an array");
    assert_eq!(resp["error"]["code"], json!(-32600));
}

#[tokio::test]
async fn batch_request_includes_error_response_for_malformed_entry() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!([
            {"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}},
            {"totally":"not a valid request"}
        ]))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    let responses = resp.as_array().expect("batch input should yield an array of responses");
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["id"], json!(1));
    assert_eq!(responses[1]["error"]["code"], json!(-32700));
}

#[tokio::test]
async fn tools_call_missing_params_returns_malformed_request_error() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/call"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp,
        json!({
            "jsonrpc":"2.0","id":1,
            "error": {"code": -32602, "message": "malformed request from LLM"}
        })
    );
}

#[tokio::test]
async fn resources_read_missing_params_returns_malformed_request_error() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"resources/read"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp,
        json!({
            "jsonrpc":"2.0","id":1,
            "error": {"code": -32600, "message": "malformed request from LLM: resources/read"}
        })
    );
}

#[tokio::test]
async fn tools_call_absent_arguments_key_still_succeeds_for_tool_with_no_required_fields() {
    let registry = empty_registry();
    registry.register_tool_adapter::<NoopTool>("noop");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"noop"}}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["content"], json!([{"type":"text","text":"noop"}]));
}

// The second element of MCPExecutionResult::ERROR (JSON-RPC's error.data analog) is
// surfaced as an extra content block alongside the error text, rather than being dropped.
#[tokio::test]
async fn tools_call_error_data_payload_is_surfaced_as_extra_content() {
    let registry = empty_registry();
    registry.register_tool_adapter::<ErrorWithDataTool>("errorWithData");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params": {"name": "errorWithData", "arguments": {}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"],
        json!({
            "content": [
                {"type":"text","text":"error: boom"},
                {"detail":"extra info"}
            ],
            "isError": true
        })
    );
}

#[tokio::test]
async fn tools_call_raw_content_passes_through_non_object_values() {
    let registry = empty_registry();
    registry.register_tool_adapter::<RawVariantsTool>("rawVariants");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params": {"name": "rawVariants", "arguments": {}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"]["content"],
        json!(["a plain string", 42, [1, 2, 3]])
    );
}

// page_size(0) is clamped to a minimum of 1, so pagination still makes progress instead
// of emitting the same nextCursor forever.
#[tokio::test]
async fn tools_list_page_size_zero_is_clamped_to_one() {
    let registry = empty_registry();
    registry.register_tool_adapter::<NoopTool>("a");
    registry.register_tool_adapter::<NoopTool>("b");
    let router = router_for_paged(&registry, 0);

    let resp1 = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .await;
    let resp1 = match resp1 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp1["result"]["tools"].as_array().unwrap().len(), 1);
    let cursor1 = resp1["result"]["nextCursor"]
        .as_str()
        .expect("more tools remain, so a nextCursor should be present")
        .to_string();

    let resp2 = router
        .exec_from_value(
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"cursor":cursor1}}),
        )
        .await;
    let resp2 = match resp2 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp2["result"]["tools"].as_array().unwrap().len(), 1);
    assert!(
        resp2["result"].get("nextCursor").is_none(),
        "cursor should have advanced past both items"
    );
}

#[tokio::test]
async fn initialize_capabilities_tools_only() {
    let registry = empty_registry();
    registry.register_tool_adapter::<NoopTool>("noop");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["capabilities"], json!({"tools": {}}));
}

#[tokio::test]
async fn initialize_capabilities_resources_only() {
    let registry = empty_registry();
    registry.register_resource_adapter::<MinimalResource>("minimal-resource://fixed");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["capabilities"], json!({"resources": {}}));
}

#[tokio::test]
async fn resources_read_multiple_matching_templates_picks_one_without_erroring() {
    let registry = empty_registry();
    registry.register_resource_adapter::<TemplateA>("ambiguous-template-a://{id}");
    registry.register_resource_adapter::<TemplateB>("ambiguous-template-b://{id}");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"resources/read",
            "params": {"uri": "ambiguous://99"}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    let name = resp["result"]["contents"][0]["name"]
        .as_str()
        .expect("expected a successful resource_link content block, got something else");
    assert!(name == "template-a" || name == "template-b");
}

// MCP's ping utility: the receiver must respond promptly with an empty result object.
#[tokio::test]
async fn ping_returns_empty_result() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp, json!({"jsonrpc":"2.0","id":1,"result":{}}));
}

// A ping sent as a Notification (no id) is still just any other method without an id:
// per JSON-RPC 2.0, it must get no response at all.
#[tokio::test]
async fn ping_without_id_yields_no_response() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","method":"ping"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp, Value::Null);
}

#[derive(serde::Deserialize)]
struct SimplePrompt {}

#[async_trait]
impl MCPPromptExecutor for SimplePrompt {
    async fn execute(&self) -> MCPPromptResult {
        MCPPromptResult {
            description: Some("a simple test prompt".to_string()),
            messages: vec![MCPPromptMessage {
                role: "user".to_string(),
                content: MCPExecutionResult::TEXT("hello from simple prompt".into()),
            }],
        }
    }
}

impl MCPPrompt for SimplePrompt {
    fn get_executor(&self) -> &dyn MCPPromptExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("simplePrompt").build()]
    }
    fn params() -> Value {
        json!({
            "name": "simplePrompt",
            "title": "Simple Prompt",
            "description": "a prompt with no arguments",
            "arguments": []
        })
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Prompt(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

#[derive(serde::Deserialize)]
struct ArgPrompt {
    language: String,
}

#[async_trait]
impl MCPPromptExecutor for ArgPrompt {
    async fn execute(&self) -> MCPPromptResult {
        MCPPromptResult {
            description: None,
            messages: vec![MCPPromptMessage {
                role: "user".to_string(),
                content: MCPExecutionResult::TEXT(
                    format!("Please review this {} code", self.language).into()
                ),
            }],
        }
    }
}

impl MCPPrompt for ArgPrompt {
    fn get_executor(&self) -> &dyn MCPPromptExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("argPrompt").build()]
    }
    fn params() -> Value {
        json!({
            "name": "argPrompt",
            "title": "Arg Prompt",
            "description": "a prompt requiring a language argument",
            "arguments": [{"name": "language", "description": "programming language", "required": true}]
        })
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Prompt(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

macro_rules! define_tagged_prompt {
    ($ident:ident, $tag:expr) => {
        #[derive(serde::Deserialize)]
        struct $ident {}

        #[async_trait]
        impl MCPPromptExecutor for $ident {
            async fn execute(&self) -> MCPPromptResult {
                MCPPromptResult {
                    description: None,
                    messages: vec![MCPPromptMessage {
                        role: "user".to_string(),
                        content: MCPExecutionResult::TEXT($tag.into()),
                    }],
                }
            }
        }

        impl MCPPrompt for $ident {
            fn get_executor(&self) -> &dyn MCPPromptExecutor {
                self
            }
            fn meta() -> Vec<MCPMeta> {
                vec![MCPMeta::new().name($tag).build()]
            }
            fn params() -> Value {
                json!({"name": $tag, "title": $tag, "description": $tag, "arguments": []})
            }
            fn from_args(v: &Value) -> FromArgResult {
                match serde_json::from_value::<Self>(v.clone()) {
                    Ok(s) => FromArgResult::Prompt(Box::new(s)),
                    Err(e) => FromArgResult::Error(e.to_string()),
                }
            }
        }
    };
}

define_tagged_prompt!(PromptAlpha, "alpha");
define_tagged_prompt!(PromptBeta, "beta");
define_tagged_prompt!(PromptGamma, "gamma");

static MISCONFIGURED_TOOL_AS_PROMPT: Info = Info {
    name: "misconfiguredToolAsPrompt",
    info_type: InfoType::Tool,
    params: NoopTool::params,
    from_args: NoopTool::from_args,
    meta: NoopTool::meta,
    is_template: || false,
    serves: |_| false,
    complete: |_, _| None,
};

static MISCONFIGURED_PROMPT_AS_TOOL: Info = Info {
    name: "misconfiguredPromptAsTool",
    info_type: InfoType::Prompt,
    params: SimplePrompt::params,
    from_args: SimplePrompt::from_args,
    meta: SimplePrompt::meta,
    is_template: || false,
    serves: |_| false,
    complete: SimplePrompt::complete,
};

#[tokio::test]
async fn initialize_capabilities_present_when_prompts_registered() {
    let registry = empty_registry();
    registry.register_prompt_adapter::<SimplePrompt>("simplePrompt");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["capabilities"], json!({"prompts": {}}));
}

#[tokio::test]
async fn prompts_list_empty_registry_returns_empty_array() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"prompts/list"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["prompts"], json!([]));
    assert!(resp["result"].get("nextCursor").is_none());
}

#[tokio::test]
async fn prompts_list_returns_prompt_params_verbatim() {
    let registry = empty_registry();
    registry.register_prompt_adapter::<ArgPrompt>("argPrompt");
    let router = router_for_paged(&registry, 50);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"prompts/list"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["prompts"], json!([ArgPrompt::params()]));
}

#[tokio::test]
async fn prompts_list_paginates_and_sorts_by_registered_name() {
    let registry = empty_registry();
    registry.register_prompt_adapter::<PromptAlpha>("keyC");
    registry.register_prompt_adapter::<PromptGamma>("keyA");
    registry.register_prompt_adapter::<PromptBeta>("keyB");
    let router = router_for_paged(&registry, 2);

    let resp1 = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"prompts/list"}))
        .await;
    let resp1 = match resp1 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    let names1: Vec<String> = resp1["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names1, vec!["gamma", "beta"]);
    let cursor1 = resp1["result"]["nextCursor"].as_str().unwrap().to_string();

    let resp2 = router
        .exec_from_value(
            json!({"jsonrpc":"2.0","id":2,"method":"prompts/list","params":{"cursor":cursor1}}),
        )
        .await;
    let resp2 = match resp2 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    let names2: Vec<String> = resp2["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names2, vec!["alpha"]);
    assert!(resp2["result"].get("nextCursor").is_none());
}

#[tokio::test]
async fn prompts_get_returns_rendered_messages() {
    let registry = empty_registry();
    registry.register_prompt_adapter::<ArgPrompt>("argPrompt");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"prompts/get",
            "params": {"name": "argPrompt", "arguments": {"language": "rust"}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"],
        json!({
            "messages": [{"role":"user","content":{"type":"text","text":"Please review this rust code"}}]
        })
    );
}

#[tokio::test]
async fn prompts_get_includes_description_when_present() {
    let registry = empty_registry();
    registry.register_prompt_adapter::<SimplePrompt>("simplePrompt");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"prompts/get",
            "params": {"name": "simplePrompt", "arguments": {}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["description"], json!("a simple test prompt"));
}

#[tokio::test]
async fn prompts_get_unknown_name_returns_invalid_params_error() {
    let registry = empty_registry();
    registry.register_prompt_adapter::<SimplePrompt>("simplePrompt");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"prompts/get",
            "params": {"name": "does-not-exist", "arguments": {}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["error"]["code"], json!(-32602));
}

#[tokio::test]
async fn prompts_get_malformed_arguments_returns_invalid_params_error() {
    let registry = empty_registry();
    registry.register_prompt_adapter::<ArgPrompt>("argPrompt");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"prompts/get",
            "params": {"name": "argPrompt", "arguments": {}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["error"]["code"], json!(-32602));
}

#[tokio::test]
async fn prompts_get_tool_registered_as_prompt_is_rejected_as_misconfigured() {
    let mut prompts = HashMap::new();
    prompts.insert("misconfiguredToolAsPrompt".to_string(), &MISCONFIGURED_TOOL_AS_PROMPT);
    let registry = Registry::new_from_all(HashMap::new(), HashMap::new(), prompts);
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"prompts/get",
            "params": {"name": "misconfiguredToolAsPrompt", "arguments": {}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["error"]["code"], json!(-32600));
}

#[tokio::test]
async fn tools_call_prompt_registered_as_tool_is_rejected_as_misconfigured() {
    let mut tools = HashMap::new();
    tools.insert("misconfiguredPromptAsTool".to_string(), &MISCONFIGURED_PROMPT_AS_TOOL);
    let registry = Registry::new_from_all(tools, HashMap::new(), HashMap::new());
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params": {"name": "misconfiguredPromptAsTool", "arguments": {}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["error"]["code"], json!(-32600));
}

#[derive(serde::Deserialize)]
struct CompletablePrompt {}

#[async_trait]
impl MCPPromptExecutor for CompletablePrompt {
    async fn execute(&self) -> MCPPromptResult {
        MCPPromptResult {
            description: None,
            messages: vec![],
        }
    }
}

impl MCPPrompt for CompletablePrompt {
    fn get_executor(&self) -> &dyn MCPPromptExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("completablePrompt").build()]
    }
    fn params() -> Value {
        json!({
            "name": "completablePrompt",
            "arguments": [{"name": "language", "required": false}]
        })
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Prompt(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
    fn complete(argument_name: &str, partial_value: &str) -> Option<Vec<String>> {
        if argument_name == "language" {
            Some(
                ["python", "pytorch", "rust"]
                    .into_iter()
                    .filter(|l| l.contains(partial_value))
                    .map(String::from)
                    .collect(),
            )
        } else {
            None
        }
    }
}

#[derive(serde::Deserialize)]
struct CompletableResource {
    dsn: udsn::DSN,
}

#[async_trait]
impl MCPResourceExecutor for CompletableResource {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        (
            vec![MCPResourceResult::new(self.dsn.to_string(), "completable".to_string()).build()],
            None,
        )
    }
    fn serves(dsn: &udsn::DSN) -> bool {
        dsn.protocol == "completable-resource"
    }
    fn is_template() -> bool {
        true
    }
}

impl MCPResource for CompletableResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        ["100", "101", "102", "209", "310"]
            .into_iter()
            .map(|id| {
                MCPMeta::new()
                    .name(id)
                    .uri(&format!("completable-resource://{}", id))
                    .build()
            })
            .collect()
    }
    fn params() -> Value {
        Value::Null
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Resource(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

#[tokio::test]
async fn completion_complete_prompt_without_override_returns_empty() {
    let registry = empty_registry();
    registry.register_prompt_adapter::<SimplePrompt>("simplePrompt");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"completion/complete",
            "params": {
                "ref": {"type": "ref/prompt", "name": "simplePrompt"},
                "argument": {"name": "whatever", "value": "x"}
            }
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["completion"]["values"], json!([]));
    assert_eq!(resp["result"]["completion"]["total"], json!(0));
}

#[tokio::test]
async fn completion_complete_prompt_with_override_returns_filtered_suggestions() {
    let registry = empty_registry();
    registry.register_prompt_adapter::<CompletablePrompt>("completablePrompt");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"completion/complete",
            "params": {
                "ref": {"type": "ref/prompt", "name": "completablePrompt"},
                "argument": {"name": "language", "value": "py"}
            }
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"]["completion"]["values"],
        json!(["python", "pytorch"])
    );
    assert_eq!(resp["result"]["completion"]["total"], json!(2));
}

#[tokio::test]
async fn completion_complete_resource_strips_common_prefix_and_matches_substring() {
    let registry = empty_registry();
    registry.register_resource_adapter::<CompletableResource>("completable-resource://{id}");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"completion/complete",
            "params": {
                "ref": {"type": "ref/resource", "uri": "completable-resource://{id}"},
                "argument": {"name": "id", "value": "10"}
            }
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"]["completion"]["values"],
        json!(["100", "101", "102", "310"])
    );
    assert_eq!(resp["result"]["completion"]["total"], json!(4));
}

#[derive(serde::Deserialize)]
struct DynamicCompletableResource {
    dsn: udsn::DSN,
}

#[async_trait]
impl MCPResourceExecutor for DynamicCompletableResource {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        (
            vec![MCPResourceResult::new(self.dsn.to_string(), "dynamic".to_string()).build()],
            None,
        )
    }
    fn serves(dsn: &udsn::DSN) -> bool {
        dsn.protocol == "dynamic-completable"
    }
    fn is_template() -> bool {
        true
    }
}

impl MCPResource for DynamicCompletableResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        /* deliberately sparse: the real instances live in a database somewhere, not here,
         * so the router's automatic meta()-based matching has nothing useful to search */
        vec![MCPMeta::new()
            .name("dynamicCompletable")
            .uri("dynamic-completable://{id}")
            .build()]
    }
    fn params() -> Value {
        Value::Null
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Resource(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
    fn complete(argument_name: &str, partial_value: &str) -> Option<Vec<String>> {
        if argument_name == "id" {
            Some(
                ["live-001", "live-002", "live-100"]
                    .into_iter()
                    .filter(|v| v.contains(partial_value))
                    .map(String::from)
                    .collect(),
            )
        } else {
            None
        }
    }
}

#[tokio::test]
async fn completion_complete_resource_override_takes_precedence_over_automatic_matching() {
    let registry = empty_registry();
    registry.register_resource_adapter::<DynamicCompletableResource>("dynamic-completable://{id}");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"completion/complete",
            "params": {
                "ref": {"type": "ref/resource", "uri": "dynamic-completable://{id}"},
                "argument": {"name": "id", "value": "live-00"}
            }
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"]["completion"]["values"],
        json!(["live-001", "live-002"])
    );
}

#[tokio::test]
async fn completion_complete_unknown_ref_returns_empty() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"completion/complete",
            "params": {
                "ref": {"type": "ref/prompt", "name": "does-not-exist"},
                "argument": {"name": "whatever", "value": "x"}
            }
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["completion"]["values"], json!([]));
    assert_eq!(resp["result"]["completion"]["total"], json!(0));
}
