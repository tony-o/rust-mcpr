use base64::{Engine as _, engine::general_purpose};
use futures_util::stream::StreamExt;

const DEFAULT_PAGE_SIZE: usize = 50;

fn decode_cursor(cursor: &Option<String>) -> usize {
    cursor
        .as_ref()
        .and_then(|c| general_purpose::STANDARD.decode(c).ok())
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0)
}

fn encode_cursor(offset: usize) -> String {
    general_purpose::STANDARD.encode(offset.to_string())
}

fn extract_cursor(params: Option<&serde_json::Value>) -> Option<String> {
    params?.get("cursor")?.as_str().map(String::from)
}

fn strip_common_prefix(values: &[String]) -> Vec<String> {
    let prefix_len = match values.split_first() {
        Some((first, rest)) => rest.iter().fold(first.chars().count(), |acc, v| {
            first
                .chars()
                .zip(v.chars())
                .take_while(|(a, b)| a == b)
                .count()
                .min(acc)
        }),
        None => 0,
    };
    values
        .iter()
        .map(|v| v.chars().skip(prefix_len).collect())
        .collect()
}

fn paginate<T: Clone>(
    items: &[T],
    cursor: &Option<String>,
    page_size: usize,
) -> (Vec<T>, Option<String>) {
    let offset = decode_cursor(cursor);
    if offset >= items.len() {
        return (Vec::new(), None);
    }
    let end = (offset + page_size).min(items.len());
    let next = if end < items.len() {
        Some(encode_cursor(end))
    } else {
        None
    };
    (items[offset..end].to_vec(), next)
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
#[serde(untagged)]
enum RequestID {
    Str(String),
    Number(i64),
    Null,
    #[default]
    Absent,
}

/// A parsed JSON-RPC request, as accepted by [`Router::exec`]. You won't normally build one of
/// these by hand — [`Router::exec_from_value`] is the entry point almost everyone wants, since it
/// also handles batches and malformed input; `exec` exists for when you've already deserialized
/// (and validated) a single request yourself.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct Request {
    jsonrpc: String,
    #[serde(default)]
    id: RequestID,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct ToolCall {
    name: String,
    arguments: Option<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct ResourceCall {
    uri: String,
}

pub type ServerIcon = crate::registry::MCPMetaIcon;

/// What your server calls itself in the `initialize` handshake — set it via
/// [`Router::server_info`], or leave the (frankly not very inspired) default.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icons: Option<Vec<ServerIcon>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    website_url: Option<String>,
}

impl ServerInfo {
    /// Starts a new builder with placeholder name/version. Chain `.name()`/`.description()`,
    /// then finish with [`ServerInfo::build`].
    pub fn new() -> Self {
        Self {
            name: "Example MCP Server".to_string(),
            version: "1.0.0".to_string(),
            title: None,
            description: None,
            icons: None,
            website_url: None,
        }
    }

    pub fn name(&mut self, name: &str) -> &mut Self {
        self.name = name.to_string();
        self
    }

    pub fn description(&mut self, description: &str) -> &mut Self {
        self.description = Some(description.to_string());
        self
    }

    pub fn build(&mut self) -> Self {
        self.to_owned()
    }
}

fn request_id_to_json(id: &RequestID) -> serde_json::Value {
    match id {
        RequestID::Number(n) => serde_json::Value::Number((*n).into()),
        RequestID::Str(s) => serde_json::Value::String(s.clone()),
        RequestID::Null | RequestID::Absent => serde_json::Value::Null,
    }
}

/// The inbound half of a [`RouterStream`] — how you feed a reply to one of the tool's own
/// server-initiated requests (e.g. `sampling/createMessage`) back to it.
///
/// For a single streaming tool this wraps just that one tool's channel. For a batch request where
/// more than one item streams at once, it wraps *all* of their channels: [`RouterStreamSender::send`]
/// broadcasts the same value to every one of them, synchronously and without spawning any
/// background task. Each tool is already filtering its own inbound channel for the correlation id
/// it invented, so a reply meant for a different tool in the same batch is simply ignored rather
/// than delivered incorrectly.
#[derive(Debug)]
pub struct RouterStreamSender {
    senders: Vec<futures_channel::mpsc::Sender<serde_json::Value>>,
}

impl RouterStreamSender {
    fn new(senders: Vec<futures_channel::mpsc::Sender<serde_json::Value>>) -> Self {
        Self { senders }
    }

    /// Delivers `value` to every tool currently listening on this stream. A tool that already
    /// finished (and dropped its receiver) simply doesn't get it — that's expected, not an error.
    pub async fn send(&mut self, value: serde_json::Value) {
        use futures_util::SinkExt;
        for sender in &mut self.senders {
            let _ = sender.send(value.clone()).await;
        }
    }
}

/// What [`RouterResponse::Stream`] carries: a `receiver` of already-transformed JSON-RPC messages
/// ready to forward over whatever transport you're using, and a `sender` to feed replies back in.
///
/// Every item off `receiver` is already a complete, ready-to-send JSON value: anything with a
/// `"method"` field is a notification or a server-initiated request awaiting a reply (relay it
/// verbatim); anything without one is a final answer, already wrapped as
/// `{"jsonrpc":"2.0","id":...,"result":...}`, and the stream ends right after it. If a tool's own
/// channel closes without ever producing a final answer, the last item you'll see is a
/// synthesized `-32603` error rather than the stream just silently going quiet.
pub struct RouterStream {
    pub receiver: std::pin::Pin<Box<dyn futures_util::stream::Stream<Item = serde_json::Value> + Send>>,
    pub sender: RouterStreamSender,
}

impl std::fmt::Debug for RouterStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RouterStream").finish_non_exhaustive()
    }
}

