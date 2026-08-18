use actix_web::web::Bytes;
use actix_web::{App, HttpResponse, HttpServer, web};
use async_trait::async_trait;
use mcp_router::registry::{
    FromArgResult, MCPExecutionResult, MCPExecutionResultStream, MCPMeta, MCPResource,
    MCPResourceExecutor, MCPResourceResult, MCPTool, MCPToolExecutor,
};
use mcp_router::router::{Router, RouterResponse};
use mcp_router::{SinkExt, StreamExt, stream_channel};
use serde_json::Value;

fn format_book(book: &mcp_examples_shared_db::Book) -> String {
    format!(
        "{} by {} ({})\n\n{}",
        book.title, book.author, book.year, book.summary
    )
}

// Manual mode: no `#[derive(MCPResource)]`, so every trait method below is hand-written
// instead of macro-generated. That's the whole point of this example -- see README.md.
#[derive(serde::Deserialize)]
struct BookResource {
    dsn: udsn::DSN,
}

// A bare `proto://5` DSN always lands the trailing segment in `Resource::URI`, never
// `.database` and never `Resource::Path` -- those only show up once there's a further
// `/` or `:` for the parser to split on.
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
        match mcp_examples_shared_db::get_book(&conn, id) {
            Some(book) => {
                let uri = format!("book://{}", id);
                let name = book.title.clone();
                let text = format_book(&book);
                (vec![MCPResourceResult::new(uri, name).text(&text)], None)
            }
            None => (vec![], None),
        }
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

    // Resources don't feed a JSON schema through `params()` the way tools do -- the
    // router only ever renders resources from `meta()` -- so this is unused in practice.
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

#[derive(serde::Deserialize)]
struct SearchBooksTool {
    query: String,
}

#[async_trait]
impl MCPToolExecutor for SearchBooksTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let conn = mcp_examples_shared_db::open();
        let books = mcp_examples_shared_db::search_books(&conn, &self.query);

        if books.is_empty() {
            return (
                vec![MCPExecutionResult::TEXT("No books matched.".into())],
                None,
            );
        }

        (
            books
                .iter()
                .map(|b| MCPExecutionResult::TEXT(format_book(b).into()))
                .collect(),
            None,
        )
    }
}

impl MCPTool for SearchBooksTool {
    fn get_executor(&self) -> &dyn MCPToolExecutor {
        self
    }

    fn meta() -> Vec<MCPMeta> {
        vec![
            MCPMeta::new()
                .name("searchBooks")
                .title("Search Books")
                .description("Searches the shared library's books by title or author")
                .build(),
        ]
    }

    // No derive means no macro-generated schema either -- tools/list renders this object
    // verbatim, so it has to be the full tool descriptor, not just the input shape.
    fn params() -> Value {
        serde_json::json!({
            "name": "searchBooks",
            "title": "Search Books",
            "description": "Searches the shared library's books by title or author",
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

// Same tool as stdio-streaming's `CountTo`, hand-implemented for manual mode: counts up to
// n, reporting progress after every step, then answers with the final count. One-way --
// nothing here expects a reply back from the client, so the paired reply receiver is just
// created and dropped.
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

        // actix-web runs on tokio under the hood via actix-rt, so `actix_web::rt::spawn`
        // (not `tokio::spawn` directly) is the way to get a task onto its runtime here.
        actix_web::rt::spawn(async move {
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
                    "n": { "type": "integer" }
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

fn build_router() -> Router<'static> {
    // actix-web app data must be 'static + Send + Sync since handlers run across the
    // runtime's worker pool; leaking the Registry for the program's lifetime is the
    // simplest way to get a `&'static Registry` to hand to the Router.
    let registry: &'static mcp_router::registry::Registry = Box::leak(Box::new(
        mcp_router::registry::Registry::new_from(
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
        ),
    ));
    registry.register_resource_adapter::<BookResource>("book://{id}");
    registry.register_tool_adapter::<SearchBooksTool>("searchBooks");
    registry.register_tool_adapter::<CountTo>("countTo");
    Router::new().registry(registry).build()
}

async fn mcp(router: web::Data<Router<'static>>, body: web::Json<Value>) -> HttpResponse {
    match router.exec_from_value(body.into_inner()).await {
        RouterResponse::Value(v) if v.is_null() => HttpResponse::Accepted().finish(),
        RouterResponse::Value(v) => HttpResponse::Ok().json(v),
        RouterResponse::Stream(stream) => {
            // `stream.sender` is how a reply would be routed back into a server-initiated
            // request (e.g. sampling/createMessage) mid-stream; countTo never issues one, so
            // it's dropped here rather than wired up -- this demo is one-way only.
            let sse_body = stream.receiver.map(|item| {
                let frame = format!("data: {}\n\n", serde_json::to_string(&item).unwrap());
                Ok::<_, std::convert::Infallible>(Bytes::from(frame))
            });
            HttpResponse::Ok()
                .content_type("text/event-stream")
                .streaming(sse_body)
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let router = web::Data::new(build_router());

    // App::new() runs once per worker thread via this factory closure; `data` is cloned
    // (cheap -- it's an Arc under the hood) into each one rather than rebuilt.
    HttpServer::new(move || {
        App::new()
            .app_data(router.clone())
            .route("/mcp", web::post().to(mcp))
    })
    .bind(("127.0.0.1", 3001))?
    .run()
    .await
}
