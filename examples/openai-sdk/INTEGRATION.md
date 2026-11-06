# Integration record

## Between waves 2-3

Coordinator integration of the between-wave library refinements (`send Policy;`,
`Prepared::follow`, `Received::decode_limited`) at Concord `1f71432c`. Role: integrator,
Claude Opus 5.5 agent session (requested model; this record does not re-derive it from
harness metadata). Not an independent review: nobody has reviewed this change yet.

**Verdict: adopted for the declaration (`send SendPolicy;`, SSE endpoint `-> stream`); the
`OpenAi` domain methods stay.** Decisive reason: `OpenAiApi` is public, and its generated
`send()` gave callers `concord::Error` without OpenAI's error document. For
`response_stream` it was worse: `-> Vec<u8>` + `Binary` buffered the whole event stream with no
bound (probe below). Now every generated `send()` returns the SDK's `Result<Response<T>, Error>`,
and `response_stream().send()` returns the unread body after the status check. `stream_response`
uses the same path as the JSON operations instead of its own `checked(prepare()?.receive())`.

What stays SDK-local, as before:

- The `OpenAi` methods carry per-operation input rules (empty IDs, the `stream` flag, the
  multipart filename and boundary-collision refusal, polling bounds, the per-file download
  limit); AUTHOR_GUIDE §5.13 says to keep domain methods for those.
- The SSE parser.
- The hand-built multipart body.

No shared SSE or multipart code was written. `Prepared::follow` does not apply (no server URLs
are followed). `decode_limited` was not adopted for JSON successes: no friction asked for it.
`content` already bounds downloads with `bytes_limited(limit)`.

### Declaration

Before (`src/lib.rs`, `1f71432c`):

```rust,ignore
    /// Wire declarations for the supported operations.
    pub client OpenAiApi(#[sensitive] header(AUTHORIZATION) authorization: HeaderValue) {
        base "https://api.openai.com";
        path "v1";
        request Json;
        response Json;
        /// Create a response.
        POST response(body request: ResponseRequest) -> ResponseObject { path "responses"; }
        /// Open a response event stream.
        POST response_stream(body request: ResponseRequest) -> Vec<u8> {
            path "responses";
            response Binary;
            headers { "accept" = "text/event-stream"; }
        }
```

After:

```rust,ignore
    /// Wire declarations for the supported operations. Every `send()` returns
    /// `Result<Response<T>, Error>` (this crate's, through [`SendPolicy`]); [`OpenAi`] adds the
    /// per-operation input rules, the SSE parser and the bounded downloads.
    pub client OpenAiApi(#[sensitive] header(AUTHORIZATION) authorization: HeaderValue) {
        base "https://api.openai.com";
        path "v1";
        send SendPolicy;
        request Json;
        response Json;
        /// Create a response.
        POST response(body request: ResponseRequest) -> ResponseObject { path "responses"; }
        /// Open a response event stream: the unread body, for the SDK's SSE parser.
        POST response_stream(body request: ResponseRequest) -> stream {
            path "responses";
            headers { "accept" = "text/event-stream"; }
        }
```

The other five operations are unchanged. The explicit `accept: text/event-stream` still reaches
the wire (`tests/workflows.rs:70`, `tests/first_use.rs:91`, and the new protocol test).

### Adapter / policy code

Before: a helper that each wrapper called with a prepared request:

```rust,ignore
async fn send<T>(p: Prepared<T>) -> Result<Response<T>, Error> {
    let (r, meta) = checked(p.receive().await?).await?;
    let value = r.decode().await.map_err(|error| Error::Decode {
        meta: Box::new(meta.clone()),
        error,
    })?;
    Ok(Response { value, meta })
}
```

After: the same body as the client's policy:

```rust,ignore
pub struct SendPolicy;

impl<T: Send> SendWith<T> for SendPolicy {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T> + Send>(endpoint: E) -> Self::Output {
        let (r, meta) = checked(endpoint.prepare()?.receive().await?).await?;
        let value = r.decode().await.map_err(|error| Error::Decode {
            meta: Box::new(meta.clone()),
            error,
        })?;
        Ok(Response { value, meta })
    }
}
```

`checked` (status branch, 64 KiB error document) is unchanged.

SSE, before (`stream_response`): native response access through `Received`:

```rust,ignore
        let (r, meta) = checked(
            self.api
                .response_stream(request)
                .prepare()?
                .receive()
                .await?,
        )
        .await?;
        // ... content-type check on meta ...
        Ok(EventStream {
            response: r.into_response(),
```

