# Advanced response specifications, streams, and record decoders

> Audience: Advanced  
> Covers: `ResponseBodySpec`, sealed acquisition modes, `ByteStream`, and custom `RecordDecoder` implementations

Response extensions choose one of three core-owned acquisition modes. They can interpret semantic data, but they cannot define unbounded acquisition or receive a native response.

## Response body specifications

`ResponseBodySpec` maps a successful response into a semantic output while selecting one core-sealed acquisition mode.

```rust
pub trait ResponseBodySpec: Send + Sync + 'static {
    type Output: Send + 'static;
    type Mode: ResponseMode;

    fn accept() -> Option<&'static str> { None }
    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> { Ok(()) }
    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError>;
}
```

### Modes

Only core implements `ResponseMode`:

- `Buffered` provides complete bounded `Bytes`.
- `Streaming` provides an unpolled bounded `ByteStream`.
- `Discard` provides `()` and never polls the body.

Applications select a mode but cannot add native or unbounded acquisition.

### Ordering

```text
2xx classification
→ known-length mode preflight
→ validate(response head)
→ core acquisition
→ from_body
→ fixed completion
```

`validate` always runs before the first body poll. Status classification happens first, so a custom specification cannot reinterpret a non-success response as success.

### `ResponseHead`

The head exposes:

- endpoint identity;
- method;
- logical URL;
- status and HTTP version;
- actual response headers;
- content length;
- execution metadata.

These values can be operationally sensitive. The extension owns its logging and storage.

### Examples

Buffered output:

```rust
struct Length;
impl ResponseBodySpec for Length {
    type Output = usize;
    type Mode = Buffered;
    fn from_body(body: Bytes, _: &ResponseHead<'_>) -> Result<usize, ResponseBodyError> {
        Ok(body.len())
    }
}
```

Streaming output can wrap or return the supplied `ByteStream`. Discard output can validate headers/status semantics and return `()` without body polling.

### Deadline and completion

Synchronous `validate` and `from_body` work is checked against the operation deadline after return. Buffered and discard outputs complete as `Success`; streaming outputs complete as `StreamOpened`.

### Pagination capability

A custom buffered page response can implement `PageResponseSpec`. Streaming, discard, bytes identity, and record streams do not become paginated automatically.

## Custom record decoders

Implement `RecordDecoder` for an incremental framed protocol over a bounded byte stream.

```rust
pub trait RecordDecoder: Sized + Send + 'static {
    type Record: Send + 'static;

    fn accept() -> Option<&'static str> { None }
    fn validate(head: &ResponseHead<'_>) -> Result<(), RecordError> { Ok(()) }
    fn start(head: &ResponseHead<'_>) -> Result<Self, RecordError>;
    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError>;
    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError>;
    fn max_buffered_bytes() -> NonZeroUsize;
}
```

Declare:

```rust
GET events() -> Records<crate::VendorDecoder>;
```

### Decoder protocol

- `validate` and `start` run before body polling.
- One decoder instance belongs to one returned stream.
- `decode` can consume zero or more bytes and return at most one record per call.
- Core repeatedly calls `decode` while records remain available before polling another network chunk.
- `decode_eof` distinguishes a complete final record from incomplete framing.
- Network chunk boundaries have no semantic meaning.

### Finite staging

`max_buffered_bytes` must be positive. Core rejects the first append or framed record that would exceed it without retaining or exposing the offending bytes. This bound is separate from the cumulative stream-response limit.

### Errors

Use `RecordErrorKind` to classify invalid media, framing, decode, incomplete record, limit, body, or internal failures. Formatting and sources must not reproduce record bytes, parser text, headers, filenames, CSV fields, or JSON values.

### Batching and cancellation

`RecordStream::batches(size)` groups already decoded records. Dropping the stream or batch adapter synchronously drops decoder state and the underlying `ByteStream`. Later decode/body errors are stream items; completion remains `StreamOpened`.

### What not to do

Do not perform blocking I/O in decoder methods, keep unbounded internal state, assume chunks equal records, or spawn detached background parsing.
