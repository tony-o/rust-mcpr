# mcp-router - an MCP router and serializer

this library is for people who don't want to be experts in MCP dances, and inspired by the lack of cohesion/poor documentation in current MCP libraries.

there are a few modes this library was intended to be used in:

- see no evil, hear no evil, speak (see: ez mode below)
- see no evil, hear, and speak mode (see: medium difficulty below)

## pagination

tools/list, resources/list, resources/templates/list, and prompts/list are all paginated for you
automatically, you don't even need to work for a living. results get sliced into pages (default size
50 and can be changed with `Router::new().page_size(100).build()`), and a nextCursor shows up in the response
when there's more cake to be had. treat the cursor as a black box, don't decode it or build one yourself, just
hand back whatever you were given.

tools/call and resources/read work differently, since there's no way for the router to know what "more"
means for your own data. whatever string gets sent as a "cursor" argument goes straight into your
`execute(cursor: Option<String>)` untouched, and whatever you hand back as the second half of your
`(Vec<MCPExecutionResult>, Option<String>)` becomes that call's nextCursor. it's on you to decide what
the cursor actually means, could be a row offset, a keyset, a token from some upstream api, whatever
fits your data:

```rust
#[async_trait]
impl MCPToolExecutor for BigListTool {
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        const PAGE: usize = 100;
        let offset: usize = cursor.and_then(|c| c.parse().ok()).unwrap_or(0);
        /* fetch one extra row so we can tell "exactly 100 left" apart from "more than 100 left" */
        let mut rows = fetch_rows(offset, PAGE + 1);
        let has_more = rows.len() > PAGE;
        rows.truncate(PAGE);
        let next = has_more.then(|| (offset + PAGE).to_string());
        (rows.into_iter().map(|r| MCPExecutionResult::TEXT(r.into())).collect(), next)
    }
}
```

when `next` is `Some(...)` it comes back to the caller as nextCursor next to your content array, same
as the router's own list pagination, the LLM sees it and can call you again with `"cursor": "<value>"`.

## other stuff this handles for you

- process a batch (a json array of requests) instead of one at a time and get an array back, matched up
  by id. notifications inside a batch just don't get an entry in the response array. an empty batch
  array is a straight buggin', not a valid empty batch (and is handled gracefully).
- ping works out of the box, no registration needed, just answers with an empty result
- a request with no id at all is a notification and gets nothing back. a request with an explicit
  `"id": null` is a real request and gets `id: null` echoed back. these are not the same thing even
  though it's tempting to treat them the same
- if you return `MCPExecutionResult::ERROR((msg, Some(data)))`, that data doesn't get thrown away, it
  comes back as an extra content block on tools/call (next to isError: true), or as error.data
  wherever a real json-rpc error object gets returned instead
- completion/complete works for resources out of the box, no code required so you can keep eating your cake and
  having it too. default impl looks at every uri your resource's meta() knows about, strips off whatever's common
  across all of them, and substring-matches what's left against whatever the client is typing. override `complete()` on your
  own MCPResource impl if your instances live somewhere the router can't enumerate up front (eg a
  database) and you want real suggestions instead of whatever happens to be in meta(). prompts
  don't get the free automatic version since there's nothing for the router to search through, they
  just get the override, see ez mode prompts below


## transport modes

this router can be used in both transport mechanisms, http et stdio.  these should get you started

### stdio

> Stan: Now, you know it's up to you whether or not you want to just do the bare minimum. Or... well, like Brian, for example, has thirty seven pieces of flair, okay. And a terrific smile.
>
> Joanna: Okay. So you... you want me to wear more?
>
> Stan: Look. Joanna.
>
> Joanna: Yeah.
>
> Stan: People can get a cheeseburger anywhere, okay? They come to Chotchkie's for the atmosphere and the attitude. Okay? That's what the flair's about. It's about fun.
>
> Joanna: Yeah. Okay. So more then, yeah?
>
> Stan: Look, we want you to express yourself, okay? Now if you feel that the bare minimum is enough, then okay. But some people choose to wear more and we encourage that, okay? You do want to express yourself, don't you?

