# Streaming, multipart, CSV, and NDJSON

> Audience: Basic  
> Covers: `StreamBody`, `ByteStream`, multipart requests, typed record streams, batching, and cancellation

Concord exposes bounded byte streaming and incremental record decoding while retaining private native-body ownership. Network chunks are never treated as semantic record boundaries.

## Streaming

Concord uses streaming in both directions while retaining finite limits and private native-body ownership.

### Streaming request bodies

Declare:

```text
PUT Upload(body: Stream<OctetStream>) -> NoContent;
```

Create a `StreamBody`:

```rust
use concord_core::prelude::{Bytes, StreamBody};

let body = StreamBody::from_bytes(Bytes::from_static(b"payload"));
client.upload(body).await?;
```

`StreamBody` can also wrap a `Stream<Item = Result<Bytes, E>>` or an async reader. An optional exact length is checked against actual production. Direct streams are one-shot.

### Streaming responses

Declare:

```text
GET Download() -> Stream<OctetStream>;
```

Awaiting returns `ByteStream`:

```rust
use futures_core::Stream;
use std::pin::Pin;

let mut stream = client.download().await?;
while let Some(chunk) = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
    let bytes = chunk?;
    consume(bytes);
}
```

`ByteStream` contains no metadata or media marker. Use `.response()` to receive `ApiResponse<ByteStream>`.

### Limits

- Known response length is checked before returning the stream.
- Unknown streams are lazy.
- Cumulative accounting happens after decompression.
- The first chunk that would exceed the stream limit is withheld.
- Body read errors become safe `ApiError` items.

### Completion and cancellation

Opening the stream fixes the endpoint result and emits `StreamOpened`. Later item errors do not emit another completion. Dropping the stream synchronously drops the owned body and cancels further reads.

The operation deadline controls opening the stream, not application consumption after return. Applications must bound their own consumption time.

### Protocol framing

`ByteStream` is media-independent. Use `Records<D>` for bounded record framing. SSE-specific reconnection, WebSockets, multipart response parsing, and bidirectional protocols remain application-level or application-level protocol layers.

## Records, CSV, and NDJSON

Record responses decode semantic items incrementally over a bounded `ByteStream`.

```text
GET CsvRows() -> Records<Csv<Row>>;
GET Events() -> Records<NdJson<Event>>;
```

Awaiting returns `RecordStream<D>`:

```rust
let mut rows = client.csv_rows().await?;
// poll rows as Stream<Item = Result<Row, RecordError>>
```

`.response()` returns `ApiResponse<RecordStream<D>>`.

### Chunk independence

Network chunks are never record boundaries. A record can span many chunks, and one chunk can contain several records. The decoder is repeatedly asked for available records before the network body is polled again.

### CSV behavior

The built-in CSV decoder provides a deliberately narrow typed dialect:

- UTF-8;
- comma delimiter;
- required header row;
- typed field deserialization;
- double-quote quoting and doubled-quote escaping;
- embedded delimiters and quoted newlines;
- LF and CRLF;
- arbitrary chunk splits;
- several rows per chunk;
- optional final row without a trailing newline;
- empty physical lines outside quoted fields ignored.

Malformed headers, quotes, or typed values return sanitized `RecordError` values.

### NDJSON behavior

The NDJSON decoder provides:

- one JSON value per non-empty logical line;
- LF and CRLF;
- arbitrary chunk splits;
- several values per chunk;
- optional final newline;
- whitespace-only lines ignored;
- safe malformed/incomplete JSON errors.

It accepts `application/x-ndjson` and `application/ndjson` with ordinary parameters.

### Bounds

CSV and NDJSON use a 1 MiB maximum undecoded record/line staging buffer. The first append or framed record that exceeds the bound is withheld. This is independent of the total response stream limit.

### Batching

```rust
use std::num::NonZeroUsize;

let batches = client
    .csv_rows()
    .await?
    .batches(NonZeroUsize::new(100).unwrap());
```

Batching is a consumer adapter over record streaming, not another response mode.

### Errors and completion

The endpoint returns successfully with `StreamOpened`. Later body or decode failures are stream items and do not change completion. Record formatting and sources omit raw record bytes, parser text, headers, filenames, CSV fields, and JSON values.

For custom framing, see [Custom record decoders](../advanced/responses-records-and-streams.md#custom-record-decoders).

## Uploads and downloads

### Complete bytes

For small known payloads:

```text
PUT Blob(body: Bytes) -> NoContent;
GET Blob() -> Bytes;
```

Bytes are reusable and bounded.

### Streaming upload

```text
PUT Upload(body: Stream<OctetStream>) -> NoContent;
```

Create `StreamBody` from bytes, a stream, or an async reader. Use `.exact_length` only when the producer will generate exactly that many bytes.

A direct stream is one-shot. Use `BodyFactory` or a request body specification returning a factory when authentication recovery may need reconstruction.

### Streaming download

```text
GET Download() -> Stream<OctetStream>;
```

Awaiting returns `ByteStream`. Media validation occurs before the first body poll. The cumulative limit is applied after decompression, and the first excess chunk is withheld.

Use `.response()` when status, headers, logical URL, or execution metadata is needed with the stream.

### Multipart upload

```text
POST Form(body: Multipart) -> Json<UploadResult>;
```

Build an owned `MultipartBody`. Reqwest generates the boundary; Concord accounts for aggregate framing and applies the request limit.

### Cancellation

Dropping an upload request drops the body source. Dropping a download stream drops the owned response body. Bytes already sent or external server effects are not rolled back.

### Unsupported protocols

Multipart response parsing, resumable upload orchestration, checksummed chunk protocols, SSE reconnection, and bidirectional streaming are not built in. Implement protocol framing over `ByteStream` or use a specialized library outside Concord.

## Typed CSV and NDJSON streams

### API declaration

```text
GET Export() -> Records<Csv<Row>>;
GET Events() -> Records<NdJson<Event>>;
```

The result is a lazy `RecordStream`. Network chunks do not need to align with rows or JSON values.

### Consumption

Use normal `Stream` polling or an ecosystem extension trait compatible with `futures-core::Stream`.

```rust
let records = client.export().await?;
let batches = records.batches(NonZeroUsize::new(250).unwrap());
```

Batching groups decoded records and does not change network acquisition or completion semantics.

### CSV decisions

The built-in dialect uses a required header row, comma delimiter, standard double quoting, LF/CRLF, and typed Serde deserialization. It is suitable for conventional tabular exports. Use a custom decoder for another delimiter, headerless records, comments, or domain-specific escaping.

### NDJSON decisions

Each non-empty logical line contains one JSON value. Whitespace-only lines are ignored. The final value may omit a terminal newline. Use a custom decoder when framing is not line-based.

### Bounds

- Total bytes remain subject to the stream response limit.
- Built-in undecoded record staging is limited to 1 MiB.
- First excess is withheld.
- Errors do not expose raw rows or JSON text.

### Completion

The endpoint completes as `StreamOpened`. Decode failures later appear as `RecordError` items and do not create another completion callback.

### Feature isolation

`csv` and `ndjson` are independent non-default features. `ndjson` does not automatically enable the general endpoint `json` feature.
