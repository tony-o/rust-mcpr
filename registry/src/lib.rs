use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

#[doc(hidden)]
pub use inventory as _i;

#[derive(Debug, Clone, PartialEq)]
pub enum InfoType {
    TOOL,
    RESOURCE,
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
    tools: HashMap<String, &'static Info>,
    resources: HashMap<String, &'static Info>,
    resource_instances: HashMap<String, Box<dyn MCPResource + Send + Sync>>,
}

impl Registry {
    fn new() -> Self {
        let mut tools = HashMap::new();
        let mut resources = HashMap::new();
        for i in inventory::iter::<Info>() {
            if i.info_type == InfoType::TOOL {
                tools.insert(i.name.to_string(), i);
            } else {
                resources.insert((i.meta)().uri.to_string(), i);
            }
        }
        Registry::new_from(tools, resources)
    }

    fn new_from(
        tools: HashMap<String, &'static Info>,
        resources: HashMap<String, &'static Info>,
    ) -> Self {
        Self {
            tools,
            resources,
            resource_instances: HashMap::new(),
        }
    }

    pub fn get_tool(&self, name: &str) -> Option<&Info> {
        Some(self.tools.get(name)?)
    }

    pub fn get_resource(&self, name: &str) -> Option<&Info> {
        Some(self.resources.get(name)?)
    }

    pub fn tools(&self) -> &HashMap<String, &Info> {
        &self.tools
    }

    pub fn resources(&self) -> &HashMap<String, &Info> {
        &self.resources
    }

    pub fn register_resource_adapter(
        &mut self,
        name: String,
        resource: Box<dyn MCPResource + Send + Sync>,
    ) {
        self.resource_instances.insert(name.clone(), resource);
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::new)
}

pub trait MCPTool {
    fn get_executor(&self) -> Box<&dyn MCPToolExecutor>;
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
    fn get_executor(&self) -> Box<&dyn MCPResourceExecutor>;
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
    pub fn builder(uri: String, name: String) -> Self {
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<MCPMetaIcon>>,
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
                    info_type: InfoType::TOOL,
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
                    info_type: InfoType::RESOURCE,
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
