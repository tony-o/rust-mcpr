use async_trait::async_trait;
use mcp_router::{SinkExt, StreamExt};
use mcp_router::registry::{
    FromArgResult, MCPExecutionResult, MCPExecutionResultStream, MCPMeta, MCPPrompt,
    MCPPromptExecutor, MCPPromptMessage, MCPPromptResult, MCPResource, MCPResourceExecutor,
    MCPResourceResult, MCPTool, MCPToolExecutor, Registry,
};
use mcp_router::router::{Router, RouterResponse};
use mcp_router::stream_channel;
use serde_json::{Value, json};
use std::collections::HashMap;

fn empty_registry() -> Registry {
    Registry::new_from(HashMap::new(), HashMap::new())
}

fn empty_registry_with_prompts() -> Registry {
    Registry::new_from_all(HashMap::new(), HashMap::new(), HashMap::new())
}

fn router_for(registry: &Registry) -> Router<'_> {
    Router::new().registry(registry).build()
}

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
struct ProgressThenFinalTool {}

#[async_trait]
impl MCPToolExecutor for ProgressThenFinalTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let (mut out_tx, out_rx) = stream_channel::<Value>(8);
        let (in_tx, _in_rx) = stream_channel::<Value>(8);
        tokio::spawn(async move {
            out_tx
                .send(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": {"progress": 1, "total": 2}
                }))
                .await
                .ok();
            out_tx
                .send(json!({"content": [{"type": "text", "text": "done"}]}))
                .await
                .ok();
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