/// What [`Router::exec`]/[`Router::exec_from_value`] hand back. `Value` is the ordinary case —
/// exactly what these methods always returned before streaming existed. `Stream` shows up when a
/// tool/resource returned `MCPExecutionResult::STREAM`/`MCPResourceResult::STREAM` (or, for a
/// batch request, when at least one item did — see [`RouterStream`]'s docs for what a batch's
/// combined stream looks like).
#[derive(Debug)]
pub enum RouterResponse {
    Value(serde_json::Value),
    Stream(RouterStream),
}

enum StreamState {
    Running(
        futures_channel::mpsc::Receiver<serde_json::Value>,
        serde_json::Value,
    ),
    Done,
}

fn build_router_stream(
    id_value: serde_json::Value,
    stream: crate::registry::MCPExecutionResultStream,
) -> RouterStream {
    let crate::registry::MCPExecutionResultStream { receiver, sender } = stream;
    let transformed = futures_util::stream::unfold(
        StreamState::Running(receiver, id_value),
        |state| async move {
            match state {
                StreamState::Running(mut receiver, id_value) => match receiver.next().await {
                    Some(item) => {
                        if item.get("method").is_some() {
                            Some((item, StreamState::Running(receiver, id_value)))
                        } else {
                            let wrapped = serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": id_value,
                                "result": item
                            });
                            Some((wrapped, StreamState::Done))
                        }
                    }
                    None => {
                        let err = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id_value,
                            "error": {"code": -32603, "message": "stream ended without a result"}
                        });
                        Some((err, StreamState::Done))
                    }
                },
                StreamState::Done => None,
            }
        },
    );
    RouterStream {
        receiver: Box::pin(transformed),
        sender: RouterStreamSender::new(vec![sender]),
    }
}

fn extract_stream(
    results: &mut Vec<crate::registry::MCPExecutionResult>,
) -> Option<crate::registry::MCPExecutionResultStream> {
    let pos = results
        .iter()
        .position(|r| matches!(r, crate::registry::MCPExecutionResult::STREAM(_)))?;
    if results.len() > 1 {
        tracing::error!(
            "execute() returned a STREAM content block alongside {} other item(s); the other items will be dropped",
            results.len() - 1
        );
    }
    match results.remove(pos) {
        crate::registry::MCPExecutionResult::STREAM(s) => Some(s),
        _ => unreachable!(),
    }
}

fn extract_resource_stream(
    results: &mut Vec<crate::registry::MCPResourceResult>,
) -> Option<crate::registry::MCPExecutionResultStream> {
    let pos = results
        .iter()
        .position(|r| matches!(r, crate::registry::MCPResourceResult::STREAM(_)))?;
    if results.len() > 1 {
        tracing::error!(
            "execute() returned a STREAM resource result alongside {} other item(s); the other items will be dropped",
            results.len() - 1
        );
    }
    match results.remove(pos) {
        crate::registry::MCPResourceResult::STREAM(s) => Some(s),
        _ => unreachable!(),
    }
}

fn resource_result_to_contents_value(r: &crate::registry::MCPResourceResult) -> serde_json::Value {
    match r {
        crate::registry::MCPResourceResult::LINK(l) => {
            let mut val = serde_json::to_value(l).unwrap_or_else(|e| {
                serde_json::json!({"error": format!("failed to serialize resource link: {}", e)})
            });
            if let serde_json::Value::Object(ref mut o) = val {
                o.insert(
                    "type".to_string(),
                    serde_json::Value::String("resource_link".to_string()),
                );
            }
            val
        }
        crate::registry::MCPResourceResult::TEXT(t) => serde_json::to_value(t)
            .unwrap_or_else(|e| serde_json::json!({"error": format!("failed to serialize resource text: {}", e)})),
        crate::registry::MCPResourceResult::BLOB(b) => serde_json::to_value(b)
            .unwrap_or_else(|e| serde_json::json!({"error": format!("failed to serialize resource blob: {}", e)})),
        crate::registry::MCPResourceResult::STREAM(_) => {
            tracing::error!(
                "attempted to render a STREAM resource result as a plain contents value; this should have been intercepted before reaching here"
            );
            serde_json::json!({"error": "streaming content is not supported in this context"})
        }
    }
}

/// The single entry point for dispatching MCP requests. Transport-agnostic — it takes/returns
/// plain [`serde_json::Value`]/[`RouterResponse`], so it's equally at home behind an HTTP handler,
/// a stdio loop, or anything else. See [`Router::exec_from_value`] to actually run a request.
///
/// Cheap to `Clone` (it's a couple of small fields plus a registry reference), which matters for
/// sharing one instance across request handlers in most web frameworks.
#[derive(Clone)]
pub struct Router<'a> {
    server_info: ServerInfo,
    registry: &'a crate::registry::Registry,
    page_size: usize,
}

