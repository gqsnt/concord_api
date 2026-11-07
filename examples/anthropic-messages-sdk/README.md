# concord-sdk-anthropic-messages

A bounded SDK for the [Anthropic Messages API](https://platform.claude.com/docs/en/build-with-claude/streaming),
built on Concord. It covers two connected workflows and nothing else:

1. **Create a message** (non-streaming) with text input, and inspect the returned content blocks.
2. **Stream a message** and consume its server-sent events incrementally through `message_stop`,
   ending with the assembled message.

Every test runs against a local stand-in server (`examples/testserver`); the real API has
**not** been called (live provider calls: NOT_RUN). Sources and fixtures are listed in
[`PROVENANCE.md`](PROVENANCE.md); the Concord costs met while writing it in
[`FRICTION.md`](FRICTION.md).

## Calling it

```rust
use concord_sdk_anthropic_messages::{Anthropic, AnthropicConfig, MessageRequest, http_client};

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let api = Anthropic::new(http_client()?, AnthropicConfig { api_key: "sk-ant-...".into() });
    let request = MessageRequest::user("claude-opus-5-5", 256, "Hello")?;

    // Workflow 1: POST /v1/messages, the whole message.
    let created = api.messages().create(request.clone()).send().await?;
    println!("request id: {:?}", created.request_id());
    for block in &created.value.content {
        match block.as_text() {
            Some(text) => println!("text: {text}"),
            None => println!("{} block (not interpreted)", block.block_type()),
        }
    }

    // Workflow 2: the same request with "stream": true.
    let mut stream = api.messages().stream(request.streaming()).send().await?.value;
    while let Some(event) = stream.next().await? {
        if let Some(text) = event.text_delta() {
            print!("{text}");
        }
    }
    let message = stream.final_message().await?; // `message_stop` arrived
    println!("\n{:?}, usage {:?}", message.stop_reason, message.usage);
    Ok(())
}
```

The crate documentation carries the same example as a compiled doctest.

- **The client** is the generated `Anthropic` (client → scope `messages` → endpoints `create`
  and `stream`). Its required value is the API key, sent as `x-api-key` and never printed by
  Concord. `anthropic-version` defaults to `2026-10-21`, the value on the cited pages
  (`API_VERSION`); `set_anthropic_version(..)` replaces it. `with_base_url(..)` points the
  client elsewhere (the `/v1` prefix is kept).
- **The transport** is yours. `http_client()` builds the one this SDK documents: no redirects
  (reqwest drops `Authorization` on a cross-origin redirect but forwards `x-api-key`; the tests
  show both) and no reqwest retries, so one `send()` is one request on the wire. Add
  `connect_timeout`/`read_timeout` as you need; a whole-request `timeout` also cuts a long
  stream.
- **Requests** are `MessageRequest::new(model, max_tokens, messages)` (or `::user(..)` for one
  user turn), with `InputMessage::user/assistant` text turns. It refuses an empty model, a zero
  `max_tokens` or no message (`InvalidRequest`, nothing sent); everything else is the API's to
  judge. `request.streaming()` is the body of the streaming endpoint, a distinct type
  (`StreamRequest`), so a streaming body cannot be sent to `create` or the other way round.
- **Every `send()`** returns `Result<Response<T>, Error>`: the value and the response head
  (`response.request_id()`, `response.meta`).

## What a message contains

`Message` has the documented fields (`id`, `type`, `role`, `content`, `model`, `stop_reason`,
`stop_sequence`, `usage`). Content blocks are `ContentBlock::Text { text }` or
`ContentBlock::Other(..)`: any other type (`tool_use`, `thinking`, `server_tool_use`, a type added
later) is kept as its JSON object and **not interpreted**. In a stream, the deltas of such a block
are kept on it uninterpreted (`OtherBlock::deltas`), so an assembled `tool_use` block still has the
`input: {}` of its `content_block_start`; its `input_json_delta` strings are there to be joined if
you choose. `usage` and each of its counts are optional: the documented thinking stream carries
none. A stream's `message_delta` counts are cumulative, so an assembled message takes each count
from the latest event that carried it.

## Failure semantics

| What happened | What you get |
|---|---|
| Invalid input (empty model, `max_tokens == 0`, no message) | `Error::InvalidRequest`, nothing sent |
| A key that cannot be a header value | `Error::Concord`, kind `Build`, `delivery() == NotSent`; the key is not echoed |
| No response (connection refused, timeout, reset) | `Error::Concord`, kind `Transport`, `delivery() == Unknown`: a create may have been generated and billed; the SDK does not resend |
| A non-2xx status (400, 401, 402, 403, 404, 409, 413, 429, 500, 504, 529, also for the streaming endpoint before its stream starts) | `Error::Api(ApiError)`: `status()`, `error_type()` (`ApiErrorType`, unknown types kept verbatim), `message()`, `request_id()` (header, else the document's `request_id`), `retry_after(now)` |
| A non-2xx whose document is HTML, unlabelled, not JSON, not the documented shape, longer than 64 KiB or cut off | `Error::Api` with the status kept and `document_error()` saying why (Concord `Metadata`, `Decode`, `Limit`, `Transport`) |
| A 2xx `create` that is not `application/json`, not a message, longer than 8 MiB or cut off | `Error::Concord` (`Metadata`, `Decode`, `Limit`, `Transport`), with the head (`error.status()`, `error.request_id()`) |
| A 2xx `stream` response that is not `text/event-stream` (charset absent or UTF-8) | `Error::Concord`, kind `Metadata`, the response kept unread |
| An `error` event after the 200 (`overloaded_error`, ...) | `Error::Incomplete`, cause `ErrorEvent` |
| The body ends before `message_stop` (cleanly, inside an event, or by a lost connection) | `Error::Incomplete`, cause `EndOfStream`, `MalformedStream` or `Transport` |
| An event over 1 MiB, data that is not JSON or not the documented shape, or an order the documented flow forbids (a delta for a block that is not open, an SSE name that differs from the data's `type`, `message_stop` with a block open or before any `message_delta`, ...) | `Error::Incomplete`, cause `EventTooLarge`, `InvalidEvent` or `Protocol` |

**An incomplete stream is never a final message.** `MessageStream::next()` returns `Ok(None)`
only after `message_stop`; after a failure every further call is `Err` again (cause
`AlreadyFailed`), never `Ok(None)`. `final_message()` returns a `Message` only for a complete
stream. What arrived before the failure is an `Incomplete::partial()`, a `PartialMessage` (text so
far, open blocks, stop reason and usage if they came), deliberately not a `Message` — even when a
`message_delta` already said `end_turn`.

**Nothing is retried or restarted.** The SDK sends each request exactly once (the tests check the
server saw one request). A 429/5xx/529 is returned with `retry_after`; a broken stream is returned
as `Incomplete`. Continuing an interrupted stream is the streaming page's "Error recovery": a new
request the application builds from the partial text (for Claude 4.6 and later, a user turn asking
to continue). That is an application decision, not this SDK's.

**Redaction.** Concord withholds the key from the client's and the prepared request's `Debug`
and from its errors. The `Display`/`Debug` of this SDK's errors and stream state (`Error`,
`ApiError`, `Incomplete`, `IncompleteCause`, `PartialMessage`, `MessageStream`) show statuses,
error types, request ids, counts and static labels — not the server's error `message`, not
generated text, not a serde error that could quote event data. Reach those deliberately:
`ApiError::message()`, `Incomplete::partial()`, `Incomplete::invalid_event_source()`. A `Message`
or a `StreamEvent` is your data: its derived `Debug` shows its content.

## Excluded

Tool execution, batches, extended-thinking interpretation and deterministic output equivalence
are outside this slice. So are system prompts, sampling parameters, images and documents, beta
headers, token counting, the Files API, platform variants (Bedrock, Vertex, Foundry, Claude
Platform on AWS), automatic retries and stream resumption. Their blocks and deltas pass through
uninterpreted when they appear in a response.

## Running the tests

```sh
cargo test --locked -p concord-sdk-anthropic-messages
cargo clippy --locked -p concord-sdk-anthropic-messages --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-anthropic-messages --no-deps
```

`tests/workflows.rs` runs both workflows end to end (chunked deliveries cut inside lines and
multi-byte characters, a per-event timeout) and the required failure (a midstream `error` event,
an end before `message_stop`, a lost connection, a body ending inside an event).
`tests/protocol.rs` checks the request shapes, every documented error class on both endpoints,
unreadable and oversized bodies, the event-flow rules, the redirect policy and redaction.
