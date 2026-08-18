use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock, RwLockReadGuard};
use tracing::{error, warn};

use serde_json::Value;

#[doc(hidden)]
pub use inventory as _i;

/// Which of the three MCP primitives a registered [`Info`] entry describes.
#[derive(Debug, Clone, PartialEq)]
pub enum InfoType {
    Tool,
    Resource,
    Prompt,
}

/// The result of turning a `tools/call`/`resources/read`/`prompts/get` request's arguments into
/// a live instance, returned by [`MCPTool::from_args`]/[`MCPResource::from_args`]/
/// [`MCPPrompt::from_args`]. `Error` is a deserialization failure — the router turns it into a
/// JSON-RPC `-32602` (Invalid params) response.
pub enum FromArgResult {
    Tool(Box<dyn MCPTool + Send>),
    Resource(Box<dyn MCPResource + Send>),
    Prompt(Box<dyn MCPPrompt + Send>),
    Error(String),
}

/// A single registered tool/resource/prompt's type-erased vtable, as stored inside a
/// [`Registry`]. You won't normally construct one of these by hand — the `#[derive(MCPTool)]` /
/// `#[derive(MCPResource)]` / `#[derive(MCPPrompt)]` macros build one from your trait impl and
/// submit it via `inventory`, and [`Registry::register_tool_adapter`]/
/// [`Registry::register_resource_adapter`]/[`Registry::register_prompt_adapter`] build one for
/// you from a type parameter in manual-registration mode.
#[derive(Debug, Clone)]
pub struct Info {
    pub name: &'static str,
    pub info_type: InfoType,
    pub params: fn() -> Value,
    pub meta: fn() -> Vec<MCPMeta>,
    pub from_args: fn(&serde_json::Value) -> FromArgResult,
    pub is_template: fn() -> bool,
    pub serves: fn(&udsn::DSN) -> bool,
    pub complete: fn(&str, &str) -> Option<Vec<String>>,
}

inventory::collect!(Info);

/// A collection of registered tools, resources, and prompts that a [`crate::router::Router`]
/// dispatches against.
///
/// Most consumers never construct one directly — [`registry()`] returns the process-wide global
/// registry that `#[derive(MCPTool)]`/`#[derive(MCPResource)]`/`#[derive(MCPPrompt)]` auto-populate
/// via `inventory`, and [`crate::router::Router::new`] uses it by default. Build your own with
/// [`Registry::new_from`]/[`Registry::new_from_all`] plus the `register_*_adapter` methods when
/// you want explicit control instead — e.g. multiple independent registries in one process, or
/// resources generated at runtime from a config file rather than known at compile time.
pub struct Registry {
    tools: RwLock<HashMap<String, &'static Info>>,
    resources: RwLock<HashMap<String, &'static Info>>,
    prompts: RwLock<HashMap<String, &'static Info>>,
}

impl Registry {
    fn new() -> Self {
        let mut tools = HashMap::new();
        let mut resources = HashMap::new();
        let mut prompts = HashMap::new();
        for i in inventory::iter::<Info>() {
            match i.info_type {
                InfoType::Tool => {
                    use tracing::info;
                    info!("registering tool: {}", &i.name);
                    tools.insert(i.name.to_string(), i);
                }
                InfoType::Prompt => {
                    use tracing::info;
                    info!("registering prompt: {}", &i.name);
                    prompts.insert(i.name.to_string(), i);
                }
                InfoType::Resource => {
                    for meta in (i.meta)() {
                        resources.insert(meta.uri.to_string(), i);
                    }
                }
            }
        }
        Self {
            tools: RwLock::new(tools),
            resources: RwLock::new(resources),
            prompts: RwLock::new(prompts),
        }
    }