impl<'a> Router<'a> {
    /// Starts a builder using the global, `inventory`-populated registry
    /// ([`crate::registry::registry`]) — what `#[derive(MCPTool)]`/`#[derive(MCPResource)]`/
    /// `#[derive(MCPPrompt)]` auto-register into. Chain `.registry()` to use your own instead,
    /// `.page_size()`/`.server_info()` as needed, then finish with [`Router::build`].
    pub fn new() -> Self {
        Router {
            registry: crate::registry::registry(),
            server_info: ServerInfo::new(),
            page_size: DEFAULT_PAGE_SIZE,
        }
    }

    /// How many items `tools/list`/`resources/list`/`resources/templates/list`/`prompts/list`
    /// return per page (default 50). Clamped to at least 1.
    pub fn page_size(&mut self, n: usize) -> &mut Self {
        self.page_size = n.max(1);
        self
    }

    /// Uses your own [`crate::registry::Registry`] instead of the global one — see the crate's
    /// `README.md` for when manual registration is worth the extra setup (multiple independent
    /// registries in one process, resources generated at runtime, etc.).
    pub fn registry(&mut self, registry: &'a crate::registry::Registry) -> &mut Self {
        self.registry = registry;
        self
    }

    /// Sets what this server calls itself in the `initialize` handshake.
    pub fn server_info(&mut self, server_info: ServerInfo) -> &mut Self {
        self.server_info = server_info;
        self
    }

    pub fn build(&mut self) -> Self {
        self.to_owned()
    }

    /// The registry this router is currently configured to dispatch against.
    pub fn registry_ref(&self) -> &crate::registry::Registry {
        self.registry
    }

    fn mcp_execution_result_to_value(item: &crate::registry::MCPExecutionResult) -> serde_json::Value {
        match item {
            crate::registry::MCPExecutionResult::TEXT(t) => {
                let mut v = serde_json::Map::new();
                v.insert(
                    "type".to_string(),
                    serde_json::Value::String("text".to_string()),
                );
                v.insert(
                    "text".to_string(),
                    serde_json::Value::String(t.text.clone()),
                );
                if let Some(a) = &t.annotations {
                    let annotations = serde_json::to_value(a).unwrap_or_else(|e| {
                        serde_json::json!({
                            "error": format!("failed to serialize annotations: {}", e)
                        })
                    });
                    v.insert("annotations".to_string(), annotations);
                }
                serde_json::Value::Object(v)
            }
            crate::registry::MCPExecutionResult::AUDIO(a) => {
                let mut v = serde_json::Map::new();
                v.insert(
                    "type".to_string(),
                    serde_json::Value::String("audio".to_string()),
                );
                v.insert(
                    "data".to_string(),
                    serde_json::Value::String(
                        general_purpose::STANDARD.encode(&a.data).to_string(),
                    ),
                );
                v.insert(
                    "mimeType".to_string(),
                    serde_json::Value::String(a.mime_type.to_string()),
                );
                if let Some(b) = &a.annotations {
                    let annotations = serde_json::to_value(b).unwrap_or_else(|e| {
                        serde_json::json!({
                            "error": format!("failed to serialize annotations: {}", e)
                        })
                    });
                    v.insert("annotations".to_string(), annotations);
                }
                serde_json::Value::Object(v)
            }
            crate::registry::MCPExecutionResult::IMAGE(a) => {
                let mut v = serde_json::Map::new();
                v.insert(
                    "type".to_string(),
                    serde_json::Value::String("image".to_string()),
                );
                v.insert(
                    "data".to_string(),
                    serde_json::Value::String(general_purpose::STANDARD.encode(&a.data)),
                );
                v.insert(
                    "mimeType".to_string(),
                    serde_json::Value::String(a.mime_type.to_string()),
                );
                if let Some(an) = &a.annotations {
                    let annotations = serde_json::to_value(an).unwrap_or_else(|e| {
                        serde_json::json!({
                            "error": format!("failed to serialize annotations: {}", e)
                        })
                    });
                    v.insert("annotations".to_string(), annotations);
                }
                serde_json::Value::Object(v)
            }
            crate::registry::MCPExecutionResult::RAW(v) => v.clone(),
            crate::registry::MCPExecutionResult::RESOURCE(r) => match r {
                crate::registry::MCPResourceResult::LINK(l) => {
                    let mut val = serde_json::to_value(l).unwrap_or_else(|e| {
                        serde_json::json!({
                            "type": "text",
                            "text": format!("error: {:?} serializing resource link: {}", l, e)
                        })
                    });
                    if let serde_json::Value::Object(ref mut o) = val {
                        o.insert(
                            "type".to_string(),
                            serde_json::Value::String("resource_link".to_string()),
                        );
                    }
                    val
                }
                crate::registry::MCPResourceResult::TEXT(t) => {
                    let resource = serde_json::to_value(t).unwrap_or_else(|e| {
                        serde_json::json!({"error": format!("failed to serialize resource text: {}", e)})
                    });
                    serde_json::json!({"type": "resource", "resource": resource})
                }
                crate::registry::MCPResourceResult::BLOB(b) => {
                    let resource = serde_json::to_value(b).unwrap_or_else(|e| {
                        serde_json::json!({"error": format!("failed to serialize resource blob: {}", e)})
                    });
                    serde_json::json!({"type": "resource", "resource": resource})
                }
                crate::registry::MCPResourceResult::STREAM(_) => {
                    tracing::error!(
                        "attempted to embed a STREAM resource result as tool content; streaming resources cannot be embedded"
                    );
                    serde_json::json!({
                        "type": "text",
                        "text": "error: streaming resource content cannot be embedded in tool output"
                    })
                }
            },
            crate::registry::MCPExecutionResult::ERROR((s, _)) => serde_json::json!({
                "type": "text",
                "text": format!("error: {}", s)
            }),
            crate::registry::MCPExecutionResult::STREAM(_) => {
                tracing::error!(
                    "attempted to render a STREAM content block as a plain value; this should have been intercepted before reaching here"
                );
                serde_json::json!({
                    "type": "text",
                    "text": "error: streaming content is not supported in this context"
                })
            }
        }
    }

