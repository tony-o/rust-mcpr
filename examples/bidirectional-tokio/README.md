# bidirectional-tokio

A stdio MCP server with one tool, `summarizeViaSampling`, that pauses mid-call to ask the
*client's* own LLM to summarize some text via `sampling/createMessage`, then answers using
whatever the client sends back. Demonstrates `MCPExecutionResult::STREAM` end to end: outbound
notifications/requests, a bidirectional reply, and the final wrapped result -- all driven by
`tokio::spawn`, which is entirely the tool's own choice, not the router's.

The `main()` loop distinguishes a normal client request (has `"method"`) from a reply to one of
the router's own outgoing requests (has `"id"` but no `"method"`), routing the latter into the
currently-open stream's `sender` instead of dispatching it as a new call.

## run it

```sh
cargo run -p bidirectional-tokio
```

Then interact with it over stdin/stdout using raw JSON-RPC lines, or use `verify.py` for a
scripted round trip:

```sh
cargo build -p bidirectional-tokio
python3 examples/bidirectional-tokio/verify.py target/debug/bidirectional-tokio
```