    /// Builds an empty registry from your own tool/resource maps (prompts start empty — use
    /// [`Registry::new_from_all`] if you need prompts too), instead of the global,
    /// `inventory`-populated one [`registry()`] returns. Typically you'll pass empty `HashMap`s
    /// and populate them afterward with `register_tool_adapter`/`register_resource_adapter`.
    pub fn new_from(
        tools: HashMap<String, &'static Info>,
        resources: HashMap<String, &'static Info>,
    ) -> Self {
        Self {
            tools: RwLock::new(tools),
            resources: RwLock::new(resources),
            prompts: RwLock::new(HashMap::new()),
        }
    }

    /// Same as [`Registry::new_from`], but also takes an initial prompts map.
    pub fn new_from_all(
        tools: HashMap<String, &'static Info>,
        resources: HashMap<String, &'static Info>,
        prompts: HashMap<String, &'static Info>,
    ) -> Self {
        Self {
            tools: RwLock::new(tools),
            resources: RwLock::new(resources),
            prompts: RwLock::new(prompts),
        }
    }

    /// Looks up a registered tool by its exact `tools/call` name.
    pub fn get_tool(&self, name: &str) -> Option<&'static Info> {
        match self.tools.read() {
            Ok(t) => t.get(name).copied(),
            Err(e) => Some(e.into_inner().get(name)?),
        }
    }

    /// Looks up a registered resource by its exact URI (not a template pattern — template
    /// matching against a URI is done separately by the router via each `Info`'s `serves` fn).
    pub fn get_resource(&self, uri: &str) -> Option<&'static Info> {
        match self.resources.read() {
            Ok(t) => t.get(uri).copied(),
            Err(e) => Some(e.into_inner().get(uri)?),
        }
    }

    /// Looks up a registered prompt by its exact `prompts/get` name.
    pub fn get_prompt(&self, name: &str) -> Option<&'static Info> {
        match self.prompts.read() {
            Ok(t) => t.get(name).copied(),
            Err(e) => Some(e.into_inner().get(name)?),
        }
    }

    /// All registered tools, keyed by name. Panics if the internal lock is poisoned (a previous
    /// writer panicked mid-registration) — a registry in that state can't be trusted anyway.
    pub fn tools(&self) -> RwLockReadGuard<'_, HashMap<String, &'static Info>> {
        match self.tools.read() {
            Ok(t) => t,
            Err(e) => {
                error!("Error reading from tool lock: {}", e);
                panic!("Error reading from tool lock: {}", e);
            }
        }
    }

    /// All registered resources, keyed by URI (or URI template pattern for templated resources).
    /// Panics if the internal lock is poisoned; see [`Registry::tools`].
    pub fn resources(&self) -> RwLockReadGuard<'_, HashMap<String, &'static Info>> {
        match self.resources.read() {
            Ok(t) => t,
            Err(e) => {
                error!("Error reading from resources lock: {}", e);
                panic!("Error reading from resources lock: {}", e);
            }
        }
    }

    /// All registered prompts, keyed by name. Panics if the internal lock is poisoned; see
    /// [`Registry::tools`].
    pub fn prompts(&self) -> RwLockReadGuard<'_, HashMap<String, &'static Info>> {
        match self.prompts.read() {
            Ok(t) => t,
            Err(e) => {
                error!("Error reading from prompts lock: {}", e);
                panic!("Error reading from prompts lock: {}", e);
            }
        }
    }

    /// Manually registers a resource type under the given URI (or URI template, e.g.
    /// `"file:///{path}"`) — the manual-mode equivalent of what `#[derive(MCPResource)]`'s
    /// `inventory` submission does automatically for the global registry. Registering the same
    /// URI twice logs a warning and overwrites the previous handler.
    pub fn register_resource_adapter<T>(&self, uri: &str)
    where
        T: MCPResource + MCPResourceExecutor + Send + Sync + 'static,
    {
        let nfo: &'static Info = Box::leak(Box::new(Info {
            name: Box::leak(uri.to_string().into_boxed_str()),
            info_type: InfoType::Resource,
            params: T::params,
            from_args: T::from_args,
            meta: T::meta,
            is_template: T::is_template,
            serves: T::serves,
            complete: T::complete,
        }));
        let mut resources = match self.resources.write() {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to lock resources for writing: {}", e);
                return;
            }
        };
        if resources.get(uri).is_some() {
            warn!("Overwriting resource handler {}", uri);
        }
        resources.insert(uri.to_string(), nfo);
    }

    /// Manually registers a tool type under the given `tools/call` name — the manual-mode
    /// equivalent of what `#[derive(MCPTool)]`'s `inventory` submission does automatically for
    /// the global registry. Registering the same name twice logs a warning and overwrites the
    /// previous handler.
    pub fn register_tool_adapter<T>(&self, name: &str)
    where
        T: MCPTool + MCPToolExecutor + Send + Sync + 'static,
    {
        let nfo: &'static Info = Box::leak(Box::new(Info {
            name: Box::leak(name.to_string().into_boxed_str()),
            info_type: InfoType::Tool,
            params: T::params,
            from_args: T::from_args,
            meta: T::meta,
            is_template: || false,
            serves: |_| false,
            complete: |_, _| None,
        }));
        let mut tools = match self.tools.write() {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to lock tools for writing: {}", e);
                return;
            }
        };
        if tools.get(name).is_some() {
            warn!("Overwriting tool handler {}", name);
        }
        tools.insert(name.to_string(), nfo);
    }

    /// Manually registers a prompt type under the given `prompts/get` name — the manual-mode
    /// equivalent of what `#[derive(MCPPrompt)]`'s `inventory` submission does automatically for
    /// the global registry. Registering the same name twice logs a warning and overwrites the
    /// previous handler.
    pub fn register_prompt_adapter<T>(&self, name: &str)
    where
        T: MCPPrompt + MCPPromptExecutor + Send + Sync + 'static,
    {
        let nfo: &'static Info = Box::leak(Box::new(Info {
            name: Box::leak(name.to_string().into_boxed_str()),
            info_type: InfoType::Prompt,
            params: T::params,
            from_args: T::from_args,
            meta: T::meta,
            is_template: || false,
            serves: |_| false,
            complete: T::complete,
        }));
        let mut prompts = match self.prompts.write() {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to lock prompts for writing: {}", e);
                return;
            }
        };
        if prompts.get(name).is_some() {
            warn!("Overwriting prompt handler {}", name);
        }
        prompts.insert(name.to_string(), nfo);
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// The process-wide global [`Registry`], populated automatically by every
/// `#[derive(MCPTool)]`/`#[derive(MCPResource)]`/`#[derive(MCPPrompt)]` type in your binary via
/// `inventory`, the moment this function is first called (and cached for the rest of the
/// process's life). [`crate::router::Router::new`] uses this by default.
pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::new)
}