impl MCPTool for ProgressThenFinalTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("progressThenFinal").build()]
    }
    fn params() -> Value {
        json!({
            "name": "progressThenFinal",
            "title": "Progress Then Final",
            "description": "streams a progress notification then a final result",
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
struct AbruptCloseTool {}

#[async_trait]
impl MCPToolExecutor for AbruptCloseTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let (mut out_tx, out_rx) = stream_channel::<Value>(8);
        let (in_tx, _in_rx) = stream_channel::<Value>(8);
        tokio::spawn(async move {
            out_tx
                .send(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": {"progress": 1, "total": 2}
                }))
                .await
                .ok();
            // task ends here; out_tx is dropped without ever sending a method-less item
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

impl MCPTool for AbruptCloseTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("abruptClose").build()]
    }
    fn params() -> Value {
        json!({
            "name": "abruptClose",
            "title": "Abrupt Close",
            "description": "closes its stream without ever producing a result",
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
struct SamplingRoundTripTool {}

#[async_trait]
impl MCPToolExecutor for SamplingRoundTripTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let (mut out_tx, out_rx) = stream_channel::<Value>(8);
        let (in_tx, mut in_rx) = stream_channel::<Value>(8);
        tokio::spawn(async move {
            out_tx
                .send(json!({
                    "jsonrpc": "2.0",
                    "id": "sample-1",
                    "method": "sampling/createMessage",
                    "params": {
                        "messages": [{"role": "user", "content": {"type": "text", "text": "summarize this"}}]
                    }
                }))
                .await
                .ok();

            let reply = loop {
                match in_rx.next().await {
                    Some(v) if v.get("id") == Some(&json!("sample-1")) => break v,
                    Some(_) => continue,
                    None => return,
                }
            };
            let text = reply["result"]["content"]["text"]
                .as_str()
                .unwrap_or("")
                .to_string();
            out_tx
                .send(json!({"content": [{"type": "text", "text": text}]}))
                .await
                .ok();
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

impl MCPTool for SamplingRoundTripTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("samplingRoundTrip").build()]
    }
    fn params() -> Value {
        json!({
            "name": "samplingRoundTrip",
            "title": "Sampling Round Trip",
            "description": "asks the client to sample a message mid-call",
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
struct SamplingRoundTripToolB {}

#[async_trait]
impl MCPToolExecutor for SamplingRoundTripToolB {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let (mut out_tx, out_rx) = stream_channel::<Value>(8);
        let (in_tx, mut in_rx) = stream_channel::<Value>(8);
        tokio::spawn(async move {
            out_tx
                .send(json!({
                    "jsonrpc": "2.0",
                    "id": "sample-2",
                    "method": "sampling/createMessage",
                    "params": {
                        "messages": [{"role": "user", "content": {"type": "text", "text": "summarize that instead"}}]
                    }
                }))
                .await
                .ok();

            let reply = loop {
                match in_rx.next().await {
                    Some(v) if v.get("id") == Some(&json!("sample-2")) => break v,
                    Some(_) => continue,
                    None => return,
                }
            };
            let text = reply["result"]["content"]["text"]
                .as_str()
                .unwrap_or("")
                .to_string();
            out_tx
                .send(json!({"content": [{"type": "text", "text": text}]}))
                .await
                .ok();
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

impl MCPTool for SamplingRoundTripToolB {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("samplingRoundTripB").build()]
    }
    fn params() -> Value {
        json!({
            "name": "samplingRoundTripB",
            "title": "Sampling Round Trip B",
            "description": "asks the client to sample a message mid-call, using a distinct correlation id",
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
struct StreamingPrompt {}

#[async_trait]
impl MCPPromptExecutor for StreamingPrompt {
    async fn execute(&self) -> MCPPromptResult {
        let (_out_tx, out_rx) = stream_channel::<Value>(1);
        let (in_tx, _in_rx) = stream_channel::<Value>(1);
        MCPPromptResult {
            description: None,
            messages: vec![
                MCPPromptMessage {
                    role: "user".to_string(),
                    content: MCPExecutionResult::TEXT("kept".into()),
                },
                MCPPromptMessage {
                    role: "assistant".to_string(),
                    content: MCPExecutionResult::STREAM(MCPExecutionResultStream {
                        receiver: out_rx,
                        sender: in_tx,
                    }),
                },
            ],
        }
    }
}

impl MCPPrompt for StreamingPrompt {
    fn get_executor(&self) -> &dyn MCPPromptExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("streamingPrompt").build()]
    }
    fn params() -> Value {
        json!({
            "name": "streamingPrompt",
            "description": "tries (and fails) to stream",
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

#[tokio::test]
async fn tools_call_stream_relays_progress_then_wraps_final_result() {
    let registry = empty_registry();
    registry.register_tool_adapter::<ProgressThenFinalTool>("progressThenFinal");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {"name": "progressThenFinal", "arguments": {}}
        }))
        .await;
    match resp {
        RouterResponse::Stream(mut s) => {
            let first = s
                .receiver
                .next()
                .await
                .expect("expected progress notification");
            assert_eq!(
                first,
                json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": {"progress": 1, "total": 2}
                })
            );
            let second = s.receiver.next().await.expect("expected final result");
            assert_eq!(
                second,
                json!({
                    "jsonrpc": "2.0",
                    "id": 7,
                    "result": {"content": [{"type": "text", "text": "done"}]}
                })
            );
            assert!(s.receiver.next().await.is_none());
        }
        RouterResponse::Value(v) => panic!("expected a stream response, got {:?}", v),
    }
}

#[tokio::test]
async fn tools_call_stream_ending_without_final_result_synthesizes_error() {
    let registry = empty_registry();
    registry.register_tool_adapter::<AbruptCloseTool>("abruptClose");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {"name": "abruptClose", "arguments": {}}
        }))
        .await;
    match resp {
        RouterResponse::Stream(mut s) => {
            let notif = s
                .receiver
                .next()
                .await
                .expect("expected progress notification");
            assert_eq!(notif["method"], "notifications/progress");
            let err = s.receiver.next().await.expect("expected synthesized error");
            assert_eq!(
                err,
                json!({
                    "jsonrpc": "2.0",
                    "id": 9,
                    "error": {"code": -32603, "message": "stream ended without a result"}
                })
            );
            assert!(s.receiver.next().await.is_none());
        }
        RouterResponse::Value(v) => panic!("expected a stream response, got {:?}", v),
    }
}

#[tokio::test]
async fn tools_call_stream_supports_bidirectional_sampling_round_trip() {
    let registry = empty_registry();
    registry.register_tool_adapter::<SamplingRoundTripTool>("samplingRoundTrip");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {"name": "samplingRoundTrip", "arguments": {}}
        }))
        .await;
    match resp {
        RouterResponse::Stream(mut s) => {
            let req = s.receiver.next().await.expect("expected sampling request");
            assert_eq!(req["method"], "sampling/createMessage");
            assert_eq!(req["id"], "sample-1");

            s.sender
                .send(json!({"id": "sample-1", "result": {"content": {"text": "hello"}}}))
                .await;

            let final_msg = s.receiver.next().await.expect("expected final result");
            assert_eq!(
                final_msg,
                json!({
                    "jsonrpc": "2.0",
                    "id": 11,
                    "result": {"content": [{"type": "text", "text": "hello"}]}
                })
            );
            assert!(s.receiver.next().await.is_none());
        }
        RouterResponse::Value(v) => panic!("expected a stream response, got {:?}", v),
    }
}

#[tokio::test]
async fn prompts_get_drops_stream_content_and_keeps_normal_messages() {
    let registry = empty_registry_with_prompts();
    registry.register_prompt_adapter::<StreamingPrompt>("streamingPrompt");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "prompts/get",
            "params": {"name": "streamingPrompt", "arguments": {}}
        }))
        .await;
    match resp {
        RouterResponse::Value(v) => {
            let messages = v["result"]["messages"].as_array().unwrap();
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0]["content"]["text"], "kept");
        }
        RouterResponse::Stream(_) => panic!("prompts/get should never elevate to a stream"),
    }
}

