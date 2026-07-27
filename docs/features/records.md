# Record delivery

Decode CSV, NDJSON, or another framed format either as one complete vector or
as a lazy record stream. Both choices use the same `RecordDecoder` and the
same bounded framing driver; choose between them for lifecycle semantics, not
for parser behavior.

## Choose buffered or streaming delivery

Enable `records` plus the format feature you need. `csv` and `ndjson` both
enable the shared record surface.

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:records_contract}}
```

The response declaration selects the output and lifecycle:

| Declaration | Output | Lifecycle |
|---|---|---|
| `BufferedRecords<D>` | `Vec<D::Record>` | complete, buffered, all-or-error |
| `Records<D>` | `RecordStream<D>` | lazy, streaming |
| `Csv<T, H, D>` | `Vec<T>` | complete, buffered, all-or-error |
| `Records<Csv<T, H, D>>` | `RecordStream<Csv<T, H, D>>` | lazy, streaming |
| `NdJson<T>` | `Vec<T>` | complete, buffered, all-or-error |
| `Records<NdJson<T>>` | `RecordStream<NdJson<T>>` | lazy, streaming |

Use `BufferedRecords<MyDecoder>` for an application-defined decoder when the
caller needs a complete vector. Direct `Csv<T>` and `NdJson<T>` are the
first-party shorthand for that complete form:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:collect_buffered_records}}
```

The complete operation remains active through bounded body acquisition, EOF,
and decoding. It returns every record or one `ApiError`; no partial vector is
delivered. It supports `.response()`, `.response_bytes()`, and generated
pagination when the endpoint declares pagination. The response-bytes terminal
preserves status acceptance and `Accept` but deliberately skips record media
validation and decoding.

Use `Records<D>` when the caller should process values as they arrive.
Awaiting the endpoint validates the head, starts one decoder, completes with
`StreamOpened`, and returns a `RecordStream<D>`:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:consume_records}}
```

Later body, framing, or decode failures are `RecordError` stream items. A
successfully yielded prefix may already be application-owned, and dropping the
stream cancels the unread response. Record streams do not support
`.response_bytes()` or pagination.

## Configure CSV

The default `Csv<T>` has a header row and comma delimiter. Trailing type
policies select other supported behavior in both complete and streaming forms:

```rust
use concord::records::{Csv, HasHeader, SemicolonDelimiter};

type SemicolonRows<T> = Csv<T, HasHeader, SemicolonDelimiter>;
```

Use `NoHeader` when every nonempty record is application data. Headerless
decoding is positional, so tuples and tuple structs are usually clearer than
named structs. Built-in delimiters are comma, semicolon, and tab; every
configuration still validates the `text/csv` media type. A tab delimiter does
not change it to `text/tab-separated-values`.

CSV width acceptance comes from the target type. Structs, tuples, and tuple
structs normally reject incompatible widths through deserialization.
Sequence-like targets such as `Vec<String>` may accept varying widths. Concord
does not add a separate cross-record width rule that overrides the decoder.

The decoder accepts UTF-8, CSV quoting and doubled-quote escapes, LF or CRLF,
quoted embedded newlines, ignored empty physical lines, and an optional final
newline. Exact policy and staging ceilings remain in Rustdoc and the
[limits reference](../reference/limits.md).

## Configure NDJSON

`NdJson<T>` uses Serde JSON for one value per logical line. It accepts the
documented NDJSON media types, tolerates LF and CRLF framing, and can decode a
final record without a trailing newline. Direct buffered delivery collects
those framed values into a vector; it does not switch to a separate
whole-document JSON-array parser.

CSV and NDJSON currently decode responses only. They do not implement request
encoding.

## Batch a stream at the consumer boundary

Group successful streamed records into nonempty bounded batches when
downstream work is more efficient in chunks:

```rust
let stream = client.csv_stream().await?;
let batches = stream.batches(
    std::num::NonZeroUsize::new(100).expect("positive batch size"),
);
```

The adapter holds at most the requested number of records. If the underlying
stream fails, it discards a partial batch and yields the error instead of
presenting incomplete success.

## Progress and limits

Decoder staging and pending source bytes are separate. Network chunks are
never record boundaries: a large transport chunk containing many small
records succeeds as long as each undecoded logical record fits the decoder's
positive staging limit. The driver feeds only bounded slices into staging and
decodes between slices.

A single logical record that first exceeds the staging limit fails before the
first excess staging bytes are retained. Returning a record without consuming
staged bytes or growing the staging buffer is an internal contract failure.
The same private driver enforces these rules for buffered collection and
streaming.

Complete delivery is bounded first by the aggregate buffered response-body
limit and then by decoder staging and progress. Streaming uses the cumulative
byte-stream limit plus the same staging contract. These wire and staging
limits do not bound heap allocation inside decoded values; consumers and
custom decoders remain responsible for bounded CPU, decoded expansion,
retained values, and downstream queues.

## Related

- [Streaming responses](streaming.md)
- [Pagination](pagination.md)
- [Feature flags](../reference/features.md)
- [Limits reference](../reference/limits.md)
- [Record decoders](../extending/record-decoders.md)
