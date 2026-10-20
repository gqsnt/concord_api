# Streams

A streaming endpoint's ordinary `.await` returns after the accepted response
head is validated and the stream is opened. **Acquisition is not EOF**
(STREAM-01): the body is consumed afterwards, under its own finite lease, and
its completion or failure is reported by the stream itself.

## Declaring streams

<!-- concord-pseudocode -->
```rust
GET download() -> ByteStream via Binary;          // managed bytes
GET watch() -> ItemStream<Event> via NdJson;      // managed records
GET rows() -> ItemStream<(String, u32)> via Csv;  // managed CSV records
GET events() -> ItemStream<ServerEvent> via EventStream; // server-sent events
```

`ByteStream` and `ItemStream<T>` (feature `records`) are managed, owned,
non-`Clone` values. The same record representations also produce complete
bounded collections (`-> Vec<T> via Csv`), acquired and decoded before
`.await` returns:

<!-- concord-source: crates/concord-examples/src/request_response/records.rs#records_contract -->
<!-- concord-dsl -->
```rust
api! {
    client RecordsApi {
        base "http://records.example.test/";
        timeout 30s;

        GET custom() -> ItemStream<String> via PipeDelimited;
        GET buffered_custom() -> Vec<String> via PipeDelimited;
        GET csv_rows() -> Vec<(String, u32)> via Csv;
        GET csv_stream() -> ItemStream<(String, u32)> via Csv;
        GET configured_csv_rows() -> Vec<(String, u32)> via ConfCsv;
        GET configured_csv_stream() -> ItemStream<(String, u32)> via ConfCsv;
        GET nd_json_values() -> Vec<u32> via NdJson;
        GET nd_json_stream() -> ItemStream<u32> via NdJson;
    }
}
```

## Server-sent events

The opt-in `sse` feature implies `records`. It exports `EventStream`,
`ServerEvent` and `EventSource` through `concord::records` and
`concord::prelude`. Declare an SSE response as
`ItemStream<ServerEvent> via EventStream`; the representation validates
`text/event-stream` and frames events within the managed record stream's
finite byte, record, frame, idle and lifetime limits.

`ServerEvent` carries the event's protocol fields. SSE comments and control
fields are handled by the decoder; consumers receive complete events through
`next()`. Opening an SSE response is acquisition, and its summary reports
consumption just like other record streams.

`EventSource` provides explicit application-controlled reconnection around
managed SSE operations. Each connection remains a separate bounded operation;
selecting `EventStream` alone does not reconnect. Reconnection and continuation
are distinct from the runtime's request retry policy.

<!-- concord-pseudocode -->
```rust
use concord::records::{EventSource, EventStream, ServerEvent};

// In api!: GET resume(header("Last-Event-ID") last_event_id?: String)
//              -> ItemStream<ServerEvent> via EventStream;
let mut source = EventSource::new(move |last_event_id| {
    let feed = feed.clone();
    async move { feed.resume().set_last_event_id_if_some(last_event_id).await }
}).with_max_reconnects(5);
while let Some(event) = source.next().await {
    let event = event?;
    consume(event.event(), event.data(), event.last_event_id());
}
```

EOF and transport failures reconnect after three seconds by default, or the
latest valid `retry` value in milliseconds. IDs and retry values persist across
connections, including updates in blocks without data; an empty `id` clears
the resume ID. `with_last_event_id` sets the initial resume ID and
`with_reconnect_time` changes the initial delay. `with_max_reconnects(n)` bounds
consecutive reconnections without an event; delivering an event resets that
count. A 204 response ends the source. Status, media-type, limit and other
non-transport failures are yielded once and end it. Dropping a pending source
`next()` cancels its wait and open connection.

SSE uses UTF-8 replacement decoding, ignores one leading BOM, and recognizes
CRLF, LF and CR line endings. Multiple `data` lines join with LF; no data field
means no dispatched event, and EOF discards a block without a terminating blank
line. Each line (without its terminator) and each event's data is limited to
1 MiB. The runtime frame limit also bounds staged line and event data together.
Comments and ignored fields are consumed without accumulating between blocks.

