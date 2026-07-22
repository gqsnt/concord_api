# API and capability reference

> Audience: Users and extension authors  
> Exact signatures: generated Rustdoc

This page maps the public namespaces, generated capabilities, request and response terminals, prepared endpoint API, and advanced extension types. Rustdoc remains authoritative for exact signatures and trait bounds.

## Capability matrix

| Capability | Basic DSL/built-in | Advanced extension | Principal boundary |
|---|---|---|---|
| Generated clients | `api!`, facade methods | explicit generated endpoints | compile-time API; no runtime schema |
| Runtime endpoints | — | `PreparedEndpoint` | no prepared pagination |
| Request bodies | JSON, text, bytes, stream, multipart | codecs, `RequestBodySpec`, factories | one managed `RequestBody` |
| Responses | JSON, text, bytes, no-content, stream, records | `ResponseBodySpec` | buffered/streaming/discard only |
| Metadata | `.response()` / `ApiResponse<T>` | arbitrary semantic `T` | no native response |
| Authentication | API key, bearer, Basic, OAuth2, endpoint-backed | custom provider, provider HTTP | private native materialization |
| Pagination | offset, page, cursor | custom controller | sequential all-or-error vector |
| Rate limiting | lexical rules, default observer | custom limiter/observer, prepared explicit keys | generated keys are global |
| Records | CSV, NDJSON | custom `RecordDecoder` | bounded staging over `ByteStream` |
| Hooks | built-in debug | `RuntimeHooks` | observation, not native mutation |
| Retry | off, Reqwest default | dangerous custom retry | native retry stays inside one attempt |
| Raw success | — | dangerous bounded raw response | no native Reqwest response |
| Transport | managed HTTP/TLS/proxy | approved HTTP configuration | no custom executor/transport |

### Output terminal matrix

| Endpoint response | Direct await | `.response()` | `.paginate()` | `.raw_response()` |
|---|---|---|---|---|
| Buffered codec/page output | semantic output | `ApiResponse<T>` | when `PageResponseSpec` | feature/capability dependent |
| `Bytes` | `Bytes` | `ApiResponse<Bytes>` | no | eligible under dangerous feature |
| `NoContent` | `()` | `ApiResponse<()>` | no | no |
| `Stream<M>` | `ByteStream` | `ApiResponse<ByteStream>` | no | no |
| `Records<D>` | `RecordStream<D>` | `ApiResponse<RecordStream<D>>` | no | no |
| Prepared response | selected output | `ApiResponse<T>` | no | no |

### Feature ownership

Feature-disabled types disappear at compile time. The macro does not silently choose an alternate representation.

## Prelude versus advanced

### `concord_core::prelude`

The prelude contains normal application-facing types used by generated clients:

- `Bytes`;
- protected material wrappers: `SecretString`, `AccessToken`, `ApiKey`, `BasicCredential`;
- body types: `RequestBody`, `Stream`, `StreamBody`, optional `Multipart`;
- codecs/media markers: `Json`, `Text`, `OctetStream`;
- request/result types: `Request`, `ResponseRequest`, `PaginatedRequest`;
- response types: `ApiResponse`, `DecodedResponse`, `ByteStream`, `NoContent`;
- pagination `PageLimit`;
- records types under their features;
- common errors and `DebugLevel`.

Generated modules may also provide their own `prelude` containing endpoint capability traits required for method syntax.

### `concord_core::advanced`

Use advanced for implementation contracts and runtime configuration:

- HTTP types and `Url`;
- auth placements, providers, refs, handles, errors;
- body/codec/specification traits;
- multipart recipes/factories;
- provider HTTP;
- pagination controllers;
- limiter/governor/observer contracts;
- hooks and events;
- prepared endpoints;
- response modes and heads;
- runtime and HTTP config builders;
- redaction wrappers.

### `concord_core::dangerous`

Available only under dangerous features:

- `RawResponse`/`RawResponseRequest`;
- Reqwest retry builder alias.

### Rule of thumb

If application code only calls a generated client, start with the prelude. Import advanced types narrowly on pages where you implement a trait or configure the runtime. Do not import `__private` or `__development`; those are not supported extension APIs.

## Generated API reference

An `api!` declaration generates a module containing a client, builder, scope types, endpoint types, facade methods, conditional request terminals, and generated Rustdoc.

### Client and builder

A generated client normally provides:

