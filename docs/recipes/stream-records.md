# Collect or stream CSV and NDJSON records

## Goal

Choose complete all-or-error collection for ordinary CSV or NDJSON responses,
or incremental delivery when the caller should process records as they
arrive.

## Complete code

The compiled contract declares direct CSV and NDJSON response formats,
streaming wrappers, configured CSV, and a custom decoder in both modes:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:records_contract}}
```

Collect a direct CSV response as one vector:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:collect_buffered_records}}
```

Or consume the streaming form item by item:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/records.rs:consume_records}}
```

The deterministic runtime tests feed CSV and NDJSON through complete,
one-byte, and arbitrary chunk boundaries and verify the same decoded values.
Network chunks are never record boundaries.

## Choose a declaration

| Declaration | Awaited result | Lifecycle |
|---|---|---|
| `Csv<Row>` | `Vec<Row>` | buffered, complete, all-or-error |
| `NdJson<Event>` | `Vec<Event>` | buffered, complete, all-or-error |
| `BufferedRecords<MyDecoder>` | `Vec<MyDecoder::Record>` | buffered, complete, all-or-error |
| `Records<Csv<Row>>` | `RecordStream<Csv<Row>>` | lazy streaming |
| `Records<NdJson<Event>>` | `RecordStream<NdJson<Event>>` | lazy streaming |
| `Records<MyDecoder>` | `RecordStream<MyDecoder>` | lazy streaming |

Use direct `Csv<T>` or `NdJson<T>` when one accepted response comfortably fits
the configured aggregate buffered-response limit and the caller should receive
only a complete vector. These direct formats and `BufferedRecords<D>` also
implement `PageResponseSpec`, so they can be page outputs.

Use `Records<D>` when early processing or lower record-retention pressure
matters. Awaiting `csv_stream()` validates the accepted head, starts the
decoder, and returns the stream. Later polls yield one row or a
`RecordError`; record streams are not page outputs.

The default `Csv<T>` expects a header and comma delimiter. The configured type
alias uses `HasHeader` and `SemicolonDelimiter` while retaining the
`text/csv` media contract. `NdJson<T>` accepts the documented NDJSON media
types and frames one JSON value per logical line.

CSV and NDJSON are response formats only. These declarations do not encode
request bodies.

## Bounds and chunk behavior

Both delivery forms use one private record driver. Pending source bytes are
separate from decoder staging. A source chunk larger than the staging limit
can contain many small records and still succeed; a single logical record
that first exceeds staging fails before the first excess staging byte is
retained.

Buffered delivery first obeys the aggregate buffered-response byte limit and
then returns all decoded values or one `ApiError`. Streaming delivery obeys
the aggregate stream-response byte limit and yields later source/decode
failures as `RecordError` items. Neither byte limit bounds decoded object
graphs, consumer queues, or records retained by application code.

## Adaptations

Select `NoHeader` for positional headerless CSV and prefer tuple or tuple
struct targets. Choose `CommaDelimiter`, `SemicolonDelimiter`, or
`TabDelimiter`; the tab policy still validates `text/csv`, not
`text/tab-separated-values`.

Target-type deserialization decides CSV width acceptance. Structs and tuples
normally reject incompatible rows, while `Vec<String>` may accept varying
widths.

Use `.batches(NonZeroUsize)` on a record stream when downstream work benefits
from bounded nonempty vectors. The adapter retains at most that many records
and discards a partial batch if the underlying stream fails.

Implement `RecordDecoder` only for another finite framing format. It must
choose an independent positive staging limit, handle EOF explicitly, and emit
category-safe errors without exposing record content.

## Production notes

Buffered decoding remains inside the endpoint deadline. Streaming opening is
covered by that deadline, but later polling is caller-owned; wrap consumption
in an application timeout or cancellation policy when the whole transfer
needs a deadline.

Do not treat an early streaming prefix as complete success unless the
application explicitly owns a partial-result protocol. Dropping the stream
cancels the underlying response body and emits no second completion event.

Never include raw records, cursors, or arbitrary field values in unbounded
logs or metric labels. Test media types, CRLF/LF, quoted newlines, EOF without
a final newline, one-byte chunks, oversized records, and malformed inputs
locally.

## Related

- [Record delivery](../features/records.md)
- [Streaming responses](../features/streaming.md)
- [Pagination](../features/pagination.md)
- [Limits reference](../reference/limits.md)
- [Finite behavior](../concepts/finite-behavior.md)
- [Testing API clients](testing-api-clients.md)