/// Describes a tool: its metadata, its JSON schema, and how to build one from `tools/call`
/// arguments. Implemented for you by `#[derive(MCPTool)]`, or by hand for manual registration —
/// see the crate's `README.md` for a full manual-mode example. Pair with [`MCPToolExecutor`]
/// for the actual `execute()` logic.
pub trait MCPTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor;
    /// One or more metadata entries describing this tool (name/title/description/icons). Tools
    /// normally return exactly one; the `Vec` shape is shared with [`MCPResource::meta`], where a
    /// single type can back several distinct URIs.
    fn meta() -> Vec<MCPMeta>
    where
        Self: Sized;
    /// The full `tools/list` entry for this tool, including its JSON schema — normally
    /// `#[derive(MCPTool)]`-generated from your struct's fields.
    fn params() -> Value
    where
        Self: Sized;
    /// Deserializes `tools/call` arguments into a live instance of this tool.
    fn from_args(v: &serde_json::Value) -> FromArgResult
    where
        Self: Sized;
}

/// Describes a resource (or resource template): its metadata, its URI matching rules, and how to
/// build one from a `resources/read` request. Implemented for you by `#[derive(MCPResource)]` —
/// whose generated struct may only have a single `dsn: udsn::DSN` field (or none) — or by hand
/// for manual registration. Pair with [`MCPResourceExecutor`] for the actual `execute()` logic.
pub trait MCPResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor;
    /// One or more metadata entries (uri/name/title/description/mimeType/icons). A single
    /// resource type backing several fixed URIs returns one entry per URI here.
    fn meta() -> Vec<MCPMeta>
    where
        Self: Sized;
    fn params() -> Value
    where
        Self: Sized;
    /// Deserializes a `resources/read` request's target `dsn` into a live instance of this
    /// resource.
    fn from_args(v: &serde_json::Value) -> FromArgResult
    where
        Self: Sized;
    /// Suggests completions for a `completion/complete` request against one of this resource's
    /// URI template variables. The default (`None`) falls back to the router's automatic
    /// substring matching over every URI [`MCPResource::meta`] reports; override this when your
    /// resource's real URIs live somewhere the router can't enumerate up front (e.g. a database).
    fn complete(_argument_name: &str, _partial_value: &str) -> Option<Vec<String>>
    where
        Self: Sized,
    {
        None
    }
}

