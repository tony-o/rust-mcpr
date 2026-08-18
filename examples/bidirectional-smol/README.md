# bidirectional-smol

A stdio MCP server with one tool, `summarizeViaSampling`, that pauses mid-call to ask the
*client's* own LLM to summarize some text via `sampling/createMessage`, then answers using
whatever the client sends back. Demonstrates `MCPExecutionResult::STREAM` end to end: outbound
notifications/requests, a bidirectional reply, and the final wrapped result -- all driven by
`smol::spawn`, which is entirely the tool's own choice, not the router's.

The `main()` loop distinguishes a normal client request (has `"method"`) from a reply to one of
the router's own outgoing requests (has `"id"` but no `"method"`), routing the latter into the
currently-open stream's `sender` instead of dispatching it as a new call.

This is a port of `bidirectional-tokio` to the `smol` runtime, to demonstrate that mcp-router
never spawns tasks or picks a runtime for you -- it works identically under any executor.

## smol gotcha: `.detach()`

Unlike tokio's `JoinHandle`, which keeps a spawned task running even if you drop the handle,
smol's `Task` returned by `smol::spawn` is **cancelled on drop** if you don't call `.detach()`
on it. Both `smol::spawn` calls in `main.rs` (the tool's background sampling round trip, and the
stream-draining print loop) call `.detach()` for exactly this reason -- forgetting it means the
background work silently stops running and the server appears to hang.

## run it

```sh
cargo run -p bidirectional-smol
```

Then interact with it over stdin/stdout using raw JSON-RPC lines, or use `verify.py` for a
scripted round trip:

```sh
cargo build -p bidirectional-smol
python3 examples/bidirectional-smol/verify.py target/debug/bidirectional-smol
```
