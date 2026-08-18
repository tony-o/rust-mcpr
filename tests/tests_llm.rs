use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use mcp_router::registry::{
    FromArgResult, Info, InfoType, MCPExecutionResult, MCPExecutionResultAnnotations,
    MCPExecutionResultAudio, MCPExecutionResultImage, MCPMeta, MCPResource, MCPResourceExecutor,
    MCPResourceResult, MCPTool, MCPToolExecutor, Registry,
};
use mcp_router::router::{Router, RouterResponse};
use serde_json::{Value, json};
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

macro_rules! define_tagged_tool {
    ($ident:ident, $tag:expr) => {
        #[derive(serde::Deserialize)]
        struct $ident {}

        #[async_trait]
        impl MCPToolExecutor for $ident {
            async fn execute(
                &self,
                _cursor: Option<String>,
            ) -> (Vec<MCPExecutionResult>, Option<String>) {
                (vec![MCPExecutionResult::TEXT($tag.into())], None)
            }
        }

        impl MCPTool for $ident {
            fn get_executor(&self) -> &dyn MCPToolExecutor {
                self
            }
            fn meta() -> Vec<MCPMeta> {
                vec![MCPMeta::new().name($tag).build()]
            }
            fn params() -> Value {
                json!({
                    "name": $tag,
                    "title": $tag,
                    "description": $tag,
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
    };
}

define_tagged_tool!(ToolAlpha, "alpha");
define_tagged_tool!(ToolBeta, "beta");
define_tagged_tool!(ToolGamma, "gamma");

#[derive(serde::Deserialize)]
struct EchoArgsTool {
    value: String,
}

#[async_trait]
impl MCPToolExecutor for EchoArgsTool {
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        (
            vec![MCPExecutionResult::TEXT(
                format!("value={},cursor={:?}", self.value, cursor).into()
            )],
            None,
        )
    }
}

impl MCPTool for EchoArgsTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("echoArgs").build()]
    }
    fn params() -> Value {
        json!({
            "name": "echoArgs",
            "title": "Echo Args",
            "description": "echoes back its value argument",
            "inputSchema": {
                "type": "object",
                "properties": {"value": {"type": "string"}, "cursor": {"type": "string"}},
                "required": ["value"]
            }
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
struct AllContentTypesTool {}

#[async_trait]
impl MCPToolExecutor for AllContentTypesTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        (
            vec![
                MCPExecutionResult::TEXT("hello".into()),
                MCPExecutionResult::IMAGE(MCPExecutionResultImage {
                    mime_type: "image/png".to_string(),
                    data: vec![10, 20, 30],
                    annotations: None,
                }),
                MCPExecutionResult::AUDIO(MCPExecutionResultAudio {
                    mime_type: "audio/wav".to_string(),
                    data: vec![1, 2, 3],
                    annotations: Some(MCPExecutionResultAnnotations {
                        audience: vec!["user".to_string()],
                        priority: 0.5,
                    }),
                }),
                MCPExecutionResult::AUDIO(MCPExecutionResultAudio {
                    mime_type: "audio/wav".to_string(),
                    data: vec![9, 9],
                    annotations: None,
                }),
                MCPExecutionResult::RAW(json!({"custom": "passthrough"})),
                MCPExecutionResult::RESOURCE(
                    MCPResourceResult::new("res://x".to_string(), "resname".to_string()).build(),
                ),
            ],
            None,
        )
    }
}

impl MCPTool for AllContentTypesTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("allContentTypes").build()]
    }
    fn params() -> Value {
        json!({
            "name": "allContentTypes",
            "title": "All Content Types",
            "description": "returns one of every content block variant",
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
struct AlwaysErrorsTool {}

#[async_trait]
impl MCPToolExecutor for AlwaysErrorsTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        (
            vec![
                MCPExecutionResult::TEXT("before error".into()),
                MCPExecutionResult::ERROR(("boom".to_string(), None)),
            ],
            None,
        )
    }
}

impl MCPTool for AlwaysErrorsTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("alwaysErrors").build()]
    }
    fn params() -> Value {
        json!({
            "name": "alwaysErrors",
            "title": "Always Errors",
            "description": "always returns an error content block",
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
struct PagedCountingTool {}

const PAGED_COUNTING_TOTAL: usize = 7;
const PAGED_COUNTING_PAGE: usize = 3;

#[async_trait]
impl MCPToolExecutor for PagedCountingTool {
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let offset: usize = cursor.and_then(|c| c.parse().ok()).unwrap_or(0);
        let end = (offset + PAGED_COUNTING_PAGE).min(PAGED_COUNTING_TOTAL);
        let items: Vec<MCPExecutionResult> = (offset..end)
            .map(|i| MCPExecutionResult::TEXT(format!("item-{}", i).into()))
            .collect();
        let next = if end < PAGED_COUNTING_TOTAL {
            Some(end.to_string())
        } else {
            None
        };
        (items, next)
    }
}

impl MCPTool for PagedCountingTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("pagedCounting").build()]
    }
    fn params() -> Value {
        json!({
            "name": "pagedCounting",
            "title": "Paged Counting",
            "description": "returns numbered items across pages, cursor-driven by the tool itself",
            "inputSchema": {
                "type": "object",
                "properties": {"cursor": {"type": "string"}},
                "required": []
            }
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
struct DummyResourceForMisuse {
    #[allow(dead_code)]
    dsn: udsn::DSN,
}

#[async_trait]
impl MCPResourceExecutor for DummyResourceForMisuse {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        (vec![], None)
    }
    fn serves(_dsn: &udsn::DSN) -> bool {
        false
    }
    fn is_template() -> bool {
        false
    }
}

impl MCPResource for DummyResourceForMisuse {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("dummy").uri("dummy://x").build()]
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

static MISCONFIGURED_INFO: Info = Info {
    name: "misconfigured",
    info_type: InfoType::Resource,
    params: DummyResourceForMisuse::params,
    from_args: DummyResourceForMisuse::from_args,
    meta: DummyResourceForMisuse::meta,
    is_template: DummyResourceForMisuse::is_template,
    serves: DummyResourceForMisuse::serves,
    complete: |_, _| None,
};

#[derive(serde::Deserialize)]
struct SingleResource {
    dsn: udsn::DSN,
}

#[async_trait]
impl MCPResourceExecutor for SingleResource {
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        (
            vec![
                MCPResourceResult::new(self.dsn.to_string(), "single".to_string())
                    .text(&format!("cursor-received={:?}", cursor)),
            ],
            Some("next-page-token".to_string()),
        )
    }
    fn serves(_dsn: &udsn::DSN) -> bool {
        false
    }
    fn is_template() -> bool {
        false
    }
}

impl MCPResource for SingleResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![
            MCPMeta::new()
                .name("single")
                .uri("single-resource://fixed")
                .build(),
        ]
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

#[derive(serde::Deserialize)]
struct TemplateResource {
    dsn: udsn::DSN,
}

#[async_trait]
impl MCPResourceExecutor for TemplateResource {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        (
            vec![
                MCPResourceResult::new(self.dsn.to_string(), "templated-match".to_string())
                    .build(),
            ],
            None,
        )
    }
    fn serves(dsn: &udsn::DSN) -> bool {
        dsn.protocol == "templated"
    }
    fn is_template() -> bool {
        true
    }
}

