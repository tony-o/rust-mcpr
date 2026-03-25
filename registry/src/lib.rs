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

#[derive(Debug, Clone)]
pub struct Info {
    pub name: &'static str,
    pub info_type: InfoType,
    pub params: fn() -> Value,
    pub meta: fn() -> MCPMeta,
    pub from_args:
        fn(&serde_json::Value) -> Result<Result<Box<dyn MCPTool>, Box<dyn MCPResource>>, String>,
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
                resources.insert(
                    (i.meta)()
                        .uri
                        .unwrap_or_else(|| {
                            panic!("{} must have a URI defined, please see the docs", i.name)
                        })
                        .to_string(),
                    i,
                );
            }
        }
        Registry::new_from(tools.clone(), resources.clone())
    }

    fn new_from(
        tools: HashMap<String, &'static Info>,
        resources: HashMap<String, &'static Info>,
    ) -> Self {
        Self {
            tools: tools.clone(),
            resources: resources.clone(),
            resource_instances: HashMap::new(),
        }
    }

    pub fn get_tool(&self, name: String) -> Option<&Info> {
        Some(self.tools.clone().get(&name)?)
    }

    pub fn get_resource(&self, name: String) -> Option<&Info> {
        Some(self.resources.clone().get(&name)?)
    }

    pub fn tools(&self) -> HashMap<String, &Info> {
        self.tools.clone()
    }

    pub fn resources(&self) -> HashMap<String, &Info> {
        self.resources.clone()
    }

    /*
      "resources": [
        {
          "uri": "git://github.com/org/repo",
          "name": "repo",
          "title": "My Repository",
          "description": "Source code for the main application",
          "mimeType": "text/plain"
        },
        {
          "uri": "file:///etc/config/app.yaml",
          "name": "app-config",
          "title": "Application Config",
          "description": "Main application configuration file",
          "mimeType": "application/yaml"
        },
        {
          "uri": "https://api.example.com/data/users",
          "name": "users",
          "title": "Users API",
          "description": "REST endpoint returning user records",
          "mimeType": "application/json"
        },
        {
          "uri": "catalog://entity-types",
          "name": "entity-types",
          "title": "Entity Types",
          "description": "All entity types defined in the catalog",
          "mimeType": "application/json"
        }
    */
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
    fn from_args(
        v: &serde_json::Value,
    ) -> Result<Result<Box<dyn MCPTool>, Box<dyn MCPResource>>, String>
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
    fn from_args(
        v: &serde_json::Value,
    ) -> Result<Result<Box<dyn MCPTool>, Box<dyn MCPResource>>, String>
    where
        Self: Sized;
}

pub struct MCPExecutionResultImage {
    pub mime_type: String,
    pub data: Vec<u8>,
}

pub struct MCPExecutionResultAudioAnnotations {
    pub audience: Vec<String>,
    pub priority: f32,
}

pub struct MCPExecutionResultAudio {
    pub mime_type: String,
    pub data: Vec<u8>,
    pub annotations: Option<MCPExecutionResultAudioAnnotations>,
}

pub enum MCPExecutionResult {
    TEXT(String),
    IMAGE(MCPExecutionResultImage),
    AUDIO(MCPExecutionResultAudio),
    RESOURCE(String),
    RAW(serde_json::Value),
    ERROR((String, Option<Value>)),
}

pub trait MCPToolExecutor {
    fn execute(&self) -> Vec<MCPExecutionResult>;
}
pub trait MCPResourceExecutor {
    fn execute(&self) -> Vec<MCPExecutionResult>;
    fn serves(&self, dsn: &udsn::DSN) -> bool;
}
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPMetaIcon {
    pub src: String,
    pub mime_type: String,
    pub sizes: Vec<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPMeta {
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<MCPMetaIcon>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry() {
        let r = Registry::new_from(
            Some(HashMap::from([(
                "t1".to_string(),
                &Info {
                    name: "abc",
                    path: "abc",
                    fields: &["abc1"],
                    info_type: InfoType::TOOL,
                },
            )])),
            Some(HashMap::from([(
                "r1".to_string(),
                &Info {
                    name: "xyz",
                    path: "xyz",
                    fields: &["xyz1"],
                    info_type: InfoType::RESOURCE,
                },
            )])),
        );

        assert_eq!(r.tools().unwrap().len(), 1);
        assert_eq!(r.get_tool(String::from("t1")).unwrap().name, "abc");
        assert!(r.get_tool(String::from("r1")).is_none());
        assert_eq!(r.resources().unwrap().len(), 1);
        assert_eq!(r.get_resource(String::from("r1")).unwrap().name, "xyz");
        assert!(r.get_resource(String::from("t1")).is_none());
    }
}
