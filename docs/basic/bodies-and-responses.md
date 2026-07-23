# Request bodies, responses, metadata, and errors

> Audience: Basic  
> Covers: built-in body/response types, `ApiResponse<T>`, status handling, and safe errors

Use built-in body and response specifications for most APIs. Concord keeps body acquisition bounded, validates media before polling, and exposes semantic results rather than native responses.

## Request bodies

Bare `#[body]` selects the endpoint's `RequestBodySpec`. The annotated
parameter may use any name and may appear anywhere in declaration order.

```rust
POST create(tenant: String, #[body] input: Json<CreateInput>) -> Json<Item>;
POST echo(#[body] input: Text) -> Text;
PUT blob(#[body] bytes: Bytes) -> NoContent;
PUT upload(#[body] stream: Stream<OctetStream>) -> NoContent;
POST form(#[body] form: Multipart) -> Json<UploadResult>;
```

### Built-in body specifications

#### `Json<T>`

Requires the `json` feature. Input is the codec's serializable value. Concord encodes complete bounded bytes after the before-send hook approves the attempt.

#### `Text`

Input is `String`. Content type is text-oriented and encoding is buffered.

#### `Bytes`

Input is `bytes::Bytes`. It is reusable, exact-length, and can be replayed for the one bounded authentication recovery attempt.

#### `Stream<M>`

Input is `StreamBody`. `M: MediaType` supplies an exact content type. A direct stream is one-shot; a body factory is required when recovery may need reconstruction.

#### `Multipart`

Requires `multipart`. Input is `MultipartBody`. Reqwest owns final multipart serialization and boundary generation. Concord still applies aggregate request-body limits, including framing.

### Body preparation order

```mermaid
flowchart LR
    A[Body metadata] --> B[Known-length checks]
    B --> C[before_send hook]
    C --> D[Prepare body]
    D --> E[Consistency checks]
    E --> F[Credentials and limiter]
    F --> G[Native send]
```

Metadata is available before the hook without encoding, opening a stream, invoking a factory, or constructing a native body. A hook veto prevents body preparation. Concord validates the produced body against its declared metadata and fails closed on contradiction.

### Stream bodies

```rust
use concord_core::prelude::{Bytes, StreamBody};

let body = StreamBody::from_bytes(Bytes::from_static(b"payload"));
client.upload(body).await?;
```

Other constructors accept a stream or asynchronous reader. `.exact_length(n)` adds a checked exact length.

### Multipart bodies

```rust
use concord_core::advanced::{MultipartBody, MultipartPart};

let body = MultipartBody::new()
    .part("description", MultipartPart::text("sample"))?
    .part("data", MultipartPart::bytes(bytes))?;

client.form(body).await?;
```

Parts can be text, bytes, or streams and may specify filename and content type.

### Limits and failure

- Known exact or lower-bound excess can fail before the hook.
- Encoded or streamed first excess is withheld.
- Producer errors are categorized safely.
- Application body values are not included in built-in diagnostics.