impl MCPResource for TemplateResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![
            MCPMeta::new()
                .name("templated")
                .uri("templated://{id}")
                .build(),
        ]
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

#[derive(serde::Deserialize)]
struct ManyResources {
    dsn: udsn::DSN,
}

const MANY_RESOURCES_COUNT: usize = 45;

#[async_trait]
impl MCPResourceExecutor for ManyResources {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        (
            vec![MCPResourceResult::new(self.dsn.to_string(), "many".to_string()).build()],
            None,
        )
    }
    fn serves(dsn: &udsn::DSN) -> bool {
        dsn.protocol == "many-resource"
    }
    fn is_template() -> bool {
        false
    }
}

impl MCPResource for ManyResources {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        (0..MANY_RESOURCES_COUNT)
            .map(|i| {
                MCPMeta::new()
                    .name(&format!("item{:03}", i))
                    .uri(&format!("many-resource://{:03}", i))
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

#[derive(serde::Deserialize)]
struct ManyTemplates {
    dsn: udsn::DSN,
}

const MANY_TEMPLATES_COUNT: usize = 12;

#[async_trait]
impl MCPResourceExecutor for ManyTemplates {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        (
            vec![MCPResourceResult::new(self.dsn.to_string(), "template".to_string()).build()],
            None,
        )
    }
    fn serves(dsn: &udsn::DSN) -> bool {
        dsn.protocol == "many-template"
    }
    fn is_template() -> bool {
        true
    }
}

impl MCPResource for ManyTemplates {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        (0..MANY_TEMPLATES_COUNT)
            .map(|i| {
                MCPMeta::new()
                    .name(&format!("tmpl{:03}", i))
                    .uri(&format!("many-template://{:03}/{{id}}", i))
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

fn empty_registry() -> Registry {
    Registry::new_from(HashMap::new(), HashMap::new())
}

fn router_for(registry: &Registry) -> Router<'_> {
    Router::new().registry(registry).build()
}

fn router_for_paged(registry: &Registry, page_size: usize) -> Router<'_> {
    Router::new()
        .registry(registry)
        .page_size(page_size)
        .build()
}

// --- JSON-RPC envelope ---

#[tokio::test]
async fn parse_error_on_malformed_request() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"totally": "not a valid request"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp,
        json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {
                "code": -32700,
                "message": "invalid request format, expected {jsonrpc:string, id:number|string, method:string, params:optional<object>}"
            }
        })
    );
}

