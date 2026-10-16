# Request bodies

A body is exactly the value the caller supplies. Its request representation
prepares one managed `PreparedBody` that alone owns the actual
`Content-Type`, trustworthy length information and mechanical replay
capability (§9.2, BODY-01).

## Declaration

A body is an endpoint parameter with the `body` role and an optional
representation:

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#buffered_endpoints -->
```concord
POST json_round_trip(body body: Vec<u32> via Json) -> Vec<u32> via Json;
POST text_round_trip(body body: String via Text) -> String via Text;
POST custom_round_trip(body body: String via ExampleCodec) -> String via ExampleCodec;
```

- At most one body per endpoint. A required body is a positional constructor
  argument in declaration order and takes its exact type (no `IntoValue`
  conversion).
- `body input: T via R` selects `R` for this body; without `via`, the nearest
  `request` default applies, and a scope's `request explicit;` fence forces a
  local choice. A bodyless endpoint needs no request representation.
- Bodies have no initializers: `= V` and `#[default]` are rejected.
- A body is protected by role: it never appears in plans, errors, observers or
  debug output, and it does not need `#[sensitive]`.

### Optional bodies

`body input?: T` makes presence explicit. The request gets `set_input(T)`
(exact payload), `replace_input(Option<T>)`, `set_input_if_some(Option<T>)` and
`omit_input()`; there is no reset. An outer-absent body means no representation
call, no body bytes and no body-owned media header. A present payload whose
representation produces zero bytes is still a present body (BODY-02). Requests
whose declaration has a body slot are not `Clone`, even while the optional body
is absent.

## Preparation

Preparation happens once per direct operation (or page), after public shaping
and before credential acquisition and rate admission, under the operation
deadline and the request-body limit (16 MiB by default). Core then verifies the
final head/body media ownership and any known length against the limit before
credentials or dispatch. An invalid body therefore fails before any provider or
limiter work.

Automatic attempts within one operation reuse or reconstruct the prepared body
through its mechanical replay capability; they never call the representation
again because a retry was allowed. Planning never prepares a body: a plan
reports the static `BodyDescription` hint and says the exact Content-Type and
length are deferred to preparation.

## `PreparedBody`

| Constructor | Replay capability |
|---|---|
| `PreparedBody::empty()` | mechanically replayable, zero bytes |
| `PreparedBody::bytes(bytes)` | mechanically replayable |
| `PreparedBody::stream(RequestStream)` | one-shot; never replayed automatically |
| `PreparedBody::factory(factory)` | reconstructible: a `BodyFactory` creates a fresh stream per admitted attempt |
| `PreparedBody::multipart(MultipartBody)` (feature `multipart`) | boundary generated once; replayable unless a part streams |
| `PreparedBody::multipart_factory(factory)` | reconstructible multipart |

`.content_type(value)` validates the media type once; a conflicting second
value is an error and an identical repeat is harmless. Length hints
(`LengthHint::Unknown`, `AtLeast(n)`, `Exact(n)`, set with `at_least` /
`exact_length` on a `RequestStream`) are validated against observed
production: excess, truncated or dishonest production is a `BodyError` carrying
truthful dispatch knowledge.

Mechanical replayability never makes a `POST` safe or idempotent. Retry and
auth recovery still require the endpoint's effect, dispatch knowledge and every
other gate (see [Retry](retry.md)).

## First-party request representations

| Representation | Input | Behavior |
|---|---|---|
| `Json` (feature `json`) | `T: Serialize` | bounded serialization, `application/json`; page-stable for `Clone` inputs |
| `Text` | `String` | UTF-8 text, `text/plain; charset=utf-8`; page-stable |
| `Binary` | `Bytes`, `Vec<u8>` | raw bytes, `application/octet-stream`; page-stable |
| `Binary` | `RequestStream` | raw bytes, `application/octet-stream`, from a lazy one-shot stream |
| `NdJson` (feature `ndjson`) | `Vec<T>`, `T: Serialize` | one compact JSON value + LF per record, in order, `application/x-ndjson`; each record at most 1 MiB; page-stable for `Clone` inputs |
| `Multipart` (feature `multipart`) | `MultipartBody` | request-only `multipart/form-data` |
| `PreparedBody` | `PreparedBody` | the caller's own prepared body, as built |
| any `EncodeBody` codec | `C::Input` | the codec's bytes and single `CONTENT_TYPE` |

First-party serializers write into bounded buffers and stop at the first
excess; they never serialize unbounded and truncate. `NdJson` frames typed
request records under the same aggregate request-body limit; `Csv` is a
response representation. Record responses are covered in [Streams](streams.md).

