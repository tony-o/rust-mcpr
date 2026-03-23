pub use macros::{MCPResource, MCPTool};
pub use registry;
pub mod router;

#[cfg(test)]
mod tests {
    use super::MCPTool;
    use registry::MCPTool;
    use serde::{Deserialize, Serialize};
    use serde_json::{Map, Value, json};

    #[derive(MCPTool, Deserialize, Serialize)]
    #[meta(title = "ABCCamel struct", description = "abc camel description")]
    struct ABCCamel {
        test: u32,
        oooptional: Option<i16>,
        arr: Vec<i32>,
        ooarr: Option<Vec<i32>>,
    }

    #[test]
    fn basic_registry_tool_test() {
        assert_eq!(super::registry::registry().tools().unwrap().len(), 1);
        assert!(super::registry::registry().resources().is_none());
    }
}
