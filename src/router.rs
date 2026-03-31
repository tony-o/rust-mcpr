use base64::{Engine as _, engine::general_purpose};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
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

#[derive(serde::Deserialize)]
struct ResourceCall {
    uri: String,
}

pub type ServerIcon = crate::registry::MCPMetaIcon;
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icons: Option<Vec<ServerIcon>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    website_url: Option<String>,
}

impl ServerInfo {
    pub fn new() -> Self {
        Self {
            name: "Example MCP Server".to_string(),
            version: "1.0.0".to_string(),
            title: None,
            description: None,
            icons: None,
            website_url: None,
        }
    }

    pub fn name(&mut self, name: &str) -> &mut Self {
        self.name = name.to_string();
        self
    }

    pub fn description(&mut self, description: &str) -> &mut Self {
        self.description = Some(description.to_string());
        self
    }

    pub fn build(&mut self) -> Self {
        self.to_owned()
    }
}

#[derive(Clone)]
pub struct Router<'a> {
    server_info: ServerInfo,
    registry: &'a crate::registry::Registry,
}

impl<'a> Router<'a> {
    pub fn new() -> Self {
        Router {
            registry: crate::registry::registry(),
            server_info: ServerInfo::new(),
        }
    }

    pub fn registry(&mut self, registry: &'a crate::registry::Registry) -> &mut Self {
        self.registry = registry;
        self
    }

    pub fn server_info(&mut self, server_info: ServerInfo) -> &mut Self {
        self.server_info = server_info;
        self
    }

    pub fn build(&mut self) -> Self {
        self.to_owned()
    }

    pub fn registry_ref(&self) -> &crate::registry::Registry {
        self.registry
    }