```rust
use serde_json::Value;
use tokio::io::{self, AsyncBufReadExt, BufReader};

use mcp_router::StreamExt;
use mcp_router::router::{Router, RouterResponse};

#[tokio::main]
async fn main() {
    let router = Router::new().build();
    /* register your own tools/resources/prompts on the registry here, see ez mode /
     * medium difficulty below for how */

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        match router.exec_from_value(v).await {
            /* a Notification (no id in the request) gets no meaningful response, per JSON-RPC */
            RouterResponse::Value(v) if v.is_null() => {}
            RouterResponse::Value(v) => {
                println!(
                    "{}",
                    serde_json::to_string(&v).expect("valid json in, valid json out")
                );
            }
            /* one of your tools/resources returned STREAM -- see "streaming" below. over stdio
             * each item is just another line of output; if you need to feed a reply back in for
             * a server-initiated request (sampling/roots/elicitation), send it into `s.sender` */
            RouterResponse::Stream(mut s) => {
                while let Some(item) = s.receiver.next().await {
                    println!(
                        "{}",
                        serde_json::to_string(&item).expect("valid json in, valid json out")
                    );
                }
            }
        }
    }
}
```

### http

this example uses rocket but contains enough information to hook it up to your own framework

```rust
use rocket::{http::Status, post, response::status, routes, serde::json::Json, State};
use serde_json::Value;

use mcp_router::router::{Router, RouterResponse};

#[post("/mcp", format = "json", data = "<body>")]
async fn mcp(body: Json<Value>, router: &State<Router<'static>>) -> status::Custom<Json<Value>> {
    match router.exec_from_value(body.into_inner()).await {
        /* a Notification (no id in the request) gets no meaningful response, per JSON-RPC */
        RouterResponse::Value(v) if v.is_null() => status::Custom(Status::Accepted, Json(Value::Null)),
        RouterResponse::Value(v) => status::Custom(Status::Ok, Json(v)),
        /* a streaming tool/resource can't be answered as one JSON body over plain
         * request/response HTTP -- see "streaming" below for turning this into an SSE response */
        RouterResponse::Stream(_) => status::Custom(
            Status::NotImplemented,
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "error": {"code": -32000, "message": "this endpoint doesn't support streaming responses yet"}
            })),
        ),
    }
}

#[rocket::launch]
fn rocket() -> _ {
    /* build the router once and share it, rather than constructing one per request */
    rocket::build()
        .manage(Router::new().build())
        .mount("/", routes![mcp])
}
```

## ez mode

this mode is for mostly fully managed users who just want to bootstrap an MCP server and don't really need a lot of edge case management. you can likely
implement everything you need to do in this mode. see below in this readme for a further discussion as to why you'd want to go to a less managed level.

### ez mode tools

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(MCPTool, Deserialize, Serialize)]
#[meta(title = "FingerSaw", description = "Cuts off one of the user's fingers")]
struct FingerSaw {
    example: u32,
    optional_example: Option<u32>,
    /* anything JSON serializable works here */
}

#[async_trait]
impl MCPToolExecutor for FingerSaw {
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        /* self here is a FingerSaw, you can do whatever you need to in here */

        (
            vec![MCPExecutionResult::TEXT(
                format!(
                    "example={},optional_example={:?}",
                    self.example,
                    self.optional_example,
                )
                .into(),
            )],
            None,
        )
    }
}

/* somewhere else in a transport mechanism, shown here as a rocket route */
#[post("/mcp", format = "json", data = "<body>")]
pub async fn mcp(body: Json<Value>) -> Json<Value> {
    /* you might put this default router into a fairing or whatever your HTTP framework's analog,
     * see "transport modes" above for a fuller example including notification handling, and
     * "streaming" below if any of your tools/resources return STREAM */
    match mcp_router::router::Router::new().exec_from_value(body.into_inner()).await {
        mcp_router::router::RouterResponse::Value(v) => Json(v),
        mcp_router::router::RouterResponse::Stream(_) => Json(serde_json::json!({
            "jsonrpc": "2.0",
            "error": {"code": -32000, "message": "this endpoint doesn't support streaming responses yet"}
        })),
    }
}
```

### ez mode resources

```rust
use async_trait::async_trait;
use mcp_router::registry::{
    MCPResource, MCPResourceExecutor, MCPResourceResult,
};
use serde::{Deserialize, Serialize};

