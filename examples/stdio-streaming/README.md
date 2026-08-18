# stdio-streaming

A stdio MCP server with two tools registered side by side, showing that a normal tool needs no
special handling just because a streaming tool exists alongside it:

- `reverseString` -- a plain, non-streaming tool, identical in shape to the one in `stdio-basic`.
- `countTo` -- counts from 1 to `n`, emitting a `notifications/progress` item after every step,
  then a final `{"countedTo": n}` result. Demonstrates `MCPExecutionResult::STREAM` driven by
  `tokio::spawn`, one-way: unlike `bidirectional-tokio`'s sampling tool, nothing here expects a
  reply from the client, so the stream's paired reply sender/receiver are created and immediately
  dropped/discarded rather than wired up.

The `main()` loop handles `RouterResponse::Stream` for real: it spawns a task that drains the
stream's `receiver` and prints each item as it arrives, so progress notifications interleave with
whatever else is happening on stdout, followed by the id-wrapped final result.

## run it

```sh
cargo run -p stdio-streaming
```

Then interact with it over stdin/stdout using raw JSON-RPC lines, or use `verify.py` for a
scripted round trip:

```sh
cargo build -p stdio-streaming
python3 examples/stdio-streaming/verify.py target/debug/stdio-streaming
```