/// Describes a prompt: its metadata, its arguments, and how to build one from a `prompts/get`
/// request. Implemented for you by `#[derive(MCPPrompt)]` (with `#[arg(description = "...")]` on
/// fields to document individual arguments), or by hand for manual registration. Pair with
/// [`MCPPromptExecutor`] for the actual `execute()` logic.
pub trait MCPPrompt {
    fn get_executor(&self) -> &dyn MCPPromptExecutor;
    fn meta() -> Vec<MCPMeta>
    where
        Self: Sized;
    /// The full `prompts/list` entry for this prompt, including its argument list.
    fn params() -> Value
    where
        Self: Sized;
    /// Deserializes `prompts/get` arguments into a live instance of this prompt.
    fn from_args(v: &serde_json::Value) -> FromArgResult
    where
        Self: Sized;
    /// Suggests completions for a `completion/complete` request against one of this prompt's
    /// arguments. Unlike [`MCPResource::complete`], there's no automatic fallback for
    /// prompts — the default (`None`) simply offers no suggestions, since there's nothing for
    /// the router to search through on its own.
    fn complete(_argument_name: &str, _partial_value: &str) -> Option<Vec<String>>
    where
        Self: Sized,
    {
        None
    }
}

/// One message in a [`MCPPromptResult`] — a `role` (`"user"`/`"assistant"`) paired with its
/// content. `content` may be any [`MCPExecutionResult`] variant except `STREAM`, which prompts
/// don't support (the router drops such a message and logs an error rather than sending it to
/// the client).
#[derive(Debug)]
pub struct MCPPromptMessage {
    pub role: String,
    pub content: MCPExecutionResult,
}

/// The full result of rendering a prompt via `prompts/get` — returned from
/// [`MCPPromptExecutor::execute`].
#[derive(Debug)]
pub struct MCPPromptResult {
    pub description: Option<String>,
    pub messages: Vec<MCPPromptMessage>,
}

/// The actual prompt-rendering logic paired with an [`MCPPrompt`] impl. Unlike
/// [`MCPToolExecutor`]/[`MCPResourceExecutor`], prompts have no cursor/pagination parameter and
/// no notion of streaming — a prompt just hands back a fixed set of messages.
#[async_trait]
pub trait MCPPromptExecutor: Send {
    async fn execute(&self) -> MCPPromptResult;
}

/// Audience/priority hints for a content block, per the MCP spec's annotations object. Shared
/// across [`MCPExecutionResultText`], [`MCPExecutionResultImage`], and
/// [`MCPExecutionResultAudio`].
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPExecutionResultAnnotations {
    pub audience: Vec<String>,
    pub priority: f32,
}