#[tokio::test]
async fn method_not_found_for_unknown_method() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"totally/unknown","params":{}}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {"code": -32601, "message": "method not found: totally/unknown"}
        })
    );
}

#[tokio::test]
async fn numeric_id_roundtrips_as_number() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":123,"method":"tools/list","params":{}}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["id"], json!(123));
}

#[tokio::test]
async fn string_id_roundtrips_as_string() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":"abc-123","method":"tools/list","params":{}}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["id"], json!("abc-123"));
}

// Per JSON-RPC 2.0, a response's "jsonrpc" member MUST be exactly "2.0" regardless of
// what the client sent. This may currently fail since the router echoes req.jsonrpc verbatim.
#[tokio::test]
async fn response_jsonrpc_version_should_always_be_2_0_per_spec() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"1.0","id":1,"method":"tools/list","params":{}}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["jsonrpc"], json!("2.0"));
}

// Per JSON-RPC 2.0, a Request object with no "id" member is a Notification, and the server
// MUST NOT reply. This may currently fail since only "notifications/initialized" is special-cased
// by method name, rather than "no id" being handled generically for any method.
#[tokio::test]
async fn absent_id_is_treated_as_notification_per_spec_for_any_method() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","method":"tools/list","params":{}}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp, Value::Null);
}

// --- initialize ---

#[tokio::test]
async fn initialize_default_protocol_version_when_omitted() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["protocolVersion"], json!("2025-11-25"));
}

#[tokio::test]
async fn initialize_echoes_provided_protocol_version() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params": {"protocolVersion": "custom-version-xyz"}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"]["protocolVersion"],
        json!("custom-version-xyz")
    );
}

#[tokio::test]
async fn initialize_capabilities_empty_when_no_tools_or_resources() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["capabilities"], json!({}));
}

#[tokio::test]
async fn initialize_capabilities_present_when_tools_and_resources_registered() {
    let registry = empty_registry();
    registry.register_tool_adapter::<NoopTool>("noop");
    registry.register_resource_adapter::<SingleResource>("single-resource://fixed");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"]["capabilities"],
        json!({"tools": {}, "resources": {}})
    );
}

#[tokio::test]
async fn initialize_server_info_omits_absent_optional_fields() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"]["serverInfo"],
        json!({"name": "Example MCP Server", "version": "1.0.0"})
    );
}

// --- notifications/initialized ---

#[tokio::test]
async fn notifications_initialized_yields_no_response() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp, Value::Null);
}

// --- tools/list ---

#[tokio::test]
async fn tools_list_empty_registry_returns_empty_array_no_cursor() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["tools"], json!([]));
    assert!(resp["result"].get("nextCursor").is_none());
}

#[tokio::test]
async fn tools_list_returns_all_when_under_page_size() {
    let registry = empty_registry();
    registry.register_tool_adapter::<NoopTool>("a");
    registry.register_tool_adapter::<NoopTool>("b");
    let router = router_for_paged(&registry, 50);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["tools"].as_array().unwrap().len(), 2);
    assert!(resp["result"].get("nextCursor").is_none());
}

