use async_trait::async_trait;
use mcp_router::registry::{
    MCPExecutionResult, MCPExecutionResultStream, MCPResource, MCPResourceExecutor,
    MCPResourceResult, MCPTool, MCPToolExecutor,
};
use mcp_router::router::{Router, RouterResponse, RouterStream};
use mcp_router::{MCPResource, MCPTool, SinkExt, StreamExt, stream_channel};
use rocket::{
    Request, State,
    http::{ContentType, Status},
    post,
    response::{self, Responder, Response, stream::{Event, EventStream}},
    routes,
    serde::json::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;

fn format_book(book: &mcp_examples_shared_db::Book) -> String {
    format!(
        "{} by {} ({})\n\n{}",
        book.title, book.author, book.year, book.summary
    )
}

#[derive(MCPResource, Deserialize, Serialize)]
#[meta(
    title = "Book",
    description = "Looks up one book from the shared library by id",
    uri = "book://{id}"
)]
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
                (
                    vec![MCPResourceResult::new(uri, name).text(&text)],
                    None,
                )
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

#[derive(MCPTool, Deserialize, Serialize)]
#[meta(
    name = "searchBooks",
    title = "Search Books",
    description = "Searches the shared library's books by title or author"
)]
struct SearchBooks {
    query: String,
}

#[async_trait]
impl MCPToolExecutor for SearchBooks {
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

/// Simulates a slow job: counts from 1 to n, reporting progress after every step, then answers
/// with the final count. One-way -- nothing here ever expects a reply from the client, so the
/// paired reply receiver is just created and dropped.
#[derive(MCPTool, Deserialize, Serialize)]
#[meta(
    name = "countTo",
    title = "Count To",
    description = "Counts up to n, emitting a progress notification at each step"
)]
struct CountTo {
    n: u32,
}

#[async_trait]
impl MCPToolExecutor for CountTo {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let (mut out_tx, out_rx) = stream_channel::<Value>(16);
        let (in_tx, _in_rx) = stream_channel::<Value>(16);
        let n = self.n;

        rocket::tokio::spawn(async move {
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

// The handler needs to return either a plain JSON response or an `EventStream`, and those two
// types can't share a `#[derive(Responder)]`: Rocket's `Json<T>` only implements
// `Responder<'r, 'static>`, while `EventStream` only implements `Responder<'r, 'r>` -- no single
// `'o` satisfies both, so the derive's generated `impl<'r, 'o: 'r>` can't type-check either way.
// Implementing `Responder<'r, 'r>` by hand for just this enum sidesteps the mismatch: the JSON
// variant is serialized directly into a `Response` instead of going through `Json`'s responder.
enum McpResponse {
    Value(Status, Value),
    Stream(EventStream<Pin<Box<dyn rocket::futures::stream::Stream<Item = Event> + Send>>>),
}

impl<'r> Responder<'r, 'r> for McpResponse {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'r> {
        match self {
            McpResponse::Value(status, value) => {
                let body = serde_json::to_string(&value).unwrap_or_default();
                Response::build()
                    .status(status)
                    .header(ContentType::JSON)
                    .sized_body(body.len(), std::io::Cursor::new(body))
                    .ok()
            }
            McpResponse::Stream(stream) => stream.respond_to(req),
        }
    }
}

#[post("/mcp", format = "json", data = "<body>")]
async fn mcp(body: Json<Value>, router: &State<Router<'static>>) -> McpResponse {
    match router.exec_from_value(body.into_inner()).await {
        RouterResponse::Value(v) if v.is_null() => McpResponse::Value(Status::Accepted, Value::Null),
        RouterResponse::Value(v) => McpResponse::Value(Status::Ok, v),
        // The `sender` half (for replies to server-initiated requests like `sampling/createMessage`)
        // is unused by this one-way demo, so it's dropped here rather than plumbed anywhere.
        RouterResponse::Stream(RouterStream { receiver, .. }) => {
            let events: Pin<Box<dyn rocket::futures::stream::Stream<Item = Event> + Send>> =
                Box::pin(receiver.map(|item| Event::json(&item)));
            McpResponse::Stream(EventStream::from(events))
        }
    }
}

#[rocket::launch]
fn rocket() -> _ {
    rocket::build()
        .manage(Router::new().build())
        .mount("/", routes![mcp])
}
