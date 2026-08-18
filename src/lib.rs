extern crate self as mcp_router;

pub use futures_channel::mpsc::{channel as stream_channel, Receiver as StreamReceiver, Sender as StreamSender};
pub use futures_util::{SinkExt, StreamExt};
pub use mcp_router_macros::{MCPPrompt, MCPResource, MCPTool};
pub mod registry;
pub mod router;

#[cfg(test)]
mod tests {
    use crate::registry::{
        MCPExecutionResult, MCPPrompt, MCPPromptExecutor, MCPPromptMessage, MCPPromptResult,
        MCPResource, MCPResourceExecutor, MCPResourceResult, MCPTool, MCPToolExecutor,
    };
    use async_trait::async_trait;
    use mcp_router_macros::{MCPPrompt, MCPResource, MCPTool};
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    #[derive(MCPTool, Deserialize, Serialize)]
    #[meta(title = "ABCCamel struct", description = "abc camel description")]
    struct ABCCamel {
        test: u32,
        oooptional: Option<i16>,
        arr: Vec<i32>,
        ooarr: Option<Vec<i32>>,
    }

    #[async_trait]
    impl MCPToolExecutor for ABCCamel {
        async fn execute(
            &self,
            _cursor: Option<String>,
        ) -> (Vec<MCPExecutionResult>, Option<String>) {
            (
                vec![MCPExecutionResult::TEXT(
                    format!(
                        "test={},oooptional={},arr={:?},ooarr={:?}",
                        self.test,
                        self.oooptional.unwrap_or(-1),
                        self.arr,
                        self.ooarr.clone().unwrap_or(vec![])
                    )
                    .into(),
                )],
                None,
            )
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

    #[async_trait]
    impl MCPResourceExecutor for TestResource {
        async fn execute(
            &self,
            _cursor: Option<String>,
        ) -> (Vec<MCPResourceResult>, Option<String>) {
            (
                vec![
                    MCPResourceResult::new("test://forward".to_string(), self.dsn.to_string())
                        .build(),
                    MCPResourceResult::new(
                        "test://reverse".to_string(),
                        self.dsn.to_string().chars().rev().collect(),
                    )
                    .build(),
                ],
                None,
            )
        }

        fn serves(_dsn: &udsn::DSN) -> bool {
            true
        }

        fn is_template() -> bool {
            false
        }
    }

    #[derive(MCPPrompt, Deserialize)]
    #[meta(title = "TestPrompt", description = "rot13s the given string")]
    struct TestPrompt {
        #[arg(description = "the string to rotate")]
        string_to_rot13: String,
    }

    #[async_trait]
    impl MCPPromptExecutor for TestPrompt {
        async fn execute(&self) -> MCPPromptResult {
            MCPPromptResult {
                description: None,
                messages: vec![MCPPromptMessage {
                    role: "user".to_string(),
                    content: MCPExecutionResult::TEXT(
                        self.string_to_rot13
                            .chars()
                            .map(|c| match c {
                                'a'..='m' | 'A'..='M' => ((c as u8) + 13) as char,
                                'n'..='z' | 'N'..='Z' => ((c as u8) - 13) as char,
                                _ => c,
                            })
                            .collect::<String>()
                            .into(),
                    ),
                }],
            }
        }
    }

    #[test]
    fn basic_registry_tool_test() {
        assert!(super::registry::registry().tools().len() == 1);
        assert!(super::registry::registry().resources().len() == 1);
        assert!(super::registry::registry().prompts().len() == 1);
        assert!(
            super::registry::registry()
                .get_resource("git://some-repo")
                .is_some()
        );
        assert!(
            super::registry::registry()
                .get_prompt("TestPrompt")
                .is_some()
        );
    }

    #[test]
    fn prompt_argument_description_is_present_in_params() {
        assert_eq!(
            TestPrompt::params()["arguments"],
            serde_json::json!([{
                "name": "string_to_rot13",
                "description": "the string to rotate",
                "required": true
            }])
        );
    }
}
