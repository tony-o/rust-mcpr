use base64::{Engine as _, engine::general_purpose};

#[derive(serde::Deserialize, serde::Serialize)]
enum RequestID {
    STRING(String),
    NUMBER(i64),
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Request {
    jsonrpc: String,
    id: RequestID,
    method: String,
    params: Option<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct ToolCall {
    name: String,
    arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ResourceCall {
    uri: String,
}

impl Request {
    pub fn from_str(v: &String) -> Result<Self, String> {
        serde_json::from_str(v).map_err(|e| format!("{}", e))
    }
}

pub struct Router;

impl Router {
    fn execution_result_to_mcp(
        mcper: Vec<registry::MCPExecutionResult>,
        content_key: String,
    ) -> serde_json::Value {
        let mut content: Vec<serde_json::Value> = Vec::new();
        let mut result = serde_json::Map::new();
        for mcpr in &mcper {
            match mcpr {
                registry::MCPExecutionResult::TEXT(s) => content.push(serde_json::json!({
                    "type": "text",
                    "text": s.to_string(),
                })),
                registry::MCPExecutionResult::AUDIO(a) => content.push(serde_json::json!({
                    "type": "audio",
                    "data": general_purpose::STANDARD.encode(&a.data),
                    "mimeType": a.mime_type,
                    "annotations": if let Some(b) = &a.annotations {
                        serde_json::json!({ "audience": b.audience, "priority": b.priority })
                    } else { serde_json::Value::Null },
                })),
                registry::MCPExecutionResult::IMAGE(a) => content.push(serde_json::json!({
                    "type": "image",
                    "data": general_purpose::STANDARD.encode(&a.data),
                    "mimeType": a.mime_type,
                })),
                registry::MCPExecutionResult::RAW(v) => content.push(v.clone()),
                registry::MCPExecutionResult::RESOURCE(r) => {
                    // TODO
                    return serde_json::json!({
                        "error": {"code": -32603, "message": format!("Server requested resource response for a resource the tool has no knowledge of {}", r)
                        }
                    });
                }
                registry::MCPExecutionResult::ERROR((s, _)) => {
                    if mcper.len() == 0 {
                        content.push(serde_json::json!({
                            "error": {"code": -32600, "message": s }
                        }))
                    } else {
                        content.push(
                            serde_json::json!({"type":"text", "text": format!("error: {}", s)}),
                        );
                        result.insert("isError".to_string(), serde_json::Value::Bool(true));
                    }
                }
            }
        }
        result.insert(content_key, serde_json::Value::Array(content));
        serde_json::json!({"result": result})
    }

    pub fn exec_from_value(v: serde_json::Value) -> serde_json::Value {
        match serde_json::from_value::<Request>(v) {
            Ok(a) => Router::exec(a),
            Err(_) => {
                serde_json::json!({"error": { "code": -32601, "message": "invalid request format, expected {jsonrpc:string, id:number|string, method:string, params:optional<object>}"}})
            }
        }
    }

    pub fn exec(req: Request) -> serde_json::Value {
        if req.method == "initialize" {
            let mut capabilities = serde_json::Map::new();
            if registry::registry().tools().len() > 0 {
                capabilities.insert(
                    "tools".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            if registry::registry().resources().len() > 0 {
                capabilities.insert(
                    "resources".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            return serde_json::json!({
                "protocolVersion": "2025-11-25",
                "capabilities": capabilities,
                "serverInfo": {
                    "name": "name",
                    "title": "title",
                    "version": "1.0.0",
                    "description": "an example mcp",
                },
            });
        } else if req.method == "tools/list" {
            return serde_json::json!({"result": { "tools": registry::registry().tools().iter().map(|(_, i)| (i.params)()).collect::<Vec<_>>() } });
        } else if req.method == "tools/call" {
            if let Ok(tool_call) = serde_json::from_value::<ToolCall>(
                req.params.clone().unwrap_or(serde_json::json!({})),
            ) {
                if let Some(tool) = registry::registry().get_tool(tool_call.name.clone()) {
                    if let Ok(Ok(caller)) = (tool.from_args)(
                        &tool_call.arguments.clone().unwrap_or(serde_json::json!({})),
                    ) {
                        let executor = caller.get_executor();
                        return Router::execution_result_to_mcp(
                            executor.execute(),
                            "content".to_string(),
                        );
                    }
                    return serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for tools/call {}", tool_call.name)}});
                }
                return serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for tools/call, \"name\" is required")}});
            }
            return serde_json::json!({"error": { "code": -32602, "message": "malformed request from LLM"}});
        } else if req.method == "resources/list" {
            // TODO: paging
            let resources: Vec<registry::MCPMeta> = registry::registry()
                .resources()
                .iter()
                .map(|(_, i)| (i.meta)())
                .collect();
            return serde_json::json!({"result": {"resources": resources }});
        } else if req.method == "resources/read" {
            if let Ok(resource_call) = serde_json::from_value::<ResourceCall>(
                req.params.clone().unwrap_or(serde_json::json!({})),
            ) {
                if let Some(r) =
                    registry::registry().get_resource(resource_call.clone().uri.clone())
                {
                    // exact match
                    if let Ok(Err(a)) =
                        (r.from_args)(&serde_json::json!({ "dsn": resource_call.uri.clone() }))
                    {
                        return Router::execution_result_to_mcp(
                            a.get_executor().execute(),
                            "contents".to_string(),
                        );
                    } else {
                        return serde_json::json!({"error": { "code": -32603, "message": "Internal error: resource structs may only contain a DSN field or must be empty"}});
                    }
                } else {
                    {
                        let dsn = match udsn::DSN::parse(resource_call.clone().uri) {
                            Some(d) => d,
                            _ => {
                                return serde_json::json!({"error": { "code": -32602, "message": "malformed requested, expected uri in params"}});
                            }
                        };
                        for (_, i) in registry::registry().resources().iter() {
                            if let Ok(Err(a)) = (i.from_args)(
                                &serde_json::json!({ "dsn": resource_call.clone().uri }),
                            ) && a.get_executor().serves(&dsn)
                            {
                                return Router::execution_result_to_mcp(
                                    a.get_executor().execute(),
                                    "contents".to_string(),
                                );
                            }
                        }
                    }
                };
                return serde_json::json!({"error": {"code": -32601, "message": "no valid resource handler found for requested uri"}});
            }
            return serde_json::json!({"error": { "code": -32602, "message": format!("malformed request from LLM: {}", req.method)}});
        }
        // Method not found: -32601 (Capability not supported)
        // Invalid prompt name: -32602 (Invalid params)
        // Missing required arguments: -32602 (Invalid params)
        // Internal errors: -32603 (Internal error)
        serde_json::json!({"error": { "code": -32601, "message": format!("method not found: {}", req.method)}})
    }
}

#[cfg(test)]
mod tests {
    use super::{Request, RequestID, Router};
    use serde_json::json;

    #[test]
    fn basic_router() {
        let resp = Router::exec(Request {
            jsonrpc: "2.0".to_string(),
            id: RequestID::NUMBER(123),
            method: "tools/list".to_string(),
            params: json!({
                "test": 15,
                "oooptional": 5,
            })
            .into(),
        });
        let cmp = json!({
            "result": { "tools": [
                { "description": "abc camel description",
                  "title": "ABCCamel struct",
                  "name": "abc_camel",
                  "inputSchema": {
                      "type": "object",
                      "properties": {
                          "oooptional": { "type": "integer" },
                          "test": { "type": "integer" },
                          "arr": { "type": "array", "items": { "type": "integer" } },
                          "ooarr": { "type": "array", "items": { "type": "integer" } },
                      },
                      "required": ["test", "arr"],
                  }
               },
            ]}
        }
        );
        assert_eq!(cmp, resp);
    }

    #[test]
    fn basic_tool_call() {
        let resp = Router::exec(Request {
            jsonrpc: "2.0".to_string(),
            id: RequestID::NUMBER(42),
            method: "tools/call".to_string(),
            params: json!({
                "name": "abc_camel",
                "arguments": {
                    "test": 15,
                    "arr": [5],
                }
            })
            .into(),
        });
        let cmp = json!({
            "result": {
                "content": [{"type": "text", "text": "test=15,oooptional=-1,arr=[5],ooarr=[]"}],
            }
        });
        assert_eq!(cmp, resp);
    }

    #[test]
    fn basic_tool_call_err() {
        let resp = Router::exec(Request {
            jsonrpc: "2.0".to_string(),
            id: RequestID::NUMBER(42),
            method: "tools/call".to_string(),
            params: json!({
                "name": "abc_camel",
                "arguments": {
                    "arr": [5],
                }
            })
            .into(),
        });
        let cmp = json!({
            "error": {
                "code": -32602,
                "message": "invalid parameters for tools/call abc_camel",
            }
        });
        println!("{}", resp);
        assert_eq!(cmp, resp);
    }

    #[test]
    fn basic_resource_list() {
        let resp = Router::exec(Request {
            jsonrpc: "2.0".to_string(),
            id: RequestID::NUMBER(42),
            method: "resources/list".to_string(),
            params: None,
        });
        let cmp = json!({
            "result": {
                "resources": [
                    {"title": "TestResource"
                    ,"description": "a test resource"
                    ,"uri": "git://some-repo"
                    }
                ],
            }
        });
        assert_eq!(cmp, resp);
    }
    #[test]
    fn basic_resource_call() {
        let resp = Router::exec(Request {
            jsonrpc: "2.0".to_string(),
            id: RequestID::NUMBER(42),
            method: "resources/read".to_string(),
            params: Some(json!({ "uri": "git://some-repo" })),
        });
        let cmp = json!({
            "result": {
                "contents": [
                    {"type": "text",
                     "text": "git://some-repo"
                    },
                    {"type": "text",
                     "text": "oper-emos//:tig"
                    }
                ],
            }
        });
        assert_eq!(cmp, resp);
    }
}