- `Client::builder()` and `ClientBuilder::new()`;
- `Client::new(...)` when all mandatory values can be supplied positionally;
- named setters for required/optional client values, protected secrets, and custom providers;
- `.http(...)` and `.runtime(...)` configuration closures;
- `.build() -> Result<Client, ClientBuildError>`;
- facade methods for scopes and endpoints.

Required positional constructor order is public values, protected secrets, then custom providers. Prefer the builder when optional configuration is involved.

### Scope values

A scope method constructs an immutable scope snapshot. Nested scopes and endpoint values are normal typed Rust values. Cloning a client shares its immutable runtime and state; constructing another client creates independent defaults.

### Endpoint types

Each endpoint declaration produces a concrete endpoint value containing its typed request-local state. A facade method usually creates the endpoint and immediately returns `Request<E>`. The endpoint module also supports explicit endpoint construction where documented.

### Conditional terminals

Method availability depends on endpoint capabilities:

| Capability | Generated terminal |
|---|---|
| Any endpoint | direct `.await` |
| Any semantic output | `.response().await` |
| Buffered page output with pagination declaration | `.paginate(limit).await` |
| Eligible bounded successful response under feature | `.raw_response().await` |

Streaming, record, no-content, prepared, and non-paginated endpoints do not gain unsupported terminals merely because similarly named types exist.

### Body and response typing

For `body: B`, generated code requires `B: RequestBodySpec` and exposes `B::Input` as the final body parameter.

For response `R`, generated code requires `R: ResponseBodySpec` and direct await returns `R::Output`.

The macro is type-neutral. It does not recognize application specifications or built-in types by semantic name.

### Pagination integration

Generated pagination owns:

- the associated controller type;
- complete initial controller construction;
- live field binding into a fresh endpoint clone;
- page item capability.

Core owns the page loop. Generated code does not execute pages independently.

### Rate-limit integration

Generated lexical declarations produce an immutable ordered plan:

```text
client rule
outer scope rules
inner scope rules
endpoint rule
```

Each normal generated rule uses a deterministic declaration identity and a global key. Runtime values are not formatted into generated keys.

### Generated Rustdoc

Generated docs describe constructors, endpoint parameters, body/response types, pagination capability, lexical rate limits, feature requirements, and method availability using stable public paths. Private bridge names must not appear.

### Compatibility

Generated code and core currently share private contract version 14. Users should keep `concord_core` and `concord_macros` versions aligned.

## Request API reference

All generated, explicit generated, and prepared endpoints enter the public `Request<E>` family.

### `Request<E>`

A request owns:

- the client runtime snapshot;
- typed endpoint state;
- request-local timeout/debug choices;
- the endpoint body input where applicable.

Awaiting it returns the semantic response output:

```rust
let value: T = request.await?;
```

Construction is lazy. Creating or cloning endpoint values does not perform network I/O.

### `.response()`

`request.response()` returns `ResponseRequest<E>`. Awaiting it returns `ApiResponse<T>` with logical URL, status, protocol version, public response headers, execution metadata, and the semantic value.

The response wrapper is structural: its `Debug` output does not format `T` or header values.

### Pagination

Eligible generated endpoints return `PaginatedRequest<E>` from `.paginate(limit)`. Awaiting it returns `Vec<Item>` using sequential all-or-error collection.

Pagination is unavailable for:

- `Bytes` identity responses;
- no-content responses;
- byte streams;
- record streams;
- prepared endpoints;
- endpoints without a pagination declaration.

### Raw successful response

Under `dangerous-raw-response`, eligible ordinary requests expose `.raw_response()`. It returns bounded complete successful bytes and metadata, never the native Reqwest response. Non-success statuses remain ordinary typed errors.

### Request-local options

Generated request surfaces may expose timeout, timeout disabling where approved, and debug-level selection. Request-local configuration cannot replace the client’s HTTP clients, hooks, limiter, finite limits, or provider channel.

### Execution guarantees

Every request uses:

- one operation identity;
- one fixed deadline;
- one managed HTTP path;
- finite body and response limits;
- private authentication materialization;
- one bounded authentication recovery;
- one completion outcome.

Dropping the future cancels by ownership. Completed native effects are not rolled back.

### What request APIs cannot do

A request cannot expose or accept a native Reqwest request, install a per-request transport, replace hooks or the limiter, opt out of an inherited lexical rate limit, disable ordinary finite limits, or return a partial pagination result.

## Response API reference

Concord separates successful metadata, semantic output, and body acquisition.

### `ApiResponse<T>`

`.response().await` returns `ApiResponse<T>`.

Accessors provide:

- logical final URL;
- status code;
- HTTP version;
- public response headers;
- `ExecutionMeta`;
- `value()` and `into_value()`.

`DecodedResponse<T>` is a compatibility alias to the same implementation, not a second wrapper.

### `ExecutionMeta`

Execution metadata describes the Concord operation, such as endpoint identity and request-count information. It does not expose native request/response objects, credentials, response bodies, rate-limit keys, or private execution plans.

### Response specifications

`ResponseBodySpec` defines:

- semantic `Output`;
- one sealed acquisition `Mode`;
- default `Accept` policy;
- response-head validation;
- conversion from the core-owned acquisition body.

Core executes status classification and finite preflight before extension conversion.

### Sealed modes

#### `Buffered`

Core collects complete bounded `Bytes`. The extension never receives partial buffered output.

#### `Streaming`

Core returns a bounded lazy `ByteStream`. Validation happens before the first body poll. Completion is fixed as `StreamOpened`; later read errors are stream items.

#### `Discard`

Core does not poll the body and supplies `()` to the response specification. `NoContent` uses this mode and accepts successful 2xx statuses.

Applications may select these modes but cannot implement `ResponseMode` or add a native/unbounded mode.

### `ResponseHead<'_>`

A custom response specification can inspect the successful semantic head:

- endpoint and method;
- logical URL;
- status and version;
- public headers;
- content length;
- execution metadata.

It does not receive the native Reqwest response.

### Errors

`ResponseBodyError` and `ResponseBodyErrorKind` let extensions report safe categories. Concord-provided formatting does not reproduce application body values or parser-authored unsafe text.

### Streams and records

- `ByteStream` yields bounded `Result<Bytes, ApiError>` chunks.
- `RecordStream<D>` layers bounded framing over `ByteStream` and yields `Result<D::Record, RecordError>`.
- `RecordBatches<D>` groups already decoded records; batching is not another response acquisition mode.

### What responses cannot do

A response specification cannot bypass status handling, raise the configured body limit, define another native acquisition mode, replace completion, extend the operation deadline, or expose native response ownership.

## Prepared endpoint API reference

Prepared endpoints define an endpoint at runtime while preserving Concord’s normal execution lifecycle.

### Construction

Start with:

```rust
use concord_core::advanced::PreparedEndpoint;
use http::Method;

let endpoint = PreparedEndpoint::builder("health-check", Method::GET)
    .path_segment("health")
    .response_no_content()
    .build()?;
```

The builder defaults to a no-content response specification.

### Request shaping methods

Prepared builders support typed methods equivalent to:

```text
path_segment
query
query_optional
query_values
header
```

Path segments are encoded as one segment. Values such as `/`, `..`, `?`, `#`, and `%` cannot become route traversal or inject query/fragment syntax through `path_segment`.

Headers replace case-insensitively. Repeated query values preserve iterator order.

### Bodies

Generic selection:

```rust
.body_spec::<B>(input)
```

Conveniences delegate to the same managed body path:

```rust
body(RequestBody)
body_codec::<C>(input)
```

Prepared bodies can use bytes, buffered codecs, streams, multipart recipes, factories, or downstream `RequestBodySpec` implementations.

### Responses

Generic selection:

```rust
.response::<R>()
```

Conveniences include bytes, no-content, and stream selections. Arbitrary downstream `ResponseBodySpec` implementations work without a private import.

### Authentication, timeout, and limiting

Prepared builders can select:

- `auth(...)` placements;
- a timeout or timeout disabling where permitted;
- explicit `RateLimitRule` values.

Rate-limit rules compose in call order. Advanced keys may be global or contain bounded explicit parts. Oversized keys fail before send and remain redacted from automatic diagnostics.

### Execution

A generated client accepts a compatible prepared endpoint through its normal request method. The request then uses the same:

- body lifecycle;
- authentication and recovery;
- limiter acquisition and observation;
- hooks and debug;
- managed HTTP clients;
- status classification;
- response acquisition;
- deadlines, cancellation, and completion.

### Reuse

Prepared endpoint values are immutable recipes. Reuse depends on the selected body source:

- bytes and reconstructible factories can be reused;
- direct one-shot streams remain one-shot;
- core derives replayability from the actual managed body form.

### Unsupported behavior

Prepared endpoints do not support pagination, raw path fragments, native request/response access, custom transports, limit bypass, or arbitrary generated-facade mutation.

## Public API index

This page maps the stable public namespaces to the manual. Rustdoc remains the source of truth for exact signatures and trait bounds.

### `concord_core::prelude`