#[tokio::test]
async fn tools_list_paginates_and_chains_cursors_correctly() {
    let registry = empty_registry();
    for i in 0..5 {
        registry.register_tool_adapter::<NoopTool>(&format!("tool{:03}", i));
    }
    let router = router_for_paged(&registry, 2);

    let resp1 = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .await;
    let resp1 = match resp1 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp1["result"]["tools"].as_array().unwrap().len(), 2);
    let cursor1 = resp1["result"]["nextCursor"].as_str().unwrap().to_string();

    let resp2 = router
        .exec_from_value(
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"cursor":cursor1}}),
        )
        .await;
    let resp2 = match resp2 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp2["result"]["tools"].as_array().unwrap().len(), 2);
    let cursor2 = resp2["result"]["nextCursor"].as_str().unwrap().to_string();

    let resp3 = router
        .exec_from_value(
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{"cursor":cursor2}}),
        )
        .await;
    let resp3 = match resp3 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp3["result"]["tools"].as_array().unwrap().len(), 1);
    assert!(resp3["result"].get("nextCursor").is_none());
}

#[tokio::test]
async fn tools_list_cursor_beyond_end_returns_empty_page() {
    let registry = empty_registry();
    registry.register_tool_adapter::<NoopTool>("a");
    let router = router_for_paged(&registry, 50);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/list",
            "params": {"cursor": general_purpose::STANDARD.encode("9999")}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["tools"], json!([]));
    assert!(resp["result"].get("nextCursor").is_none());
}

#[tokio::test]
async fn tools_list_invalid_cursor_falls_back_to_first_page() {
    let registry = empty_registry();
    registry.register_tool_adapter::<NoopTool>("a");
    registry.register_tool_adapter::<NoopTool>("b");
    let router = router_for_paged(&registry, 50);

    let baseline = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .await;
    let baseline = match baseline {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    let with_garbage_cursor = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":2,"method":"tools/list",
            "params": {"cursor": "not-a-valid-cursor!!"}
        }))
        .await;
    let with_garbage_cursor = match with_garbage_cursor {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        baseline["result"]["tools"],
        with_garbage_cursor["result"]["tools"]
    );
}

#[tokio::test]
async fn tools_list_entries_sorted_by_registered_name_not_insertion_order() {
    let registry = empty_registry();
    registry.register_tool_adapter::<ToolAlpha>("keyC");
    registry.register_tool_adapter::<ToolGamma>("keyA");
    registry.register_tool_adapter::<ToolBeta>("keyB");
    let router = router_for_paged(&registry, 50);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    let names: Vec<String> = resp["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["gamma", "beta", "alpha"]);
}

#[tokio::test]
async fn tools_list_returns_tool_params_verbatim() {
    let registry = empty_registry();
    registry.register_tool_adapter::<EchoArgsTool>("echoArgs");
    let router = router_for_paged(&registry, 50);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["result"]["tools"], json!([EchoArgsTool::params()]));
}

// --- tools/call ---

#[tokio::test]
async fn tools_call_unknown_tool_name_returns_invalid_params_error() {
    let registry = empty_registry();
    registry.register_tool_adapter::<NoopTool>("noop");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params": {"name": "does-not-exist", "arguments": {}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp,
        json!({
            "jsonrpc":"2.0","id":1,
            "error": {"code": -32602, "message": "invalid parameters for tools/call, unknown tool: does-not-exist"}
        })
    );
}

#[tokio::test]
async fn tools_call_malformed_arguments_return_invalid_params_error() {
    let registry = empty_registry();
    registry.register_tool_adapter::<EchoArgsTool>("echoArgs");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params": {"name": "echoArgs", "arguments": {}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp["error"]["code"], json!(-32602));
    assert!(
        resp["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("invalid parameters for tools/call ")
    );
}

#[tokio::test]
async fn tools_call_cursor_round_trip_across_calls() {
    let registry = empty_registry();
    registry.register_tool_adapter::<PagedCountingTool>("pagedCounting");
    let router = router_for(&registry);

    let resp1 = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params": {"name": "pagedCounting", "arguments": {}}
        }))
        .await;
    let resp1 = match resp1 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp1["result"]["content"],
        json!([
            {"type":"text","text":"item-0"},
            {"type":"text","text":"item-1"},
            {"type":"text","text":"item-2"}
        ])
    );
    let cursor1 = resp1["result"]["nextCursor"].as_str().unwrap().to_string();
    assert_eq!(cursor1, "3");

    let resp2 = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params": {"name": "pagedCounting", "arguments": {"cursor": cursor1}}
        }))
        .await;
    let resp2 = match resp2 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp2["result"]["content"],
        json!([
            {"type":"text","text":"item-3"},
            {"type":"text","text":"item-4"},
            {"type":"text","text":"item-5"}
        ])
    );
    let cursor2 = resp2["result"]["nextCursor"].as_str().unwrap().to_string();
    assert_eq!(cursor2, "6");

    let resp3 = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params": {"name": "pagedCounting", "arguments": {"cursor": cursor2}}
        }))
        .await;
    let resp3 = match resp3 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp3["result"]["content"],
        json!([{"type":"text","text":"item-6"}])
    );
    assert!(resp3["result"].get("nextCursor").is_none());
}