#[derive(MCPResource, Deserialize, Serialize)]
#[meta(title = "HandSaws", description = "Lets the LLM know what handsaws you can use the FingerSaw on")]
struct HandSaw {
    dsn: udsn::DSN, /* this is optional and the only field this struct can ever get populated with
                     * by way of the MCP spec. you can either use the DSN as a resource template
                     * or you need to manually list your resources per the spec.
                     * this struct member can be omitted safely if it's not needed in the resource
                     * execution
                     */
}

#[async_trait]
impl MCPResourceExecutor for HandSaw {
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        /* self here is a HandSaw, you can do whatever you need to in here */
        if self.dsn.protocol == "file" {
            /* serve files */
        } else if self.dsn.protocol == "git" {
            /* do something with git */
        }

        (
            vec![
                MCPResourceResult::new("file:///example".to_string(), "example file".to_string())
                    .build(),
            ],
            None,
        )
    }

    fn serves(dsn: &udsn::DSN) -> bool {
        dsn.protocol == "git" || dsn.protocol == "file"
    }

    fn is_template() -> bool {
        true
    }
}

/* somewhere else in a transport mechanism, shown here as a rocket route.
 * the same route and router is able to handle everything, you don't need
 * to do any manual RPC handling for MCP methods
 */
#[post("/mcp", format = "json", data = "<body>")]
pub async fn mcp(body: Json<Value>) -> Json<Value> {
    /* you might put this default router into a fairing or whatever your HTTP framework's analog,
     * see "transport modes" above for a fuller example including notification handling, and
     * "streaming" below if any of your tools/resources return STREAM */
    match mcp_router::router::Router::new().exec_from_value(body.into_inner()).await {
        mcp_router::router::RouterResponse::Value(v) => Json(v),
        mcp_router::router::RouterResponse::Stream(_) => Json(serde_json::json!({
            "jsonrpc": "2.0",
            "error": {"code": -32000, "message": "this endpoint doesn't support streaming responses yet"}
        })),
    }
}
```

### ez mode prompts

```rust
use async_trait::async_trait;
use mcp_router::registry::{
    MCPExecutionResult, MCPPrompt, MCPPromptExecutor, MCPPromptResult, MCPPromptMessage
};
use serde::{Deserialize, Serialize};

#[derive(MCPPrompt, Deserialize)]
#[meta(title = "TestPrompt", description = "rot13s the given string")]
struct TestPrompt {
    #[arg(description = "the string to rotate")]
    string_to_rot13: String,
}

#[async_trait]
impl MCPPromptExecutor for TestPrompt {
    async fn execute(&self) -> MCPPromptResult {
        MCPPromptResult {
            description: None,
            messages: vec![MCPPromptMessage {
                role: "user".to_string(),
                content: MCPExecutionResult::TEXT(
                    self.string_to_rot13
                        .chars()
                        .map(|c| match c {
                            'a'..='m' | 'A'..='M' => ((c as u8) + 13) as char,
                            'n'..='z' | 'N'..='Z' => ((c as u8) - 13) as char,
                            _ => c,
                        })
                        .collect::<String>()
                        .into(),
                ),
            }],
        }
    }
}
```

completion is optional and off by default for prompts (see notes above for resources), override `complete()` if 
you want to offer suggestions for one of your prompt's arguments. return `None` for anything you don't want to handle
(or don't impl at all, None is the default):

```rust
impl MCPPrompt for TestPrompt {
    /* ...get_executor, meta, params, from_args as above... */