After: the generated `send()` returns `Response<Incoming>`:

```rust,ignore
        let Response {
            value: incoming,
            meta,
        } = self.api.response_stream(request).send().await?;
        // ... content-type check on meta (unchanged) ...
        Ok(EventStream {
            response: incoming.into_response(),
```

The parser (`EventStream::next`) is unchanged and still reads the native `reqwest::Response`.
`Incoming` has no chunk-reading method of its own: `into_response()` or `into_parts()` is the
documented way for an SDK reader to reach the body. So `-> stream` does not remove the native
access at the parser. What it changes is that the declaration says the body is read
incrementally, and that its generated `send()` no longer drains it.

Lines of SDK code (counted by script: non-blank lines that are not `//` comments, total in
brackets):

| | before | after |
| --- | --- | --- |
| `checked` + helper / policy | 24 (25) | 28 (33) |
| `impl OpenAi` | 144 (150) | 140 (146) |
| `src/lib.rs` | 533 (561) | 533 (567) |

The code size is the same. The change is in which entry points have which contract, not in
size.

### One caller call

Through the domain client: unchanged (`README.md`):

```rust,ignore
let ordinary = client.create_response(request.clone()).await?;
```

Through the public declaration:

```rust,ignore
// before: Result<ResponseObject, concord::Error>; a 400 is ErrorKind::Status, document unread
let object = api.response(request).send().await?;
// after: Result<Response<ResponseObject>, Error>; a 400 is Error::Provider with the document
let object = api.response(request).send().await?.value;
```

Observed for `response_stream` with a 200 `text/event-stream` head and a body that never ends
(`Body::Stall`), using a temporary probe test (deleted):

- at `1f71432c`: `api.response_stream(request).send()` had **not** returned after 2 s (it
  decodes the whole body as `Vec<u8>`);
- after the change: it returned at once with `Response<Incoming>`.

The new protocol test keeps the second behaviour as a regression check.

### What a maintainer edits to add an operation

- Before: the declaration, and if callers should get the SDK error, an `OpenAi` method calling
  `send(self.api.new_op(..).prepare()?).await`. Without that method, the operation's public
  generated `send()` returned `concord::Error`.
- After: the declaration. Its generated `send()` already returns `Result<Response<T>, Error>`,
  since the policy is generic over `T: Send`. An `OpenAi` method is needed only for an input
  rule, and it calls `self.api.new_op(..).send().await`. A streaming operation is declared
  `-> stream` and goes through the same policy.

### Test changes, each explained

No existing test changed; all 30 pass as before, including the 4 in `tests/split_and_reorder.rs`
(the SSE split layouts and batch reorderings described in MAINTENANCE.md; their counts were not
re-derived here). One test was added:
`tests/protocol.rs` `generated_send_has_the_sdk_error_and_does_not_read_the_event_stream`. It
uses the public `OpenAiApi` directly:

- `response(..).send()` on a 400 is `Error::Provider` with status 400 and `param == "model"`;
- `response_stream(..).send()` on a 200 with a stalled body returns within 5 s, with status 200,
  operation `OpenAiApi.response_stream`, and `accept: text/event-stream` on the wire.

Against the `1f71432c` declaration, the first half does not compile (`send()` returned
`concord::Error`). The second half matches the probe above, which did not return within 2 s.

### Observations not changed

- `EventStream` maps body read failures to `Error::Transport(reqwest::Error)`, whose `Debug`
  can print the URL. `Incoming::into_parts()` would give the operation for
  `Operation::transport_error`, which strips the URL, but `tests/workflows.rs:205` expects the
  `Error::Transport` variant. Left for the owner.
- AUTHOR_GUIDE §5.10 suggests `EncodedBody` for a body whose media type (a multipart boundary)
  is produced with the bytes. The fixed boundary with collision refusal works and its exact
  bytes are tested, so it was not changed.

### Verification (observed)

At `1f71432c` before the change: 18 first_use + 3 protocol + 4 split_and_reorder + 5 workflows
passed; 0 doctests.

After the change (`CARGO_BUILD_JOBS=6`):

- `cargo test --locked -p concord-sdk-openai`: 18 first_use + 4 protocol (3 unchanged + 1 new) +
  4 split_and_reorder + 5 workflows passed, 0 failed; 0 doctests.