#[tokio::test]
async fn tools_call_all_content_block_shapes() {
    let registry = empty_registry();
    registry.register_tool_adapter::<AllContentTypesTool>("allContentTypes");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params": {"name": "allContentTypes", "arguments": {}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };

    let img_b64 = general_purpose::STANDARD.encode([10u8, 20, 30]);
    let audio_b64_1 = general_purpose::STANDARD.encode([1u8, 2, 3]);
    let audio_b64_2 = general_purpose::STANDARD.encode([9u8, 9]);

    assert_eq!(
        resp["result"]["content"],
        json!([
            {"type":"text","text":"hello"},
            {"type":"image","data": img_b64, "mimeType":"image/png"},
            {"type":"audio","data": audio_b64_1, "mimeType":"audio/wav","annotations":{"audience":["user"],"priority":0.5}},
            {"type":"audio","data": audio_b64_2, "mimeType":"audio/wav"},
            {"custom":"passthrough"},
            {"uri":"res://x","name":"resname","type":"resource_link"}
        ])
    );
}

#[tokio::test]
async fn tools_call_error_content_sets_is_error_flag_alongside_text() {
    let registry = empty_registry();
    registry.register_tool_adapter::<AlwaysErrorsTool>("alwaysErrors");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params": {"name": "alwaysErrors", "arguments": {}}
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
                {"type":"text","text":"before error"},
                {"type":"text","text":"error: boom"}
            ],
            "isError": true
        })
    );
}

#[tokio::test]
async fn tools_call_resource_registered_as_tool_is_rejected_as_misconfigured() {
    let mut tools = HashMap::new();
    tools.insert("misconfigured".to_string(), &MISCONFIGURED_INFO);
    let registry = Registry::new_from(tools, HashMap::new());
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"tools/call",
            "params": {"name": "misconfigured", "arguments": {"dsn": "misconfigured://thing"}}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp,
        json!({
            "jsonrpc":"2.0","id":1,
            "error": {"code": -32600, "message": "server is misconfigured, a resource was registered as a tool"}
        })
    );
}

// --- resources/list ---

#[tokio::test]
async fn resources_list_excludes_template_resources() {
    let registry = empty_registry();
    registry.register_resource_adapter::<SingleResource>("single-resource://fixed");
    registry.register_resource_adapter::<TemplateResource>("templated://{id}");
    let router = router_for_paged(&registry, 50);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    let uris: Vec<String> = resp["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(uris, vec!["single-resource://fixed"]);
}

#[tokio::test]
async fn resources_list_flattens_multi_meta_resource_and_paginates() {
    let registry = empty_registry();
    registry.register_resource_adapter::<ManyResources>("many-resource://");
    let router = router_for_paged(&registry, 20);

    let resp1 = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}))
        .await;
    let resp1 = match resp1 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp1["result"]["resources"].as_array().unwrap().len(), 20);
    let cursor1 = resp1["result"]["nextCursor"].as_str().unwrap().to_string();

    let resp2 = router
        .exec_from_value(
            json!({"jsonrpc":"2.0","id":2,"method":"resources/list","params":{"cursor":cursor1}}),
        )
        .await;
    let resp2 = match resp2 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp2["result"]["resources"].as_array().unwrap().len(), 20);
    let cursor2 = resp2["result"]["nextCursor"].as_str().unwrap().to_string();

    let resp3 = router
        .exec_from_value(
            json!({"jsonrpc":"2.0","id":3,"method":"resources/list","params":{"cursor":cursor2}}),
        )
        .await;
    let resp3 = match resp3 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(resp3["result"]["resources"].as_array().unwrap().len(), 5);
    assert!(resp3["result"].get("nextCursor").is_none());

    let mut all_uris: Vec<String> = Vec::new();
    for resp in [&resp1, &resp2, &resp3] {
        for r in resp["result"]["resources"].as_array().unwrap() {
            all_uris.push(r["uri"].as_str().unwrap().to_string());
        }
    }
    assert_eq!(all_uris.len(), MANY_RESOURCES_COUNT);
    let mut sorted = all_uris.clone();
    sorted.sort();
    assert_eq!(all_uris, sorted);
}

