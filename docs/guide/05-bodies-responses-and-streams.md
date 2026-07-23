# 5. Bodies, responses, and streams

Body and response declarations are generic Rust specification types:

```rust
POST create(#[body] input: Json<CreateInput>) -> Json<Item>;
GET export() -> Stream<OctetStream>;
```

`Bytes`, `Text`, `NoContent`, media primitives, request/response traits,
bounded acquisition, sealed response modes, records, and multipart native
construction belong to the managed runtime. `Json<T>`, `Csv<T>`, and
`NdJson<T>` are façade capabilities implemented downstream through those
semantic traits.

A body specification is inert until execution. Body factories run only after
public preparation, hooks, and limiter authority permit the attempt.
Multipart serialization remains inside the native boundary so replayability
and aggregate limits can be enforced.

Buffered responses are fully acquired under the configured response limit
before decoding. Streaming responses expose a media-independent `ByteStream`;
the first chunk that would exceed the cumulative limit is withheld. CSV and
NDJSON record decoders have an independent finite staging bound. Status
bodies, provider responses, and pagination accumulation use separate limits.
No extension receives a native transport response or can introduce a new
unbounded acquisition mode.

Use `.response().await` when response metadata is required. Use the ordinary
await terminal for the declared output. The bounded raw response terminal is
an explicit dangerous capability; see [Dangerous capabilities](../reference/dangerous.md).
Exact defaults are in [Limits](../reference/limits.md).

## Running proof

The
[body example](../../crates/concord-examples/src/request_response/bodies.rs),
[specification example](../../crates/concord-examples/src/request_response/specifications.rs),
and
[record example](../../crates/concord-examples/src/request_response/records.rs)
are importable modules. The
[local runtime test](../../crates/concord-examples/tests/local_runtime.rs)
proves bounded acquisition, while the
[construction test](../../crates/concord-examples/tests/construction.rs)
checks feature-gated public construction.

Next: [Pagination and rate limits](06-pagination-and-rate-limits.md).