- `cargo clippy --locked -p concord-sdk-openai --all-targets -- -D warnings`: clean.
- `cargo doc --locked -p concord-sdk-openai --no-deps`: no warnings.
- `cargo fmt -p concord-sdk-openai -- --check`: clean.

Offline only; no OpenAI API calls.

### After `Incoming::chunk` (coordinator, `08df6614`)

The gap above ("`Incoming` has no chunk-reading method of its own") was closed in the library
by `Incoming::chunk()`. `EventStream` now keeps the `Incoming` instead of the native response
(`body: concord::Incoming`; `self.body.chunk().await?`), so the parser never leaves Concord's
boundary. Behaviour change, observed in `tests/workflows.rs`: a body that breaks off mid-stream
was `Error::Transport(reqwest::Error)`, whose `Display` (this SDK prints `{:?}`) included the
request URL; it is now `Error::Concord(e)` with `e.kind() == Transport` and
`e.operation()` = `OpenAiApi.response_stream`, URL removed. The `Transport` variant is gone
(nothing else produced it). The test now asserts kind, operation and redaction instead of only
the variant. Code: 3 changed lines in `src/lib.rs`, one variant removed. The SSE parser itself is
unchanged; whether it becomes shared code is decided with ntfy in wave 3.

## Between waves 3-4 (shared SSE reader)

The second independent SSE consumer (ntfy, wave 3) met decision W2-8's condition, so
`concord-extras` gained an `sse` feature (`concord::sse`: `SseReader`, `SseEvents`, `Event`, the
`Sse` format) and `EventStream` now reads through it.

### Before / after (`src/lib.rs`, 566 → 551 lines; 80 removed, 65 added)

- **Removed:** the hand-written framing in `EventStream::next` (line splitting on LF with a CRLF
  strip, the `data` buffer, blank-line dispatch, the 1 MiB check, the EOF branch over
  `Incoming::chunk`; about 45 lines) and the 14-line `Content-Type` comparison in
  `stream_response`.
- **Added:** `Body`, a 13-line `concord::stream::ByteSource` over `Incoming::chunk` that keeps
  Concord's own transport error, so a body that breaks off is still `Error::Concord(e)` (kind
  `Transport`, operation `OpenAiApi.response_stream`, URL removed: `tests/workflows.rs:206`); the
  mapping of the reader's `ReadError` to this crate's `Error`; a call of the format's head check
  (`<Sse as Incremental<concord::sse::Event>>::check_content_type`); a private
  `MAX_EVENT_BYTES` (1 MiB, the reader's default stated explicitly).
- **Kept:** the endpoint (`-> stream`, `accept: text/event-stream`), `Event` and its typed
  variants, `response.completed` ends the stream (then `Ok(None)`), `response.failed` and `error`
  are typed errors, an end before completion is `Error::Stream`, malformed event JSON is
  `Error::Json` and reading may go on.

### Behaviour changes (from the code; no test assertion changed)

- **Framing follows the WHATWG interpretation:** a lone CR is a line end too (a CR-only stream
  used to end as "SSE ended before response.completed"), and a leading byte order mark is
  skipped (it used to hide the first `data` field). LF, CR LF and a CR LF split across chunks
  behave as before.
- **Invalid UTF-8** anywhere (also in a comment or an unknown field) is `Error::Stream` and ends
  the stream; it used to be `Error::Json` for `data` and ignored elsewhere.
- **An early end** is still `Error::Stream`; its message now says where: inside a line or inside
  an event (the reader's content-free `ReadError` text), or "SSE ended before
  response.completed" at an event boundary.
- **The 1 MiB bound** counts the event being assembled (its `data`, event type and unfinished
  line) instead of the buffered input plus the incoming chunk: a large chunk holding many small
  events no longer fails, and exceeding the bound now ends the stream.
- **The head check** is Concord's: exactly one `Content-Type`, `text/event-stream`, charset
  absent or UTF-8 (it used to compare only the essence of the first field); a refusal is still
  `Error::Stream("expected text/event-stream")`.
- **After a transport error** the stream is fused (`Ok(None)`); before, a further `next()` read
  the broken body again.

### Verification (observed)

`cargo test --locked -p concord-sdk-openai`: 18 first_use + 4 protocol + 4 split_and_reorder + 5
workflows passed, 0 failed; `git diff` of `tests/` is empty. The every-boundary perturbation
(`tests/split_and_reorder.rs`: every single cut and one byte per chunk, LF and CRLF copies)
passes unchanged on the shared reader. Offline only; no OpenAI API calls.
