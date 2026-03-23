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
}

inventory::collect!(Info);

pub struct Registry {
    tools: Option<HashMap<String, &'static Info>>,
    resources: Option<HashMap<String, &'static Info>>,
}

impl Registry {
    fn new() -> Self {
        let mut tools = HashMap::new();
        let mut resources = HashMap::new();
        for i in inventory::iter::<Info>() {
            if i.info_type == InfoType::TOOL {
                tools.insert(i.name.to_string(), i);
            } else {
                resources.insert(i.name.to_string(), i);
            }
        }
        Registry::new_from(
            if tools.is_empty() {
                None
            } else {
                Some(tools.clone())
            },
            if resources.is_empty() {
                None
            } else {
                Some(resources.clone())
            },
        )
    }

    fn new_from(
        tools: Option<HashMap<String, &'static Info>>,
        resources: Option<HashMap<String, &'static Info>>,
    ) -> Self {
        Self {
            tools: tools.clone(),
            resources: resources.clone(),
        }
    }

    pub fn get_tool(&self, name: String) -> Option<&Info> {
        Some(self.tools.clone()?.get(&name)?)
    }

    pub fn get_resource(&self, name: String) -> Option<&Info> {
        Some(self.resources.clone()?.get(&name)?)
    }

    pub fn tools(&self) -> Option<HashMap<String, &Info>> {
        self.tools.clone()
    }

    pub fn resources(&self) -> Option<HashMap<String, &Info>> {
        self.resources.clone()
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::new)
}

pub trait MCPTool {
    fn info() -> Info;
    fn params() -> Value;
    fn from_args(v: serde_json::Map<String, Value>) -> Result<Self, String>
    where
        Self: Sized;
}
pub trait MCPResource {
    fn info() -> Info;
    fn params() -> Value;
    fn from_args(v: serde_json::Map<String, Value>) -> Result<Self, String>
    where
        Self: Sized;
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