    fn execution_result_to_mcp(
        mcper: Vec<crate::registry::MCPExecutionResult>,
        content_key: &str,
    ) -> serde_json::Value {
        let mut content: Vec<serde_json::Value> = Vec::new();
        let mut result = serde_json::Map::new();
        for mcpr in &mcper {
            content.push(Router::mcp_execution_result_to_value(mcpr));
            if let crate::registry::MCPExecutionResult::ERROR((s, data)) = mcpr {
                if content_key == "content" {
                    result.insert("isError".to_string(), serde_json::Value::Bool(true));
                    if let Some(d) = data {
                        content.push(d.clone());
                    }
                } else {
                    let mut err = serde_json::json!({"code": -32002, "message": s});
                    if let Some(d) = data
                        && let serde_json::Value::Object(ref mut o) = err
                    {
                        o.insert("data".to_string(), d.clone());
                    }
                    return serde_json::json!({"error": err});
                }
            }
        }
        result.insert(content_key.to_string(), serde_json::Value::Array(content));
        serde_json::json!({"result": result})
    }

    /// Dispatches a raw JSON-RPC value — a single request/notification, or a batch (JSON array)
    /// of them — and returns the [`RouterResponse`] to send back. This is the entry point almost
    /// every embedder should use.
    ///
    /// Handles JSON-RPC semantics for you: a notification (no `id`) yields
    /// `RouterResponse::Value(Value::Null)`, which callers should treat as "nothing to send" (a
    /// 202 with an empty body over HTTP, or simply not printing a line over stdio); malformed
    /// input becomes a `-32700` Parse error; an empty batch array becomes a `-32600` Invalid
    /// request; and a batch where at least one item streams gets elevated to a single merged
    /// `RouterResponse::Stream` covering the whole batch — see [`RouterStream`]'s docs for exactly
    /// how that merge behaves.
    pub async fn exec_from_value(&self, v: serde_json::Value) -> RouterResponse {
        if let serde_json::Value::Array(items) = &v {
            if items.is_empty() {
                return RouterResponse::Value(serde_json::json!({"jsonrpc": "2.0", "id": null, "error": { "code": -32600, "message": "invalid request: batch array must not be empty"}}));
            }
            let mut responses = Vec::new();
            let mut streams: Vec<
                std::pin::Pin<Box<dyn futures_util::stream::Stream<Item = serde_json::Value> + Send>>,
            > = Vec::new();
            let mut senders: Vec<futures_channel::mpsc::Sender<serde_json::Value>> = Vec::new();
            for item in items {
                match serde_json::from_value::<Request>(item.clone()) {
                    Ok(a) => match self.exec(a).await {
                        RouterResponse::Value(v) => {
                            if !v.is_null() {
                                responses.push(v);
                            }
                        }
                        RouterResponse::Stream(s) => {
                            senders.extend(s.sender.senders);
                            streams.push(s.receiver);
                        }
                    },
                    Err(_) => {
                        responses.push(serde_json::json!({"jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "invalid request format, expected {jsonrpc:string, id:number|string, method:string, params:optional<object>}"}}));
                    }
                };
            }
            if streams.is_empty() {
                return RouterResponse::Value(if responses.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::Array(responses)
                });
            }
            let immediate: std::pin::Pin<
                Box<dyn futures_util::stream::Stream<Item = serde_json::Value> + Send>,
            > = Box::pin(futures_util::stream::iter(responses));
            let mut all_streams = vec![immediate];
            all_streams.append(&mut streams);
            let combined = futures_util::stream::select_all(all_streams);
            return RouterResponse::Stream(RouterStream {
                receiver: Box::pin(combined),
                sender: RouterStreamSender::new(senders),
            });
        }
        match serde_json::from_value::<Request>(v) {
            Ok(a) => self.exec(a).await,
            Err(_) => {
                RouterResponse::Value(serde_json::json!({"jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "invalid request format, expected {jsonrpc:string, id:number|string, method:string, params:optional<object>}"}}))
            }
        }
    }

    /// Dispatches a single already-parsed [`Request`]. Prefer [`Router::exec_from_value`] unless
    /// you specifically need to skip its batch handling and raw-`Value` parsing (e.g. you've
    /// already validated and deserialized the request yourself).
    pub async fn exec(&self, req: Request) -> RouterResponse {
        match self.execx(&req).await {
            RouterResponse::Value(serde_json::Value::Object(mut result_map)) => {
                result_map.insert(
                    "jsonrpc".to_string(),
                    serde_json::Value::String("2.0".to_string()),
                );
                match req.id {
                    RequestID::Number(a) => {
                        result_map.insert("id".to_string(), serde_json::Value::Number(a.into()));
                    }
                    RequestID::Str(a) => {
                        result_map.insert("id".to_string(), serde_json::Value::String(a));
                    }
                    RequestID::Null => {
                        result_map.insert("id".to_string(), serde_json::Value::Null);
                    }
                    RequestID::Absent => (),
                };
                RouterResponse::Value(serde_json::Value::Object(result_map))
            }
            other => other,
        }
    }

    async fn execx(&self, req: &Request) -> RouterResponse {
        if matches!(req.id, RequestID::Absent) {
            return RouterResponse::Value(serde_json::Value::Null);
        } else if req.method == "ping" {
            return RouterResponse::Value(serde_json::json!({"result": {}}));
        } else if req.method == "initialize" {
            let mut capabilities = serde_json::Map::new();
            if !self.registry.tools().is_empty() {
                capabilities.insert(
                    "tools".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            if !self.registry.resources().is_empty() {
                capabilities.insert(
                    "resources".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            if !self.registry.prompts().is_empty() {
                capabilities.insert(
                    "prompts".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            return RouterResponse::Value(serde_json::json!({
                "result": {
                    "protocolVersion": req.params.clone().unwrap_or(serde_json::json!({})).get("protocolVersion").unwrap_or(&serde_json::Value::String("2025-11-25".to_string())),
                    "capabilities": capabilities,
                    "serverInfo": self.server_info
                }
            }));
        } else if req.method == "tools/list" {
            let cursor = extract_cursor(req.params.as_ref());
            let mut entries: Vec<(String, &'static crate::registry::Info)> = self
                .registry
                .tools()
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let (page, next_cursor) = paginate(&entries, &cursor, self.page_size);
            let mut result = serde_json::json!({
                "tools": page.into_iter().map(|(_, i)| (i.params)()).collect::<Vec<_>>()
            });
            if let Some(c) = next_cursor
                && let serde_json::Value::Object(ref mut o) = result
            {
                o.insert("nextCursor".to_string(), serde_json::Value::String(c));
            }
            return RouterResponse::Value(serde_json::json!({"result": result}));
        } else if req.method == "tools/call" {
            if let Ok(tool_call) = serde_json::from_value::<ToolCall>(
                req.params.clone().unwrap_or(serde_json::json!({})),
            ) {
                if let Some(tool) = self.registry.get_tool(&tool_call.name) {
                    let args = tool_call.arguments.clone().unwrap_or(serde_json::json!({}));
                    let cursor = extract_cursor(Some(&args));
                    match (tool.from_args)(&args) {
                        crate::registry::FromArgResult::Tool(caller) => {
                            let executor = caller.get_executor();
                            let (mut results, next_cursor) = executor.execute(cursor).await;
                            if let Some(stream) = extract_stream(&mut results) {
                                let id_value = request_id_to_json(&req.id);
                                return RouterResponse::Stream(build_router_stream(
                                    id_value, stream,
                                ));
                            }
                            let mut mcp = Router::execution_result_to_mcp(results, "content");
                            if let Some(c) = next_cursor
                                && let serde_json::Value::Object(ref mut o) = mcp
                                && let Some(serde_json::Value::Object(r)) = o.get_mut("result")
                            {
                                r.insert("nextCursor".to_string(), serde_json::Value::String(c));
                            }
                            return RouterResponse::Value(mcp);
                        }
                        crate::registry::FromArgResult::Error(s) => {
                            return RouterResponse::Value(serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for tools/call {}", s)}}));
                        }
                        crate::registry::FromArgResult::Resource(_) => {
                            return RouterResponse::Value(serde_json::json!({"error": {"code": -32600, "message": "server is misconfigured, a resource was registered as a tool"}}));
                        }
                        crate::registry::FromArgResult::Prompt(_) => {
                            return RouterResponse::Value(serde_json::json!({"error": {"code": -32600, "message": "server is misconfigured, a prompt was registered as a tool"}}));
                        }
                    }
                }
                return RouterResponse::Value(serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for tools/call, unknown tool: {}", tool_call.name)}}));
            }
            return RouterResponse::Value(serde_json::json!({"error": { "code": -32602, "message": "malformed request from LLM"}}));
        } else if req.method == "prompts/list" {
            let cursor = extract_cursor(req.params.as_ref());
            let mut entries: Vec<(String, &'static crate::registry::Info)> = self
                .registry
                .prompts()
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let (page, next_cursor) = paginate(&entries, &cursor, self.page_size);
            let mut result = serde_json::json!({
                "prompts": page.into_iter().map(|(_, i)| (i.params)()).collect::<Vec<_>>()
            });
            if let Some(c) = next_cursor
                && let serde_json::Value::Object(ref mut o) = result
            {
                o.insert("nextCursor".to_string(), serde_json::Value::String(c));
            }
            return RouterResponse::Value(serde_json::json!({"result": result}));
        } else if req.method == "prompts/get" {
            #[derive(serde::Deserialize)]
            struct PromptCall {
                name: String,
                arguments: Option<serde_json::Value>,
            }
            if let Ok(prompt_call) = serde_json::from_value::<PromptCall>(
                req.params.clone().unwrap_or(serde_json::json!({})),
            ) {
                if let Some(prompt) = self.registry.get_prompt(&prompt_call.name) {
                    let args = prompt_call.arguments.clone().unwrap_or(serde_json::json!({}));
                    match (prompt.from_args)(&args) {
                        crate::registry::FromArgResult::Prompt(caller) => {
                            let result = caller.get_executor().execute().await;
                            let messages: Vec<serde_json::Value> = result
                                .messages
                                .iter()
                                .filter_map(|m| {
                                    if matches!(
                                        m.content,
                                        crate::registry::MCPExecutionResult::STREAM(_)
                                    ) {
                                        tracing::error!(
                                            "prompt \"{}\" tried to return a STREAM content block; streaming is not supported for prompts, dropping this message",
                                            prompt_call.name
                                        );
                                        return None;
                                    }
                                    Some(serde_json::json!({
                                        "role": m.role,
                                        "content": Router::mcp_execution_result_to_value(&m.content)
                                    }))
                                })
                                .collect();
                            let mut obj = serde_json::json!({ "messages": messages });
                            if let Some(d) = result.description
                                && let serde_json::Value::Object(ref mut o) = obj
                            {
                                o.insert("description".to_string(), serde_json::Value::String(d));
                            }
                            return RouterResponse::Value(serde_json::json!({"result": obj}));
                        }
                        crate::registry::FromArgResult::Error(s) => {
                            return RouterResponse::Value(serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for prompts/get {}", s)}}));
                        }
                        crate::registry::FromArgResult::Tool(_) => {
                            return RouterResponse::Value(serde_json::json!({"error": {"code": -32600, "message": "server is misconfigured, a tool was registered as a prompt"}}));
                        }
                        crate::registry::FromArgResult::Resource(_) => {
                            return RouterResponse::Value(serde_json::json!({"error": {"code": -32600, "message": "server is misconfigured, a resource was registered as a prompt"}}));
                        }
                    }
                }
                return RouterResponse::Value(serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for prompts/get, unknown prompt: {}", prompt_call.name)}}));
            }
            return RouterResponse::Value(serde_json::json!({"error": { "code": -32602, "message": "malformed request from LLM"}}));
        } else if req.method == "completion/complete" {
            #[derive(serde::Deserialize)]
            struct CompletionRef {
                #[serde(rename = "type")]
                ref_type: String,
                name: Option<String>,
                uri: Option<String>,
            }
            #[derive(serde::Deserialize)]
            struct CompletionArgument {
                name: String,
                value: String,
            }
            #[derive(serde::Deserialize)]
            struct CompletionCall {
                r#ref: CompletionRef,
                argument: CompletionArgument,
            }
            if let Ok(call) = serde_json::from_value::<CompletionCall>(
                req.params.clone().unwrap_or(serde_json::json!({})),
            ) {
                let values = if call.r#ref.ref_type == "ref/prompt" {
                    call.r#ref
                        .name
                        .as_deref()
                        .and_then(|n| self.registry.get_prompt(n))
                        .and_then(|i| (i.complete)(&call.argument.name, &call.argument.value))
                        .unwrap_or_default()
                } else if call.r#ref.ref_type == "ref/resource" {
                    call.r#ref
                        .uri
                        .as_deref()
                        .and_then(|u| self.registry.get_resource(u))
                        .map(|i| {
                            if let Some(custom) =
                                (i.complete)(&call.argument.name, &call.argument.value)
                            {
                                return custom;
                            }
                            let uris: Vec<String> = (i.meta)().into_iter().map(|m| m.uri).collect();
                            strip_common_prefix(&uris)
                                .into_iter()
                                .filter(|s| s.contains(&call.argument.value))
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                return RouterResponse::Value(serde_json::json!({
                    "result": {
                        "completion": {
                            "total": values.len(),
                            "values": values,
                            "hasMore": false
                        }
                    }
                }));
            }
            return RouterResponse::Value(serde_json::json!({"error": { "code": -32602, "message": "malformed request from LLM"}}));
        } else if req.method == "resources/list" {
            let cursor = extract_cursor(req.params.as_ref());
            let mut resources: Vec<crate::registry::MCPMeta> = Vec::new();
            for rsrc in self.registry.resources().values() {
                if !(rsrc.is_template)() {
                    resources.extend((rsrc.meta)());
                }
            }
            resources.sort_by(|a, b| a.uri.cmp(&b.uri));
            let (page, next_cursor) = paginate(&resources, &cursor, self.page_size);
            let mut result = serde_json::json!({ "resources": page });
            if let Some(c) = next_cursor
                && let serde_json::Value::Object(ref mut o) = result
            {
                o.insert("nextCursor".to_string(), serde_json::Value::String(c));
            }
            return RouterResponse::Value(serde_json::json!({"result": result}));
        } else if req.method == "resources/templates/list" {
            let cursor = extract_cursor(req.params.as_ref());
            let mut resources: Vec<crate::registry::MCPTemplateMeta> = Vec::new();
            for rsrc in self.registry.resources().values() {
                if (rsrc.is_template)() {
                    for meta in (rsrc.meta)() {
                        resources.push(crate::registry::MCPTemplateMeta::from_meta(&meta));
                    }
                }
            }
            resources.sort_by(|a, b| a.uri_template.cmp(&b.uri_template));
            let (page, next_cursor) = paginate(&resources, &cursor, self.page_size);
            let mut result = serde_json::json!({ "resourceTemplates": page });
            if let Some(c) = next_cursor
                && let serde_json::Value::Object(ref mut o) = result
            {
                o.insert("nextCursor".to_string(), serde_json::Value::String(c));
            }
            return RouterResponse::Value(serde_json::json!({"result": result}));
        } else if req.method == "resources/read" {
            let cursor = extract_cursor(req.params.as_ref());
            if let Ok(resource_call) = serde_json::from_value::<ResourceCall>(
                req.params.clone().unwrap_or(serde_json::json!({})),
            ) {
                if let Some(r) = self.registry.get_resource(&resource_call.uri) {
                    // exact match
                    if let crate::registry::FromArgResult::Resource(a) =
                        (r.from_args)(&serde_json::json!({ "dsn": &resource_call.uri }))
                    {
                        let (mut results, next_cursor) =
                            a.get_executor().execute(cursor.clone()).await;
                        if let Some(stream) = extract_resource_stream(&mut results) {
                            let id_value = request_id_to_json(&req.id);
                            return RouterResponse::Stream(build_router_stream(id_value, stream));
                        }
                        let contents: Vec<serde_json::Value> = results
                            .iter()
                            .map(resource_result_to_contents_value)
                            .collect();
                        let mut mcp = serde_json::json!({"result": {"contents": contents}});
                        if let Some(c) = next_cursor
                            && let serde_json::Value::Object(ref mut o) = mcp
                            && let Some(serde_json::Value::Object(r)) = o.get_mut("result")
                        {
                            r.insert("nextCursor".to_string(), serde_json::Value::String(c));
                        }
                        return RouterResponse::Value(mcp);
                    } else {
                        return RouterResponse::Value(serde_json::json!({"error": { "code": -32603, "message": "Internal error: resource structs may only contain a DSN field or must be empty"}}));
                    }
                } else {
                    let dsn = match udsn::DSN::parse(resource_call.uri.clone()) {
                        Some(d) => d,
                        _ => {
                            return RouterResponse::Value(serde_json::json!({"error": { "code": -32602, "message": "malformed request, expected uri in params"}}));
                        }
                    };
                    let ris: Vec<&'static crate::registry::Info> =
                        self.registry.resources().values().copied().collect();
                    for i in ris {
                        if (i.is_template)()
                            && (i.serves)(&dsn)
                            && let crate::registry::FromArgResult::Resource(a) =
                                (i.from_args)(&serde_json::json!({ "dsn": &resource_call.uri }))
                        {
                            let (mut results, next_cursor) =
                                a.get_executor().execute(cursor.clone()).await;
                            if let Some(stream) = extract_resource_stream(&mut results) {
                                let id_value = request_id_to_json(&req.id);
                                return RouterResponse::Stream(build_router_stream(
                                    id_value, stream,
                                ));
                            }
                            let contents: Vec<serde_json::Value> = results
                                .iter()
                                .map(resource_result_to_contents_value)
                                .collect();
                            let mut mcp = serde_json::json!({"result": {"contents": contents}});
                            if let Some(c) = next_cursor
                                && let serde_json::Value::Object(ref mut o) = mcp
                                && let Some(serde_json::Value::Object(r)) = o.get_mut("result")
                            {
                                r.insert("nextCursor".to_string(), serde_json::Value::String(c));
                            }
                            return RouterResponse::Value(mcp);
                        }
                    }
                }
                return RouterResponse::Value(serde_json::json!({"error": {"code": -32602, "message": "no valid resource handler found for requested uri"}}));
            }
            return RouterResponse::Value(serde_json::json!({"error": { "code": -32600, "message": format!("malformed request from LLM: {}", req.method)}}));
        }
        RouterResponse::Value(serde_json::json!({"error": { "code": -32601, "message": format!("method not found: {}", req.method)}}))
    }
}

#[cfg(test)]
mod tests {
    use super::{Request, RequestID, Router, RouterResponse, ServerInfo};
    use async_trait::async_trait;
    use serde_json::json;

    #[tokio::test]
    async fn initialize() {
        use std::collections::HashMap;
        let registry = crate::registry::Registry::new_from(HashMap::new(), HashMap::new());
        let resp = Router::new()
            .registry(&registry)
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "initialize".to_string(),
                params: None,
            })
            .await;
        let resp = match resp {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": {
                "capabilities": {},
                "protocolVersion": "2025-11-25",
                "serverInfo": {
                    "name": "Example MCP Server",
                    "version": "1.0.0",
                }
            }
        }
        );
        assert_eq!(cmp, resp);
    }

    #[tokio::test]
    async fn initialize_w_protocolv() {
        use std::collections::HashMap;
        let registry = crate::registry::Registry::new_from(HashMap::new(), HashMap::new());
        let resp = Router::new()
            .registry(&registry)
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "initialize".to_string(),
                params: Some(serde_json::json!({
                    "protocolVersion": "abc"
                })),
            })
            .await;
        let resp = match resp {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": {
                "capabilities": {},
                "protocolVersion": "abc",
                "serverInfo": {
                    "name": "Example MCP Server",
                    "version": "1.0.0",
                }
            }
        }
        );
        assert_eq!(cmp, resp);
    }
    #[tokio::test]
    async fn initialize_w_server_info() {
        use std::collections::HashMap;
        let registry = crate::registry::Registry::new_from(HashMap::new(), HashMap::new());
        let resp = Router::new()
            .registry(&registry)
            .server_info(
                ServerInfo::new()
                    .name("test")
                    .description("Hello world!")
                    .build(),
            )
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "initialize".to_string(),
                params: None,
            })
            .await;
        let resp = match resp {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": {
                "capabilities": {},
                "protocolVersion": "2025-11-25",
                "serverInfo": {
                    "name": "test",
                    "description": "Hello world!",
                    "version": "1.0.0",
                }
            }
        }
        );
        assert_eq!(cmp, resp);
    }

    #[tokio::test]
    async fn basic_router() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "tools/list".to_string(),
                params: json!({
                    "test": 15,
                    "oooptional": 5,
                })
                .into(),
            })
            .await;
        let resp = match resp {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": { "tools": [
                { "description": "abc camel description",
                  "title": "ABCCamel struct",
                  "name": "ABCCamel",
                  "inputSchema": {
                      "type": "object",
                      "properties": {
                          "oooptional": { "type": "integer" },
                          "test": { "type": "integer" },
                          "arr": { "type": "array", "items": { "type": "integer" } },
                          "ooarr": { "type": "array", "items": { "type": "integer" } },
                          "cursor": { "type": "string" },
                      },
                      "required": ["test", "arr"],
                  }
               },
            ]}
        }
        );
        assert_eq!(cmp, resp);
    }

