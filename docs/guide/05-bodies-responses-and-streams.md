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
and aggregate limits can be enforced. Every direct, factory-produced, or
multipart-backed request stream crosses one final managed boundary. Empty
request chunks are suppressed before native transport and bounded by
`concord::body::MAX_EMPTY_REQUEST_CHUNKS`; they consume neither the nonempty
byte limit nor an exact declared length. The first empty-chunk excess wins
before any later EOF exact-length error.

Buffered responses are fully acquired under the configured response limit
before decoding. Streaming responses expose a media-independent `ByteStream`.
Every core-owned response acquisition path—buffered success, streaming
success, terminal status capture, and provider HTTP—suppresses empty
transport chunks and accepts at most
`concord::response::MAX_EMPTY_RESPONSE_CHUNKS` for one response. Empty chunks
are never application items, their total is not reset by nonempty data, and
bounded per-poll or per-turn work yields cooperatively to the executor.
Nonempty bytes retain the applicable cumulative byte limit, with the first
chunk that would exceed it withheld. CSV and NDJSON record decoders have an
independent finite staging bound. Status bodies, provider responses, and
pagination accumulation retain their separate byte/item limits. No extension
receives a native transport response or can introduce a new unbounded
acquisition mode.

The endpoint deadline governs opening a returned `ByteStream`, not later
caller-driven polling. Stream polling remains cumulatively byte-bounded but
can wait independently of that endpoint deadline. When liveness needs a
polling or whole-download deadline, wrap stream consumption in the timeout or
cancellation primitive owned by the calling runtime.

Wire and staging limits bound Concord-owned bytes, not the allocation size of
the decoded application value. Custom decoders and response specifications
must bound their own CPU, memory, state, and side effects. Custom request
streams may return `Pending`, perform trusted application work, and produce
safe upstream errors, but core does not permit empty chunks to serve as an
unbounded progress mechanism.

`RecordDecoder` receives a mutable staging buffer, but core can enforce only
the net length observed across a callback. The resulting length must not
increase. Returning a record requires a strict net decrease; returning no
record may still consume bytes, while an unchanged length makes core poll for
more body data or finish EOF handling. Decoder mutations must stay within
those net-progress constraints.

## Configuring CSV records

`Csv<T>` remains header-bearing, comma-delimited CSV. The record type stays
first, while trailing type policies select header handling and the delimiter:

```rust
use concord::records::{
    Csv,
    HasHeader,
    Records,
    SemicolonDelimiter,
};

type ConfCsv<T> =
    Csv<T, HasHeader, SemicolonDelimiter>;

concord::api! {
    client RowsApi {
        base "https://records.example.test/";
    }

    GET rows() -> Records<ConfCsv<Row>>;
}
```

Use `NoHeader` when every non-empty record is application data. Headerless
decoding is positional, so tuples and tuple structs are recommended:

```rust
use concord::records::{
    Csv,
    NoHeader,
    TabDelimiter,
};

type HeaderlessTsv<T> =
    Csv<T, NoHeader, TabDelimiter>;
```

The built-in delimiters are `CommaDelimiter`, `SemicolonDelimiter`, and
`TabDelimiter`. All combinations retain the same safety and media contract:
UTF-8, double-quote quoting and doubled-quote escaping, LF and CRLF, quoted
embedded newlines, ignored empty physical lines, an optional final newline,
redacted errors, and a one-MiB staged-record limit. They all accept and validate
`text/csv`, including valid parameters. `TabDelimiter` changes parsing only; it
does not select `text/tab-separated-values`.

CSV width acceptance is target-type-driven. Structs, tuples, and tuple structs
usually reject incompatible widths through deserialization, while
sequence-like targets such as `Vec<String>` may accept varying widths. Concord
does not impose a separate global cross-record width check.

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