    fn complete(argument_name: &str, partial_value: &str) -> Option<Vec<String>> {
        if argument_name == "string_to_rot13" {
            Some(
                ["all your base", "CCRU, IYKYK", "attack at dawn"]
                    .into_iter()
                    .filter(|s| s.contains(partial_value))
                    .map(String::from)
                    .collect(),
            )
        } else {
            None
        }
    }
}
```

resources get the same override, same shape, `fn complete(argument_name: &str, partial_value: &str)
-> Option<Vec<String>>` on your `MCPResource` impl. return `Some(values)` to short-circuit the
automatic meta()-based matching entirely and hand back your own answer instead, or return `None` (or
just don't override it) to fall through to the automatic behavior described above.

## medium difficulty

medium difficulty is where you might end up if you are generating resources or have specific routing requirements. good use cases for this
are:

- you want multiple MCPs handled in one transport mechanism
  - eg one router for http transport @ /v1/mcp and another @ /v2/mcp
- you have static resources (not templates) you want to list in the router initialization handshake
  - eg you only want to mcp to know of files in a directory at server startup
  - you're generating access/commands from a config file and will handle them programmatically
  - you are willing to implement both traits needed manually rather than using a resource template

example resource:

```rust
use async_trait::async_trait;
use mcp_router::registry::{
    FromArgResult, MCPMeta, MCPResource, MCPResourceExecutor, MCPResourceResult,
};
use serde_json::Value;

#[derive(serde::Deserialize)]
pub struct ManualResource {
    dsn: udsn::DSN,
}

#[async_trait]
impl MCPResourceExecutor for ManualResource {
    async fn execute(&self, cursor: Option<String>) -> (Vec<MCPResourceResult>, Option<String>) {
        println!("dsn executor called: {}", self.dsn.to_string());
        (vec![], None)
    }

    fn serves(dsn: &udsn::DSN) -> bool {
        /* this is only called when is_template is true
         * the DSN must match exactly if is_template is false
         */
        dsn.protocol == "manual-resource"
    }

    fn is_template() -> bool {
        true
    }
}

impl MCPResource for ManualResource {
    fn get_executor(&self) -> &dyn MCPResourceExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new()
            .name("meta_example")
            .uri("manual-resource:///")
            .build()]
    }
    fn params() -> Value {
        Value::Null
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Resource(Box::new(s)),
            Err(e) => {
                /* handle your error here */
                FromArgResult::Error(e.to_string())
            }
        }
    }
}

/* elsewhere in your registry initializer */

pub fn init_router() {
    use std::collections::HashMap;
    let registry = registry::Registry::new_from(HashMap::new(), HashMap::new());
    registry.register_resource_adapter::<ManualResource>("manual-resource://{some path}");
    let router = Router::default().registry(&registry).build();

    /* do whatever you need to do with your router here */
}
```

same idea, but a prompt instead of a resource:

```rust
use async_trait::async_trait;
use mcp_router::registry::{
    FromArgResult, MCPExecutionResult, MCPMeta, MCPPrompt, MCPPromptExecutor, MCPPromptMessage,
    MCPPromptResult,
};
use serde_json::Value;

#[derive(serde::Deserialize)]
pub struct ManualPrompt {
    topic: String,
}

#[async_trait]
impl MCPPromptExecutor for ManualPrompt {
    async fn execute(&self) -> MCPPromptResult {
        MCPPromptResult {
            description: None,
            messages: vec![MCPPromptMessage {
                role: "user".to_string(),
                content: MCPExecutionResult::TEXT(format!("Let's talk about {}", self.topic).into()),
            }],
        }
    }
}

impl MCPPrompt for ManualPrompt {
    fn get_executor(&self) -> &dyn MCPPromptExecutor {
        self
    }
    fn meta() -> Vec<MCPMeta> {
        vec![MCPMeta::new().name("manual_prompt").build()]
    }
    fn params() -> Value {
        serde_json::json!({
            "name": "manual_prompt",
            "arguments": [{"name": "topic", "required": true}]
        })
    }
    fn from_args(v: &Value) -> FromArgResult {
        match serde_json::from_value::<Self>(v.clone()) {
            Ok(s) => FromArgResult::Prompt(Box::new(s)),
            Err(e) => FromArgResult::Error(e.to_string()),
        }
    }
}

