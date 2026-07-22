# Advanced endpoints, request bodies, codecs, and factories

> Audience: Advanced  
> Imports: `concord_core::advanced`

Use these extension points when the DSL built-ins are not enough. Each extension still enters the same managed execution path and cannot obtain native request or transport ownership.

## Extension model

Concord extensions are typed semantic adapters, not escape hatches to native HTTP.

```mermaid
flowchart LR
    A[Application extension] -->|typed semantic input/output| B[Concord boundary]
    B -->|private native ownership| C[Reqwest and transport]
```

### What core always owns

- logical and native URL separation;
- public-header validation and authentication collisions;
- request body limits and replayability;
- operation deadlines and cancellation;
- credential preflight/finalization and recovery;
- limiter acquisition and response observation;
- native request construction and HTTP execution;
- status classification;
- response acquisition mode and finite limits;
- completion outcome;
- built-in redaction.

### What extensions can own

Depending on the trait, an extension may own:

- complete buffered encoding/decoding;
- semantic request-body metadata and preparation;
- semantic response validation and output conversion;
- record framing over bounded bytes;
- credential acquisition and refresh;
- explicit rate-limit authority;
- server cooldown interpretation;
- pagination state transitions;
- lifecycle observation.

### Obligations shared by advanced traits

Application implementations must:

- be `Send`, `Sync`, or `'static` where required;
- keep synchronous callbacks short and non-blocking;
- bound CPU, memory, queues, retries, maps, metric cardinality, and external I/O;
- own confidentiality in their own logging and storage;
- return safe categorized errors without embedding protected values;
- tolerate concurrent calls when the trait permits overlap;
- understand that cancellation drops futures but cannot roll back completed external effects.

### What extensions cannot do

They cannot add a fourth response acquisition mode, receive a native response, mutate the shared client, replace the executor, disable finite core limits, override status classification, emit a second completion, or request unbounded buffering.

### Selecting the smallest extension

- Complete bytes in and out: implement `EncodeBody`/`DecodeBody`.
- Custom request representation: implement `RequestBodySpec`.
- Custom successful response representation: implement `ResponseBodySpec`.
- Framed records: implement `RecordDecoder`.
- Runtime endpoint construction: use `PreparedEndpoint`.
- Dynamic auth material: implement `CredentialProvider`.
- Application-specific quotas: implement `RateLimiter`.
- Observation only: implement `RuntimeHooks` or `RateLimitObserver`.

## Explicit generated endpoints

Every DSL endpoint has a public endpoint value under the generated `endpoints` module. The facade method is a convenience over this explicit value.

```rust
let endpoint = reference_api::endpoints::regions::Resource::new(
    "eu-west".to_owned(),
    "42".to_owned(),
    vec!["stable".to_owned()],
)
.language("fr".to_owned());

let request = client.request(endpoint);
let value = request.await?;
```

### When to use explicit endpoints

- Store or choose request descriptions before associating them with a client.
- Build generic application workflows over a generated endpoint family.
- Separate endpoint creation from request execution.
- Use generated endpoint-specific builders without calling the facade.

### Ownership and typing

An explicit endpoint owns its client/scope/endpoint parameter snapshots. `client.request(endpoint)` consumes the endpoint and creates the same `Request<E>` used by the facade method.

Generated capability sealing prevents:

- passing an endpoint from the wrong generated client;
- forging an arbitrary application type as an endpoint;
- importing the generated-private integration trait;
- accessing the private executor or response handler.

### Bodies, pagination, and raw response

The explicit endpoint has the same capabilities as its facade request:

- body input is part of the endpoint constructor when declared;
- `.paginate(...)` is generated only for page-capable endpoints;
- `.raw_response()` is generated only for eligible endpoints under the dangerous feature;
- stream, prepared, or incompatible responses do not gain unsupported terminals.

### Difference from prepared endpoints

Explicit endpoints are statically generated from the DSL and retain their exact generated type. Prepared endpoints are runtime-composed routes and response specifications accepted through the generated client's prepared capability.

## Prepared endpoints

`PreparedEndpoint` builds a runtime-defined endpoint while reusing the same client and execution lifecycle as generated endpoints.

