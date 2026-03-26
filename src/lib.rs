pub use macros::{MCPResource, MCPTool};
pub use registry;
pub mod router;

#[cfg(test)]
mod tests {
    use macros::{MCPResource, MCPTool};
    use registry::{
        MCPExecutionResult, MCPResource, MCPResourceExecutor, MCPResourceResult, MCPTool,
        MCPToolExecutor,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};

    #[derive(MCPTool, Deserialize, Serialize)]
    #[meta(title = "ABCCamel struct", description = "abc camel description")]
    struct ABCCamel {
        test: u32,
        oooptional: Option<i16>,
        arr: Vec<i32>,
        ooarr: Option<Vec<i32>>,
    }

    impl MCPToolExecutor for ABCCamel {
        fn execute(&self) -> Vec<MCPExecutionResult> {
            vec![MCPExecutionResult::TEXT(format!(
                "test={},oooptional={},arr={:?},ooarr={:?}",
                self.test,
                self.oooptional.unwrap_or(-1),
                self.arr,
                self.ooarr.clone().unwrap_or(vec![])
            ))]
        }
    }

    #[derive(MCPResource, Deserialize, Serialize)]
    #[meta(
        title = "TestResource",
        description = "a test resource",
        uri = "git://some-repo"
    )]
    struct TestResource {
        dsn: udsn::DSN,
    }

    impl MCPResourceExecutor for TestResource {
        fn execute(&self) -> Vec<MCPResourceResult> {
            vec![
                MCPResourceResult::new("test://forward".to_string(), self.dsn.to_string()),
                MCPResourceResult::new(
                    "test://reverse".to_string(),
                    self.dsn.to_string().chars().rev().collect(),
                ),
            ]
        }

        fn serves(_dsn: &udsn::DSN) -> bool {
            true
        }

        fn is_template() -> bool {
            false
        }
    }

    #[test]
    fn basic_registry_tool_test() {
        assert!(super::registry::registry().tools().len() == 1);
        assert!(super::registry::registry().resources().len() == 1);
        assert!(
            super::registry::registry()
                .get_resource("git://some-repo")
                .is_some()
        );
    }
}