/* elsewhere in your registry initializer */

pub fn init_router() {
    use std::collections::HashMap;
    let registry = registry::Registry::new_from(HashMap::new(), HashMap::new());
    registry.register_prompt_adapter::<ManualPrompt>("manual_prompt");
    let router = Router::default().registry(&registry).build();

    /* do whatever you need to do with your router here */
}
```

## streaming

most tools/resources are one-and-done: you get cornered in the alley and you cough up your cash.
sometimes, though, you just want to have a chit chat; a report that takes 30 seconds and shouldn't leave
the LLM reading War et Peace at the bar, alone, a tool might need to lean over and ask the client's
own LLM something mid-task (`sampling/createMessage`), a resource might rather push updates than
get polled every 5 seconds like it soiled the LLM's morning cheerios. `MCPExecutionResult::STREAM` and
`MCPResourceResult::STREAM` handle all of it with one primitive, and `execute()`'s signature doesn't
change one bit to get it - streaming is just a different thing you're allowed to hand back, not a
different method you have to write.

### making a tool/resource stream

grab a channel pair from `mcp_router::stream_channel` (a re-export of `futures_channel::mpsc::channel`,
so it works under whatever executor you're already running - this crate has never forced tokio on
anybody, it's only ever lived in `[dev-dependencies]` here), go spawn whatever background work you need
on *your own* project's runtime, and hand back the receiver/sender pair wrapped in `STREAM(...)`:

```rust
use mcp_router::registry::{MCPExecutionResult, MCPExecutionResultStream, MCPToolExecutor};
use mcp_router::{SinkExt, stream_channel};

