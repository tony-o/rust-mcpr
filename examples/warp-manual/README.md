# warp-manual

An HTTP MCP server built on [warp](https://github.com/seanmonstar/warp), using **manual mode**:
`MCPResource`/`MCPResourceExecutor` and `MCPTool`/`MCPToolExecutor` are implemented by hand and
wired into an explicit `Registry`, instead of using the `#[derive(MCPTool)]` / `#[derive(MCPResource)]`
macros with their global `inventory`-based auto-registration. Reach for this pattern when you want
control over exactly which handlers exist on a given `Router` -- e.g. serving different tool sets
per route, or building the registry at runtime -- rather than every `#[derive]`'d struct in the
binary showing up automatically.

Data comes from the committed `examples/shared.db` SQLite file via the `mcp-examples-shared-db`
helper crate (see `examples/shared-db/`). Each request opens its own `rusqlite::Connection` --
cheap for a local SQLite file, and it sidesteps `Connection` not being `Send`/`Sync`, which would
otherwise fight with warp's threaded handlers.

It exposes:

- **Resource template** `book://{id}` -- `BookResource` parses the id out of the `udsn::DSN` warp's
  handler is called with, looks it up via `mcp_examples_shared_db::get_book`, and returns one
  `MCPResourceResult::TEXT`.
- **Tool** `searchBooks` -- `SearchBooksTool` takes a `query: String` and returns one
  `MCPExecutionResult::TEXT` per matching book (LIKE match on title or author), or a single
  "No books matched." result if nothing hits.
- **Tool** `countTo` -- `CountTo` takes an `n: u32` and streams progress over Server-Sent Events
  (see below) instead of returning a single result.

## How this differs from ez mode

In ez mode (see `stdio-basic`, `bidirectional-tokio`), `#[derive(MCPTool)]` generates
`get_executor`/`meta`/`params`/`from_args` for you and registers the struct globally via
`inventory` the moment the binary starts -- `Router::new().build()` just picks up whatever is
registered. Here there's no derive and no global registry:

- `MCPMeta`, `params()` (the raw JSON schema for tools), and `from_args()` are written out by hand.
- Registration is explicit and local: `registry.register_resource_adapter::<BookResource>(...)` /
  `register_tool_adapter::<SearchBooksTool>(...)` against a `Registry` built with `new_from`, not
  the process-wide one.
- Because warp needs its filter state to be `'static + Clone + Send + Sync`, the `Registry` is
  `Box::leak`'d once in `build_router()` so the `Router` built on top of it can be cloned cheaply
  into the warp filter chain (`Router` itself only holds a `&'static Registry` plus a couple of
  small fields).

## Streaming: `countTo`

`countTo` is the manual-mode counterpart of `stdio-streaming`'s derive-based `CountTo`: it counts
from 1 to `n`, sending a `notifications/progress` item after each step, then a final
`{"countedTo": n}` answer. Its `execute()` builds a channel pair with `mcp_router::stream_channel`,
spawns a `tokio::task` that feeds it, and returns
`MCPExecutionResult::STREAM(MCPExecutionResultStream { receiver, sender })`. This is a one-way demo
-- nothing here expects a reply from the client -- so `handle_mcp` drops the stream's `sender` half
and only relays `receiver`.

`handle_mcp` turns that `receiver` into a `warp::sse::reply` response by mapping each item through
`warp::sse::Event::default().json_data(...)`. `receiver`'s concrete type
(`Pin<Box<dyn Stream<Item = serde_json::Value> + Send>>`) is `Send` but not `Sync`, while
`warp::sse::reply` requires both, so it's wrapped in a small `SyncSendStream(Mutex<...>)` newtype
first -- a `Mutex` is `Sync` for any `Send` inner value, which satisfies the bound without any
`unsafe` code. Every item off `receiver` is already either a full notification (has a `"method"`
field) or the final JSON-RPC-wrapped result, so the handler doesn't need to know anything about
`countTo` specifically -- any streaming tool would flow through the same arm.

Because an SSE reply and the existing `warp::reply::with_status(warp::reply::json(...), ...)` reply
are different concrete types, `handle_mcp` now returns `Box<dyn warp::Reply>` and boxes each match
arm, rather than returning `impl warp::Reply`.

## Run it

```sh
cargo run -p warp-manual
```

This starts the server on `http://127.0.0.1:3002`.

## Verify it

```sh
cargo build -p warp-manual
python3 examples/warp-manual/verify.py target/debug/warp-manual
```

Or talk to it by hand with plain JSON-RPC over HTTP POST:

```sh
curl -s -X POST http://127.0.0.1:3002/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}'

curl -s -X POST http://127.0.0.1:3002/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"resources/templates/list"}'

curl -s -X POST http://127.0.0.1:3002/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"book://1"}}'

curl -s -X POST http://127.0.0.1:3002/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":4,"method":"tools/list"}'

curl -s -X POST http://127.0.0.1:3002/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"searchBooks","arguments":{"query":"Gibson"}}}'

# -N disables curl's output buffering, so progress frames print as they arrive
# instead of all at once when the stream ends.
curl -N -s -X POST http://127.0.0.1:3002/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"countTo","arguments":{"n":4}}}'
```

`resources/read` for `book://1` returns real data from `shared.db` (Kernighan & Ritchie's
*The C Programming Language*), and `searchBooks` with `"Gibson"` returns *Neuromancer*. The
`countTo` call streams back four `data: {...}` SSE frames of `notifications/progress`, followed by
a fifth frame containing `{"id":6,"jsonrpc":"2.0","result":{"countedTo":4}}`.

A response with a `null` result (a JSON-RPC notification) comes back as HTTP `202`; everything else
is `200`. A streaming tool call comes back as a `text/event-stream` SSE response instead of a plain
JSON body.

Uses port `3002` specifically so it doesn't collide with the sibling `axum` (3000) or `actix-web`
(3001) examples of the same book-catalog idea.
