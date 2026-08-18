# actix-manual

An HTTP MCP server built on `actix-web`, using **manual mode**: `BookResource`,
`SearchBooksTool`, and `CountTo` hand-implement `MCPResource`/`MCPResourceExecutor` and
`MCPTool`/`MCPToolExecutor` directly instead of using `#[derive(MCPResource)]` /
`#[derive(MCPTool)]`. All three are registered explicitly on a `Registry` in `build_router()`,
rather than relying on the crate-wide `inventory`-based auto-registration the derive macros wire
up behind the scenes.

Data comes from the same seeded `examples/shared.db` SQLite file the other book-catalog
examples use, via `mcp-examples-shared-db`:

- A resource template, `book://{id}`, looks up one book by id.
- A `searchBooks` tool does a `LIKE` search over title/author.
- A `countTo` tool streams progress over Server-Sent Events -- see below.

## how this differs from ez mode

The ez-mode equivalent (see `rocket-ezmode`) puts `#[derive(MCPResource)]` / `#[derive(MCPTool)]`
plus a `#[meta(...)]` attribute on the struct, and the derive macro generates `meta()`,
`params()`, and `from_args()` for you, `inventory::submit!`-ing the type into a process-wide
registry that `Router::new().build()` picks up automatically with no explicit registration step.

Here, every one of those trait methods is written out by hand:

- `meta()` builds the `MCPMeta`/description manually instead of reading it off an attribute.
- `params()` for the tool returns the *entire* tool descriptor object (name, title, description,
  `inputSchema`) as a literal `serde_json::json!` value -- there's no macro generating a JSON
  schema from the struct's fields, so this example writes it out by hand. (Resources don't use
  `params()` for rendering -- the router only ever reads `meta()` for those -- so
  `BookResource::params()` is just a `Value::Null` stub.)
- `from_args()` does the `serde_json::from_value::<Self>` + `FromArgResult::Resource`/`Tool`
  wrapping that the derive macro would otherwise generate.
- Registration is explicit: `build_router()` constructs a fresh `Registry` via `new_from(...)`
  (empty maps, since manual mode doesn't use the global `inventory` registry) and calls
  `registry.register_resource_adapter::<BookResource>("book://{id}")` /
  `registry.register_tool_adapter::<SearchBooksTool>("searchBooks")` /
  `registry.register_tool_adapter::<CountTo>("countTo")` itself.

Manual mode is the crate's "I want explicit control over what's registered and when" escape
hatch -- useful if you don't want every `MCPTool`/`MCPResource` impl anywhere in the binary to
auto-register itself just by being linked in.

## streaming: `countTo` over SSE

`countTo` is the same streaming tool as `stdio-streaming`'s `CountTo` (counts from 1 to `n`,
emitting a `notifications/progress` item at each step, then a final `{"countedTo": n}`), wired
into HTTP instead of stdio. Its `execute()` spawns a background task via `actix_web::rt::spawn`
(actix-web runs on tokio under the hood via `actix-rt`, so that's the right way to get a task
onto its runtime rather than depending on `tokio` directly) and hands back
`MCPExecutionResult::STREAM`.

In the `/mcp` handler, a `RouterResponse::Stream` is turned into a real Server-Sent Events
response: `stream.receiver` (already-transformed JSON-RPC messages -- notifications relayed
verbatim, then one final `id`-wrapped result) is mapped into `data: <json>\n\n` frames and handed
to `HttpResponse::Ok().content_type("text/event-stream").streaming(...)`. This is a one-way demo
-- `stream.sender` (for routing replies back into server-initiated requests like
`sampling/createMessage`) is dropped unused, since `countTo` never asks for one.

Watch it stream with `curl -N` (no buffering) once the server is running:

```sh
curl -N -s -X POST http://127.0.0.1:3001/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"countTo","arguments":{"n":4}}}'
```

which prints four progress frames followed by the final result frame:

```
data: {"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":1,"total":4}}

data: {"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":2,"total":4}}

data: {"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":3,"total":4}}

data: {"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":4,"total":4}}

data: {"id":6,"jsonrpc":"2.0","result":{"countedTo":4}}
```

## actix-web + `'static` registry

actix-web spins up one `App` instance per worker thread via a factory closure, so shared state
handed to `App::app_data` has to be `'static + Send + Sync` and cheaply cloneable. `Registry` has
no natural `'static` owner in a normal `fn main`, so `build_router()` leaks it with `Box::leak`
to get a `&'static Registry` for the whole process lifetime -- an intentional, one-time leak,
not a bug. `Router` itself derives `Clone` (just a `ServerInfo`, a page size, and that `&'static`
reference), so it's built once and wrapped in `web::Data`, which actix clones into each worker
closure.

## run it

```sh
cargo run -p actix-manual
```

This starts an HTTP server on `127.0.0.1:3001` (not `3000`, to avoid colliding with the `axum`
book-catalog example) with a single `POST /mcp` endpoint that speaks JSON-RPC over HTTP.

Verify it:

```sh
cargo build -p actix-manual
python3 examples/actix-manual/verify.py target/debug/actix-manual
```

Or drive it by hand with curl:

```sh
curl -s -X POST http://127.0.0.1:3001/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}'

curl -s -X POST http://127.0.0.1:3001/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"resources/templates/list"}'

curl -s -X POST http://127.0.0.1:3001/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"book://1"}}'

curl -s -X POST http://127.0.0.1:3001/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":4,"method":"tools/list"}'

curl -s -X POST http://127.0.0.1:3001/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"searchBooks","arguments":{"query":"Gibson"}}}'

curl -N -s -X POST http://127.0.0.1:3001/mcp -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"countTo","arguments":{"n":4}}}'
```

The `resources/read` call returns real data from `examples/shared.db` (*The C Programming
Language* by Kernighan & Ritchie), and the `searchBooks` call for `"Gibson"` returns
*Neuromancer*. The `countTo` call streams Server-Sent Events -- see below.