/// An image content block (base64-encoded on the wire; `data` holds the raw decoded bytes here).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPExecutionResultImage {
    pub mime_type: String,
    pub data: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<MCPExecutionResultAnnotations>,
}

/// A plain text content block. Build one with `.into()` from a `String`/`&str` when you don't
/// need [`MCPExecutionResultAnnotations`] — `MCPExecutionResult::TEXT("hello".into())`.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct MCPExecutionResultText {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<MCPExecutionResultAnnotations>,
}

impl From<String> for MCPExecutionResultText {
    fn from(text: String) -> Self {
        Self {
            text,
            annotations: None,
        }
    }
}

impl From<&str> for MCPExecutionResultText {
    fn from(text: &str) -> Self {
        Self {
            text: text.to_string(),
            annotations: None,
        }
    }
}

/// An audio content block (base64-encoded on the wire; `data` holds the raw decoded bytes here).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPExecutionResultAudio {
    pub mime_type: String,
    pub data: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<MCPExecutionResultAnnotations>,
}

/// One icon entry (src/mimeType/sizes) attached to a resource's metadata or link.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPResourceIcons {
    pub src: String,
    pub mime_type: String,
    pub sizes: Vec<String>,
}

/// A bare resource reference — uri/name/title/description/mimeType, no content. This is what a
/// `resource_link` content block is per spec, and what [`MCPResourceResultBuilder::build`]
/// produces.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPResourceResultLink {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<MCPResourceIcons>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// A resource's actual text content, produced by [`MCPResourceResultBuilder::text`].
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPResourceResultText {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<MCPResourceIcons>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub text: String,
}

/// A resource's actual binary content (base64-encoded for you), produced by
/// [`MCPResourceResultBuilder::blob`].
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPResourceResultBlob {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<MCPResourceIcons>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub blob: String,
}

/// What a resource's `execute()` hands back per item: a bare reference ([`MCPResourceResultLink`]),
/// real inline content ([`MCPResourceResultText`]/[`MCPResourceResultBlob`]), or a live stream
/// ([`MCPExecutionResultStream`], for a resource that wants to push updates or run long enough to
/// need progress notifications). Build one via [`MCPResourceResult::new`]'s builder rather than
/// constructing a variant directly.
#[derive(Debug)]
pub enum MCPResourceResult {
    LINK(MCPResourceResultLink),
    TEXT(MCPResourceResultText),
    BLOB(MCPResourceResultBlob),
    STREAM(MCPExecutionResultStream),
}

/// Accumulates a resource result's shared metadata (uri/name/title/description/mimeType) before
/// you pick which variant to finish as. Get one from [`MCPResourceResult::new`].
pub struct MCPResourceResultBuilder {
    uri: String,
    name: String,
    title: Option<String>,
    description: Option<String>,
    icons: Option<Vec<MCPResourceIcons>>,
    mime_type: Option<String>,
}

impl MCPResourceResult {
    /// Starts building a resource result for the given uri/name. Chain `.title()`/
    /// `.description()`/`.mime_type()` as needed, then finish with exactly one of
    /// [`MCPResourceResultBuilder::build`] (a bare link, no content),
    /// [`MCPResourceResultBuilder::text`], or [`MCPResourceResultBuilder::blob`].
    pub fn new(uri: String, name: String) -> MCPResourceResultBuilder {
        MCPResourceResultBuilder {
            uri,
            name,
            title: None,
            description: None,
            icons: None,
            mime_type: None,
        }
    }
}

