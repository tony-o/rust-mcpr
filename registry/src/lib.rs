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
}

pub enum FromArgResult {
    Tool(Box<dyn MCPTool>),
    Resource(Box<dyn MCPResource>),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct Info {
    pub name: &'static str,
    pub info_type: InfoType,
    pub params: fn() -> Value,
    pub meta: fn() -> MCPMeta,
    pub from_args: fn(&serde_json::Value) -> FromArgResult,
    pub is_template: fn() -> bool,
    pub serves: fn(&udsn::DSN) -> bool,
}

inventory::collect!(Info);

pub struct Registry {
    tools: RwLock<HashMap<String, &'static Info>>,
    resources: RwLock<HashMap<String, &'static Info>>,
}

impl Registry {
    fn new() -> Self {
        let mut tools = HashMap::new();
        let mut resources = HashMap::new();
        for i in inventory::iter::<Info>() {
            if i.info_type == InfoType::Tool {
                tools.insert(i.name.to_string(), i);
            } else {
                resources.insert((i.meta)().uri.to_string(), i);
            }
        }
        Registry::new_from(tools, resources)
    }

    pub fn new_from(
        tools: HashMap<String, &'static Info>,
        resources: HashMap<String, &'static Info>,
    ) -> Self {
        Self {
            tools: RwLock::new(tools),
            resources: RwLock::new(resources),
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
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::new)
}

pub trait MCPTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor;
    fn meta() -> MCPMeta
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
    fn meta() -> MCPMeta
    where
        Self: Sized;
    fn params() -> Value
    where
        Self: Sized;
    fn from_args(v: &serde_json::Value) -> FromArgResult
    where
        Self: Sized;
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPExecutionResultImage {
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPExecutionResultAudioAnnotations {
    pub audience: Vec<String>,
    pub priority: f32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPExecutionResultAudio {
    pub mime_type: String,
    pub data: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<MCPExecutionResultAudioAnnotations>,
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
pub struct MCPResourceResult {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl MCPResourceResult {
    pub fn new(uri: String, name: String) -> Self {
        Self {
            uri,
            name,
            title: None,
            description: None,
            icons: None,
            mime_type: None,
            size: None,
            blob: None,
            text: None,
        }
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
    pub fn size(&mut self, size: u64) -> &mut Self {
        self.size = Some(size);
        self
    }
    pub fn blob(&mut self, data: Vec<u8>) -> &mut Self {
        let blob = general_purpose::STANDARD.encode(&data).to_string();
        self.blob = Some(blob);
        self
    }
    pub fn text(&mut self, text: &str) -> &mut Self {
        self.text = Some(text.to_string());
        self
    }
    pub fn build(&mut self) -> Self {
        self.to_owned()
    }
}

pub enum MCPExecutionResult {
    TEXT(String),
    IMAGE(MCPExecutionResultImage),
    AUDIO(MCPExecutionResultAudio),
    RESOURCE(MCPResourceResult),
    RAW(serde_json::Value),
    ERROR((String, Option<Value>)),
}

pub trait MCPToolExecutor {
    fn execute(&self) -> Vec<MCPExecutionResult>;
}

pub trait MCPResourceExecutor {
    fn execute(&self) -> Vec<MCPResourceResult>;
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
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
                    params: || serde_json::Value::String("".to_string()),
                    meta: || MCPMeta {
                        title: None,
                        uri: "".to_string(),
                        name: "".to_string(),
                        description: None,
                        mime_type: None,
                        icons: None,
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
                    meta: || MCPMeta {
                        title: None,
                        uri: "".to_string(),
                        name: "".to_string(),
                        description: None,
                        mime_type: None,
                        icons: None,
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