| Export family | Types | Manual |
|---|---|---|
| Request values | `Request`, `ResponseRequest`, `PaginatedRequest`, `PageLimit` | [Request API](api.md#request-api-reference), [Pagination](../basic/pagination-and-rate-limiting.md#pagination) |
| Successful responses | `ApiResponse`, `DecodedResponse`, `ByteStream`, `NoContent` | [Response API](api.md#response-api-reference) |
| Body specifications | `RequestBody`, `Stream`, `StreamBody`, optional `Multipart` | [Request bodies](../basic/bodies-and-responses.md#request-bodies) |
| Buffered specifications | `Json`, `Text`, `Bytes`, `OctetStream` | [Bodies and responses](dsl.md#dsl-bodies-and-responses) |
| Protected materials | `SecretString`, `AccessToken`, `ApiKey`, `BasicCredential` | [Authentication](../basic/authentication.md#authentication) |
| Records | `Records`, `RecordStream`, `RecordBatches`, optional `Csv`, `NdJson` | [Records](../basic/streaming-records-and-multipart.md#records-csv-and-ndjson) |
| Errors and debug | `ApiError`, build/configuration errors, `ErrorCategory`, `DebugLevel` | [Error reference](features-limits-and-errors.md#error-reference) |

### `concord_core::advanced`

#### Authentication

`Auth`, `ChallengePolicy`, `CredentialHandle`, `CredentialMaterial`, `CredentialProvider`, `CredentialReason`, `CredentialRef`, `CredentialRequest`, `RecoveryMode`, and `SecretCredential` support custom provider and mutation workflows.

See [Custom credential providers](../advanced/authentication-hooks-and-debug.md#custom-credential-providers).

#### Bodies and codecs

`RequestBodySpec`, `RequestBodyContext`, `RequestBodyMetadata`, `RequestContentType`, `LengthHint`, `BodyKind`, `BodyFactory`, `EncodeBody`, `DecodeBody`, codec contexts/errors, `MediaType`, and multipart recipes/factories define managed body extensions.

See [Request body specifications](../advanced/endpoints-bodies-and-codecs.md#request-body-specifications), [Response body specifications](../advanced/responses-records-and-streams.md#response-body-specifications), and [Custom codecs](../advanced/endpoints-bodies-and-codecs.md#custom-codecs).

#### Responses

`ResponseBodySpec`, sealed `ResponseMode`, `Buffered`, `Streaming`, `Discard`, `ResponseHead`, `ResponseBodyError`, `ExecutionMeta`, and `PageResponseSpec` define semantic response extensions without exposing native responses.

See [Response API](api.md#response-api-reference).

#### HTTP and runtime configuration

`HttpConfigBuilder`, `RuntimeConfigBuilder`, `SafeProxy`, and `ProviderRetry` configure managed clients. `SafeProxy` accepts a validated proxy origin; it is not a general proxy callback.

See [Client configuration](../basic/dsl-and-requests.md#client-configuration).

#### Provider HTTP

`ProviderHttp`, `ProviderRequest`, `ProviderResponse`, `ProviderBody`, and `ProviderBodyFactory` expose the bounded credential-provider channel.

See [Provider HTTP](../advanced/authentication-hooks-and-debug.md#provider-http).

#### Pagination

`PaginationController`, `PaginationError`, `OffsetPagination`, `PagePagination`, `CursorPagination`, and `HasNextCursor` define typed page state.

See [Custom pagination controllers](../advanced/pagination-and-rate-limits.md#custom-pagination-controllers).

#### Rate limiting

`RateLimitId`, `RateLimitKeyPart`, `RateLimitKey`, `RateLimitWindow`, `RateLimitRule`, `RateLimitPlan`, `RateLimitRequest`, `RateLimitResponse`, `RateLimitResponseHead`, `RateLimitPermit`, `RateLimiter`, `RateLimitObserver`, `DefaultRateLimitObserver`, `NoopRateLimiter`, optional `GovernorRateLimiter`, `Cooldown`, and `CooldownScope` define advanced quota integration.

See [Custom rate limiters](../advanced/pagination-and-rate-limits.md#custom-rate-limiters) and [Rate-limit observers](../advanced/pagination-and-rate-limits.md#rate-limit-observers).

#### Hooks and redaction

`RuntimeHooks`, hook events, completion events/outcomes, sanitized header wrappers, and `OperationId` support safe observation.

See [Runtime hooks](../advanced/authentication-hooks-and-debug.md#runtime-hooks) and [Debug and redaction](../advanced/authentication-hooks-and-debug.md#debug-and-redaction).

#### Prepared endpoints

`PreparedEndpoint` and `PreparedEndpointBuilder` create runtime-defined endpoints that still use the managed runtime.

See [Prepared API](api.md#prepared-endpoint-api-reference).

#### Records

`RecordDecoder`, `RecordStream`, `RecordBatches`, `RecordError`, and optional `Csv`/`NdJson` define bounded record framing.

See [Custom record decoders](../advanced/responses-records-and-streams.md#custom-record-decoders).

### `concord_core::dangerous`

- `RawResponse` and `RawResponseRequest` under `dangerous-raw-response`.
- `ReqwestRetryBuilder` under `dangerous-custom-retry`.

These types reduce ordinary safety guarantees and are never default features. See the dedicated dangerous pages before enabling them.

### Not public extension APIs

Do not depend on `concord_core::__private`, `concord_core::__development`, generated seals, endpoint-spec bridges, native execution plans, or internal response inputs. They may change without public compatibility guarantees.

## Complete public export checklist

This compact list prevents extension families from disappearing from the manual. Exact signatures, feature gates, and generic bounds remain in Rustdoc.

### `concord_core::prelude`

`AccessToken`, `ApiError`, `ApiKey`, `ApiResponse`, `BasicCredential`, `ByteStream`, `Bytes`, `ClientBuildError`, `ConfigurationError`, `Csv`, `DebugLevel`, `DecodedResponse`, `ErrorCategory`, `Json`, `Multipart`, `NdJson`, `NoContent`, `OctetStream`, `PageLimit`, `PaginatedRequest`, `RecordBatches`, `RecordError`, `RecordStream`, `Records`, `Request`, `RequestBody`, `ResponseRequest`, `SecretString`, `Stream`, `StreamBody`, `Text`.

### `concord_core::advanced`

`ApiError`, `ApiResponse`, `Auth`, `AuthError`, `AuthErrorKind`, `BeforeSend`, `BodyError`, `BodyErrorKind`, `BodyFactory`, `BodyKind`, `BoxFuture`, `Buffered`, `ByteStream`, `Bytes`, `BytesMut`, `ChallengePolicy`, `ClientBuildError`, `ClientBuildErrorKind`, `CodecError`, `CompletionEvent`, `CompletionOutcome`, `ConfigurationError`, `ConfigurationErrorKind`, `Cooldown`, `CooldownScope`, `CredentialHandle`, `CredentialMaterial`, `CredentialProvider`, `CredentialReason`, `CredentialRef`, `CredentialRequest`, `Csv`, `CursorPagination`, `DebugLevel`, `DecodeBody`, `DecodeContext`, `DecodedResponse`, `DefaultRateLimitObserver`, `Discard`, `EncodeBody`, `EncodeContext`, `ErrorCategory`, `ExecutionMeta`, `GovernorRateLimiter`, `HasNextCursor`, `HeaderMap`, `HeaderName`, `HeaderValue`, `HookError`, `HttpConfigBuilder`, `HttpResponseFailure`, `Json`, `LengthHint`, `MediaType`, `Method`, `Multipart`, `MultipartBody`, `MultipartFactory`, `MultipartPart`, `NdJson`, `NoContent`, `NoopRateLimiter`, `OctetStream`, `OffsetPagination`, `OperationId`, `PagePagination`, `PageResponseSpec`, `PaginationCompletionEvent`, `PaginationController`, `PaginationError`, `PaginationErrorKind`, `PreparedEndpoint`, `PreparedEndpointBuilder`, `ProviderBody`, `ProviderBodyFactory`, `ProviderHttp`, `ProviderRequest`, `ProviderResponse`, `ProviderRetry`, `RateLimitError`, `RateLimitErrorKind`, `RateLimitId`, `RateLimitKey`, `RateLimitKeyPart`, `RateLimitObserver`, `RateLimitPermit`, `RateLimitPlan`, `RateLimitRequest`, `RateLimitResponse`, `RateLimitResponseHead`, `RateLimitRule`, `RateLimitWindow`, `RateLimiter`, `RecordBatches`, `RecordDecoder`, `RecordError`, `RecordErrorKind`, `RecordStream`, `Records`, `RecoveryMode`, `RequestBody`, `RequestBodyContext`, `RequestBodyMetadata`, `RequestBodySpec`, `RequestContentType`, `ResponseBodyError`, `ResponseBodyErrorKind`, `ResponseBodySpec`, `ResponseHead`, `ResponseHeadEvent`, `ResponseMode`, `RuntimeConfigBuilder`, `RuntimeHooks`, `SafeProxy`, `SanitizedHeaderMap`, `SanitizedHeaderValue`, `SanitizedHeaders`, `SecretCredential`, `StatusBody`, `StatusBodyDecodeError`, `StatusBodyState`, `StatusCode`, `Stream`, `StreamBody`, `Streaming`, `Text`, `Url`, `Version`.

### `concord_core::dangerous`

`RawResponse`, `RawResponseRequest`, `ReqwestRetryBuilder`.

## Complete public export index

The following names are exported through `concord_core::prelude`, `concord_core::advanced`, or the feature-gated `concord_core::dangerous` module. Exact availability and bounds remain feature- and Rustdoc-defined.

- `AccessToken`
- `ApiError`
- `ApiKey`
- `ApiResponse`
- `Auth`
- `AuthError`
- `AuthErrorKind`
- `BasicCredential`
- `BeforeSend`
- `BodyError`
- `BodyErrorKind`
- `BodyFactory`
- `BodyKind`
- `BoxFuture`
- `Buffered`
- `ByteStream`
- `Bytes`
- `BytesMut`
- `ChallengePolicy`
- `ClientBuildError`
- `ClientBuildErrorKind`
- `CodecError`
- `CompletionEvent`
- `CompletionOutcome`
- `ConfigurationError`
- `ConfigurationErrorKind`
- `Cooldown`
- `CooldownScope`
- `CredentialHandle`
- `CredentialMaterial`
- `CredentialProvider`
- `CredentialReason`
- `CredentialRef`
- `CredentialRequest`
- `Csv`
- `CursorPagination`
- `DebugLevel`
- `DecodeBody`
- `DecodeContext`
- `DecodedResponse`
- `DefaultRateLimitObserver`
- `Discard`
- `EncodeBody`
- `EncodeContext`
- `ErrorCategory`
- `ExecutionMeta`
- `GovernorRateLimiter`
- `HasNextCursor`
- `HeaderMap`
- `HeaderName`
- `HeaderValue`
- `HookError`
- `HttpConfigBuilder`
- `HttpResponseFailure`
- `Json`
- `LengthHint`
- `MediaType`
- `Method`
- `Multipart`
- `MultipartBody`
- `MultipartFactory`
- `MultipartPart`
- `NdJson`
- `NoContent`
- `NoopRateLimiter`
- `OctetStream`
- `OffsetPagination`
- `OperationId`
- `PageLimit`
- `PagePagination`
- `PageResponseSpec`
- `PaginatedRequest`
- `PaginationCompletionEvent`
- `PaginationController`
- `PaginationError`
- `PaginationErrorKind`
- `PreparedEndpoint`
- `PreparedEndpointBuilder`
- `ProviderBody`
- `ProviderBodyFactory`
- `ProviderHttp`
- `ProviderRequest`
- `ProviderResponse`
- `ProviderRetry`
- `RateLimitError`
- `RateLimitErrorKind`
- `RateLimitId`
- `RateLimitKey`
- `RateLimitKeyPart`
- `RateLimitObserver`
- `RateLimitPermit`
- `RateLimitPlan`
- `RateLimitRequest`
- `RateLimitResponse`
- `RateLimitResponseHead`
- `RateLimitRule`
- `RateLimitWindow`
- `RateLimiter`
- `RawResponse`
- `RawResponseRequest`
- `RecordBatches`
- `RecordDecoder`
- `RecordError`
- `RecordErrorKind`
- `RecordStream`
- `Records`
- `RecoveryMode`
- `Request`
- `RequestBody`
- `RequestBodyContext`
- `RequestBodyMetadata`
- `RequestBodySpec`
- `RequestContentType`
- `ReqwestRetryBuilder`
- `ResponseBodyError`
- `ResponseBodyErrorKind`
- `ResponseBodySpec`
- `ResponseHead`
- `ResponseHeadEvent`
- `ResponseMode`
- `ResponseRequest`
- `RuntimeConfigBuilder`
- `RuntimeHooks`
- `SafeProxy`
- `SanitizedHeaderMap`
- `SanitizedHeaderValue`
- `SanitizedHeaders`
- `SecretCredential`
- `SecretString`
- `StatusBody`
- `StatusBodyDecodeError`
- `StatusBodyState`
- `StatusCode`
- `Stream`
- `StreamBody`
- `Streaming`
- `Text`
- `Url`
- `Version`
