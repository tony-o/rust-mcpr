# stdio-basic

The simplest possible MCP server: a single stdio binary with one plain (non-streaming) tool,
`reverseString`, and no database. Nothing here streams -- the `main()` loop still matches on
both `RouterResponse` variants, but the `Stream` arm is `unreachable!()` since no tool in this
example ever returns one.

## run it

```sh
cargo run -p stdio-basic
```

Then interact with it over stdin/stdout using raw JSON-RPC lines, or use `verify.py` for a
scripted round trip:

```sh
cargo build -p stdio-basic
python3 examples/stdio-basic/verify.py target/debug/stdio-basic
```
