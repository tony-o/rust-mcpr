#[derive(serde::Deserialize, serde::Serialize)]
enum RequestID {
    STRING(String),
    NUMBER(i64),
}

#[derive(serde::Deserialize, serde::Serialize)]
struct Request {
    jsonrpc: String,
    id: RequestID,
    method: String,
    params: Option<serde_json::Map<String, serde_json::Value>>,
}

impl Request {
    pub fn from_str(v: &String) -> Result<Self, String> {
        serde_json::from_str(v).map_err(|e| format!("{}", e))
    }
}

enum Response {
    Raw(serde_json::Value),
    Error(serde_json::Value),
}

struct Router {}

impl Router {
    fn exec_route(req: Request) -> serde_json::Value {
        if req.method == "tools/list" {
            let tools = match registry::registry().tools() {
                Some(ts) => ts.iter().map(|(_, i)| (i.params)()).collect(),
                None => Vec::new(),
            };
            return serde_json::json!({"result": { "tools": tools } });
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
    use crate::MCPTool;
    use registry::MCPTool;
    use serde::{Deserialize, Serialize};
    use serde_json::{Map, Value, json};

    #[test]
    fn basic_router() {
        let resp = Router::exec_route(Request {
            jsonrpc: "2.0".to_string(),
            id: RequestID::NUMBER(123),
            method: "tools/list".to_string(),
            params: None,
        });
        println!("resp\n{}", resp);
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
        println!("resp\n{}", cmp);
        assert_eq!(cmp, resp);
    }
}