// --- resources/templates/list ---

#[tokio::test]
async fn resources_templates_list_includes_only_template_resources() {
    let registry = empty_registry();
    registry.register_resource_adapter::<SingleResource>("single-resource://fixed");
    registry.register_resource_adapter::<TemplateResource>("templated://{id}");
    let router = router_for_paged(&registry, 50);
    let resp = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"resources/templates/list"}))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    let names: Vec<String> = resp["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["templated"]);
}

#[tokio::test]
async fn resources_templates_list_paginates_and_sorts_by_uri_template() {
    let registry = empty_registry();
    registry.register_resource_adapter::<ManyTemplates>("many-template://");
    let router = router_for_paged(&registry, 5);

    let resp1 = router
        .exec_from_value(json!({"jsonrpc":"2.0","id":1,"method":"resources/templates/list"}))
        .await;
    let resp1 = match resp1 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp1["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    let cursor1 = resp1["result"]["nextCursor"].as_str().unwrap().to_string();

    let resp2 = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":2,"method":"resources/templates/list",
            "params":{"cursor":cursor1}
        }))
        .await;
    let resp2 = match resp2 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp2["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    let cursor2 = resp2["result"]["nextCursor"].as_str().unwrap().to_string();

    let resp3 = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":3,"method":"resources/templates/list",
            "params":{"cursor":cursor2}
        }))
        .await;
    let resp3 = match resp3 {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp3["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(resp3["result"].get("nextCursor").is_none());
}

// --- resources/read ---

#[tokio::test]
async fn resources_read_exact_match_forwards_cursor_and_surfaces_next_cursor() {
    let registry = empty_registry();
    registry.register_resource_adapter::<SingleResource>("single-resource://fixed");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"resources/read",
            "params": {"uri": "single-resource://fixed", "cursor": "abc"}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp,
        json!({
            "jsonrpc":"2.0","id":1,
            "result": {
                "contents": [{
                    "uri": "single-resource://fixed",
                    "name": "single",
                    "text": "cursor-received=Some(\"abc\")"
                }],
                "nextCursor": "next-page-token"
            }
        })
    );
}

#[tokio::test]
async fn resources_read_content_key_is_contents_not_content() {
    let registry = empty_registry();
    registry.register_resource_adapter::<SingleResource>("single-resource://fixed");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"resources/read",
            "params": {"uri": "single-resource://fixed"}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert!(resp["result"].get("contents").is_some());
    assert!(resp["result"].get("content").is_none());
}

#[tokio::test]
async fn resources_read_falls_back_to_template_match_via_serves() {
    let registry = empty_registry();
    registry.register_resource_adapter::<TemplateResource>("templated://{id}");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"resources/read",
            "params": {"uri": "templated://99"}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"]["contents"],
        json!([{"uri": "templated://99", "name": "templated-match", "type": "resource_link"}])
    );
    assert!(resp["result"].get("nextCursor").is_none());
}

#[tokio::test]
async fn resources_read_unknown_uri_returns_no_handler_error() {
    let registry = empty_registry();
    registry.register_resource_adapter::<SingleResource>("single-resource://fixed");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"resources/read",
            "params": {"uri": "totally-unknown://thing"}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp,
        json!({
            "jsonrpc":"2.0","id":1,
            "error": {"code": -32602, "message": "no valid resource handler found for requested uri"}
        })
    );
}

#[tokio::test]
async fn resources_read_uri_missing_scheme_delimiter_returns_invalid_params_error() {
    let registry = empty_registry();
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"resources/read",
            "params": {"uri": "not-a-uri-at-all"}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp,
        json!({
            "jsonrpc":"2.0","id":1,
            "error": {"code": -32602, "message": "malformed request, expected uri in params"}
        })
    );
}
