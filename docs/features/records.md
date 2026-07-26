# Record streams

Decode CSV, NDJSON, or another framed format incrementally when the response
contains many typed records. Concord layers record staging and progress checks
on the bounded byte stream without buffering the complete response.

## Declare the decoder

Enable `records` plus the format feature you need. `csv` and `ndjson` both
enable the shared record-stream surface:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:records_contract}}
```

`Records<D>` is the response specification. `D` selects media validation,
framing, and the record type:

- `Csv<(String, u32)>` returns a stream of typed tuple rows;
- `NdJson<u32>` returns one decoded JSON value per logical line;
- an application `RecordDecoder` can support another finite framing format.

Awaiting the endpoint opens a `RecordStream<D>`. Poll that stream for
individual decoded values:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:consume_records}}
```

The opening can fail with `ApiError`; later body and framing failures are
`RecordError` items from the stream. Dropping the stream cancels the underlying
response body.

## Configure CSV

The default `Csv<T>` has a header row and comma delimiter. Trailing type
policies select other supported behavior:

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
final record without a trailing newline. One JSON value is one stream item;
the complete response is never first collected into a JSON array.

## Batch at the consumer boundary

One-record streaming is the primitive. Group successful records into
nonempty bounded batches when downstream work is more efficient in chunks:

```rust
let stream = client.csv_rows().await?;
let batches = stream.batches(
    std::num::NonZeroUsize::new(100).expect("positive batch size"),
);
```

The adapter holds at most the requested number of records. If the underlying
stream fails, it discards a partial batch and yields the error instead of
presenting incomplete success.

## Progress and limits

Record staging has an independent positive limit in addition to the
cumulative `ByteStream` limit. Network chunks are not record boundaries.
Concord repeatedly lets the decoder consume complete records before polling
for more bytes.

At a user level, every returned record must consume staged input, and a
decoder that needs more data must not grow the staging buffer. Concord rejects
violations as an internal record error. An oversized or incomplete record
fails the stream without yielding a partial value.

The built-in decoders bound staged bytes, but a decoded application value can
allocate more memory than its wire representation. Consumers and custom
decoders remain responsible for bounded CPU, retained values, and downstream
queues.

## Related

- [Streaming responses](streaming.md)
- [Pagination](../guide/06-pagination-and-rate-limits.md)
- [Feature flags](../reference/features.md)
- [Limits reference](../reference/limits.md)
- [Extension model](../concepts/extension-model.md)