    #[tokio::test]
    async fn basic_tool_call() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(42),
                method: "tools/call".to_string(),
                params: json!({
                    "name": "ABCCamel",
                    "arguments": {
                        "test": 15,
                        "arr": [5],
                    }
                })
                .into(),
            })
            .await;
        let resp = match resp {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "content": [{"type": "text", "text": "test=15,oooptional=-1,arr=[5],ooarr=[]"}],
            }
        });
        assert_eq!(cmp, resp);
    }

    #[tokio::test]
    async fn basic_tool_call_err() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Str("a666".to_string()),
                method: "tools/call".to_string(),
                params: json!({
                    "name": "ABCCamel",
                    "arguments": {
                        "arr": [5],
                    }
                })
                .into(),
            })
            .await;
        let resp = match resp {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": "a666",
            "error": {
                "code": -32602,
                "message": "invalid parameters for tools/call missing field `test`",
            }
        });
        assert_eq!(cmp, resp);
    }

    #[tokio::test]
    async fn basic_resource_list() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(42),
                method: "resources/list".to_string(),
                params: None,
            })
            .await;
        let resp = match resp {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "resources": [
                    {"title": "TestResource"
                    ,"description": "a test resource"
                    ,"uri": "git://some-repo"
                    ,"name": "TestResource"
                    }
                ],
            }
        });
        assert_eq!(cmp, resp);
    }
    #[tokio::test]
    async fn basic_resource_call() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Str("123".to_string()),
                method: "resources/read".to_string(),
                params: Some(json!({ "uri": "git://some-repo" })),
            })
            .await;
        let resp = match resp {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": "123",
            "result": {
                "contents": [
                    {"uri": "test://forward",
                     "name": "git://some-repo",
                     "type": "resource_link"
                    },
                    {"uri": "test://reverse",
                     "name": "oper-emos//:tig",
                     "type": "resource_link"
                    }
                ],
            }
        });
        assert_eq!(cmp, resp);
    }

    #[tokio::test]
    async fn override_router() {
        use std::collections::HashMap;
        let registry = crate::registry::Registry::new_from(HashMap::new(), HashMap::new());
        let router = Router::new().registry(&registry).build();
        let resp = router
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(42),
                method: "resources/list".to_string(),
                params: None,
            })
            .await;
        let resp = match resp {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "resources": [],
            }
        });
        assert_eq!(cmp, resp);
        let resp2 = router
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "tools/list".to_string(),
                params: None,
            })
            .await;
        let resp2 = match resp2 {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp2 = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": { "tools": [
            ]}
        }
        );
        assert_eq!(cmp2, resp2);
    }

    #[derive(serde::Deserialize)]
    pub struct ManualResource {
        _dsn: udsn::DSN,
    }

    use crate::registry::{
        FromArgResult, MCPMeta, MCPResource, MCPResourceExecutor, MCPResourceResult,
    };
    use serde_json::Value;

    #[async_trait]
    impl MCPResourceExecutor for ManualResource {
        async fn execute(&self, _c: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
            (
                vec![
                    MCPResourceResult::new(
                        "file:///example".to_string(),
                        "example file".to_string(),
                    )
                    .build(),
                ],
                None,
            )
        }

        fn serves(dsn: &udsn::DSN) -> bool {
            !dsn.protocol.is_empty()
        }

        fn is_template() -> bool {
            false
        }
    }

    impl MCPResource for ManualResource {
        fn get_executor(&self) -> &dyn MCPResourceExecutor {
            self
        }
        fn meta() -> Vec<MCPMeta> {
            vec![
                MCPMeta::new()
                    .name("meta_example")
                    .uri("manual-resource:///")
                    .build(),
            ]
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

    #[tokio::test]
    async fn override_router_w_static_resource() {
        use std::collections::HashMap;
        let registry = crate::registry::Registry::new_from(HashMap::new(), HashMap::new());
        registry.register_resource_adapter::<ManualResource>("file:///config");
        let router = Router::new().registry(&registry).build();
        let resp = router
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(42),
                method: "resources/list".to_string(),
                params: None,
            })
            .await;
        let resp = match resp {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "resources": [
                    {"name": "meta_example",
                     "uri": "manual-resource:///",
                    }
                ],
            }
        });
        assert_eq!(cmp, resp);
        let resp2 = router
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "tools/list".to_string(),
                params: None,
            })
            .await;
        let resp2 = match resp2 {
            RouterResponse::Value(v) => v,
            RouterResponse::Stream(_) => panic!("expected a RouterResponse::Value"),
        };
        let cmp2 = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": { "tools": [
            ]}
        }
        );
        assert_eq!(cmp2, resp2);
    }
}