## Streaming bodies

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#streaming_endpoints -->
```concord
POST upload(body body: RequestStream via Binary) -> Bytes via Binary;
GET download() -> ByteStream via Binary;
POST exact_upload(body body: RequestStream via Binary) -> Bytes via Binary;
GET exact_download() -> ByteStream via Binary;
```

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#streaming_requests -->
```rust
let _: body_api::UploadRequest = client.upload(RequestStream::from_bytes(Bytes::from_static(
    b"stream payload",
)));
let _: body_api::DownloadRequest = client.download();
let _: body_api::ExactUploadRequest = client.exact_upload(RequestStream::from_bytes(
    Bytes::from_static(b"binary payload"),
));
let _: body_api::ExactDownloadRequest = client.exact_download();
```

`RequestStream` is built lazily from bytes (`from_bytes`), a stream
(`from_stream`), an async reader (`from_reader`) or an explicitly opened file
(`from_file(path).await`). Opening a file is an explicit application operation,
never hidden request construction. The stream is not polled until transport
begins; empty chunks are bounded (at most 1,024 in total for one production), reader
backpressure is respected, and pulling stops after an error. Request debug
never serializes input.

## Multipart

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#multipart_endpoint -->
```concord
POST multipart_upload(body body: MultipartBody via Multipart) -> () via Empty;
```

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#multipart_recipe -->
```rust
let multipart = MultipartBody::new()
    .part("description", MultipartPart::text("deterministic example")?)?
    .part(
        "payload",
        MultipartPart::bytes(Bytes::from_static(b"multipart bytes"))?,
    )?;
let _: body_api::MultipartUploadRequest = client.multipart_upload(multipart);
```

A file part streams from an opened file and makes the body one-shot:

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#multipart_file -->
```rust
pub async fn file_upload(
    client: &BodyApi,
    path: impl AsRef<std::path::Path>,
) -> Result<body_api::MultipartUploadRequest, concord::body::BodyError> {
    let file = RequestStream::from_file(path).await?;
    let multipart = MultipartBody::new()
        .part("description", MultipartPart::text("file upload")?)?
        .part(
            "payload",
            MultipartPart::stream(file)
                .file_name("payload.bin")?
                .content_type("application/octet-stream")?,
        )?;
    Ok(client.multipart_upload(multipart))
}
```

The boundary is generated and validated once, during preparation, and the same
boundary appears in the header and in every serialized byte of every replay.
Part names, file names, media types and counts are bounded. A plan says
"multipart/form-data; boundary generated during execution" without generating
one.

## Custom request representations

A full representation has one authority, `prepare`, returning the managed body:

<!-- concord-source: crates/concord-examples/src/request_response/specifications.rs#request_body_spec_extension -->
```rust
/// A raw-bytes request representation without a media type.
pub struct ExampleRequestSpec;

impl RequestRepresentation<Bytes> for ExampleRequestSpec {
    fn prepare(input: Bytes, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        Ok(PreparedBody::bytes(input))
    }
}
```

`RequestBodyContext` supplies the validated endpoint ID, method, the positive
body limit and remaining timing facts; it exposes no credentials, native
request, client or limiter. The optional `DESCRIPTION: BodyDescription` constant
is static explanation metadata for plans, not a header promise.

A simple buffered codec implements `EncodeBody` instead, and a blanket bridge
provides the full representation: `encode` is called once under the body limit
and its single `CONTENT_TYPE` becomes the actual header (§9.6):

<!-- concord-source: crates/concord-examples/src/request_response/specifications.rs#codec_extension -->
```rust
pub struct ExampleCodec;

impl EncodeBody for ExampleCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/vnd.concord.example");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for ExampleCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/vnd.concord.example");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == <Self as DecodeBody>::ACCEPT {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new(CodecErrorKind::Decode))
    }
}
```

A codec chooses either the convenience traits or the full traits for one pair,
never both. `CodecError` is category-only (`Encode`, `Decode`, `Limit`); the
bridge maps it and core attaches the operation journal. An application form
encoder is an `EncodeBody` with an explicit ordered payload and
`application/x-www-form-urlencoded`; there is no implicit query-object or form
inference.

## Page-stable bodies

A paginated endpoint with a body prepares that body again for each page, which
requires the explicit promise `PageRequestRepresentation<Input>` (input
`Clone`): the same semantic body and exact media type for every repeated
preparation. `Json`, `Text` and `Binary` buffered pairs opt in; one-shot streams
and multipart do not. Implementing it grants no safe effect, native replay or
extra attempt. See [Pagination](pagination.md).

## Errors

A preparation failure is an `ApiError` at stage `Preparation` with
`DispatchKnowledge::NotDispatched`. A failure while a body streams to the
transport keeps the dispatch knowledge settled at that point (never a
fabricated "not sent"). Both carry a value-free `BodyErrorKind`; no error prints
body bytes, file names or part values.