#[async_trait]
impl MCPToolExecutor for SlowReportTool {
    async fn execute(&self, _cursor: Option<String>) -> (Vec<MCPExecutionResult>, Option<String>) {
        let (mut out_tx, out_rx) = stream_channel(16);
        let (in_tx, _in_rx) = stream_channel(16); // only wire this up if you're expecting a reply, see below

        tokio::spawn(async move {
            for pct in [20, 60, 100] {
                out_tx.send(serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": {"progress": pct, "total": 100}
                })).await.ok();
                do_some_work(pct).await;
            }
            /* no "method" field means this is the actual answer -- the router wraps it with the
             * original request's id and closes the stream right behind it */
            out_tx.send(serde_json::json!({
                "content": [{"type": "text", "text": "report ready"}]
            })).await.ok();
        });

        (
            vec![MCPExecutionResult::STREAM(MCPExecutionResultStream { receiver: out_rx, sender: in_tx })],
            None,
        )
    }
}
```

that `tokio::spawn` is yours, not the router's. this crate still doesn't know tokio exists outside its
own test suite, and it never spawns anything on your behalf - eat whatever nachos you brought to the superbowl
party.

here's the whole rulebook the router applies to every item that shows up on your `receiver`:

- has a `"method"` field (a notification, or a server-initiated request like `sampling/createMessage`
  waiting on a reply) -> gets relayed to the client verbatim, stream stays open
- no `"method"` field -> that's your actual answer, it gets wrapped as
  `{"jsonrpc": "2.0", "id": <the original request's id>, "result": <your value>}` and the stream closes
  right after
- your sender gets dropped without ever sending a method-less item -> the router doesn't just shrug,
  it synthesizes `{"jsonrpc": "2.0", "id": <the original id>, "error": {"code": -32603, "message":
  "stream ended without a result"}}` so the client finds out something went sideways instead of
  sitting there waiting on the calvary

resources get the exact same deal via `MCPResourceResult::STREAM(MCPExecutionResultStream {...})`,
right alongside the `LINK`/`TEXT`/`BLOB` variants from up above. prompts do **not** get to stream,
full stop - a prompt's whole job is handing back a fixed set of messages to splice into a
conversation, not running a background job, so a prompt returning a `STREAM` gets
that message quietly dropped and a `tracing::error!` fired off instead of anything reaching the
client.

### bidirectional: sampling, roots, elicitation

the `sender` half you got handed back is how a reply finds its way back to your still-running tool.
fire off a `{"id": ..., "method": "sampling/createMessage", ...}` item through your `receiver`, then
just sit there awaiting your own `in_rx` for the matching id like a normal person:

```rust
use mcp_router::StreamExt;

out_tx.send(serde_json::json!({
    "jsonrpc": "2.0", "id": "sample-1", "method": "sampling/createMessage",
    "params": {"messages": [/* ... */], "maxTokens": 100}
})).await.ok();

let reply = loop {
    match in_rx.next().await {
        Some(v) if v.get("id") == Some(&serde_json::json!("sample-1")) => break v,
        Some(_) => continue,   // not your reply, mind your business
        None => return,        // client bailed on you
    }
};
```

`roots/list` and `elicitation/create` work exactly the same way - lob a method+id item, wait for the
matching reply. there's no correlation helper bolted on for you on purpose, so the router's own
surface area stays small; the request id is yours to invent and yours to match back up.

### consuming a RouterResponse

`exec`/`exec_from_value` hand back a `RouterResponse` these days, not a bare `serde_json::Value`:

```rust
pub enum RouterResponse {
    Value(serde_json::Value),
    Stream(RouterStream),
}

pub struct RouterStream {
    pub receiver: /* impl Stream<Item = serde_json::Value> + Send, already run through the rules above */,
    pub sender: RouterStreamSender, // feed replies to server-initiated requests in here
}
```

```rust
use mcp_router::StreamExt;

match router.exec_from_value(body).await {
    RouterResponse::Value(v) => { /* business as usual, nothing changed for you here */ }
    RouterResponse::Stream(mut s) => {
        while let Some(item) = s.receiver.next().await {
            /* forward each item over whatever your transport looks like -- SSE `data:` frames
             * over HTTP, one more line over stdio, you get the idea */
        }
        /* need to feed a reply back in for sampling/roots/elicitation? send it into s.sender */
    }
}
```

`mcp_router::{StreamExt, SinkExt}` are just re-exports of the underlying `futures_util` traits, so you
don't have to go add `futures-util` to your own `Cargo.toml` just to call `.next()`/`.send()` on these.
i've already filled out the paperwork, you just need to sign it.

### batch requests

drop even one streaming item into a batch (a json array of requests) and the whole response gets
elevated to `RouterResponse::Stream` instead of the usual plain array. anything that finishes
immediately gets shoved onto that merged stream the second it's ready - it doesn't sit around
waiting for the springtime breeze - and the stream stays open until every single item, streaming or not, has
handed over its final answer. nothing goes missing just because one entry in the batch decided to
stop to tie its shoe, and this goes for as many streaming items as you throw into one batch, not just
one.

### what streaming still doesn't fix

- `notifications/tools|resources|prompts/list_changed` wants a connection-lifetime stream that isn't
  tied to answering any particular client request - the server just shouts onto it whenever the
  registry changes, whether anyone asked or not. that's a genuinely different primitive (a
  per-connection subscription, not a per-call stream) and this router doesn't have one.
- `logging/setLevel` is session state that's supposed to quietly affect every later, unrelated call on
  the same connection - not something a per-call return value can express no matter how you squint at
  it. if you actually need it, the real answer is your own `tracing::Layer` tagging spans with a
  session id and forwarding matching log lines to that session's own connection, entirely in your own
  embedding code. this crate stays willfully oblivious to logging semantics.

# known corners for you to put your nose in 

* there is no guard against conflicting resource URIs, if you register two then the first is chosen from an unordered HashMap (making it non-deterministic)
* same deal with two resource templates that both claim to serves() the same uri, whichever gets iterated first wins and that order isn't guaranteed
* no `notifications/tools|resources|prompts/list_changed` and no `logging/setLevel` - see "what streaming still doesn't fix" above, both need a per-session concept this router doesn't have


## TODO

- document MCPExecutionResult
- talk about transport or make examples of them, really stick it to 'em
