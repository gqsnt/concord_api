# Extension boundaries

Concord exposes traits only where runtime behavior genuinely varies. Static DSL
operations stay generated concrete data.

| Extension | May decide | Cannot decide |
|---|---|---|
| `EncodeBody` | bounded complete encoding | route, auth, retry, native request |
| `DecodeBody` | typed conversion from accepted bounded bytes | acquisition mechanics |
| `RequestBodySpec` | media, preparation, replay/page stability | route, auth, rate, retry policy |
| `ResponseSpec` | acceptance, media, sealed mode, conversion | native polling/deadline |
| `RecordDecoder` | framing and record decode | network polling/global buffers |
| `CredentialProvider` | acquire/recover its material | placement target or endpoint mutation |
| `PaginationController` | next bound values/completion | arbitrary request mutation/order |
| `RateLimiter` | admit/wait/reject canonical policies | auth or endpoint semantics |
| `RetryClassifier` | retry recommendation | core safety gates |
| request/response gate | reject at one named phase | native/private mutation |
| completion observer | record a fixed event | alter the outcome |

Response acquisition modes are sealed and finite. Credential providers receive
isolated bounded provider HTTP. Rate limiters can see opaque canonical key values
under their documented trust boundary; those values do not leak to other
observers. No extension receives a universal application context.

## Response specification example

<!-- concord-source: crates/concord-examples/src/request_response/specifications.rs#response_body_spec_extension -->
```rust
pub struct ExampleBufferedSpec;

impl ResponseSpec for ExampleBufferedSpec {
    type Output = usize;
    type Mode = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<usize, ResponseBodyError> {
        Ok(body.len())
    }
}

pub struct ExampleStreamingSpec;

impl ResponseSpec for ExampleStreamingSpec {
    type Output = ByteStream;
    type Mode = Streaming;

    fn from_body(
        body: ByteStream,
        _head: &ResponseHead<'_>,
    ) -> Result<ByteStream, ResponseBodyError> {
        Ok(body)
    }
}

pub struct ExampleDiscardSpec;

impl ResponseSpec for ExampleDiscardSpec {
    type Output = ();
    type Mode = Discard;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}
```

Extension callbacks are trusted application work. Core bounds inputs it owns and
checks deadlines around synchronous result-determining callbacks, but cannot
preempt them or automatically bound arbitrary retained application state.
Implementations must bound CPU, memory, queues, map/metric cardinality, spawned
work, external I/O, and diagnostic output.

Optional first-party codecs, OAuth/provider support, CSV/NDJSON records,
multipart/streams, local limiting, and bounded export adapters belong in
`concord-extras`; the macro never recognizes their type names.

See the [design invariants](design_invariants.md) and
[runtime trust model](runtime.md#extension-trust).
