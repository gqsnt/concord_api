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


## Between waves 3-4 (shared multipart format)

Library refinement R2: `concord::multipart` (feature `multipart` of `concord-extras` and the
facade), on top of Concord `7354d5a3`. Role: implementer, a Claude agent session spawned by the
coordinator (model and effort not re-derived from harness metadata here). Not an independent
review: nobody has reviewed this change yet.

**Verdict: adopted.** `upload_batch_file` no longer frames the body by hand; the declaration
states a `MultipartBody` and the format writes the parts, chooses the boundary and sends it as
the body's runtime media type. `OpenAi::upload_batch_file` keeps its signature, its unsafe-filename
refusal (`Error::Invalid`, before any HTTP) and the exact part layout.

### Declaration and method

Before (`src/lib.rs`, `7354d5a3`):

```rust,ignore
        /// Upload batch input.
        POST upload(body bytes: Vec<u8>) -> FileObject {
            path "files";
            request Binary;
            headers { "content-type" = "multipart/form-data; boundary=concord-batch-boundary"; }
        }

        if filename.is_empty()
            || filename
                .bytes()
                .any(|b| b < 32 || b == 127 || b == b'"' || b == b'\\')
            || bytes.windows(22).any(|w| w == b"concord-batch-boundary")
        {
            return Err(Error::Invalid(
                "unsafe filename or multipart boundary collision",
            ));
        }
        let mut body = format!("--concord-batch-boundary\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nbatch\r\n--concord-batch-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/jsonl\r\n\r\n").into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n--concord-batch-boundary--\r\n");
        self.api.upload(body).send().await
```

After:

```rust,ignore
        /// Upload batch input: a `multipart/form-data` body whose boundary the format chooses.
        POST upload(body form: MultipartBody) -> FileObject {
            path "files";
            request Multipart;
        }

        if filename.is_empty()
            || filename
                .bytes()
                .any(|b| b < 32 || b == 127 || b == b'"' || b == b'\\')
        {
            return Err(Error::Invalid("unsafe filename"));
        }
        let form = MultipartBody::form_data().text("purpose", "batch").file(
            "file",
            filename,
            "application/jsonl",
            bytes.to_vec(),
        );
        self.api.upload(form).send().await
```

