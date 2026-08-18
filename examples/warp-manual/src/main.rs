use async_trait::async_trait;
use mcp_examples_shared_db::Book;
use mcp_router::registry::{
    FromArgResult, MCPExecutionResult, MCPExecutionResultStream, MCPMeta, MCPResource,
    MCPResourceExecutor, MCPResourceResult, MCPTool, MCPToolExecutor,
};
use mcp_router::router::{Router, RouterResponse, RouterStream};
use mcp_router::{SinkExt, StreamExt, stream_channel};
use serde_json::Value;
use std::collections::HashMap;
use warp::Filter;
use warp::http::StatusCode;

fn format_book(book: &Book) -> String {
    format!(
        "{} by {} ({})\n\n{}",
        book.title, book.author, book.year, book.summary
    )
}

// Manual mode: the resource struct only ever holds the parsed `dsn`, and everything
// else (which record it maps to, how to render it) is figured out by hand in `execute`.
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
            return (Vec::new(), None);
        };
        let conn = mcp_examples_shared_db::open();
        let Some(book) = mcp_examples_shared_db::get_book(&conn, id) else {
            return (Vec::new(), None);
        };
        let result = MCPResourceResult::new(self.dsn.to_string(), book.title.clone())
            .text(&format_book(&book));
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
        vec![
            MCPMeta::new()
                .name("book")
                .uri("book://{id}")
                .title("Book")
                .description("A single book from the catalog")
                .build(),
        ]
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

// Same manual shape as the resource above: a plain struct plus hand-written trait
// impls, with `params()` returning the actual JSON schema since there's no derive
// macro generating it for us.
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
        vec![MCPMeta::new().name("searchBooks").build()]
    }
    fn params() -> Value {
        serde_json::json!({
            "name": "searchBooks",
            "title": "Search Books",
            "description": "Searches the book catalog by title or author",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
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

// Same manual shape again: a plain struct plus hand-written trait impls, this time
// returning MCPExecutionResult::STREAM instead of TEXT. Structurally identical to
// stdio-streaming's CountTo, just without the derive macro doing the boilerplate.
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
        vec![
            MCPMeta::new()
                .name("countTo")
                .title("Count To")
                .description("Counts up to n, emitting a progress notification at each step")
                .build(),
        ]
    }
    fn params() -> Value {
        serde_json::json!({
            "name": "countTo",
            "title": "Count To",
            "description": "Counts up to n, emitting a progress notification at each step",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "n": { "type": "integer", "minimum": 0 }
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

// Leaked so the registry -- and the Router built on top of it -- can be 'static,
// which warp's filter system requires of anything captured by a handler closure.
fn build_router() -> Router<'static> {
    let registry: &'static mcp_router::registry::Registry = Box::leak(Box::new(
        mcp_router::registry::Registry::new_from(HashMap::new(), HashMap::new()),
    ));
    registry.register_resource_adapter::<BookResource>("book://{id}");
    registry.register_tool_adapter::<SearchBooksTool>("searchBooks");
    registry.register_tool_adapter::<CountTo>("countTo");
    Router::new().registry(registry).build()
}

async fn handle_mcp(
    body: Value,
    router: Router<'static>,
) -> Result<Box<dyn warp::Reply>, std::convert::Infallible> {
    match router.exec_from_value(body).await {
        RouterResponse::Value(v) => {
            // A JSON-RPC Notification produces no response body; 202 signals "accepted,
            // nothing to say back" rather than lying with a 200 + null payload.
            let status = if v.is_null() {
                StatusCode::ACCEPTED
            } else {
                StatusCode::OK
            };
            Ok(Box::new(warp::reply::with_status(
                warp::reply::json(&v),
                status,
            )))
        }
        // countTo never expects a reply, so the stream's own reply sender (for
        // server-initiated requests like sampling/createMessage) is discarded here
        // rather than plumbed into some other route that could feed it.
        RouterResponse::Stream(RouterStream { receiver, .. }) => {
            let event_stream = SyncSendStream(std::sync::Mutex::new(receiver)).map(|item| {
                Ok::<_, std::convert::Infallible>(
                    warp::sse::Event::default().json_data(item).unwrap(),
                )
            });
            Ok(Box::new(warp::sse::reply(
                warp::sse::keep_alive().stream(event_stream),
            )))
        }
    }
}

// warp::sse::reply requires its stream to be Sync, but RouterStream's `receiver` is only
// Send (a `dyn Stream + Send` trait object erases Sync even though nothing here ever
// actually shares it across threads). A Mutex is Sync for any Send inner value, so wrapping
// the receiver in one satisfies the bound safely without resorting to an unsafe impl.
struct SyncSendStream(
    std::sync::Mutex<std::pin::Pin<Box<dyn warp::Stream<Item = Value> + Send>>>,
);

impl warp::Stream for SyncSendStream {
    type Item = Value;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Value>> {
        self.get_mut().0.lock().unwrap().as_mut().poll_next(cx)
    }
}

#[tokio::main]
async fn main() {
    let router = build_router();

    let routes = warp::post()
        .and(warp::path("mcp"))
        .and(warp::body::json())
        .and(warp::any().map(move || router.clone()))
        .and_then(handle_mcp);

    warp::serve(routes).run(([127, 0, 0, 1], 3002)).await;
}