#[tokio::test]
async fn batch_with_one_streaming_and_one_immediate_item_elevates_and_merges_both() {
    let registry = empty_registry();
    registry.register_tool_adapter::<NoopTool>("noop");
    registry.register_tool_adapter::<ProgressThenFinalTool>("progressThenFinal");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!([
            {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "noop", "arguments": {}}},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "progressThenFinal", "arguments": {}}}
        ]))
        .await;
    match resp {
        RouterResponse::Stream(mut s) => {
            let mut items = Vec::new();
            while let Some(item) = s.receiver.next().await {
                items.push(item);
            }
            assert_eq!(items.len(), 3);
            assert!(
                items
                    .iter()
                    .any(|i| i["id"] == 1 && i["result"]["content"][0]["text"] == "noop")
            );
            assert!(items.iter().any(|i| i["method"] == "notifications/progress"));
            assert!(
                items
                    .iter()
                    .any(|i| i["id"] == 2 && i["result"]["content"][0]["text"] == "done")
            );
        }
        RouterResponse::Value(v) => panic!("expected batch to elevate to a stream, got {:?}", v),
    }
}

#[tokio::test]
async fn batch_with_two_streaming_items_elevates_and_merges_until_both_finalize() {
    let registry = empty_registry();
    registry.register_tool_adapter::<ProgressThenFinalTool>("progressThenFinalA");
    registry.register_tool_adapter::<ProgressThenFinalTool>("progressThenFinalB");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!([
            {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "progressThenFinalA", "arguments": {}}},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "progressThenFinalB", "arguments": {}}}
        ]))
        .await;
    match resp {
        RouterResponse::Stream(mut s) => {
            let mut items = Vec::new();
            while let Some(item) = s.receiver.next().await {
                items.push(item);
            }
            assert_eq!(items.len(), 4);
            assert_eq!(
                items
                    .iter()
                    .filter(|i| i["method"] == "notifications/progress")
                    .count(),
                2
            );
            assert!(
                items
                    .iter()
                    .any(|i| i["id"] == 1 && i["result"]["content"][0]["text"] == "done")
            );
            assert!(
                items
                    .iter()
                    .any(|i| i["id"] == 2 && i["result"]["content"][0]["text"] == "done")
            );
        }
        RouterResponse::Value(v) => panic!("expected batch to elevate to a stream, got {:?}", v),
    }
}

#[derive(serde::Deserialize)]
struct TextContentResource {
    dsn: udsn::DSN,
}

#[async_trait]
impl MCPResourceExecutor for TextContentResource {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        (
            vec![
                MCPResourceResult::new(self.dsn.to_string(), "greeting".to_string())
                    .mime_type("text/plain")
                    .text("hello from a resource"),
            ],
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

impl MCPResource for TextContentResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("textContentResource").build()]
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
struct StreamingResource {
    #[allow(dead_code)]
    dsn: udsn::DSN,
}

#[async_trait]
impl MCPResourceExecutor for StreamingResource {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        let (mut out_tx, out_rx) = stream_channel::<Value>(8);
        let (in_tx, _in_rx) = stream_channel::<Value>(8);
        tokio::spawn(async move {
            out_tx
                .send(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/resources/updated",
                    "params": {"uri": "streaming-resource://fixed"}
                }))
                .await
                .ok();
            out_tx
                .send(json!({"contents": [{"uri": "streaming-resource://fixed", "text": "final snapshot"}]}))
                .await
                .ok();
        });
        (
            vec![MCPResourceResult::STREAM(MCPExecutionResultStream {
                receiver: out_rx,
                sender: in_tx,
            })],
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

impl MCPResource for StreamingResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("streamingResource").build()]
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
async fn resources_read_text_variant_renders_flat_contents_with_no_type_tag() {
    let registry = empty_registry();
    registry.register_resource_adapter::<TextContentResource>("text-resource://fixed");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {"uri": "text-resource://fixed"}
        }))
        .await;
    let resp = match resp {
        RouterResponse::Value(v) => v,
        RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
    };
    assert_eq!(
        resp["result"]["contents"],
        json!([
            {
                "uri": "text-resource://fixed",
                "name": "greeting",
                "mimeType": "text/plain",
                "text": "hello from a resource"
            }
        ])
    );
}