impl MCPResourceResultBuilder {
    pub fn title(&mut self, title: &str) -> &mut Self {
        self.title = Some(title.to_string());
        self
    }
    pub fn description(&mut self, description: &str) -> &mut Self {
        self.description = Some(description.to_string());
        self
    }
    pub fn mime_type(&mut self, mime_type: &str) -> &mut Self {
        self.mime_type = Some(mime_type.to_string());
        self
    }
    /// Finishes as a bare [`MCPResourceResultLink`] — just the reference, no content.
    pub fn build(&mut self) -> MCPResourceResult {
        MCPResourceResult::LINK(MCPResourceResultLink {
            uri: self.uri.clone(),
            name: self.name.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            icons: self.icons.clone(),
            mime_type: self.mime_type.clone(),
        })
    }
    /// Finishes as [`MCPResourceResultText`] with the given content.
    pub fn text(&mut self, text: &str) -> MCPResourceResult {
        MCPResourceResult::TEXT(MCPResourceResultText {
            uri: self.uri.clone(),
            name: self.name.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            icons: self.icons.clone(),
            mime_type: self.mime_type.clone(),
            text: text.to_string(),
        })
    }
    /// Finishes as [`MCPResourceResultBlob`], base64-encoding `data` for you.
    pub fn blob(&mut self, data: Vec<u8>) -> MCPResourceResult {
        let blob = general_purpose::STANDARD.encode(&data);
        MCPResourceResult::BLOB(MCPResourceResultBlob {
            uri: self.uri.clone(),
            name: self.name.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            icons: self.icons.clone(),
            mime_type: self.mime_type.clone(),
            blob,
        })
    }
}

/// A tool's or resource's own channel pair for streaming — build one with [`crate::stream_channel`]
/// inside your `execute()`, keep whichever ends your background task needs, and hand the rest
/// back wrapped in `MCPExecutionResult::STREAM`/`MCPResourceResult::STREAM`. `receiver` is what
/// the router relays to the client (any item with a `"method"` field is passed through as-is;
/// the first item without one is treated as the final answer); `sender` is where a reply to a
/// server-initiated request (e.g. `sampling/createMessage`) arrives back from the client, if your
/// tool sent one. See the crate's `README.md` streaming section for the full protocol and
/// worked examples.
#[derive(Debug)]
pub struct MCPExecutionResultStream {
    pub receiver: futures_channel::mpsc::Receiver<serde_json::Value>,
    pub sender: futures_channel::mpsc::Sender<serde_json::Value>,
}

/// One content block, as returned by [`MCPToolExecutor::execute`] or embedded in a
/// [`MCPPromptMessage`]. `STREAM` is not one content block among several — a tool that streams
/// returns a `Vec` containing exactly that one item, and the router dispatches the whole
/// response differently as a result (see [`crate::router::RouterResponse::Stream`]).
#[derive(Debug)]
pub enum MCPExecutionResult {
    TEXT(MCPExecutionResultText),
    IMAGE(MCPExecutionResultImage),
    AUDIO(MCPExecutionResultAudio),
    RESOURCE(MCPResourceResult),
    RAW(serde_json::Value),
    ERROR((String, Option<Value>)),
    STREAM(MCPExecutionResultStream),
}

/// The actual tool-running logic paired with an [`MCPTool`] impl. `cursor` is whatever opaque
/// string a previous call's returned cursor was (or `None` on a fresh call) — the router never
/// interprets it, it's yours to define (a row offset, a keyset, an upstream API token, whatever
/// fits your data). Returning `Some(cursor)` surfaces it to the client as this call's
/// `nextCursor`.
#[async_trait]
pub trait MCPToolExecutor: Send {
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>);
}

/// The actual resource-reading logic paired with an [`MCPResource`] impl.
#[async_trait]
pub trait MCPResourceExecutor: Send {
    /// Same cursor contract as [`MCPToolExecutor::execute`].
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>);
    /// For a templated resource ([`MCPResourceExecutor::is_template`] `true`), whether this type
    /// should handle a given request URI's `dsn`. Ignored for non-templated resources, which are
    /// matched by their exact registered URI instead.
    fn serves(dsn: &udsn::DSN) -> bool
    where
        Self: Sized;
    /// Whether this resource represents a URI template (many possible URIs, one handler type) or
    /// a single fixed URI.
    fn is_template() -> bool
    where
        Self: Sized;
}