    fn execution_result_to_mcp(
        mcper: Vec<crate::registry::MCPExecutionResult>,
        content_key: &str,
    ) -> serde_json::Value {
        let mut content: Vec<serde_json::Value> = Vec::new();
        let mut result = serde_json::Map::new();
        for mcpr in &mcper {
            match mcpr {
                crate::registry::MCPExecutionResult::TEXT(s) => content.push(serde_json::json!({
                    "type": "text",
                    "text": s.to_string(),
                })),
                crate::registry::MCPExecutionResult::AUDIO(a) => {
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
                crate::registry::MCPExecutionResult::IMAGE(a) => content.push(serde_json::json!({
                    "type": "image",
                    "data": general_purpose::STANDARD.encode(&a.data),
                    "mimeType": a.mime_type,
                })),
                crate::registry::MCPExecutionResult::RAW(v) => content.push(v.clone()),
                crate::registry::MCPExecutionResult::RESOURCE(r) => {
                    let mut val = serde_json::to_value(r).unwrap_or_else(|e| {
                        serde_json::json!({
                            "type": "text",
                            "text": format!("error: {:?} serializing result: {}", r, e)
                        })
                    });
                    if let serde_json::Value::Object(ref mut o) = val {
                        o.insert(
                            "type".to_string(),
                            serde_json::Value::String("resource_link".to_string()),
                        );
                    }
                    content.push(val);
                }
                crate::registry::MCPExecutionResult::ERROR((s, _)) => {
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
        result.insert(content_key.to_string(), serde_json::Value::Array(content));
        serde_json::json!({"result": result})
    }

    pub async fn exec_from_value(&self, v: serde_json::Value) -> serde_json::Value {
        match serde_json::from_value::<Request>(v) {
            Ok(a) => self.exec(a).await,
            Err(_) => {
                serde_json::json!({"jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "invalid request format, expected {jsonrpc:string, id:number|string, method:string, params:optional<object>}"}})
            }
        }
    }

    pub async fn exec(&self, req: Request) -> serde_json::Value {
        match self.execx(&req).await {
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

    async fn execx(&self, req: &Request) -> serde_json::Value {
        if req.method == "initialize" {
            let mut capabilities = serde_json::Map::new();
            if !self.registry.tools().is_empty() {
                capabilities.insert(
                    "tools".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            if !self.registry.resources().is_empty() {
                capabilities.insert(
                    "resources".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            return serde_json::json!({
                "result": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": capabilities,
                    "serverInfo": self.server_info
                }
            });
        } else if req.method == "tools/list" {
            return serde_json::json!({"result": { "tools": self.registry.tools().values().map(|i| (i.params)()).collect::<Vec<_>>() } });
        } else if req.method == "tools/call" {
            if let Ok(tool_call) = serde_json::from_value::<ToolCall>(
                req.params.clone().unwrap_or(serde_json::json!({})),
            ) {
                if let Some(tool) = self.registry.get_tool(&tool_call.name) {
                    match (tool.from_args)(
                        &tool_call.arguments.clone().unwrap_or(serde_json::json!({})),
                    ) {
                        crate::registry::FromArgResult::Tool(caller) => {
                            let executor = caller.get_executor();
                            return Router::execution_result_to_mcp(
                                executor.execute().await,
                                "content",
                            );
                        }
                        crate::registry::FromArgResult::Error(s) => {
                            return serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for tools/call {}", s)}});
                        }
                        crate::registry::FromArgResult::Resource(_) => {
                            return serde_json::json!({"error": {"code": -32600, "message": "server is misconfigured, a resource was registered as a tool"}});
                        }
                    }
                }
                return serde_json::json!({"error": {"code": -32602, "message": format!("invalid parameters for tools/call, unknown tool: {}", tool_call.name)}});
            }
            return serde_json::json!({"error": { "code": -32602, "message": "malformed request from LLM"}});
        } else if req.method == "resources/list" {
            // TODO: paging
            let mut resources: Vec<crate::registry::MCPMeta> = Vec::new();
            for rsrc in self.registry.resources().values() {
                if !(rsrc.is_template)() {
                    resources.extend((rsrc.meta)());
                }
            }
            return serde_json::json!({"result": {"resources": resources }});
        } else if req.method == "resources/templates/list" {
            let mut resources: Vec<crate::registry::MCPTemplateMeta> = Vec::new();
            for rsrc in self.registry.resources().values() {
                if (rsrc.is_template)() {
                    for meta in (rsrc.meta)() {
                        resources.push(crate::registry::MCPTemplateMeta::from_meta(&meta));
                    }
                }
            }
            return serde_json::json!({"result": {"resourceTemplates": resources }});
        } else if req.method == "resources/read" {
            if let Ok(resource_call) = serde_json::from_value::<ResourceCall>(
                req.params.clone().unwrap_or(serde_json::json!({})),
            ) {
                if let Some(r) = self.registry.get_resource(&resource_call.uri) {
                    // exact match
                    if let crate::registry::FromArgResult::Resource(a) =
                        (r.from_args)(&serde_json::json!({ "dsn": &resource_call.uri }))
                    {
                        return Router::execution_result_to_mcp(
                            a.get_executor()
                                .execute()
                                .await
                                .iter()
                                .map(|a| crate::registry::MCPExecutionResult::RESOURCE(a.clone()))
                                .collect(),
                            "contents",
                        );
                    } else {
                        return serde_json::json!({"error": { "code": -32603, "message": "Internal error: resource structs may only contain a DSN field or must be empty"}});
                    }
                } else {
                    let dsn = match udsn::DSN::parse(resource_call.uri.clone()) {
                        Some(d) => d,
                        _ => {
                            return serde_json::json!({"error": { "code": -32602, "message": "malformed request, expected uri in params"}});
                        }
                    };
                    let ris: Vec<&'static crate::registry::Info> =
                        self.registry.resources().values().copied().collect();
                    for i in ris {
                        if (i.is_template)()
                            && (i.serves)(&dsn)
                            && let crate::registry::FromArgResult::Resource(a) =
                                (i.from_args)(&serde_json::json!({ "dsn": &resource_call.uri }))
                        {
                            return Router::execution_result_to_mcp(
                                a.get_executor()
                                    .execute()
                                    .await
                                    .iter()
                                    .map(|a| {
                                        crate::registry::MCPExecutionResult::RESOURCE(a.clone())
                                    })
                                    .collect(),
                                "contents",
                            );
                        }
                    }
                }
                return serde_json::json!({"error": {"code": -32602, "message": "no valid resource handler found for requested uri"}});
            }
            return serde_json::json!({"error": { "code": -32600, "message": format!("malformed request from LLM: {}", req.method)}});
        }
        serde_json::json!({"error": { "code": -32601, "message": format!("method not found: {}", req.method)}})
    }
}

#[cfg(test)]
mod tests {
    use super::{Request, RequestID, Router, ServerInfo};
    use async_trait::async_trait;
    use serde_json::json;

    #[tokio::test]
    async fn initialize() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "initialize".to_string(),
                params: None,
            })
            .await;
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": {
                "capabilities": {
                    "tools": {},
                    "resources": {},
                },
                "protocolVersion": "2025-11-25",
                "serverInfo": {
                    "name": "Example MCP Server",
                    "version": "1.0.0",
                }
            }
        }
        );
        assert_eq!(cmp, resp);
    }

    #[tokio::test]
    async fn initialize_w_server_info() {
        let resp = Router::new()
            .server_info(
                ServerInfo::new()
                    .name("test")
                    .description("Hello world!")
                    .build(),
            )
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "initialize".to_string(),
                params: None,
            })
            .await;
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": {
                "capabilities": {
                    "tools": {},
                    "resources": {},
                },
                "protocolVersion": "2025-11-25",
                "serverInfo": {
                    "name": "test",
                    "description": "Hello world!",
                    "version": "1.0.0",
                }
            }
        }
        );
        assert_eq!(cmp, resp);
    }