```rust
use concord_core::advanced::{HeaderName, Method, PreparedEndpoint};

let endpoint = PreparedEndpoint::builder("InventoryApi::lookup", Method::POST)
    .path_segment("records")
    .path_segment("42")
    .query("view", "summary")
    .header(HeaderName::from_static("x-example"), "canonical")
    .body_codec::<MyCodec>(input)
    .response::<MyCodec>()
    .timeout(Duration::from_secs(5))
    .build()?;

let output = client.request(endpoint).await?;
```

### Builder operations

- `path_segment(value)` appends one encoded segment.
- `query`, `query_optional`, and `query_values` append ordered query values.
- `header` inserts or replaces a case-insensitive public header.
- `body(RequestBody)` selects an already managed body.
- `body_spec::<B>(B::Input)` selects any `RequestBodySpec`.
- `body_codec::<C>(C::Input)` uses a complete buffered codec.
- `auth(Auth)` appends one prepared authentication placement.
- `timeout` and `disable_timeout` select endpoint timeout behavior.
- `rate_limit(RateLimitRule)` appends an explicit rule.
- `response::<S>()` selects any `ResponseBodySpec`.
- convenience response methods select bytes, no-content, or stream.
- `build()` validates the route and returns a typed endpoint.

### Response typing

The builder type parameter is the response specification. The default is `NoContent`. Calling `.response::<S>()` changes the resulting `PreparedEndpoint<S>` type and therefore the request output.

### Same lifecycle

Prepared endpoints use the same:

- public request preparation;
- body metadata/hook/prepare ordering;
- authentication and rate limiting;
- operation deadline;
- native HTTP client;
- status classification;
- buffered/streaming/discard acquisition;
- completion and redaction.

Private body erasure exists only for storage and delegates to the same managed body lifecycle.

### Safety rules

Path segments are encoded as one segment; route delimiters cannot be injected. Reserved public headers, auth collisions, invalid media, invalid rate rules, and malformed routes fail as typed `ConfigurationError` values before execution.

### Limitations

Prepared endpoints do not paginate. They cannot expose a native response, select an application-defined acquisition mode, disable ordinary body limits, or replace the executor.

## Custom codecs

Use `EncodeBody` and `DecodeBody` for complete buffered formats.

```rust
use concord_core::advanced::{CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext};
use concord_core::prelude::Bytes;

struct VendorCodec;

impl EncodeBody for VendorCodec {
    type Input = VendorRequest;
    const CONTENT_TYPE: Option<&'static str> = Some("application/vnd.example+bin");

    fn encode(value: VendorRequest, _cx: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        encode_vendor(value)
    }
}

impl DecodeBody for VendorCodec {
    type Output = VendorResponse;
    const ACCEPT: Option<&'static str> = Some("application/vnd.example+bin");

    fn validate_content_type(value: Option<&str>) -> Result<(), CodecError> {
        validate_vendor_media(value)
    }

    fn decode(bytes: Bytes, _cx: DecodeContext<'_>) -> Result<VendorResponse, CodecError> {
        decode_vendor(bytes)
    }
}
```

DSL use:

```text
POST Exchange(body: crate::VendorCodec) -> crate::VendorCodec;
```

### Managed adapters

Core automatically adapts:

- `EncodeBody` to `RequestBodySpec` with a complete buffered body;
- `DecodeBody` to `ResponseBodySpec<Mode = Buffered>`.

A normal complete codec should implement only the codec traits, not a parallel body lifecycle.

### Timing and limits

- Encoding runs after the before-send hook.
- Encoded bytes are checked against the request-body limit.
- Decoding receives only complete bounded successful bytes.
- Content-type validation occurs before body polling/collection where the response contract requires it.
- Synchronous codec time is checked against the operation deadline after return.

### Error safety

`CodecError` messages belong to the extension and can be operationally sensitive. Concord classifies failures without reproducing response or request bytes. Do not include raw payloads, credentials, headers, filenames, or tokens in codec errors.

### When not to use a codec

Use `RequestBodySpec` for streaming, multipart-like generated media, or custom metadata. Use `ResponseBodySpec` for streaming or discard behavior. Use `RecordDecoder` for incremental framed records.

## Request body specifications

`RequestBodySpec` maps an application input to one managed outgoing `RequestBody` and supplies structural metadata before expensive work.

```rust
pub trait RequestBodySpec: Send + Sync + 'static {
    type Input: Send + 'static;

    fn metadata(
        input: &Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError>;

    fn prepare(
        input: Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError>;
}
```

