# Streaming responses

Use a streaming response when the caller should process bytes incrementally
instead of waiting for one buffered value. Concord bounds the cumulative wire
data and non-progress behavior, while the caller owns how long and how much
application state its consumption retains.

## Declare streaming media

`Stream<OctetStream>` is a media-aware request or response specification:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/bodies.rs:streaming_endpoints}}
```

For uploads, the generated method accepts `StreamBody`. It can wrap owned
bytes, an async reader, an application byte stream, or an opened file:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/bodies.rs:streaming_requests}}
```

For downloads, awaiting the endpoint opens and validates the successful
response and returns `ByteStream`. Consume it as a Rust `Stream`:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/bodies.rs:consume_byte_stream}}
```

The example counts bytes without joining them into a second complete buffer.
Real consumers can write each chunk to a file, feed an incremental parser, or
forward it to another bounded application component.

## Opening and polling are different phases

The endpoint operation deadline governs public preparation, authentication,
limiter and hook work, the HTTP exchange, the final response head, successful
status classification, media validation, and opening the stream. It ends when
the `ByteStream` is returned.

Later caller-driven polling is not governed by that endpoint deadline. A slow
server or stalled consumer can therefore wait after the operation has opened.
When liveness needs a per-poll or whole-download deadline, wrap stream
consumption in the timeout or cancellation primitive owned by the async
runtime:

```rust
let result = tokio::time::timeout(
    std::time::Duration::from_secs(30),
    download_size(&client),
).await;
```

Choose that timeout according to the application workflow; Concord cannot
infer whether a long-lived stream is healthy.

## Bounds and progress

Every yielded nonempty chunk has passed the configured cumulative stream byte
limit, measured after supported transport decompression. The first chunk that
would exceed the limit is withheld and the stream yields a safe error.
Collecting those chunks into application memory creates a second allocation
that Concord does not own or bound.

Transport streams sometimes produce empty chunks. Concord treats them as
framing artifacts: they never become `ByteStream` items and do not consume the
nonempty byte limit. A response-wide empty-chunk ceiling and bounded work per
poll prevent them from becoming an unlimited progress signal; ready-empty
runs yield cooperatively.

An error discovered after opening is yielded as a stream item. Completion for
the endpoint opening has already occurred, so later stream failure does not
emit a second completion event.

## Cancellation and replay

Dropping `ByteStream` cancels and drops its owned native response body. Unknown
chunks remain unpolled. This is the normal way to stop a download early.

Request streams cross a similar final managed boundary. Empty request chunks
are suppressed and bounded before native transport, and nonempty bytes remain
under the request-body limit. A one-shot stream may not be replayable for an
authentication recovery or explicitly enabled transport retry; Concord fails
according to the body contract rather than silently recreating application
input.

`ByteStream` is deliberately media-independent and does not expose a native
response. Use `.response()` when the endpoint supports metadata and you need
an `ApiResponse<ByteStream>`.

## Related

- [Request bodies and buffered responses](bodies-and-buffered-responses.md)
- [Record streams](records.md)
- [The managed lifecycle](../concepts/managed-lifecycle.md)
- [Finite behavior](../concepts/finite-behavior.md)
- [Limits reference](../reference/limits.md)
