use base64::{Engine as _, engine::general_purpose};

#[derive(serde::Deserialize, serde::Serialize)]
enum RequestID {
    Str(String),
    Number(i64),
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
                registry::MCPExecutionResult::AUDIO(a) => {
                    let mut v = serde_json::Map::new();
                    v.insert(
                        "type".to_string(),
                        serde_json::Value::String("audio".to_string()),
                    );
                    v.insert(
                        "data".to_string(),
                        serde_json::Value::String(
                            general_purpose::STANDARD.encode(&a.data).to_string(),
                        ),
                    );
                    v.insert(
                        "mimeType".to_string(),
                        serde_json::Value::String(a.mime_type.to_string()),
                    );
                    if let Some(b) = &a.annotations {
                        v.insert("annotations".to_string(), serde_json::to_value(b).unwrap());
                    }
                    content.push(serde_json::Value::Object(v));
                }
                registry::MCPExecutionResult::IMAGE(a) => content.push(serde_json::json!({
                    "type": "image",
                    "data": general_purpose::STANDARD.encode(&a.data),
                    "mimeType": a.mime_type,
                })),
                registry::MCPExecutionResult::RAW(v) => content.push(v.clone()),
                registry::MCPExecutionResult::RESOURCE(r) => {
                    content.push(
                        serde_json::to_value(r)
                            .unwrap_or_else(|e|
                                serde_json::json!({"type": "text",
                                                   "text": format!("error: {:?} serializing result: {}", r, e)
                                })));
                }
                registry::MCPExecutionResult::ERROR((s, _)) => {
                    content
                        .push(serde_json::json!({"type":"text", "text": format!("error: {}", s)}));
                    if content_key == "content" {
                        result.insert("isError".to_string(), serde_json::Value::Bool(true));
                    } else {
                        return serde_json::json!({"error": { "code": -32002, "message": s } });
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
                serde_json::json!({"jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "invalid request format, expected {jsonrpc:string, id:number|string, method:string, params:optional<object>}"}})
            }
        }
    }

    pub fn exec(req: Request) -> serde_json::Value {
        match Router::execx(&req) {
            serde_json::Value::Object(mut result_map) => {
                result_map.insert(
                    "jsonrpc".to_string(),
                    serde_json::Value::String(req.jsonrpc),
                );
                result_map.insert(
                    "id".to_string(),
                    match req.id {
                        RequestID::Number(a) => serde_json::Value::Number(a.into()),
                        RequestID::Str(a) => serde_json::Value::String(a),
                    },
                );
                serde_json::Value::Object(result_map)
            }
            a => a,
        }
    }

    fn execx(req: &Request) -> serde_json::Value {
        if req.method == "initialize" {
            let mut capabilities = serde_json::Map::new();
            if !registry::registry().tools().is_empty() {
                capabilities.insert(
                    "tools".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            if !registry::registry().resources().is_empty() {
                capabilities.insert(
                    "resources".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            return serde_json::json!({
                "result": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": capabilities,
                    "serverInfo": {
                        "name": "name",
                        "title": "title",
                        "version": "1.0.0",
                        "description": "an example mcp",
                    },
                }
            });
        } else if req.method == "tools/list" {
            return serde_json::json!({"result": { "tools": registry::registry().tools().values().map(|i| (i.params)()).collect::<Vec<_>>() } });
        } else if req.method == "tools/call" {
            if let Ok(tool_call) = serde_json::from_value::<ToolCall>(
                req.params.clone().unwrap_or(serde_json::json!({})),
            ) {
                if let Some(tool) = registry::registry().get_tool(&tool_call.name) {
                    match (tool.from_args)(
                        &tool_call.arguments.clone().unwrap_or(serde_json::json!({})),
                    ) {
                        registry::FromArgResult::Tool(caller) => {
                            let executor = caller.get_executor();
                            return Router::execution_result_to_mcp(
                                executor.execute(),
                                "content".to_string(),
                            );
                        }
                        registry::FromArgResult::Error(s) => {
                            return serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for tools/call {}", s)}});
                        }
                        _ => {
                            return serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for tools/call {}", tool_call.name)}});
                        }
                    }
                }
                return serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for tools/call, unknown tool: {}", tool_call.name)}});
            }
            return serde_json::json!({"error": { "code": -32602, "message": "malformed request from LLM"}});
        } else if req.method == "resources/list" {
            // TODO: paging
            let resources: Vec<registry::MCPMeta> = registry::registry()
                .resources()
                .iter()
                .filter_map(|(_, i)| {
                    if !(i.is_template)() {
                        Some((i.meta)())
                    } else {
                        None
                    }
                })
                .collect();
            return serde_json::json!({"result": {"resources": resources }});
        } else if req.method == "resources/templates/list" {
            let resources: Vec<registry::MCPTemplateMeta> = registry::registry()
                .resources()
                .values()
                .filter_map(|i| {
                    if (i.is_template)() {
                        Some(registry::MCPTemplateMeta::from_meta(&(i.meta)()))
                    } else {
                        None
                    }
                })
                .collect();
            return serde_json::json!({"result": {"resourceTemplates": resources }});
        } else if req.method == "resources/read" {
            if let Ok(resource_call) = serde_json::from_value::<ResourceCall>(
                req.params.clone().unwrap_or(serde_json::json!({})),
            ) {
                if let Some(r) = registry::registry().get_resource(&resource_call.uri) {
                    // exact match
                    if let registry::FromArgResult::Resource(a) =
                        (r.from_args)(&serde_json::json!({ "dsn": &resource_call.uri }))
                    {
                        return Router::execution_result_to_mcp(
                            a.get_executor()
                                .execute()
                                .iter()
                                .map(|a| registry::MCPExecutionResult::RESOURCE(a.clone()))
                                .collect(),
                            "contents".to_string(),
                        );
                    } else {
                        return serde_json::json!({"error": { "code": -32603, "message": "Internal error: resource structs may only contain a DSN field or must be empty"}});
                    }
                } else {
                    {
                        let dsn = match udsn::DSN::parse(resource_call.uri.clone()) {
                            Some(d) => d,
                            _ => {
                                return serde_json::json!({"error": { "code": -32602, "message": "malformed requested, expected uri in params"}});
                            }
                        };
                        for (_, i) in registry::registry().resources().iter() {
                            if (i.is_template)()
                                && (i.serves)(&dsn)
                                && let registry::FromArgResult::Resource(a) =
                                    (i.from_args)(&serde_json::json!({ "dsn": &resource_call.uri }))
                            {
                                return Router::execution_result_to_mcp(
                                    a.get_executor()
                                        .execute()
                                        .iter()
                                        .map(|a| registry::MCPExecutionResult::RESOURCE(a.clone()))
                                        .collect(),
                                    "contents".to_string(),
                                );
                            }
                        }
                    }
                };
                return serde_json::json!({"error": {"code": -32602, "message": "no valid resource handler found for requested uri"}});
            }
            return serde_json::json!({"error": { "code": -32600, "message": format!("malformed request from LLM: {}", req.method)}});
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
            id: RequestID::Number(123),
            method: "tools/list".to_string(),
            params: json!({
                "test": 15,
                "oooptional": 5,
            })
            .into(),
        });
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 123,
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
            id: RequestID::Number(42),
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
            "jsonrpc": "2.0",
            "id": 42,
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
            id: RequestID::Str("a666".to_string()),
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
            "jsonrpc": "2.0",
            "id": "a666",
            "error": {
                "code": -32602,
                "message": "invalid parameters for tools/call missing field `test`",
            }
        });
        assert_eq!(cmp, resp);
    }

    #[test]
    fn basic_resource_list() {
        let resp = Router::exec(Request {
            jsonrpc: "2.0".to_string(),
            id: RequestID::Number(42),
            method: "resources/list".to_string(),
            params: None,
        });
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "resources": [
                    {"title": "TestResource"
                    ,"description": "a test resource"
                    ,"uri": "git://some-repo"
                    ,"name": "TestResource"
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
            id: RequestID::Str("123".to_string()),
            method: "resources/read".to_string(),
            params: Some(json!({ "uri": "git://some-repo" })),
        });
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": "123",
            "result": {
                "contents": [
                    {"uri": "test://forward",
                     "name": "git://some-repo"
                    },
                    {"uri": "test://reverse",
                     "name": "oper-emos//:tig"
                    }
                ],
            }
        });
        assert_eq!(cmp, resp);
    }
}