#[tokio::test]
async fn resources_read_stream_relays_update_then_wraps_final_contents() {
    let registry = empty_registry();
    registry.register_resource_adapter::<StreamingResource>("streaming-resource://fixed");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "resources/read",
            "params": {"uri": "streaming-resource://fixed"}
        }))
        .await;
    match resp {
        RouterResponse::Stream(mut s) => {
            let first = s
                .receiver
                .next()
                .await
                .expect("expected a resources/updated notification");
            assert_eq!(
                first,
                json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/resources/updated",
                    "params": {"uri": "streaming-resource://fixed"}
                })
            );
            let second = s.receiver.next().await.expect("expected final contents");
            assert_eq!(
                second,
                json!({
                    "jsonrpc": "2.0",
                    "id": 13,
                    "result": {
                        "contents": [{"uri": "streaming-resource://fixed", "text": "final snapshot"}]
                    }
                })
            );
            assert!(s.receiver.next().await.is_none());
        }
        RouterResponse::Value(v) => panic!("expected a stream response, got {:?}", v),
    }
}

#[tokio::test]
async fn batch_elevation_does_not_drop_other_immediate_items() {
    let registry = empty_registry();
    registry.register_tool_adapter::<NoopTool>("noopA");
    registry.register_tool_adapter::<NoopTool>("noopB");
    registry.register_tool_adapter::<NoopTool>("noopC");
    registry.register_tool_adapter::<ProgressThenFinalTool>("progressThenFinal");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!([
            {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "noopA", "arguments": {}}},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "noopB", "arguments": {}}},
            {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "progressThenFinal", "arguments": {}}},
            {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "noopC", "arguments": {}}}
        ]))
        .await;
    match resp {
        RouterResponse::Stream(mut s) => {
            let mut items = Vec::new();
            while let Some(item) = s.receiver.next().await {
                items.push(item);
            }
            // 3 immediate noop responses + 1 progress notification + 1 wrapped final = 5,
            // none of the immediate items should be dropped by elevation.
            assert_eq!(items.len(), 5);
            for id in [1, 2, 4] {
                assert!(
                    items
                        .iter()
                        .any(|i| i["id"] == id && i["result"]["content"][0]["text"] == "noop"),
                    "missing immediate response for id {id}"
                );
            }
            assert!(items.iter().any(|i| i["method"] == "notifications/progress"));
            assert!(
                items
                    .iter()
                    .any(|i| i["id"] == 3 && i["result"]["content"][0]["text"] == "done")
            );
        }
        RouterResponse::Value(v) => panic!("expected batch to elevate to a stream, got {:?}", v),
    }
}

#[tokio::test]
async fn batch_elevation_broadcasts_replies_to_all_concurrently_streaming_tools() {
    let registry = empty_registry();
    registry.register_tool_adapter::<SamplingRoundTripTool>("samplingRoundTrip");
    registry.register_tool_adapter::<SamplingRoundTripToolB>("samplingRoundTripB");
    let router = router_for(&registry);
    let resp = router
        .exec_from_value(json!([
            {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "samplingRoundTrip", "arguments": {}}},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "samplingRoundTripB", "arguments": {}}}
        ]))
        .await;
    match resp {
        RouterResponse::Stream(mut s) => {
            let mut requests = Vec::new();
            while requests.len() < 2 {
                let item = s.receiver.next().await.expect("expected both sampling requests");
                assert_eq!(item["method"], "sampling/createMessage");
                requests.push(item);
            }

            // Reply to both correlation ids through the single elevated sender -- it should
            // broadcast each reply to every concurrently-streaming tool, and each tool filters
            // for its own id, so both replies land correctly with no cross-talk.
            s.sender
                .send(json!({"id": "sample-1", "result": {"content": {"text": "first"}}}))
                .await;
            s.sender
                .send(json!({"id": "sample-2", "result": {"content": {"text": "second"}}}))
                .await;

            let mut finals = Vec::new();
            while let Some(item) = s.receiver.next().await {
                finals.push(item);
            }
            assert_eq!(finals.len(), 2);
            assert!(
                finals
                    .iter()
                    .any(|i| i["id"] == 1 && i["result"]["content"][0]["text"] == "first")
            );
            assert!(
                finals
                    .iter()
                    .any(|i| i["id"] == 2 && i["result"]["content"][0]["text"] == "second")
            );
        }
        RouterResponse::Value(v) => panic!("expected batch to elevate to a stream, got {:?}", v),
    }
}
