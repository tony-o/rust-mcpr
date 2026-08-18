use async_trait::async_trait;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Json, Response};
use mcp_examples_shared_db::Book;
use mcp_router::registry::{
    FromArgResult, MCPExecutionResult, MCPExecutionResultStream, MCPMeta, MCPResource,
    MCPResourceExecutor, MCPResourceResult, MCPTool, MCPToolExecutor,
};
use mcp_router::router::{Router, RouterResponse};
use mcp_router::{SinkExt, StreamExt, stream_channel};
use serde_json::Value;
use std::collections::HashMap;

fn format_book(book: &Book) -> String {
    format!(
        "{} by {} ({})\n\n{}",
        book.title, book.author, book.year, book.summary
    )
}

/// Resource template `book://{id}`, hand-registered instead of derived so the
/// id extraction from `udsn::DSN` is explicit rather than macro-generated.
#[derive(serde::Deserialize)]
struct BookResource {
    dsn: udsn::DSN,
}

fn book_id(dsn: &udsn::DSN) -> Option<i64> {
    match &dsn.resource {
        Some(udsn::Resource::URI(s)) => s.parse::<i64>().ok(),
        _ => None,
    }
}

#[async_trait]
impl MCPResourceExecutor for BookResource {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        let Some(id) = book_id(&self.dsn) else {
            return (vec![], None);
        };
        let conn = mcp_examples_shared_db::open();
        let Some(book) = mcp_examples_shared_db::get_book(&conn, id) else {
            return (vec![], None);
        };
        let uri = format!("book://{}", book.id);
        let result = MCPResourceResult::new(uri, book.title.clone()).text(&format_book(&book));
        (vec![result], None)
    }

    fn serves(dsn: &udsn::DSN) -> bool {
        dsn.protocol == "book"
    }

    fn is_template() -> bool {
        true
    }
}

impl MCPResource for BookResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }

    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new()
            .name("book")
            .uri("book://{id}")
            .title("Book")
            .description("A single book from the catalog")
            .build()]
    }

    fn params() -> Value {
        Value::Null
    }

    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Resource(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

/// `searchBooks` tool, hand-registered alongside `BookResource` above.
#[derive(serde::Deserialize)]
struct SearchBooksTool {
    query: String,
}

#[async_trait]
impl MCPToolExecutor for SearchBooksTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let conn = mcp_examples_shared_db::open();
        let hits = mcp_examples_shared_db::search_books(&conn, &self.query);
        if hits.is_empty() {
            return (
                vec![MCPExecutionResult::TEXT("No books matched.".into())],
                None,
            );
        }
        let results = hits
            .iter()
            .map(|b| MCPExecutionResult::TEXT(format_book(b).into()))
            .collect();
        (results, None)
    }
}

impl MCPTool for SearchBooksTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }

    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new()
            .name("searchBooks")
            .title("Search Books")
            .description("Search the book catalog by title or author")
            .build()]
    }

    fn params() -> Value {
        serde_json::json!({
            "name": "searchBooks",
            "title": "Search Books",
            "description": "Search the book catalog by title or author",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                },
                "required": ["query"]
            }
        })
    }

    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Tool(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

/// `countTo` tool, hand-registered like `SearchBooksTool` above -- the manual-mode counterpart
/// of `stdio-streaming`'s derive-based `CountTo`. Streams a `notifications/progress` item per
/// step, then a final count. One-way: nothing here expects a reply, so the paired reply
/// receiver from `stream_channel` is just created and dropped.
#[derive(serde::Deserialize)]
struct CountTo {
    n: u32,
}

#[async_trait]
impl MCPToolExecutor for CountTo {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let (mut out_tx, out_rx) = stream_channel::<Value>(16);
        let (in_tx, _in_rx) = stream_channel::<Value>(16);
        let n = self.n;

        tokio::spawn(async move {
            for i in 1..=n {
                out_tx
                    .send(serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/progress",
                        "params": {"progress": i, "total": n}
                    }))
                    .await
                    .ok();
            }
            out_tx.send(serde_json::json!({"countedTo": n})).await.ok();
        });

        (
            vec![MCPExecutionResult::STREAM(MCPExecutionResultStream {
                receiver: out_rx,
                sender: in_tx,
            })],
            None,
        )
    }
}

impl MCPTool for CountTo {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }

    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new()
            .name("countTo")
            .title("Count To")
            .description("Counts up to n, emitting a progress notification at each step")
            .build()]
    }

    fn params() -> Value {
        serde_json::json!({
            "name": "countTo",
            "title": "Count To",
            "description": "Counts up to n, emitting a progress notification at each step",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "n": {"type": "integer", "minimum": 0}
                },
                "required": ["n"]
            }
        })
    }

    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Tool(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

// axum's shared state must be 'static since handlers can run on any worker
// thread for the life of the server; leaking the Registry once at startup is
// the simplest way to get that lifetime for a hand-rolled (non-inventory) registry.
fn build_router() -> Router<'static> {
    let registry: &'static mcp_router::registry::Registry = Box::leak(Box::new(
        mcp_router::registry::Registry::new_from(HashMap::new(), HashMap::new()),
    ));
    registry.register_resource_adapter::<BookResource>("book://{id}");
    registry.register_tool_adapter::<SearchBooksTool>("searchBooks");
    registry.register_tool_adapter::<CountTo>("countTo");
    Router::new().registry(registry).build()
}

async fn mcp_handler(
    State(router): State<Router<'static>>,
    Json(body): Json<Value>,
) -> Response {
    match router.exec_from_value(body).await {
        RouterResponse::Value(v) if v.is_null() => StatusCode::ACCEPTED.into_response(),
        RouterResponse::Value(v) => (StatusCode::OK, Json(v)).into_response(),
        // `countTo` never expects a reply, so the stream's own reply sender is
        // dropped here rather than stashed for routing anything back into it.
        RouterResponse::Stream(stream) => {
            let events = stream.receiver.map(|item| {
                Event::default()
                    .json_data(item)
                    .map_err(axum::Error::new)
            });
            Sse::new(events).into_response()
        }
    }
}

#[tokio::main]
async fn main() {
    let router = build_router();

    let app = axum::Router::new()
        .route("/mcp", axum::routing::post(mcp_handler))
        .with_state(router);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("failed to bind 127.0.0.1:3000");
    println!("axum-manual listening on http://127.0.0.1:3000/mcp");
    axum::serve(listener, app).await.expect("server error");
}