/// One icon entry (src/mimeType/sizes) attached to a tool/resource/prompt's [`MCPMeta`].
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPMetaIcon {
    pub src: String,
    pub mime_type: String,
    pub sizes: Vec<String>,
}

/// Registration metadata for a tool, resource, or prompt — what shows up in `tools/list`,
/// `resources/list`, or `prompts/list`. Build one with the [`MCPMeta::new`] builder; `uri` only
/// matters for resources (name/title/description/icons are shared across all three primitives).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub uri: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<MCPMetaIcon>>,
}

impl MCPMeta {
    /// Starts a new, empty builder. Chain `.uri()`/`.name()`/`.title()`/`.description()`/
    /// `.mime_type()` as needed, then finish with [`MCPMeta::build`].
    pub fn new() -> Self {
        Self {
            uri: "".to_string(),
            name: "".to_string(),
            title: None,
            description: None,
            mime_type: None,
            icons: None,
        }
    }
    pub fn uri(&mut self, uri: &str) -> &mut Self {
        self.uri = uri.to_string();
        self
    }
    pub fn name(&mut self, name: &str) -> &mut Self {
        self.name = name.to_string();
        self
    }
    pub fn title(&mut self, title: &str) -> &mut Self {
        self.title = Some(title.to_string());
        self
    }
    pub fn description(&mut self, description: &str) -> &mut Self {
        self.description = Some(description.to_string());
        self
    }
    pub fn mime_type(&mut self, mime_type: &str) -> &mut Self {
        self.mime_type = Some(mime_type.to_string());
        self
    }
    pub fn build(&mut self) -> Self {
        self.to_owned()
    }
}

/// The `resources/templates/list` shape of a resource's metadata — same fields as [`MCPMeta`],
/// but `uri` becomes `uri_template` to match what the spec calls it in this context. Built from
/// an [`MCPMeta`] via [`MCPTemplateMeta::from_meta`], not constructed directly.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPTemplateMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub uri_template: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<MCPMetaIcon>>,
}

impl MCPTemplateMeta {
    pub fn from_meta(m: &MCPMeta) -> Self {
        Self {
            title: m.title.clone(),
            uri_template: m.uri.clone(),
            name: m.name.clone(),
            description: m.description.clone(),
            mime_type: m.mime_type.clone(),
            icons: m.icons.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry() {
        let r = Registry::new_from(
            HashMap::from([(
                "t1".to_string(),
                &Info {
                    name: "abc",
                    info_type: InfoType::Tool,
                    from_args: |_| FromArgResult::Error("tool".to_string()),
                    is_template: || false,
                    serves: |_| false,
                    complete: |_, _| None,
                    params: || serde_json::Value::String("".to_string()),
                    meta: || {
                        vec![MCPMeta {
                            title: None,
                            uri: "".to_string(),
                            name: "".to_string(),
                            description: None,
                            mime_type: None,
                            icons: None,
                        }]
                    },
                },
            )]),
            HashMap::from([(
                "r1".to_string(),
                &Info {
                    name: "xyz",
                    info_type: InfoType::Resource,
                    from_args: |_| FromArgResult::Error("resource".to_string()),
                    params: || serde_json::Value::String("".to_string()),
                    is_template: || false,
                    serves: |_| false,
                    complete: |_, _| None,
                    meta: || {
                        vec![MCPMeta {
                            title: None,
                            uri: "".to_string(),
                            name: "".to_string(),
                            description: None,
                            mime_type: None,
                            icons: None,
                        }]
                    },
                },
            )]),
        );

        assert_eq!(r.tools().len(), 1);
        assert_eq!(r.get_tool("t1").unwrap().name, "abc");
        assert!(r.get_tool("r1").is_none());
        assert_eq!(r.resources().len(), 1);
        assert_eq!(r.get_resource("r1").unwrap().name, "xyz");
        assert!(r.get_resource("t1").is_none());
    }
}