## Consuming a stream

Decoder callbacks are synchronous. Core checks lease, idle and cancellation
again after each callback before yielding a record or completing EOF, including
callbacks that consume bytes and return `None`. A callback may finish after a
limit; its record is then withheld and staged/native state is released.

NDJSON's built-in limit is a 1,048,576-byte JSON payload, excluding LF or CRLF.
Its driver stages at most that payload plus two delimiter bytes. The runtime
frame cap counts the entire staged frame, including delimiters, and defaults
to 1,048,578 bytes; a smaller configured cap remains a hard ceiling. Buffered
and streaming NDJSON share the driver and payload check. CSV keeps its own
unchanged framing bound. Aggregate body and record limits remain independent.

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#consume_byte_stream -->
```rust
pub async fn download_size(client: &BodyApi) -> Result<u64, concord::ApiError> {
    let mut stream = client.download().await?;
    let mut received = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        received = received.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
    }
    Ok(received)
}
```

<!-- concord-source: crates/concord-examples/src/request_response/records.rs#consume_records -->
```rust
pub async fn read_csv_rows(
    client: &RecordsApi,
) -> Result<Vec<(String, u32)>, Box<dyn std::error::Error>> {
    let mut stream = client.csv_stream().await?;
    let mut rows = Vec::new();
    while let Some(row) = stream.next().await {
        rows.push(row?);
    }
    Ok(rows)
}
```

| Method | Meaning |
|---|---|
| `next()` | `Option<Result<Bytes, ApiError>>` (or `Option<Result<T, ApiError>>` for `ItemStream<T>`) |
| `summary()` | a `StreamSummary` snapshot of the current journal |
| `finish()` | ends consumption and returns the final `StreamSummary`; it does not drain |

