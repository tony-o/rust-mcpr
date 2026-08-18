use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock, RwLockReadGuard};
use tracing::{error, warn};

use serde_json::Value;

#[doc(hidden)]
pub use inventory as _i;

#[derive(Debug, Clone, PartialEq)]
pub enum InfoType {
    Tool,
    Resource,
    Prompt,
}

pub enum FromArgResult {
    Tool(Box<dyn MCPTool + Send>),
    Resource(Box<dyn MCPResource + Send>),
    Prompt(Box<dyn MCPPrompt + Send>),
    Error(String),
}

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

    pub fn get_tool(&self, name: &str) -> Option<&'static Info> {
        match self.tools.read() {
            Ok(t) => t.get(name).copied(),
            Err(e) => Some(e.into_inner().get(name)?),
        }
    }

    pub fn get_resource(&self, uri: &str) -> Option<&'static Info> {
        match self.resources.read() {
            Ok(t) => t.get(uri).copied(),
            Err(e) => Some(e.into_inner().get(uri)?),
        }
    }

    pub fn get_prompt(&self, name: &str) -> Option<&'static Info> {
        match self.prompts.read() {
            Ok(t) => t.get(name).copied(),
            Err(e) => Some(e.into_inner().get(name)?),
        }
    }

    pub fn tools(&self) -> RwLockReadGuard<'_, HashMap<String, &'static Info>> {
        match self.tools.read() {
            Ok(t) => t,
            Err(e) => {
                error!("Error reading from tool lock: {}", e);
                panic!("Error reading from tool lock: {}", e);
            }
        }
    }

    pub fn resources(&self) -> RwLockReadGuard<'_, HashMap<String, &'static Info>> {
        match self.resources.read() {
            Ok(t) => t,
            Err(e) => {
                error!("Error reading from resources lock: {}", e);
                panic!("Error reading from resources lock: {}", e);
            }
        }
    }

    pub fn prompts(&self) -> RwLockReadGuard<'_, HashMap<String, &'static Info>> {
        match self.prompts.read() {
            Ok(t) => t,
            Err(e) => {
                error!("Error reading from prompts lock: {}", e);
                panic!("Error reading from prompts lock: {}", e);
            }
        }
    }

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

pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::new)
}

pub trait MCPTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor;
    fn meta() -> Vec<MCPMeta>
    where
        Self: Sized;
    fn params() -> Value
    where
        Self: Sized;
    fn from_args(v: &serde_json::Value) -> FromArgResult
    where
        Self: Sized;
}
pub trait MCPResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor;
    fn meta() -> Vec<MCPMeta>
    where
        Self: Sized;
    fn params() -> Value
    where
        Self: Sized;
    fn from_args(v: &serde_json::Value) -> FromArgResult
    where
        Self: Sized;
    fn complete(_argument_name: &str, _partial_value: &str) -> Option<Vec<String>>
    where
        Self: Sized,
    {
        None
    }
}
pub trait MCPPrompt {
    fn get_executor(&self) -> &dyn MCPPromptExecutor;
    fn meta() -> Vec<MCPMeta>
    where
        Self: Sized;
    fn params() -> Value
    where
        Self: Sized;
    fn from_args(v: &serde_json::Value) -> FromArgResult
    where
        Self: Sized;
    fn complete(_argument_name: &str, _partial_value: &str) -> Option<Vec<String>>
    where
        Self: Sized,
    {
        None
    }
}

#[derive(Debug)]
pub struct MCPPromptMessage {
    pub role: String,
    pub content: MCPExecutionResult,
}

#[derive(Debug)]
pub struct MCPPromptResult {
    pub description: Option<String>,
    pub messages: Vec<MCPPromptMessage>,
}

#[async_trait]
pub trait MCPPromptExecutor: Send {
    async fn execute(&self) -> MCPPromptResult;
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPExecutionResultAnnotations {
    pub audience: Vec<String>,
    pub priority: f32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPExecutionResultImage {
    pub mime_type: String,
    pub data: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<MCPExecutionResultAnnotations>,
}

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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPExecutionResultAudio {
    pub mime_type: String,
    pub data: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<MCPExecutionResultAnnotations>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPResourceIcons {
    pub src: String,
    pub mime_type: String,
    pub sizes: Vec<String>,
}

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

#[derive(Debug)]
pub enum MCPResourceResult {
    LINK(MCPResourceResultLink),
    TEXT(MCPResourceResultText),
    BLOB(MCPResourceResultBlob),
    STREAM(MCPExecutionResultStream),
}

pub struct MCPResourceResultBuilder {
    uri: String,
    name: String,
    title: Option<String>,
    description: Option<String>,
    icons: Option<Vec<MCPResourceIcons>>,
    mime_type: Option<String>,
}

impl MCPResourceResult {
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

#[derive(Debug)]
pub struct MCPExecutionResultStream {
    pub receiver: futures_channel::mpsc::Receiver<serde_json::Value>,
    pub sender: futures_channel::mpsc::Sender<serde_json::Value>,
}

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

#[async_trait]
pub trait MCPToolExecutor: Send {
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>);
}

#[async_trait]
pub trait MCPResourceExecutor: Send {
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>);
    fn serves(dsn: &udsn::DSN) -> bool
    where
        Self: Sized;
    fn is_template() -> bool
    where
        Self: Sized;
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPMetaIcon {
    pub src: String,
    pub mime_type: String,
    pub sizes: Vec<String>,
}

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