    #[tokio::test]
    async fn basic_router() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "tools/list".to_string(),
                params: json!({
                    "test": 15,
                    "oooptional": 5,
                })
                .into(),
            })
            .await;
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": { "tools": [
                { "description": "abc camel description",
                  "title": "ABCCamel struct",
                  "name": "ABCCamel",
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

    #[tokio::test]
    async fn basic_tool_call() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(42),
                method: "tools/call".to_string(),
                params: json!({
                    "name": "ABCCamel",
                    "arguments": {
                        "test": 15,
                        "arr": [5],
                    }
                })
                .into(),
            })
            .await;
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "content": [{"type": "text", "text": "test=15,oooptional=-1,arr=[5],ooarr=[]"}],
            }
        });
        assert_eq!(cmp, resp);
    }

    #[tokio::test]
    async fn basic_tool_call_err() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Str("a666".to_string()),
                method: "tools/call".to_string(),
                params: json!({
                    "name": "ABCCamel",
                    "arguments": {
                        "arr": [5],
                    }
                })
                .into(),
            })
            .await;
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

    #[tokio::test]
    async fn basic_resource_list() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(42),
                method: "resources/list".to_string(),
                params: None,
            })
            .await;
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
    #[tokio::test]
    async fn basic_resource_call() {
        let resp = Router::new()
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Str("123".to_string()),
                method: "resources/read".to_string(),
                params: Some(json!({ "uri": "git://some-repo" })),
            })
            .await;
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": "123",
            "result": {
                "contents": [
                    {"uri": "test://forward",
                     "name": "git://some-repo",
                     "type": "resource_link"
                    },
                    {"uri": "test://reverse",
                     "name": "oper-emos//:tig",
                     "type": "resource_link"
                    }
                ],
            }
        });
        assert_eq!(cmp, resp);
    }

    #[tokio::test]
    async fn override_router() {
        use std::collections::HashMap;
        let registry = crate::registry::Registry::new_from(HashMap::new(), HashMap::new());
        let router = Router::new().registry(&registry).build();
        let resp = router
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(42),
                method: "resources/list".to_string(),
                params: None,
            })
            .await;
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "resources": [],
            }
        });
        assert_eq!(cmp, resp);
        let resp2 = router
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "tools/list".to_string(),
                params: None,
            })
            .await;
        let cmp2 = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": { "tools": [
            ]}
        }
        );
        assert_eq!(cmp2, resp2);
    }

    #[derive(serde::Deserialize)]
    pub struct ManualResource {
        _dsn: udsn::DSN,
    }

    use crate::registry::{
        FromArgResult, MCPMeta, MCPResource, MCPResourceExecutor, MCPResourceResult,
    };
    use serde_json::Value;

    #[async_trait]
    impl MCPResourceExecutor for ManualResource {
        async fn execute(&self) -> Vec<MCPResourceResult> {
            vec![MCPResourceResult::new(
                "file:///example".to_string(),
                "example file".to_string(),
            )]
        }

        fn serves(dsn: &udsn::DSN) -> bool {
            !dsn.protocol.is_empty()
        }

        fn is_template() -> bool {
            false
        }
    }

    impl MCPResource for ManualResource {
        fn get_executor(&self) -> &dyn MCPResourceExecutor {
            self
        }
        fn meta() -> Vec<MCPMeta> {
            vec![
                MCPMeta::new()
                    .name("meta_example")
                    .uri("manual-resource:///")
                    .build(),
            ]
        }
        fn params() -> Value {
            Value::Null
        }
        fn from_args(v: &Value) -> FromArgResult {
            match serde_json::from_value::<Self>(v.clone()) {
                Ok(s) => FromArgResult::Resource(Box::new(s)),
                Err(e) => {
                    /* handle your error here */
                    FromArgResult::Error(e.to_string())
                }
            }
        }
    }

    #[tokio::test]
    async fn override_router_w_static_resource() {
        use std::collections::HashMap;
        let registry = crate::registry::Registry::new_from(HashMap::new(), HashMap::new());
        registry.register_resource_adapter::<ManualResource>("file:///config");
        let router = Router::new().registry(&registry).build();
        let resp = router
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(42),
                method: "resources/list".to_string(),
                params: None,
            })
            .await;
        let cmp = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "resources": [
                    {"name": "meta_example",
                     "uri": "manual-resource:///",
                    }
                ],
            }
        });
        assert_eq!(cmp, resp);
        let resp2 = router
            .exec(Request {
                jsonrpc: "2.0".to_string(),
                id: RequestID::Number(123),
                method: "tools/list".to_string(),
                params: None,
            })
            .await;
        let cmp2 = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "result": { "tools": [
            ]}
        }
        );
        assert_eq!(cmp2, resp2);
    }
}