Both types also implement `futures_core::Stream`. The acquisition metadata
(`.response()`'s `ApiResponse::metadata()`) is a snapshot as of opening; later
consumption is reported by the summary.

## Summaries and terminal states

`StreamSummary` is owned and cloneable, with `state()`, `operation_id()`,
`attempt()`, `status()`, `acquired_bytes()`, `delivered_records()`,
`body_disposition()` and `dispatch_knowledge()`. `StreamState` is one of:

| `StreamState` | Meaning |
|---|---|
| `Active` | consumption is still possible |
| `Eof` | the body ended and every record was delivered |
| `Application` | the application stopped early with `finish()` |
| `Cancelled` | the stream was dropped before a terminal state, or a cancellation token fired |
| `Failed` | a read, framing, limit or time failure terminated the stream |

After a terminal failure, `next()` yields exactly one error and then `None`;
the failure stays in the summary. Every later error carries `ResponseObserved`
and the original request correlation. `finish()` after actual EOF preserves
`Eof`; before EOF it records `Application`. Dropping the stream before a
terminal state records cancellation, never EOF. A `ByteStream` reports zero
delivered records; an `ItemStream` counts complete yielded records.

## The finite lease

After the response head, a stream runs under a lease (STREAM-02):

| Limit | Default | Runtime setter |
|---|---|---|
| Total elapsed lease | 5 minutes | `stream_lifetime(Duration)` |
| Cumulative idle time during pending reads | 30 seconds | `stream_idle_timeout(Duration)` |
| Decoded nonempty body bytes | 16 MiB | `stream_response_limit(usize)` |
| Records | 100,000 | `stream_max_records(usize)` |
| Bytes per framed record, including delimiters | 1 MiB + 2 bytes | `stream_frame_limit(usize)` |

All are positive and finite; trusted runtime configuration can change them and
call options can narrow them (`CallOptions::cap_stream_lifetime`,
`cap_stream_idle`, `cap_response_bytes`). The total lease includes the
consumer's pauses between polls. Idle time counts only while an underlying read
is pending, not while the application processes a delivered chunk; nonempty data
resets it, empty chunks do not. The total lease bounds the stream's lifetime: a
read observed to complete at or after its end yields one `LeaseExpired` error
instead of its data or a clean end of body, and when the idle budget has run out
too, `LeaseExpired` is the error reported. Empty chunks acquire no data, so the
acquisition stays pending across the cooperative yield after 32 empty chunks:
the time until the next poll counts as idle. A whole-operation `cap_timeout`
starts at the first request poll and also bounds consumption; without it the
selected request timeout ends at successful acquisition and the lease applies.
There is no infinite mode: a long-lived feed configures an explicitly larger
finite lease.

## Cancel-safe reads

Dropping a pending `next()` future does **not** end the stream (STREAM-03). The
poll state, decoder buffer, byte accounting and idle baseline live in the
stream handle, so the next call resumes the same bounded acquisition without
resending the request, and repeated abandonment cannot reset the idle timeout.
The idle time of an abandoned read (including a call dropped at a cooperative
empty-chunk yield) keeps counting until the next call: if it reaches the idle
limit before that call, the call yields one `IdleTimeout` error even when data,
empty chunks or the end of the body became available in the meantime. Such late data is not delivered, a late end of body does not
complete a staged final record or report a clean end, and the summary records
`Failed`. Resuming before the limit continues normally.
This intentionally differs from page fetches, where dropping a started fetch
cancels the traversal. An external cancellation token cancels the whole stream.

Zero-length transport chunks are suppressed: at most 1,024 consecutive empty
chunks are accepted, and the stream yields to the scheduler after at most 32 in
one poll. Byte limits apply after decompression, and record buffers are checked
before they grow. A truncated or invalid record, or a record/byte/time limit,
terminates the stream with a correlated error; an NDJSON final nonempty line
without LF is parsed once, while invalid CSV quoting fails.

No policy, auth or transport retry happens after a stream is handed to the
caller. A reconnecting application creates a new operation with a new identity
and manages its own continuation; a partial stream never becomes a complete
`Vec` success because the transport ended.

## A bounded prefix is not EOF

The canonical Catalog's `export` streams bytes. Reading a prefix and stopping
records an application stop, and the acquired byte count can exceed the
retained prefix by the remainder of the last chunk:

<!-- concord-source: crates/concord-examples/src/application/export.rs#export_prefix -->
```rust
use concord::response::StreamSummary;
use concord::{ApiError, CallOptions};

use super::execute::execute;
use crate::contracts::catalog_v3::catalog_api;

/// The retained export prefix.
#[derive(Debug)]
pub enum ExportPrefix {
    /// Nothing was requested; no request was constructed or executed.
    NotStarted,
    /// The retained bytes and the stream summary at the stop.
    Read {
        /// Exactly the retained prefix (at most `wanted_bytes`).
        bytes: Vec<u8>,
        /// The stream summary; its acquired bytes can exceed the prefix.
        stream: StreamSummary,
    },
}

/// Reads at most `wanted_bytes` of the export stream.
///
/// # Errors
///
/// Acquisition failure or a correlated stream consumption failure.
pub async fn export_prefix(
    scope: &catalog_api::regional::Regional,
    wanted_bytes: usize,
    limits: &CallOptions,
) -> Result<ExportPrefix, ApiError> {
    if wanted_bytes == 0 {
        return Ok(ExportPrefix::NotStarted);
    }
    let mut stream = execute(scope.export(), limits).await?;
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let remaining = wanted_bytes - output.len();
        let take = remaining.min(chunk.len());
        output.extend_from_slice(&chunk[..take]);
        if output.len() == wanted_bytes {
            break;
        }
    }
    Ok(ExportPrefix::Read {
        bytes: output,
        stream: stream.finish(),
    })
}
```

The zero path is this application's explicit choice to skip request
construction; it is not inferred from `finish()`.

## Custom record formats

A custom format implements `RecordDecoder` and exposes it through a streaming
`ResponseRepresentation` (`ItemStream::decode_with::<Decoder>(body, head)`) and,
optionally, a buffered one for complete collections. See
[Extensions](extensions.md).