For custom behavior, see [Custom codecs](../advanced/endpoints-bodies-and-codecs.md#custom-codecs) and [Request body specifications](../advanced/endpoints-bodies-and-codecs.md#request-body-specifications).

### Complete example

See [`body_workflows.rs`](../../concord_examples/examples/body_workflows.rs) for bytes, codecs, streams, multipart recipes, and factory-backed bodies in one runnable example.

## Responses and metadata

Every endpoint response type implements `ResponseBodySpec` and selects one of three core-sealed acquisition modes.

| Mode | Core provides | Typical types |
|---|---|---|
| Buffered | Complete bounded `Bytes` | `Json<T>`, `Text`, `Bytes`, custom codec |
| Streaming | Bounded lazy `ByteStream` | `Stream<M>`, `Records<D>` |
| Discard | `()` without polling the body | `NoContent` |

### Built-in response types

```rust
GET user() -> Json<User>;
GET message() -> Text;
GET blob() -> Bytes;
GET download() -> Stream<OctetStream>;
DELETE item() -> NoContent;
GET rows() -> Records<Csv<Row>>;
```

### Direct output

Awaiting a request returns the semantic output:

```rust
let user: User = client.user().await?;
let bytes: Bytes = client.blob().await?;
let stream: ByteStream = client.download().await?;
```

### Metadata output

`.response()` wraps every output in `ApiResponse<T>`, including streams and record streams:

```rust
let response = client.download().response().await?;
println!("{} {}", response.status(), response.url());
let stream = response.into_value();
```

Available metadata:

- logical URL;
- status;
- HTTP version;
- response headers;
- `ExecutionMeta` containing endpoint, method, page index, and authentication recovery flag.

`DecodedResponse<T>` is a compatibility alias for `ApiResponse<T>`.

### Ordering

For successful responses:

1. response-head hooks and limiter observation run;
2. authentication challenge handling completes;
3. non-success status becomes `HttpStatus`;
4. known-length mode preflight runs;
5. response specification validation runs before body polling;
6. core acquires buffered, streaming, or discard input;
7. the specification produces its output;
8. completion is fixed.

Buffered output emits `Success`. Streaming output emits `StreamOpened` before later consumption. Later stream errors are items and do not change completion.

### No-content responses

`NoContent` accepts every successful 2xx status and does not poll the response body. It is not a request body specification.

### Media validation

Built-in codecs and streams can supply an `Accept` header and validate response content type. Application response specifications can support exact, vendor, suffix, parameterized, multiple, or missing media types.

### Native response boundary

No standard or advanced response specification receives `reqwest::Response`. Status handling, bounds, deadlines, cancellation, and native body ownership remain core-controlled.

## Errors and statuses

Concord exposes stable error classifications while deliberately limiting automatic value disclosure.

### `ApiError`

Every request failure returns `ApiError`. Begin with:

```rust
match error.category() {
    ErrorCategory::Configuration => { /* invalid request/client state */ }
    ErrorCategory::AuthenticationUnavailable => { /* no credential */ }
    ErrorCategory::RateLimit => { /* limiter failure */ }
    ErrorCategory::Timeout => { /* operation deadline */ }
    ErrorCategory::HttpStatus => { /* final non-2xx */ }
    ErrorCategory::ResponseDecoding => { /* successful body could not decode */ }
    other => { /* see reference */ }
}
```

Other top-level categories distinguish request encoding/body, provider failures, authentication rejection, hooks, connect, transport, response body/contract, pagination, and internal safe failure.

### Non-success status

A final non-success response becomes `ErrorCategory::HttpStatus`. Access:

- `http_status()`;
- sanitized `http_headers()`;
- `http_response()` or `into_http_response()`;
- bounded `retry_after()`.

`HttpResponseFailure` contains endpoint, method, status, sanitized headers, bounded terminal status body, and retry-after metadata.

### Terminal status body

Status-body capture defaults to 64 KiB and is complete-only. States distinguish not captured, complete, too large, and read failure. Bytes are available only through explicit accessors. Disabling capture discards the body rather than allowing an unbounded body.

A bounded complete status body can be decoded explicitly with a `DecodeBody` codec:

```rust
if let Some(failure) = error.http_response() {
    let problem: Problem = failure.decode_body::<Json<Problem>>()?;
}
```

### Category-specific accessors

- `configuration_kind()`
- `auth_kind()`
- `rate_limit_kind()`
- `pagination_kind()`

These expose stable non-exhaustive enums without embedding sensitive values.

### Formatting and sources

`Display`, `Debug`, and recursive sources omit request bodies, response bodies, credentials, authentication values, rate keys, pagination continuations, native authenticated URLs, and parser-authored unsafe text. Application extensions own the safety of text they log themselves.

### Error precedence

Concord reports the earliest result-determining failure in the managed lifecycle. For example, a hook veto prevents body preparation and later credential/limiter/send failures; an invalid known body length can fail before hooks; a non-success status is classified before successful-body decoding.

See the [Error reference](../reference/features-limits-and-errors.md#error-reference) for all kinds.
