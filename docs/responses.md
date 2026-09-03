# Responses and terminals

The declared arrow type is exactly what await returns. The `using` type owns
accepted status semantics, default `Accept`, one sealed acquisition mode, and
conversion to that output.

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#response_acquisition_contract -->
```rust
api! {
    client ResponseApi {
        timeout off;
        base "http://responses.example.test/";


    GET message() -> String using Text {
        expose accepted_bytes;
    }
    GET optional_message() -> Option<String> using crate::request_response::acquisition::OptionalText;
}}
```

The macro proves `Spec: ResponseSpec<Output = DeclaredOutput>` at an
endpoint-local assertion site.

## Sealed acquisition

- `Buffered` retains complete accepted bytes under a configured bound before
  conversion.
- `Streaming` returns a lazy bounded `ByteStream`; later polling remains managed.
- `Discard` accepts metadata and intentionally does not poll the body.

Applications can select these modes through response specs but cannot implement
a fourth mode or access the native response. Core owns challenge precedence,
polling, decompression, bounds, deadline/cancellation, and classification.

## Universal and explicit terminals

Await returns the declared output. `.response().await` returns
`ApiResponse<Output>` with accepted metadata. `expose accepted_bytes;` adds an
endpoint-specific `.accepted_bytes().await -> ApiResponse<Bytes>`.

Accepted bytes preserves declared challenge behavior, accepted statuses,
negotiation, response-head checks, and byte bounds while skipping declared
conversion. Buffered mode uses acquired bytes. Discard mode may explicitly
switch to bounded buffering. Streaming cannot expose the terminal. It is not raw
native response access.

## Streaming truth

Receiving a stream value means the response head was accepted and a bounded
managed stream was opened. It does not mean the remote body was fully consumed.
Dropping the stream stops further local acquisition but proves nothing about
server work. No buffering terminal silently converts an unbounded stream into
memory retention.

Response heads supplied to response extensions include actual accepted headers
and may be sensitive; the extension owns later logging/storage. Ordinary
operational views remain sanitized and body-free.

See [request bodies](request_bodies.md), the [runtime response model](runtime.md#response-authority),
and [extension boundaries](extensions.md).
