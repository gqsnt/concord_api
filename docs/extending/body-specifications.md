# Request and response body specifications

Implement a full body specification only when a buffered codec cannot express
the lifecycle you need. Request specifications own structural metadata and
managed preparation. Response specifications select one of Concord's bounded
acquisition modes and convert its semantic result.

The exact Rustdoc contracts are `concord::body::RequestBodySpec` and
`concord::response::ResponseBodySpec`.

## Complete example

The request side declares exact buffered metadata and produces a public
`RequestBody`:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/specifications.rs:request_body_spec_extension}}
```

The response side demonstrates every selectable mode:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/specifications.rs:response_body_spec_extension}}
```

These specifications are used by both generated and runtime-defined endpoints:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/specifications.rs:body_spec_usage}}
```

All three regions belong to one compiled `concord-examples` module.

## Request metadata comes first

`RequestBodySpec::metadata` receives `&Input` and a
`RequestBodyContext`. It runs before `RuntimeHooks::before_send`, so it must be
structural. It may report:

- `LengthHint::Unknown`, `AtLeast`, or `Exact`;
- `RequestContentType::None`, `Exact`, or `Generated`;
- the managed `BodyKind`.

The context contains the endpoint ID, method, and positive aggregate request
limit. It intentionally excludes the URL, headers, credentials, limiter,
transport, and native request.

Do not serialize the input, open a file, poll a stream, create a factory
result, or perform external I/O in `metadata`. A hook must be able to reject
the attempt before those effects occur. If known structural length already
exceeds the configured limit, return a category-only `BodyError`.

## Preparation returns one managed body

After hook approval, Concord calls `RequestBodySpec::prepare` synchronously at
most once for that pending execution. It consumes the stored input and returns
`concord::body::RequestBody`, which can represent:

- reusable empty or complete bytes;
- a one-shot bounded stream;
- a factory that creates a fresh stream for an attempt;
- a multipart body or multipart factory when enabled.

The extension never constructs a Reqwest body. Concord validates the prepared
body's kind, content-type ownership, and length against the earlier metadata,
then converges every variant on its managed transport path. It derives
replayability from the returned body; an extension does not assert
replayability separately.

Use a factory when a later managed attempt must reconstruct the source. A
direct stream is one-shot. Factory callbacks and stream producers are trusted
application work and must remain finite. Concord suppresses successful empty
chunks, bounds them independently of nonempty bytes, and enforces the
request-wide byte limit, but it does not bound state or tasks created by the
producer.

For a conventional value-to-complete-bytes conversion, implement
`concord::codec::EncodeBody` instead. Its blanket specification already handles
this sequence.

## Response modes are sealed

`ResponseBodySpec::Mode` must be one of:

| Mode | Input to `from_body` | Completion meaning |
|---|---|---|
| `concord::response::Buffered` | complete bounded `Bytes` | accepted conversion completes the request |
| `concord::response::Streaming` | lazy bounded `ByteStream` | opening the stream completes endpoint execution |
| `concord::response::Discard` | `()` | the accepted body is not polled |

`ResponseMode` is sealed. Applications can select a mode but cannot implement
a fourth mode, replace core acquisition, or obtain the native response.

After limiter and response-hook observation and authoritative authentication
challenge handling, Concord calls `ResponseBodySpec::accepts_status`. Its
default accepts 2xx. A specification can explicitly accept another operation
outcome such as `404`; accepted non-2xx responses use the declared acquisition
mode and limits. Rejected statuses remain on the bounded terminal status-error
path.

For an accepted response, known-length preflight runs under the selected mode.
`ResponseBodySpec::validate` then sees a borrowed `ResponseHead` before the
first body poll. After core acquires the selected semantic body, `from_body`
converts it to `Output`.

The head includes actual accepted response headers and a
credential-sanitized logical URL. Those values may still contain tokens issued
by the service, cookies, account IDs, or personal data; the specification owns
any logging and storage it performs.

`validate` and `from_body` are synchronous and cannot be preempted. A
`Streaming` specification may return the bounded stream immediately, but later
consumption time and downstream buffering belong to the caller.

## Errors and pagination capability

Return `BodyError` and `ResponseBodyError` with stable structural kinds. Do not
embed source errors, bodies, filenames, headers, native URLs, or field values.
The public item paths are `concord::body::BodyError`,
`concord::body::BodyErrorKind`, `concord::response::ResponseBodyError`, and
`concord::response::ResponseBodyErrorKind`.

Pagination has independent request and response capability markers.

On the request side, implement `concord::body::PageRequestBodySpec` only when
cloning the input creates independently preparable values that always produce
the same page-stable body. `Input: Clone` is required but is not proof of that
semantic promise: shared one-shot state must not opt in. Core implements the
marker narrowly for empty, `Bytes`, and `Text`; extras implements it for
`Json<T>` when `T: Clone + Serialize + Send + 'static`. There is no blanket
implementation for arbitrary `EncodeBody` codecs.

A custom factory-backed specification may opt in after proving that every
input clone independently creates the same replayable source. Concord prepares
the specification once per page after hook approval. Policy retries within
that page reconstruct a complete attempt body from the prepared bytes or
factory after each retry hook. They do not call the specification's
encoder/preparation again.

On the response side, a downstream buffered specification does not
automatically become page-compatible. Implement
`concord::response::PageResponseSpec` only when its output has the intended
page-item semantics. Buffered `DecodeBody` outputs that implement
`IntoIterator` receive that capability automatically. Streaming, discard,
byte-stream, and record-stream responses are not paginated by adding a marker.

## Verification

Exercise metadata and preparation as a pair. Verify exact and incorrect length
hints, content-type ownership, hook veto before preparation, one-shot versus
factory behavior, cancellation, empty chunks, and exact body limits. A
`PageRequestBodySpec` additionally needs multi-page stability, independent
per-page size enforcement, one preparation per page, and retry reuse of the
prepared page body.

For response specifications, test status acceptance and rejection, validation
before polling, each selected mode, known and unknown lengths, one-byte chunks,
empty chunk runs, conversion failure, and completion outcome. Include accepted
non-2xx behavior and authentication-challenge precedence. Run the same
specification through a generated endpoint and `RuntimeEndpoint`; both should
cross the same public contract.

## Related

- [Codecs](codecs.md)
- [Streaming responses](../features/streaming.md)
- [Multipart uploads](../features/multipart.md)
- [Hooks, trusted work, and bounds](hooks-and-trusted-work.md)
- [Limits reference](../reference/limits.md)
- Rustdoc: `concord::body::RequestBodySpec`,
  `concord::body::PageRequestBodySpec`, and
  `concord::response::ResponseBodySpec`
