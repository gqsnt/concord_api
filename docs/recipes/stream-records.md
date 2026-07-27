# Stream CSV or NDJSON records

## Goal

Open a large CSV or NDJSON response, decode typed records incrementally, and
consume them without buffering the complete response or creating an unbounded
downstream queue.

## Complete code

The compiled contract declares default CSV, configured semicolon CSV, NDJSON,
and a custom decoder:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:records_contract}}
```

This consumer opens the CSV stream and handles every item:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:consume_records}}
```

The local runtime test feeds both CSV and NDJSON through one-byte and arbitrary
chunk boundaries and verifies the same decoded values. Network chunks are
never assumed to be record boundaries.

## Why these declarations

`Records<D>` selects a streaming response specification. Awaiting
`csv_rows()` validates the accepted response head, creates the decoder, and
returns `RecordStream<Csv<(String, u32)>>`. Later polls yield either one typed
row or a `RecordError`.

The default `Csv<T>` expects a header and comma delimiter. The configured type
alias uses `HasHeader` and `SemicolonDelimiter` while retaining the
`text/csv` media contract. `NdJson<u32>` accepts the documented NDJSON media
types and frames one JSON value per logical line.

The consumer uses `poll_fn` and the standard `Stream` contract directly, so it
needs only `futures-core`. It returns a vector for demonstration. A genuinely
large production flow should process or forward each row as it arrives rather
than retain them all.

Record staging is bounded separately from the underlying cumulative byte
stream. A decoder can consume across arbitrary transport chunks, but every
returned record must make strict staged-byte progress.

## Adaptations

Select `NoHeader` for positional headerless CSV and prefer tuple or tuple
struct targets. Choose `CommaDelimiter`, `SemicolonDelimiter`, or
`TabDelimiter`; the tab policy still validates `text/csv`, not
`text/tab-separated-values`.

Target-type deserialization decides CSV width acceptance. Structs and tuples
normally reject incompatible rows, while `Vec<String>` may accept varying
widths.

Use `.batches(NonZeroUsize)` when downstream work benefits from bounded
nonempty vectors. The adapter retains at most that many records and discards a
partial batch if the underlying stream fails.

Implement `RecordDecoder` only for another finite framing format. It must
choose an independent positive staging limit, handle EOF explicitly, and emit
category-safe errors without exposing record content.

## Production notes

Opening and later polling have different liveness boundaries. The endpoint
timeout governs opening; wrap stream consumption in an application timeout or
cancellation policy when the complete transfer needs a deadline.

The byte-stream limit and decoder staging limit do not bound memory in decoded
application values or downstream queues. Bound batch size, concurrency,
retained records, serializer work, and metric cardinality separately.

After opening, body and decode failures arrive as stream items. Do not treat
an early prefix as complete success unless the application explicitly owns a
partial-result protocol. Dropping the stream cancels the underlying response
body and emits no second completion event.

Never include raw records, cursors, or arbitrary field values in unbounded
logs or metric labels. Test media types, CRLF/LF, quoted newlines, EOF without
a final newline, one-byte chunks, oversized records, and malformed inputs
locally.

## Related

- [Record streams](../features/records.md)
- [Streaming responses](../features/streaming.md)
- [Limits reference](../reference/limits.md)
- [Finite behavior](../concepts/finite-behavior.md)
- [Testing API clients](testing-api-clients.md)