The SDK's own filename check stays: its rule is stricter than the format's (it also refuses an
empty name and a tab) and its `Error::Invalid` is what `tests/first_use.rs` asserts. The format
refuses the rest (control characters, `"`, `\`) on its own as a `Build` error, so the check is
policy, not the only guard. `Cargo.toml` adds the facade feature `multipart`.

### Behaviour changes

- **Boundary.** `concord-batch-boundary` (fixed, declared as a `content-type` header) becomes
  the format's first candidate `concord-boundary-0`, or the next free one. The part bytes are
  otherwise identical.
- **Collision.** File bytes containing the boundary were refused (`Error::Invalid`, "unsafe
  filename or multipart boundary collision"); now the format chooses a boundary that occurs in no
  part, so such a file is uploaded byte for byte. The message is now `"unsafe filename"`, the only
  case left.
- **Low-level declaration.** The public `OpenAiApi::upload` takes a `MultipartBody` instead of
  pre-framed `Vec<u8>` bytes.

### Tests

`tests/protocol.rs::multipart_exact_bytes_and_validation` still asserts the exact header and
body. Two changes, both required by the boundary now being chosen by the format:

1. The expected `content-type` and the four delimiters change from `concord-batch-boundary` to
   `concord-boundary-0`: the SDK no longer declares a fixed boundary, and the format's first
   candidate does not occur in `{"custom_id":"one"}\n`. Every other expected byte is unchanged.
2. `upload_batch_file("ok", b"concord-batch-boundary").is_err()` asserted the collision refusal,
   which no longer exists (behaviour change above). In its place the test uploads
   `concord-batch-boundary\n--concord-boundary-0\n` (the former boundary and the format's first
   candidate) and asserts the exact request: boundary `concord-boundary-1`, the bytes verbatim.
   The script gains one reply for that upload.

The unsafe-filename assertions (`tests/protocol.rs` and
`tests/first_use.rs::invalid_batch_input_and_unsafe_filename_are_rejected_before_http`) and the
independent multipart reader of `tests/first_use.rs` (which takes the boundary from the header)
are unchanged.

### Verification (observed)

`CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-openai`: 18 first_use + 4 protocol +
4 split_and_reorder + 5 workflows passed, 0 failed; 0 doctests (the same counts as before the
change). The repository gate (`scripts/verify.sh`) result is in the coordinator's records.

Offline only; no OpenAI API calls.

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

## Between waves 3-4 (ResponseMeta)

Concord gained `concord::ResponseMeta` (an owned head: status, version, header map, operation;
`Received::meta()`; `Debug` prints header names only; AUTHOR_GUIDE §5.11). Base `92775757`.
Role: implementer, a Claude agent session spawned by the coordinator (model and effort not
re-derived from harness metadata here). Not an independent review.

**Verdict: adopted (re-export).** `src/lib.rs` now has `pub use concord::ResponseMeta;` in place
of its own struct and `from`; `checked` captures with `r.meta()`. Decisive: the SDK type derived
`Debug` over its `HeaderMap`, and `Display for Error` is `write!(f, "{self:?}")`, so both `Debug`
and `Display` of `Error::Provider` / `Error::Decode` (and `Debug` of `Response`, `EventStream`)
printed every response header value. Concord's type prints names only and is otherwise the
SDK's three fields plus `version`.

### Lines (non-blank, non-comment, from `git diff`; this file excluded)

| | removed | added |
|---|---|---|
| `src/lib.rs` | 18 | 4 |
| `tests/` | 13 | 11 |
| `README.md` | 0 | 0 |

### Caller-visible changes

- Fields become methods, on `Response::meta`, `EventStream::meta` and the `meta` of
  `Error::Provider` / `Error::Decode`:

  ```rust,ignore
  assert_eq!(meta.headers["retry-after"], "7");   // before
  assert_eq!(meta.headers()["retry-after"], "7"); // after
  ```

  Likewise `status()` and `operation()` (now an owned `Option<Operation>`; `Operation` is
  `Copy`). New: `version()`.
- The type is now `concord::ResponseMeta` (the same type as in the other adopting SDKs). It
  cannot be built by struct literal; `ResponseMeta::new(head, operation)` builds one. No test or
  README built one.
- `Debug` and therefore `Display` of errors (from the code): `ResponseMeta { status: 400,
  version: HTTP/1.1, header_names: [..], operation: Some("OpenAiApi.response") }`; before,
  `headers: {"x-request-id": "synthetic-request", ..}` and
  `Some(Operation(POST OpenAiApi.response))`.

### Test edits, by kind

1. Field → method, mechanical (9 lines in `tests/first_use.rs`, `tests/protocol.rs`):
   `.status` → `.status()`, `.headers` → `.headers()`, `.operation` → `.operation()`. Expected
   values unchanged.
2. Field → method that also drops a borrow (`tests/first_use.rs::assert_meta`, 4 lines → 1):
   `meta.operation.as_ref().expect(..)` borrowed the field; `meta.operation()` returns the
   `Option<Operation>` by value, so `.as_ref()` would borrow a temporary and is removed:
   `meta.operation().expect("declared operation identity")`. The three assertions on the
   operation are unchanged.
3. Added, one assertion (`tests/protocol.rs::response_and_typed_http_error`): `Debug` of the
   `Error::Provider` metadata does not contain the `x-request-id` value `synthetic-request`.

### Verification (observed)

`CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-openai`, run twice: `first_use` 18,
`protocol` 4, `split_and_reorder` 4, `workflows` 5 passed, 0 failed, 0 doctests (the same counts
as at `92775757`). `cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --check` and
`cargo doc --no-deps` (no warnings): clean. Offline only; no OpenAI API calls.

## Between waves 4-5 (reader wrapping)

Library refinement R5 (`concord-extras`: `impl ByteSource for Incoming`,
`ReadError::into_source_unredacted`, `SseReader::check_head`, `read_for`), on top of Concord
`27d23b39`. Role: implementer, a Claude agent session spawned by the coordinator (the session's
system prompt names model `claude-opus-5-5`; effort is not visible to the session; the
coordinator records requested vs observed). Not an independent review.

**Verdict: adopted; public behaviour kept.** The `Body` adapter (a `ByteSource` over
`Incoming::chunk` that stashed the `concord::Error` in a field, retrieved after the failure with
`into_source()`) is gone: `EventStream` reads `SseEvents<concord::Incoming>`, and a broken body's
error comes back by value from the reader's own error:

```rust,ignore
Some(Err(error)) => {
    self.events = None;
    let text = error.to_string();
    // Only a broken body carries a source: Concord's transport error, by value.
    return Err(match error.into_source_unredacted().map(|s| s.downcast()) {
        Some(Ok(failure)) => Error::Concord(*failure),
        _ => Error::Stream(text),
    });
}
```

The head check is the reader's `check_head` (the same function `<Sse as
Incremental<_>>::check_content_type` calls), so `Sse` and `Incremental` are no longer imported.
The stream keeps `read`, not `read_for`: naming the operation would prefix every `Error::Stream`
message with `OpenAiApi.response_stream: `, a public change this migration avoids.

### Lines (`src/lib.rs`, 540 → 526; `git diff -U0`, this file excluded)

| | added | removed | of which non-blank, non-comment: added / removed |
|---|---|---|---|
| `src/lib.rs` | 16 | 30 | 12 / 27 |
| `tests/` | 0 | 0 | 0 / 0 |

### Behaviour changes

None found (from the code; not probed beyond the tests). `Error::Concord` for a broken body is
the same `concord::Error` that `Incoming::chunk` returned (kind `Transport`, operation
`OpenAiApi.response_stream`, URL removed: `tests/workflows.rs::stream_failure_transport_failure_and_local_cancellation`);
`Error::Stream` texts are unchanged (the reader names no operation, as before); the head check and
its refusal (`Error::Stream("expected text/event-stream")`) are the same function and value; the
response is dropped at the same point (when the reader error is returned). The branch that turned
a `Transport` error without a stashed failure into `Error::Stream` is now the branch for a
`Transport` error whose source is not Concord's; neither happens with an `Incoming` source.

### Verification (observed)

`CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-openai`: 18 first_use + 4 protocol + 4
split_and_reorder + 5 workflows passed, 0 failed, 0 doctests (the same counts as at `27d23b39`);
`git diff` of `tests/` is empty. `cargo clippy --locked -p concord-sdk-openai --all-targets -- -D
warnings` and `cargo fmt -p concord-sdk-openai -- --check`: clean. Offline only; no OpenAI API
calls.