### Metadata

`RequestBodyMetadata` contains:

- `LengthHint::{Unknown, AtLeast, Exact}`;
- `RequestContentType::{None, Exact, Generated}`;
- `BodyKind::{Empty, Buffered, Stream, Multipart}`.

`metadata` runs before the before-send hook. It must not encode, invoke a factory, poll a stream, open a file, serialize multipart, or construct a native body.

### Preparation

`prepare` runs once after hook approval for the pending request execution. It returns one `RequestBody`:

- `RequestBody::empty()`;
- `RequestBody::bytes(bytes)`;
- `RequestBody::stream(StreamBody)`;
- `RequestBody::factory(factory)`;
- multipart variants under the feature.

Core derives replayability from that value. Applications cannot mark a one-shot source replayable.

### Consistency checks

Core fails closed when metadata contradicts the produced body:

- body kind mismatch;
- exact or lower-bound length mismatch;
- conflicting content-type ownership;
- invalid generated-media ownership;
- impossible managed state.

Failure occurs after hook approval and preparation but before credentials, limiting, authentication, native construction, or send.

### Context

`RequestBodyContext` exposes endpoint identity, method, and the finite body limit. It does not expose URL, headers, credentials, limiter state, native request, or transport.

### Generated and prepared use

```text
POST Vendor(body: crate::VendorRequestSpec) -> Bytes;
```

```rust
PreparedEndpoint::builder("Api::vendor", Method::POST)
    .body_spec::<VendorRequestSpec>(input)
    .response_bytes()
    .build()?;
```

Both use the same runtime.

### Extension responsibility

Keep synchronous work bounded and value-safe. If expensive or fallible production must be repeatable per Concord attempt, return a body factory rather than performing external I/O in `prepare`.

## Body factories

`BodyFactory` reconstructs one complete managed request body for a Concord attempt.

```rust
pub trait BodyFactory: Send + Sync + 'static {
    fn create(&self) -> Result<RequestBody, BodyError>;
}
```

Install it with:

```rust
RequestBody::factory(factory)
```

### When to use it

Use a factory when the body source is not reusable but can be rebuilt for the one bounded authentication recovery attempt—for example, reopening a file or creating a fresh stream.

### Invocation rules

- The factory is not called during request-body metadata.
- A before-send hook veto prevents invocation.
- It runs once per Concord attempt.
- Reqwest-internal hidden retry does not invoke it again.
- One authentication recovery can invoke it once for the recovery attempt.
- A factory failure occurs before native send.

### Replayability

Core determines replayability from the produced body. Returning bytes is reusable; returning a direct stream is one-shot. The factory itself is the reconstruction authority and must produce semantically equivalent request bodies when called again.

### Responsibility

The callback is synchronous and cannot be preempted. It must bound CPU, memory, file access, queues, retries, and logging. Do not include file paths, body values, credentials, or tokens in returned error text.

### Alternative

If a body is already complete bytes, use `Bytes` or `RequestBody::bytes`. If the body can be represented as a normal complete codec, implement `EncodeBody` instead.

## Multipart factories

`MultipartBody` is an owned recipe. It can contain reusable text/bytes parts and one-shot stream parts. `MultipartFactory` reconstructs the complete multipart recipe for another Concord attempt.

```rust
pub trait MultipartFactory: Send + Sync + 'static {
    fn create(&self) -> Result<MultipartBody, BodyError>;
}
```

Use:

```rust
RequestBody::multipart_factory(factory)
```

### Serialization boundary

Reqwest owns native multipart serialization and top-level boundary generation. Concord owns:

- recipe validation;
- part metadata;
- replayability classification;
- aggregate request accounting including framing;
- operation deadline and cancellation;
- first-excess withholding;
- safe errors.

### Parts

`MultipartPart` supports:

- `.text(value)`;
- `.bytes(Bytes)`;
- `.stream(StreamBody)`;
- optional `.filename(...)`;
- optional `.content_type(...)`.

Part names, filenames, content types, and values may be sensitive. Built-in debug output is structural.

### Recovery

A multipart body containing direct streams is one-shot. A factory can rebuild it for authentication recovery. Hidden native retry does not reconstruct it.

### Not supported

Concord does not parse multipart responses. A specialized application implementation can use `ResponseBodySpec<Mode = Streaming>` and `ByteStream` outside the core executor.
