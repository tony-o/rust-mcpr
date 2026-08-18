# rocket-ezmode

An HTTP MCP server built with [Rocket](https://rocket.rs) and the library's **ez mode** (the
derive-macro API -- `#[derive(MCPTool, ...)]` / `#[derive(MCPResource, ...)]` -- which registers
into a global registry for you, so there's no `Registry` to wire up by hand). It serves real data
out of the shared `examples/shared.db` SQLite file via `mcp-examples-shared-db`:

- **`BookResource`**, a resource template on `book://{id}` that looks up one book by id.
- **`searchBooks`**, a tool that does a `LIKE` search across book titles and authors.
- **`countTo`**, a streaming tool that counts from 1 to `n`, emitting a `notifications/progress`
  event per step before answering with the final count. See [streaming it](#streaming-it) below.

All three derive from a single struct field/parameter and never see a `Registry` -- ez mode ends at
`Router::new().build()`.

## the DSN quirk

`BookResource`'s only field is `dsn: udsn::DSN` -- ez-mode resources are only ever allowed that one
field (or none, for a non-templated resource). To get the numeric id back out of a parsed
`book://5`, you might reach for `dsn.database` first since it *sounds* right. Don't: a bare
`proto://something` with no further `/` or `:` always lands the trailing segment in
`Some(Resource::URI(String))`, not `.database` and not `Resource::Path`. So the id extraction is:

```rust
fn book_id(dsn: &udsn::DSN) -> Option<i64> {
    match &dsn.resource {
        Some(udsn::Resource::URI(s)) => s.parse::<i64>().ok(),
        _ => None,
    }
}
```

A lookup miss (bad id, or nothing found in the DB) just returns an empty `Vec` -- there's no
"not found" error variant on `MCPResourceExecutor`, so an empty content list is the honest answer.

## run it

```sh
cargo run -p rocket-ezmode
```

Rocket listens on `127.0.0.1:8000` by default and mounts everything at `POST /mcp`.

`rusqlite::Connection` isn't `Send`/shareable across requests, so each handler just calls
`mcp_examples_shared_db::open()` fresh -- it's a small local SQLite file, no pooling needed for an
example this size.

## streaming it

`countTo` answers over real Server-Sent Events instead of a single JSON body: the `POST /mcp`
handler relays `MCPExecutionResult::STREAM`'s `receiver` through Rocket's native
`rocket::response::stream::{Event, EventStream}` support, one `data:` frame per item -- progress
notifications first, then a final frame with the original request's `id`. Use `curl -N` so curl
doesn't buffer the whole response before printing it:

```sh
curl -sN -XPOST http://127.0.0.1:8000/mcp -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"countTo","arguments":{"n":4}}}'
# data:{"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":1,"total":4}}
#
# data:{"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":2,"total":4}}
#
# data:{"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":3,"total":4}}
#
# data:{"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":4,"total":4}}
#
# :
# data:{"id":10,"jsonrpc":"2.0","result":{"countedTo":4}}
```

(The bare `:` line is Rocket's SSE heartbeat comment, sent to keep idle connections alive -- clients
ignore it.)

This is a one-way demo: `MCPExecutionResultStream`'s `sender` half (for replies to server-initiated
requests like `sampling/createMessage`) is dropped rather than wired up, since nothing `countTo`
does ever expects a reply back from the client.

Since the handler needs to return either a plain JSON body or an `EventStream`, and those are
unrelated types, it returns a small `McpResponse` enum. It can't use `#[derive(Responder)]` though:
Rocket's `Json<T>` only implements `Responder<'r, 'static>` while `EventStream` only implements
`Responder<'r, 'r>`, and no single `'o` satisfies both in one derived `impl<'r, 'o: 'r>`. `McpResponse`
implements `Responder<'r, 'r>` by hand instead, serializing the JSON variant directly into a
`Response` rather than delegating to `Json`.

## verifying it

```sh
cargo build -p rocket-ezmode
python3 examples/rocket-ezmode/verify.py target/debug/rocket-ezmode
```

Or drive it by hand -- everything below was actually run against a live `cargo run -p rocket-ezmode`
instance.

```sh
curl -s -XPOST http://127.0.0.1:8000/mcp -H "Content-Type: application/json" -d '{
  "jsonrpc": "2.0", "id": 1, "method": "initialize",
  "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "curl", "version": "0.0"}}
}'
# {"id":1,"jsonrpc":"2.0","result":{"capabilities":{"resources":{},"tools":{}},"protocolVersion":"2024-11-05","serverInfo":{"name":"Example MCP Server","version":"1.0.0"}}}

curl -s -XPOST http://127.0.0.1:8000/mcp -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"resources/templates/list"}'
# {"id":2,"jsonrpc":"2.0","result":{"resourceTemplates":[{"description":"Looks up one book from the shared library by id","name":"BookResource","title":"Book","uriTemplate":"book://{id}"}]}}

curl -s -XPOST http://127.0.0.1:8000/mcp -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"book://1"}}'
# {"id":4,"jsonrpc":"2.0","result":{"contents":[{"name":"The C Programming Language","text":"The C Programming Language by Kernighan & Ritchie (1978)\n\nThe book that taught a generation why pointers deserve their reputation.","uri":"book://1"}]}}

curl -s -XPOST http://127.0.0.1:8000/mcp -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":"book://999"}}'
# {"id":5,"jsonrpc":"2.0","result":{"contents":[]}}     <- no book 999, empty contents, no drama

curl -s -XPOST http://127.0.0.1:8000/mcp -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":6,"method":"tools/list"}'
# {"id":6,"jsonrpc":"2.0","result":{"tools":[{"description":"Counts up to n, emitting a progress notification at each step","inputSchema":{"properties":{"cursor":{"type":"string"},"n":{"type":"integer"}},"required":["n"],"type":"object"},"name":"countTo","title":"Count To"},{"description":"Searches the shared library's books by title or author","inputSchema":{"properties":{"cursor":{"type":"string"},"query":{"type":"string"}},"required":["query"],"type":"object"},"name":"searchBooks","title":"Search Books"}]}}

curl -s -XPOST http://127.0.0.1:8000/mcp -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"searchBooks","arguments":{"query":"Gibson"}}}'
# {"id":7,"jsonrpc":"2.0","result":{"content":[{"text":"Neuromancer by William Gibson (1984)\n\nCoined 'cyberspace' and then made it look effortless.","type":"text"}]}}

curl -s -XPOST http://127.0.0.1:8000/mcp -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"searchBooks","arguments":{"query":"zzzznomatch"}}}'
# {"id":8,"jsonrpc":"2.0","result":{"content":[{"text":"No books matched.","type":"text"}]}}
```

`resources/list` (the non-template listing) comes back empty, as expected -- `BookResource` only
registers as a template, it never enumerates concrete book URIs up front.
