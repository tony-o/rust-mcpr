# axum-manual

An HTTP MCP server built on [axum](https://github.com/tokio-rs/axum), reading real data out of the
shared `examples/shared.db` SQLite catalog (see `examples/shared-db`). It exposes:

- a resource template `book://{id}` that looks up one book by id,
- a `searchBooks` tool that does a `LIKE` match against title/author, and
- a `countTo` tool that streams progress notifications over Server-Sent Events.

## Manual mode, not derive mode

Every other stdio example in this repo (`stdio-basic`, `stdio-streaming`, ...) uses
`#[derive(MCPTool)]` / `#[derive(MCPResource)]`, which relies on `inventory` to auto-register
types into the process-global registry returned by `mcp_router::registry::registry()`.

This example instead hand-writes the full `MCPResource`/`MCPResourceExecutor` and
`MCPTool`/`MCPToolExecutor` impls for `BookResource`, `SearchBooksTool`, and `CountTo`, and builds its own
`Registry` explicitly with `Registry::new_from(...)` plus `register_resource_adapter::<T>(uri)` /
`register_tool_adapter::<T>(name)`. There's no macro magic: `meta()`, `params()`, and `from_args()`
are all plain functions you write yourself. This is the pattern to reach for when you want
explicit control over what's registered (e.g. building several independent routers/registries in
one process) instead of relying on global auto-registration.

The other twist specific to axum: `axum::Router`'s shared state has to be `'static`, since
handlers can be scheduled onto any worker thread for the life of the server. A hand-built
`Registry` (as opposed to the macro-populated global one, which is already `'static` via
`OnceLock`) doesn't naturally have that lifetime, so `build_router()` leaks it once at startup
with `Box::leak`. `mcp_router::router::Router` itself derives `Clone` and is cheap to clone (a
`ServerInfo`, a page size, and a `&'static Registry` reference), so it's built once in `main()`
and handed to axum via `.with_state(router)`.

## Extracting an id from a `udsn::DSN`

`BookResource` has a single field, `dsn: udsn::DSN`, populated by the router from the requested
URI string. For a bare `book://5` (no path, no colon), `udsn` parses the host-like part into
`DSN.resource == Some(Resource::URI("5"))`, so the id is pulled out with:

```rust
fn book_id(dsn: &udsn::DSN) -> Option<i64> {
    match &dsn.resource {
        Some(udsn::Resource::URI(s)) => s.parse::<i64>().ok(),
        _ => None,
    }
}
```

## Route

A single `POST /mcp` handler takes the raw JSON-RPC request body, calls
`router.exec_from_value(v).await`, and maps `RouterResponse::Value` to `200` (or `202` for a
notification that produced no response) and `RouterResponse::Stream` to a `text/event-stream` SSE
response (see below).

## Streaming: `countTo`

`countTo` is the manual-mode counterpart of `stdio-streaming`'s derive-based `CountTo`: it counts
from 1 to `n`, sending a `notifications/progress` item after each step, then a final
`{"countedTo": n}` answer. Its `execute()` builds a channel pair with
`mcp_router::stream_channel`, spawns a `tokio::task` that feeds it, and returns
`MCPExecutionResult::STREAM(MCPExecutionResultStream { receiver, sender })`. This is a one-way
demo -- nothing here expects a reply from the client -- so `mcp_handler` drops the stream's
`sender` half and only relays `receiver`.

`mcp_handler` turns that `receiver` (a `Stream<Item = serde_json::Value>`) into an SSE response by
mapping each item through `axum::response::sse::Event::json_data(...)` and wrapping the result in
`Sse::new(...)`. Every item the router hands back over `receiver` is already either a full
notification (has a `"method"` field) or the final JSON-RPC-wrapped result, so the handler doesn't
need to know anything about `countTo` specifically -- any streaming tool would flow through the
same arm.

## Run it

```sh
cargo run -p axum-manual
```

This starts the server on `http://127.0.0.1:3000/mcp`. Each request opens `examples/shared.db`
fresh (SQLite + a single small file, no pooling needed).

## Verify it

```sh
cargo build -p axum-manual
python3 examples/axum-manual/verify.py target/debug/axum-manual
```

Or drive it by hand with curl:

```sh
cargo build -p axum-manual
cargo run -p axum-manual &

curl -s -X POST http://127.0.0.1:3000/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}'

curl -s -X POST http://127.0.0.1:3000/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"resources/templates/list"}'

curl -s -X POST http://127.0.0.1:3000/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"book://1"}}'

curl -s -X POST http://127.0.0.1:3000/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":4,"method":"tools/list"}'

curl -s -X POST http://127.0.0.1:3000/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"searchBooks","arguments":{"query":"Gibson"}}}'

# -N disables curl's output buffering, so progress frames print as they arrive
# instead of all at once when the stream ends.
curl -N -s -X POST http://127.0.0.1:3000/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"countTo","arguments":{"n":4}}}'
```

The `resources/read` call for `book://1` returns real catalog data (title/author/year/summary from
SQLite), and the `searchBooks` call for `"Gibson"` returns *Neuromancer*. The `countTo` call
streams back four `data: {...}` SSE frames of `notifications/progress`, followed by a fifth
frame containing `{"id":6,"jsonrpc":"2.0","result":{"countedTo":4}}`.
