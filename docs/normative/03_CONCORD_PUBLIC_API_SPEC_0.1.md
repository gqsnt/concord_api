# CONCORD_PUBLIC_API_SPEC_0.1

**Version:** 0.1  
**Status:** Normative Implementation Candidate
**Authority:** Stable hand-written public Rust API  
**Requirement prefix:** `API`  
**Depends on:** Specification Index, Product Contract  
**Supersedes:** None

## 1. Scope

This document defines the supported non-generated public Rust API of `concord_core`.

Signatures are normative at the semantic level. Minor implementation details such as private fields, private helper aliases, and internal enum representation are not public compatibility.

## 2. Public namespaces and stability

### 2.1 `concord_core::prelude`

Normal stable application API.

The prelude re-exports at least:

```text
ApiError
ErrorCategory
ClientBuildError
ConfigurationError
Json
Text
NoContent
Bytes
Stream
StreamBody
ByteStream
ApiResponse
DecodedResponse
Records
RecordStream
RecordBatches
Csv
NdJson
Request
ResponseRequest
PageLimit
DebugLevel
SecretString
AccessToken
ApiKey
BasicCredential
OctetStream
```

Feature-gated entries are present only when their feature is enabled.

### 2.2 `concord_core::advanced`

Stable advanced extension contracts.

This namespace re-exports the contracts defined in this document, including:

- codec and media traits;
- request-body specification context, metadata, trait, managed bodies, and multipart types;
- response-head, response-specification, sealed-mode, byte-stream, and successful-metadata types;
- feature-gated record decoder, stream, batching, CSV, and NDJSON contracts;
- prepared endpoint builders;
- credential materials, providers, references, and endpoint-backed handles;
- provider HTTP;
- pagination controllers;
- rate limiters and observers;
- runtime hooks and sanitized metadata;
- safe HTTP and runtime configuration.

It also re-exports the common wire types used in those signatures:

```rust
pub use bytes::{Bytes, BytesMut};
pub use http::{
    HeaderMap,
    HeaderName,
    HeaderValue,
    Method,
    StatusCode,
    Version,
};
pub use url::Url;
```

Application code may implement and depend on these contracts subject to documented lifecycle, safety, and boundedness obligations.

### 2.3 `concord_core::dangerous`

Feature-gated stable APIs with reduced safety guarantees.

It contains only the dangerous APIs explicitly defined in Section 19.

### 2.4 `concord_core::__private`

Generated-code integration only.

Its contents are not semver-stable for application code. A generated/core compatibility marker protects generated code compiled against the matching core contract.

### 2.5 `concord_core::__development`

Feature-gated unstable deterministic test infrastructure.

It is not a production extension API.

**API-NS-001 — Namespace containment.** Stable advanced types MUST be exported from `advanced` rather than requiring application imports from `__private`.

**API-NS-002 — Private instability.** Application use of `__private` or `__development` is outside normal compatibility guarantees.

## 3. Common future alias

The advanced API may expose:

```rust
pub type BoxFuture<'a, T> =
    Pin<Box<dyn Future<Output = T> + Send + 'a>>;
```

Equivalent named future wrapper types are permitted if they provide the same lifetime and `Send` behavior.

### 3.1 Stable pending-request wrappers

The stable opaque request wrappers have these exact crate-root paths and are also re-exported from
`concord_core::prelude`:

```rust
concord_core::Request<E>
concord_core::ResponseRequest<E>
```

Their fields are private. They are application-facing movable values, not transport, executor, or
application-implementable extension traits. Generated-private endpoint integration may determine
which endpoint types are awaitable, but generated public signatures and public Rustdoc do not
display that integration boundary.

```rust
pub struct Request<E> { /* private */ }
pub struct ResponseRequest<E> { /* private */ }

impl<E> Request<E> {
    pub fn timeout(
        self,
        duration: Duration,
    ) -> Self;

    pub fn disable_timeout(
        self,
    ) -> Self;

    pub fn inherit_timeout(
        self,
    ) -> Self;

    pub fn debug(
        self,
        level: DebugLevel,
    ) -> Self;

    pub fn response(
        self,
    ) -> ResponseRequest<E>;
}
```

`response()` is available only for response modes that support the metadata terminal. `Request<E>`
and `ResponseRequest<E>` implement `IntoFuture` for valid generated endpoints. Their associated
future type is a publicly nameable `BoxFuture` or an equivalent stable named public wrapper; it does
not expose a private future type, runtime `Any`, boxed decoded value, or public executor trait.

**API-REQ-001 — Stable request-wrapper paths.** The opaque application request wrappers MUST be available as `concord_core::Request<E>` and `concord_core::ResponseRequest<E>` and MUST also be re-exported from `concord_core::prelude`.

**API-REQ-002 — Private integration containment.** Public request-wrapper signatures, associated future types, generated public signatures, and generated public Rustdoc MUST NOT expose generated-private endpoint bounds, native request/response types, execution state, or a transport/executor abstraction.

## 4. Errors

### 4.1 `ApiError`

```rust
pub struct ApiError { /* private */ }
```

Required methods:

```rust
impl ApiError {
    pub fn category(&self) -> ErrorCategory;
    pub fn configuration_kind(&self)
        -> Option<ConfigurationErrorKind>;
    pub fn endpoint(&self) -> &'static str;
    pub fn method(&self) -> &http::Method;

    pub fn http_status(&self)
        -> Option<http::StatusCode>;

    pub fn http_headers(&self)
        -> Option<&SanitizedHeaderMap>;

    pub fn http_response(&self)
        -> Option<&HttpResponseFailure>;

    pub fn into_http_response(
        self,
    ) -> Result<HttpResponseFailure, Self>;

    pub fn auth_kind(&self)
        -> Option<AuthErrorKind>;

    pub fn rate_limit_kind(&self)
        -> Option<RateLimitErrorKind>;

    pub fn pagination_kind(&self)
        -> Option<PaginationErrorKind>;

    pub fn retry_after(&self)
        -> Option<Duration>;
}
```

`ApiError` implements:

```text
std::fmt::Display
std::fmt::Debug
std::error::Error
Send
Sync
'static
```

For a terminal HTTP status carrying a valid `Retry-After`, `retry_after()` returns the parsed non-negative delay clamped to the configured rate-limit cooldown cap. It accepts decimal delta-seconds and HTTP dates. Malformed values return `None`; past dates return `Some(Duration::ZERO)`. This accessor reports header semantics independently of custom limiter observation.

The actual source chain is safe and may be absent. A configuration-category failure exposes a
`ConfigurationErrorKind` when a stable specific classification is available. In particular,
authenticated cleartext public preflight is `Configuration` with `InvalidAuthentication`.

### 4.2 Terminal HTTP response

A terminal non-success application response is represented by an opaque bounded response value:

```rust
pub struct HttpResponseFailure { /* private */ }

impl HttpResponseFailure {
    pub fn endpoint(&self) -> &'static str;
    pub fn method(&self) -> &Method;
    pub fn status(&self) -> StatusCode;
    pub fn headers(&self) -> &SanitizedHeaderMap;
    pub fn body(&self) -> &StatusBody;
    pub fn into_body(self) -> StatusBody;
    pub fn retry_after(&self) -> Option<Duration>;

    pub fn decode_body<C>(
        &self,
    ) -> Result<C::Output, StatusBodyDecodeError>
    where
        C: DecodeBody;
}
```

`HttpResponseFailure` privately retains the endpoint identity, method, and parsed response content type needed by `DecodeContext`. `decode_body` is available only when the body state is `Complete`. It invokes the existing response codec with the retained endpoint, method, status, and content-type context. A decode failure contains no body fragment.

```rust
pub struct StatusBody { /* private */ }

impl StatusBody {
    pub fn state(&self) -> StatusBodyState;
    pub fn len(&self) -> Option<usize>;
    pub fn bytes(&self) -> Option<&Bytes>;
    pub fn into_bytes(self) -> Option<Bytes>;
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusBodyState {
    Disabled,
    Empty,
    Complete,
    OmittedKnownTooLarge,
    OmittedLimitExceeded,
    UnavailableReadFailure,
    UnavailableDeadline,
}

pub struct StatusBodyDecodeError { /* private */ }
```

`bytes()` and `into_bytes()` return data only for `Complete`. Concord never exposes a truncated prefix. `StatusBody`, `HttpResponseFailure`, and `StatusBodyDecodeError` use structural `Debug` and body-free `Display`; `StatusBody` does not implement `Clone`, `Deref`, `AsRef<[u8]>`, or serialization through a Concord-provided implementation.

The status response is available for `HttpStatus` and for a terminal authentication rejection backed by a final application HTTP response. It is absent when no response head was received.

**API-ERR-004 — Explicit bounded status body.** A terminal non-success body MUST be available only through `HttpResponseFailure::body` and only when finite complete capture succeeds.

**API-ERR-005 — Complete-only exposure.** `StatusBody` MUST NOT expose a truncated prefix or automatically format retained bytes.

**API-ERR-006 — Status preservation.** Failure, deadline expiry, or limit exhaustion while capturing a terminal body MUST preserve the known HTTP status and represent capture state through `StatusBodyState`.

**API-ERR-007 — Explicit codec decode.** `HttpResponseFailure::decode_body` MUST decode only a complete retained body and MUST return a body-free error for every other state.

**API-ERR-008 — Owned status-response extraction.** `ApiError::into_http_response` MUST preserve and return the complete `HttpResponseFailure` when the error contains one, and `HttpResponseFailure::into_body` MUST provide the only Concord-defined owning path to `StatusBody::into_bytes`.

### 4.3 `ClientBuildError`

Construction returns:

```rust
pub struct ClientBuildError { /* private */ }
```

Required methods:

```rust
impl ClientBuildError {
    pub fn kind(&self) -> ClientBuildErrorKind;
}
```

It implements safe `Display`, `Debug`, and `Error`.

### 4.4 `ConfigurationError`

Reusable public configuration validation returns:

```rust
pub struct ConfigurationError { /* private */ }

impl ConfigurationError {
    pub fn kind(&self) -> ConfigurationErrorKind;
}
```

It is used by client-construction validation, safe proxy parsing, prepared endpoint construction, and other non-request configuration builders.

It implements safe `Display`, `Debug`, and `Error`.

### 4.5 Error categories

```rust
#[non_exhaustive]
pub enum ErrorCategory {
    Configuration,

    RequestEncoding,
    RequestBody,

    AuthenticationUnavailable,
    AuthenticationProvider,
    AuthenticationRejected,

    RateLimit,
    Hook,

    Timeout,
    Connect,
    Transport,

    HttpStatus,

    ResponseBody,
    ResponseDecoding,
    ResponseContract,

    Pagination,

    Internal,
}
```

### 4.6 Subsystem kinds

Public non-exhaustive enums include:

```rust
pub enum AuthErrorKind { /* documented variants */ }
pub enum RateLimitErrorKind { /* documented variants */ }
pub enum PaginationErrorKind { /* documented variants */ }
pub enum ClientBuildErrorKind { /* documented variants */ }
pub enum ConfigurationErrorKind { /* documented variants */ }
```

Required 0.1 variants include at least:

```rust
#[non_exhaustive]
pub enum AuthErrorKind {
    Unavailable,
    Acquisition,
    Refresh,
    Rejected,
    InvalidConfiguration,
    RequestTooLarge,
    ResponseTooLarge,
    Internal,
}

#[non_exhaustive]
pub enum RateLimitErrorKind {
    InvalidConfiguration,
    InvalidKey,
    Acquire,
    Internal,
}

#[non_exhaustive]
pub enum PaginationErrorKind {
    InvalidLimit,
    Controller,
    Overflow,
    RepeatedRequest,
    HardCapExceeded,
    PageCeilingExceeded,
    CollectedItemCeilingExceeded,
    IdentityStorageCeilingExceeded,
}

#[non_exhaustive]
pub enum ClientBuildErrorKind {
    MissingInput,
    InvalidBaseUrl,
    TlsUnavailable,
    HttpConfiguration,
    RuntimeConfiguration,
    LimiterUnavailable,
    Internal,
}

#[non_exhaustive]
pub enum ConfigurationErrorKind {
    InvalidValue,
    InvalidRoute,
    InvalidHeader,
    InvalidAuthentication,
    InvalidRateLimit,
    InvalidProxy,
    Internal,
}
```

Additional non-exhaustive variants may be added compatibly.

**API-ERR-001 — Structured classification.** Applications MUST be able to classify errors without parsing `Display` text.

**API-ERR-002 — Diagnostic text instability.** Safe diagnostic wording is not a machine-readable compatibility format.

**API-ERR-003 — Status retry metadata.** A terminal HTTP status with valid `Retry-After` metadata MUST expose its safe parsed and capped delay without changing its `HttpStatus` category or current-response outcome.

## 5. Secrets and credential materials

### 5.1 `SecretString`

```rust
pub struct SecretString { /* private */ }
```

Required behavior:

```rust
impl SecretString {
    pub fn new(value: impl Into<String>) -> Self;
    pub fn expose_secret(&self) -> &str;
}
```

It implements `Clone`, `Send`, and `Sync`.

Its ordinary `Display` and `Debug` are redacted.

### 5.2 `CredentialMaterial`

```rust
pub trait CredentialMaterial:
    Clone + Send + Sync + 'static
{
    fn expires_at(&self)
        -> Option<Instant>
    {
        None
    }
}
```

### 5.3 `SecretCredential`

```rust
pub trait SecretCredential:
    CredentialMaterial
{
    fn expose_secret(&self) -> &str;
}
```

**API-MAT-003 — Secret-safe formatting.** An implementation MUST keep ordinary `Debug` and `Display` for credential materials secret-safe.

### 5.4 Built-in materials

```rust
pub struct AccessToken { /* private */ }
pub struct ApiKey { /* private */ }
pub struct BasicCredential { /* private */ }
```

Required constructors:

```rust
impl AccessToken {
    pub fn new(
        token: impl Into<SecretString>,
    ) -> Self;

    pub fn with_expiration(
        token: impl Into<SecretString>,
        expires_at: Instant,
    ) -> Self;
}

impl ApiKey {
    pub fn new(
        value: impl Into<SecretString>,
    ) -> Self;
}

impl BasicCredential {
    pub fn new(
        username: impl Into<SecretString>,
        password: impl Into<SecretString>,
    ) -> Self;
}
```

Both Basic username and password are treated as protected material.

**API-CREDMAT-001 — Safe formatting.** Built-in and custom credential material MUST NOT reveal secret values through ordinary formatting.

## 6. Credential providers

### 6.1 Recovery mode

```rust
#[non_exhaustive]
pub enum RecoveryMode {
    None,
    InvalidateOnly,
    Refresh,
}
```

### 6.2 Reasons

```rust
#[non_exhaustive]
pub enum CredentialReason {
    Missing,
    Expired,
    ExpiringSoon,
    Rejected,
}
```

### 6.3 `AuthError`

```rust
pub struct AuthError { /* private */ }
```

Constructors:

```rust
impl AuthError {
    pub fn new(
        kind: AuthErrorKind,
    ) -> Self;

    pub fn with_safe_message(
        kind: AuthErrorKind,
        message: impl Into<String>,
    ) -> Self;

    pub fn with_retry_after(
        self,
        duration: Duration,
    ) -> Self;

    pub fn kind(&self) -> AuthErrorKind;
    pub fn retry_after(&self)
        -> Option<Duration>;
}
```

A custom safe message is application-visible and is the provider author's responsibility.

### 6.4 Provider context

```rust
pub struct CredentialRequest<'a> {
    pub http: ProviderHttp<'a>,
    pub reason: CredentialReason,
}
```

The context contains no generic generated client-value bag.

A custom provider captures required configuration in its own type.

### 6.5 Provider trait

```rust
pub trait CredentialProvider:
    Send + Sync + 'static
{
    type Material: CredentialMaterial;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<
        'a,
        Result<Self::Material, AuthError>,
    >;

    fn refresh<'a>(
        &'a self,
        current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<
        'a,
        Result<Self::Material, AuthError>,
    > {
        self.acquire(context)
    }

    fn recovery_mode(&self) -> RecoveryMode {
        RecoveryMode::Refresh
    }
}
```

### 6.6 Provider obligations

Providers:

- may be invoked concurrently for different credential slots;
- may be canceled through future drop;
- must not require a completion callback to release resources;
- must return secret-safe errors;
- must bound external work and retained state;
- must not assume the protected application request will eventually be sent.

**API-PROV-001 — No state lock assumption.** Provider methods MUST be invoked without a Concord credential-state lock held.

**API-PROV-002 — Cancellation tolerance.** Provider futures MUST tolerate being dropped at any await point.

## 7. Credential references and handles

### 7.1 Read-only credential reference

```rust
#[derive(Clone)]
pub struct CredentialRef<M> { /* private */ }
```

A credential reference permits authentication preparation but exposes no material value and no mutation operation.

It is the input accepted by advanced `Auth` constructors.

### 7.2 Mutable endpoint-backed handle

```rust
#[derive(Clone)]
pub struct CredentialHandle<M> { /* private */ }
```

For endpoint-backed credentials:

```rust
impl<M> CredentialHandle<M>
where
    M: CredentialMaterial,
{
    pub fn credential(&self)
        -> CredentialRef<M>;

    pub async fn set(
        &self,
        value: M,
    ) -> Result<(), AuthError>;

    pub async fn clear(
        &self,
    ) -> Result<(), AuthError>;

    pub fn is_set(&self) -> bool;
}
```

`is_set()` is synchronous and immediate: it may take a short internal lock, does not wait on an in-flight provider operation, and does not start provider work. It returns a point-in-time `true` only when a non-expired ready value is currently stored.

### 7.3 Generated source-specific handles

Static and provider-backed generated credentials use source-specific handle types. They expose only source-appropriate replacement/clear operations plus:

```rust
pub fn credential(&self)
    -> CredentialRef<Material>;
```

This permits advanced authentication placement without exposing generic manual `set` for a source that should be reconfigured through generated atomic replacement.

**API-HANDLE-001 — No internal state.** Credential references and handles MUST NOT expose slot state, versions, waiters, provider-operation identifiers, or material values.

**API-HANDLE-002 — Capability separation.** A read-only credential reference MUST NOT provide generic material mutation.

**API-HANDLE-003 — Immediate presence observation.** `is_set()` MUST be synchronous, MUST NOT await provider work, and MUST return only a point-in-time observation.

## 8. Advanced authentication placement

Prepared endpoints use an opaque `Auth` value constructed from typed read-only credential references.

```rust
#[non_exhaustive]
pub enum ChallengePolicy {
    Unauthorized,
    UnauthorizedOrForbidden,
    Never,
}

pub struct Auth { /* private */ }

impl Auth {
    pub fn bearer<M>(
        credential: CredentialRef<M>,
    ) -> Self
    where
        M: SecretCredential;

    pub fn basic(
        credential: CredentialRef<BasicCredential>,
    ) -> Self;

    pub fn header<M>(
        name: HeaderName,
        credential: CredentialRef<M>,
    ) -> Self
    where
        M: SecretCredential;

    pub fn query<M>(
        name: impl Into<String>,
        credential: CredentialRef<M>,
    ) -> Self
    where
        M: SecretCredential;

    pub fn challenge(
        self,
        policy: ChallengePolicy,
    ) -> Self;
}
```

Auth construction does not expose credential values. Collision and placement validation remains part of ordinary request preparation.

Every advanced `Auth` constructor defaults to `ChallengePolicy::Never`. An application that wants automatic rejection or refresh must call `.challenge(...)` explicitly. One prepared endpoint authentication plan may contain at most one `Auth` value whose challenge policy is not `Never`.

Challenge interpretation is:

- `Unauthorized`: `401` is an authentication challenge; `403` follows ordinary status handling;
- `UnauthorizedOrForbidden`: `401` and `403` are authentication challenges;
- `Never`: neither status is classified as an authentication challenge and both follow ordinary status handling.

**API-AUTH-001 — Typed placement.** Advanced authentication placement MUST be constructed from typed credential references and MUST NOT expose material values or mutation.

**API-AUTH-002 — Advanced default never.** Every advanced `Auth` constructor MUST default to `ChallengePolicy::Never`.

**API-AUTH-003 — One challenge-enabled placement.** One prepared endpoint authentication plan MUST contain at most one placement whose challenge policy is not `Never`.

## 9. Provider HTTP

### 9.1 Provider handle

```rust
#[derive(Clone, Copy)]
pub struct ProviderHttp<'a> { /* private */ }
```

### 9.2 Provider request

```rust
pub struct ProviderRequest {
    /* private */
}

```

Builder-style constructors:

```rust
impl ProviderRequest {
    pub fn new(
        method: Method,
        url: Url,
    ) -> Self;

    pub fn header(
        self,
        name: HeaderName,
        value: HeaderValue,
    ) -> Self;

    pub fn body(
        self,
        body: ProviderBody,
    ) -> Self;

    pub fn timeout(
        self,
        duration: Duration,
    ) -> Self;

    pub fn response_limit(
        self,
        bytes: NonZeroUsize,
    ) -> Self;
}
```

### 9.3 Provider body

```rust
pub enum ProviderBody {
    Empty,
    Bytes(Bytes),
    Factory(Arc<dyn ProviderBodyFactory>),
}
```

```rust
pub trait ProviderBodyFactory:
    Send + Sync + 'static
{
    fn create(&self)
        -> Result<Bytes, AuthError>;
}
```

A provider body factory is invoked once per provider HTTP operation before native execution. Its bounded bytes are retained for any Reqwest-internal provider retry; the factory is not invoked per hidden retry.

### 9.4 Provider response

```rust
pub struct ProviderResponse { /* private */ }

impl ProviderResponse {
    pub fn status(&self) -> StatusCode;
    pub fn headers(&self) -> &HeaderMap;
    pub fn body(&self) -> &Bytes;
    pub fn into_body(self) -> Bytes;
}
```

### 9.5 Send

```rust
impl<'a> ProviderHttp<'a> {
    pub async fn send(
        self,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, AuthError>;
}
```

**API-PHTTP-001 — Bounded bodies.** Provider request and response bodies MUST remain subject to mandatory finite limits. A per-request `response_limit` may reduce the client-wide provider response cap but MUST NOT increase or disable it.

**API-PHTTP-002 — Status ownership.** Provider HTTP MUST return non-success statuses to provider logic rather than automatically converting them to application HTTP status errors.

**API-PHTTP-003 — Provider factory lifecycle.** A provider body factory MUST run once per provider HTTP operation, and its bounded result MUST be reused rather than regenerated for Reqwest-internal retries.

**API-PHTTP-004 — HTTPS-only provider request.** `ProviderHttp` MUST reject a `ProviderRequest` whose URL scheme is not HTTPS before body-factory invocation or native execution.

## 10. Codecs and media

### 10.1 `MediaType`

```rust
pub trait MediaType:
    Send + Sync + 'static
{
    const MEDIA_TYPE: &'static str;
}
```

Built-in marker:

```rust
pub struct OctetStream;
```

with media type `application/octet-stream`.

A media marker's value must parse as one valid exact media type. It is a small helper rather than a
universal request/response contract. For `body: Stream<M>`, it supplies exact request
`Content-Type` ownership. For `-> Stream<M>`, it supplies the default `Accept` and exact successful
response validation. Other response specifications may implement suffix, parameter, missing,
vendor, or multi-media policies directly.

Built-in request/response specification types are:

```rust
#[cfg(feature = "json")]
pub struct Json<T>(PhantomData<T>);

pub struct Text;
pub struct NoContent;
pub struct Stream<M>(PhantomData<M>);

#[cfg(feature = "multipart")]
pub struct Multipart;
```

`Json<T>` implements request encoding when `T: serde::Serialize` and response decoding when `T: serde::de::DeserializeOwned`.

Its request content type and default response accept value are `application/json`. Its response validator accepts `application/json` and `application/*+json`, with arbitrary valid media-type parameters.

`Text` implements `EncodeBody<Input = String>` and `DecodeBody<Output = String>`. It encodes and decodes UTF-8, uses request content type `text/plain; charset=utf-8`, uses default accept `text/plain`, and does not reject a successful response solely because the server supplied another or missing media type. Invalid UTF-8 is a response-decoding error.

`NoContent` is a discard-mode response specification and is not a request-body specification or a
general body codec. `Stream<M>` is a real request and response specification type. `Multipart` is a
real request-only specification type.

### 10.2 `CodecError`

```rust
pub struct CodecError { /* private */ }
```

```rust
impl CodecError {
    pub fn new(
        message: impl Into<String>,
    ) -> Self;
}
```

Codec messages are not propagated verbatim through ordinary endpoint errors.

### 10.3 Encode context

```rust
pub struct EncodeContext<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub body_limit: Option<usize>,
}
```

### 10.4 Decode context

```rust
pub struct DecodeContext<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub status: StatusCode,
    pub content_type: Option<&'a str>,
}
```

### 10.5 Request codec

```rust
pub trait EncodeBody:
    Send + Sync + 'static
{
    type Input: Send + 'static;

    const CONTENT_TYPE: Option<&'static str>;

    fn encode(
        input: Self::Input,
        context: EncodeContext<'_>,
    ) -> Result<Bytes, CodecError>;
}
```

### 10.6 Response codec

```rust
pub trait DecodeBody:
    Send + Sync + 'static
{
    type Output: Send + 'static;

    const ACCEPT: Option<&'static str>;

    fn validate_content_type(
        _content_type: Option<&str>,
    ) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(
        body: Bytes,
        context: DecodeContext<'_>,
    ) -> Result<Self::Output, CodecError>;
}
```

**API-CODEC-001 — Bounded decoder input.** `DecodeBody::decode` MUST receive only a complete body acquired through the sealed buffered mode after known-length preflight and bounded collection.

**API-CODEC-002 — No request metadata privilege.** `EncodeBody` and its generic `RequestBodySpec` adapter MUST NOT receive URL, headers, credentials, auth state, limiter state, native request, Reqwest, or transport access and MUST run only from `RequestBodySpec::prepare` after hook approval.

**API-CODEC-003 — Media header ownership.** A response codec adapter's `ACCEPT` value MUST be returned through `ResponseBodySpec::accept` and inserted only when the public request does not already provide `Accept`. A request codec adapter whose `CONTENT_TYPE` is `Some` MUST report exact ownership through `RequestBodyMetadata` and conflicting public or produced-body ownership MUST fail; when `CONTENT_TYPE` is `None`, an explicit public `Content-Type` MAY be used.

**API-CODEC-004 — Stream media contract.** A `Stream<M>` media marker MUST define one valid exact media type; its request specification MUST own that `Content-Type`, and its response specification MUST use it for default `Accept` and successful-head validation before any body poll, while non-stream response specifications MUST remain free to define other direction-specific media policies.

## 11. Request bodies

### 11.1 Body errors and kinds

```rust
pub struct BodyError { /* private */ }

impl BodyError {
    pub fn new(kind: BodyErrorKind) -> Self;

    pub fn with_safe_message(
        kind: BodyErrorKind,
        message: impl Into<String>,
    ) -> Self;

    pub fn kind(&self) -> BodyErrorKind;
}

#[non_exhaustive]
pub enum BodyErrorKind {
    InvalidContentType,
    InvalidLength,
    Producer,
    LimitExceeded,
    Factory,
    Multipart,
    Internal,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyKind {
    Empty,
    Buffered,
    Stream,
    Multipart,
}
```

`BodyError` implements safe `Display`, `Debug`, and `Error` and never exposes body contents or producer-provided unsafe text.

### 11.2 Request specification context and metadata

```rust
pub struct RequestBodyContext<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub body_limit: NonZeroUsize,
}

#[non_exhaustive]
pub enum LengthHint {
    Unknown,
    AtLeast(u64),
    Exact(u64),
}

#[non_exhaustive]
pub enum RequestContentType {
    None,
    Exact(HeaderValue),
    Generated,
}

pub struct RequestBodyMetadata {
    pub length: LengthHint,
    pub content_type: RequestContentType,
    pub kind: BodyKind,
}
```

`RequestBodyMetadata` contains only structural facts available before `before_send`. Computing it
does not encode a codec; invoke a body or multipart factory; poll a stream; open a file; build a
native body; or access URL, headers, credentials, authentication, limiter, native request, Reqwest,
or transport state.

### 11.3 Request body specification

```rust
pub trait RequestBodySpec:
    Send + Sync + 'static
{
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

Core calls `metadata` before the hook and calls `prepare` synchronously exactly once after hook
approval for one pending request execution. It derives replayability only from the produced
`RequestBody`.

### 11.4 `StreamBody`

```rust
pub struct StreamBody { /* private */ }
```

Constructors:

```rust
impl StreamBody {
    pub fn from_bytes(bytes: Bytes) -> Self;

    pub fn from_stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, E>>
            + Send + 'static,
        E: std::error::Error
            + Send + Sync + 'static;

    pub fn from_reader<R>(reader: R) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static;

    pub async fn from_file(
        path: impl AsRef<Path>,
    ) -> Result<Self, BodyError>;

    pub fn exact_length(
        self,
        bytes: u64,
    ) -> Self;
}
```

Producer errors are sanitized by Concord.

### 11.5 Body factories

A general replay factory creates a fresh request stream for each Concord attempt:

```rust
pub trait BodyFactory:
    Send + Sync + 'static
{
    fn content_type(&self)
        -> Option<HeaderValue>
    {
        None
    }

    fn create(
        &self,
    ) -> Result<StreamBody, BodyError>;
}
```

Under `multipart`, a multipart replay factory creates a fresh complete multipart recipe:

```rust
#[cfg(feature = "multipart")]
pub trait MultipartFactory:
    Send + Sync + 'static
{
    fn create(
        &self,
    ) -> Result<MultipartBody, BodyError>;
}
```

Factory methods are synchronous. A factory is invoked once per Concord attempt and never for a Reqwest-internal retry.

### 11.6 `RequestBody`

Advanced prepared endpoints use an opaque request-body value:

```rust
pub struct RequestBody { /* private */ }

impl RequestBody {
    pub fn empty() -> Self;
    pub fn bytes(value: Bytes) -> Self;
    pub fn stream(value: StreamBody) -> Self;

    pub fn factory<F>(factory: F) -> Self
    where
        F: BodyFactory;

    #[cfg(feature = "multipart")]
    pub fn multipart(value: MultipartBody) -> Self;

    #[cfg(feature = "multipart")]
    pub fn multipart_factory<F>(
        factory: F,
    ) -> Self
    where
        F: MultipartFactory;

    pub fn content_type(
        self,
        media_type: impl AsRef<str>,
    ) -> Result<Self, BodyError>;

    pub fn kind(&self) -> BodyKind;
}
```

`RequestBody::content_type` is valid for empty, bytes, stream, and general factory bodies. An explicit value overrides a factory's default `content_type()`. Multipart content type is owned by Reqwest and cannot be overridden.

Replayability is inferred internally:

- empty and bytes are reusable;
- a direct stream is one-shot;
- a general factory is reconstructible;
- multipart replayability follows its parts;
- a multipart factory is reconstructible.

Core validates the pre-hook metadata against the produced `RequestBody` after preparation and
before credentials, limiter acquisition, request debug, authentication materialization, native
request construction, or send. Incompatible `BodyKind`, exact-length contradiction, lower-bound
contradiction, conflicting content-type ownership, invalid `Generated` ownership,
bodyless/body-present contradiction, or an impossible managed-body state fails closed.

There is one generic implementation of `RequestBodySpec` for every `C: EncodeBody`, with
`Input = C::Input`. It reports codec content-type ownership without encoding, invokes `C::encode`
only from `prepare`, and produces one complete buffered `RequestBody`. A custom complete codec
implements only `EncodeBody` and does not implement another request lifecycle.

Built-in request specifications are:

| Specification | Input and behavior |
|---|---|
| `Text` | `String` through the generic codec adapter |
| `Json<T>` | `T` through the generic codec adapter under `json` |
| `Bytes` | reusable exact-length `Bytes` |
| `Stream<M>` | `StreamBody` with exact media type `M` |
| `Multipart` | `MultipartBody` under `multipart` |
| `RequestBody` | the managed value itself, selectable by prepared endpoints |

`NoContent` does not implement `RequestBodySpec`. A bodyless generated endpoint uses a core-private
empty specification and exposes no public request specification parameter.

### 11.7 Multipart

Under `multipart`:

```rust
pub struct MultipartBody { /* private */ }
pub struct MultipartPart { /* private */ }
```

```rust
impl MultipartBody {
    pub fn new() -> Self;

    pub fn part(
        self,
        name: impl Into<String>,
        part: MultipartPart,
    ) -> Result<Self, BodyError>;
}

impl MultipartPart {
    pub fn text(
        value: impl Into<String>,
    ) -> Self;

    pub fn bytes(
        value: Bytes,
    ) -> Self;

    pub fn stream(
        value: StreamBody,
    ) -> Self;

    pub fn filename(
        self,
        filename: impl Into<String>,
    ) -> Result<Self, BodyError>;

    pub fn content_type(
        self,
        media_type: impl Into<String>,
    ) -> Result<Self, BodyError>;
}
```

Text and bytes parts are reusable. A direct stream part makes the direct multipart recipe one-shot. `RequestBody::multipart_factory` makes the complete request reconstructible.

**API-BODY-001 — Internal replayability.** Application code and `RequestBodySpec` implementations MUST NOT manually label a body as replayable; Concord MUST derive replayability only from the produced managed `RequestBody` form.

**API-BODY-002 — Factory metadata and freshness.** `BodyFactory::content_type` and request-specification metadata MUST be deterministic and side-effect-free; every factory `create` call MUST return a fresh complete stream suitable for one Concord attempt, and metadata computation MUST NOT invoke a factory.

**API-BODY-003 — Extension error construction.** Request specifications, body factories, and multipart extensions MUST be able to construct typed, application-safe `BodyError` values without exposing input, body, multipart, filename, or producer-authored unsafe text through Concord formatting or sources.

**API-BODYSPEC-001 — Request body specification contract.** `RequestBodySpec` MUST have the exact associated input and synchronous `metadata`/`prepare` contract defined in this section, MUST be selectable by generated and prepared endpoints, and MUST receive only `RequestBodyContext`.

**API-BODYSPEC-002 — Pre-hook body metadata.** `RequestBodySpec::metadata` MUST run before `before_send` and MUST NOT encode, invoke a body or multipart factory, poll a stream, open a file, build a native body, or access URL, headers, credentials, authentication, limiter, native request, Reqwest, or transport state.

**API-BODYSPEC-003 — Produced-body consistency.** Core MUST validate metadata against the produced `RequestBody` and MUST fail closed on incompatible kind, exact/lower-bound length, content-type ownership, generated ownership, body presence, or impossible managed state before credentials, limiter acquisition, request debug, authentication materialization, native construction, or send.

**API-BODYSPEC-004 — One managed outgoing body.** Every request specification MUST produce the one stable `RequestBody`; Concord MUST add no second public outgoing-body value or parallel request lifecycle, and replayability MUST remain derived only from that produced value.

## 12. Successful responses

### 12.1 `ResponseHead<'a>`

Successful response extensions receive only the logical, core-owned response head:

```rust
pub struct ResponseHead<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub url: &'a Url,
    pub status: StatusCode,
    pub version: Version,
    pub headers: &'a HeaderMap,
    pub content_length: Option<u64>,
    pub metadata: &'a ExecutionMeta,
}
```

The URL is the logical application URL. A selected response extension intentionally receives the
successful response head and owns its own logging and storage behavior, but receives no native
response or transport value.

### 12.2 Response-body errors

```rust
pub struct ResponseBodyError { /* private */ }

#[non_exhaustive]
pub enum ResponseBodyErrorKind {
    InvalidMediaType,
    Decode,
    InvalidOutput,
    Internal,
}

impl ResponseBodyError {
    pub fn new(kind: ResponseBodyErrorKind) -> Self;

    pub fn with_safe_message(
        kind: ResponseBodyErrorKind,
        message: impl Into<String>,
    ) -> Self;

    pub fn kind(&self) -> ResponseBodyErrorKind;
}
```

Concord formatting and recursive sources do not reproduce application text, response data, or
parser-authored unsafe text.

### 12.3 Sealed acquisition modes

```rust
pub struct Buffered;
pub struct Streaming;
pub struct Discard;

pub trait ResponseMode:
    private::Sealed + Send + Sync + 'static
{
    type Body: Send + 'static;
}
```

The public marker types are selectable by application response specifications, while the trait is
sealed so only core can implement a mode. The exact core-owned mode bodies are:

```text
Buffered  -> Bytes
Streaming -> ByteStream
Discard   -> ()
```

No application can create an unbounded or native acquisition mode.

### 12.4 Response body specification

```rust
pub trait ResponseBodySpec:
    Send + Sync + 'static
{
    type Output: Send + 'static;
    type Mode: ResponseMode;

    fn accept() -> Option<&'static str> {
        None
    }

    fn validate(
        head: &ResponseHead<'_>,
    ) -> Result<(), ResponseBodyError> {
        Ok(())
    }

    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError>;
}
```

Successful `2xx` status classification occurs first. Known-length mode preflight occurs before
`validate`; validation occurs before the first body poll; core owns acquisition; and `from_body`
runs only after the selected mode body is ready. Core checks synchronous `validate` and `from_body`
work against the operation deadline immediately after return. A response specification cannot
replace status handling, limits, completion, or native response ownership.

Every `C: DecodeBody` receives one generic `ResponseBodySpec<Mode = Buffered>` implementation whose
output is `C::Output`; its decoder receives complete bounded bytes only. A custom complete buffered
codec therefore implements only `DecodeBody`.

The built-in response specifications are:

- `Text` and feature-gated `Json<T>`, through the buffered codec adapter;
- `Bytes`, returning complete bounded `Bytes`;
- `NoContent`, using `Discard`, accepting every successful `2xx`, and never polling;
- `Stream<M>`, using `Streaming`, validating media before polling, and returning `ByteStream`;
- feature-gated `Records<D>`, using `Streaming` and returning `RecordStream<D>`;
- any application-defined `ResponseBodySpec` selecting one sealed mode.

### 12.5 Universal successful metadata

```rust
pub struct ApiResponse<T> { /* private */ }

impl<T> ApiResponse<T> {
    pub fn url(&self) -> &Url;
    pub fn status(&self) -> StatusCode;
    pub fn version(&self) -> Version;
    pub fn headers(&self) -> &HeaderMap;
    pub fn metadata(&self) -> &ExecutionMeta;
    pub fn value(&self) -> &T;
    pub fn into_value(self) -> T;
}

pub type DecodedResponse<T> = ApiResponse<T>;
```

`ApiResponse<T>` is the single successful metadata implementation. Its `Debug` output is structural
and never formats `T`; `DecodedResponse<T>` is only a non-duplicating compatibility alias during the
implementation-candidate roadmap. Direct await returns `T`, while `.response()` returns
`ApiResponse<T>` for buffered values, bytes, no-content, byte streams, record streams, and custom
outputs. Streaming endpoints expose `.response()`, and direct or wrapped streaming completion
remains `StreamOpened`.

### 12.6 `ExecutionMeta`

```rust
pub struct ExecutionMeta { /* private */ }
```

Methods:

```rust
impl ExecutionMeta {
    pub fn endpoint(&self) -> &'static str;
    pub fn method(&self) -> &Method;
    pub fn page_index(&self) -> u32;
    pub fn recovered_authentication(&self)
        -> bool;
}
```

### 12.7 `ByteStream`

```rust
pub struct ByteStream { /* private */ }
```

It implements:

```rust
Stream<Item = Result<Bytes, ApiError>>
```

`ByteStream` owns one bounded lazy application response body and contains neither response metadata
nor a media marker. Unknown chunks remain unpolled before return; known-length stream-limit
preflight occurs before return; and cumulative counting occurs after decompression. The first chunk
that would exceed the limit is withheld. Native body errors become safe `ApiError` values. `Debug`
does not inspect body or response metadata, drop cancels and drops the native body, and polling after
return is outside the endpoint deadline. Stream opening emits `StreamOpened`; a later stream error
emits no second completion event.

### 12.8 Integrated record streams

Under `records`:

```rust
pub struct RecordError { /* private */ }

#[non_exhaustive]
pub enum RecordErrorKind {
    InvalidMediaType,
    InvalidFraming,
    Decode,
    IncompleteRecord,
    LimitExceeded,
    Body,
    Internal,
}

impl RecordError {
    pub fn new(kind: RecordErrorKind) -> Self;

    pub fn with_safe_message(
        kind: RecordErrorKind,
        message: impl Into<String>,
    ) -> Self;

    pub fn kind(&self) -> RecordErrorKind;
}
```

Record error formatting and sources never reproduce record bytes, parser-authored unsafe text,
response headers, filenames, JSON values, CSV fields, or body values.

```rust
pub trait RecordDecoder:
    Sized + Send + 'static
{
    type Record: Send + 'static;

    fn accept() -> Option<&'static str> {
        None
    }

    fn validate(
        head: &ResponseHead<'_>,
    ) -> Result<(), RecordError> {
        Ok(())
    }

    fn start(
        head: &ResponseHead<'_>,
    ) -> Result<Self, RecordError>;

    fn decode(
        &mut self,
        buffer: &mut BytesMut,
    ) -> Result<Option<Self::Record>, RecordError>;

    fn decode_eof(
        &mut self,
        buffer: &mut BytesMut,
    ) -> Result<Option<Self::Record>, RecordError>;

    fn max_buffered_bytes() -> NonZeroUsize;
}
```

Validation and decoder construction run before body polling. Each returned stream owns one decoder;
decoder calls are synchronous, occur without a core lock, receive no transport or native access,
and have a positive finite staging bound.

```rust
pub struct Records<D>(PhantomData<D>);
pub struct RecordStream<D> { /* private */ }
pub struct RecordBatches<D> { /* private */ }
```

`Records<D>` implements `ResponseBodySpec<Mode = Streaming, Output = RecordStream<D>>` and forwards
its default `Accept` value to `D::accept`.
`RecordStream<D>` implements `Stream<Item = Result<D::Record, RecordError>>` and provides:

```rust
impl<D> RecordStream<D>
where
    D: RecordDecoder,
{
    pub fn batches(
        self,
        size: NonZeroUsize,
    ) -> RecordBatches<D>;
}
```

`RecordBatches<D>` implements `Stream<Item = Result<Vec<D::Record>, RecordError>>`. One-record
streaming is primitive; batching is only a consumer adapter.

Network chunks are never record boundaries. A record can span chunks and a chunk can contain several
records. The decoder is called repeatedly before another chunk is polled while records remain. Its
staging limit is independent of the cumulative `ByteStream` limit: an append that first exceeds the
bound is rejected without retaining offending bytes, and an oversized framed record is never
yielded. EOF calls `decode_eof`; complete final records may be emitted when the decoder permits, and
incomplete final framing returns `IncompleteRecord`. Dropping the record stream drops the underlying
`ByteStream`; post-return body and decode failures are stream items and do not change the already
emitted `StreamOpened` completion.

Under `csv`:

```rust
pub struct Csv<T>(PhantomData<T>);
```

`Csv<T>` is a typed `RecordDecoder` for `T: serde::de::DeserializeOwned + Send + 'static` with a
1 MiB staging bound, default accept value
`text/csv`, and support for `text/csv` parameters. Its exact initial dialect is UTF-8, comma
delimiter, double-quote quoting with doubled-quote escaping, a required unique valid header row,
header-driven typed deserialization, LF or CRLF endings, quoted delimiters and newlines, arbitrary
chunk splits, multiple rows per chunk, an optional final record without a terminal newline, and
ignored empty physical lines outside quoted fields. Malformed quoting, invalid or duplicate headers,
and typed decode failures produce safe `RecordError` values.

Under `ndjson`:

```rust
pub struct NdJson<T>(PhantomData<T>);
```

`NdJson<T>` is a typed `RecordDecoder` for `T: serde::de::DeserializeOwned + Send + 'static` with a
1 MiB staging bound and default accept
value `application/x-ndjson`. It accepts `application/x-ndjson` and `application/ndjson` with ordinary
parameters. Each nonempty logical UTF-8 line is one JSON value; LF and CRLF, arbitrary chunk splits,
multiple values per chunk, an optional final newline, and ignored whitespace-only lines are
supported. Malformed JSON, incomplete final values, and oversized lines produce safe `RecordError`
values, with raw JSON and parser-authored unsafe text excluded from formatting and sources.

### 12.9 Dangerous raw response

Under `dangerous-raw-response`:

```rust
pub struct RawResponse { /* private */ }

impl RawResponse {
    pub fn url(&self) -> &Url;
    pub fn status(&self) -> StatusCode;
    pub fn version(&self) -> Version;
    pub fn headers(&self) -> &HeaderMap;
    pub fn metadata(&self) -> &ExecutionMeta;
    pub fn body(&self) -> &Bytes;
    pub fn into_body(self) -> Bytes;
}
```

It exposes successful logical URL, status, version, actual headers, metadata, and bounded bytes.

It never exposes a native Reqwest response.

**API-RESP-001 — Successful accessor boundary.** `ApiResponse<T>` and explicit dangerous raw accessors MAY expose actual successful headers and application data, and a selected `ResponseBodySpec` MAY receive its documented successful logical `ResponseHead`; none receives a native response, and every wrapper `Debug` MUST remain structural and sanitized as specified by the Security and Limits Specification.

**API-RESPSPEC-001 — Response body specification contract.** `ResponseBodySpec` MUST have the exact output, sealed mode, accept, validation, and synchronous `from_body` contract defined in this section and MUST receive only a successful `ResponseHead` and its core-acquired mode body.

**API-RESPSPEC-002 — Sealed acquisition modes.** Core MUST be the only implementor of `ResponseMode`, with exactly `Buffered -> Bytes`, `Streaming -> ByteStream`, and `Discard -> ()`; application specifications MAY select but MUST NOT define acquisition modes.

**API-RESPSPEC-003 — Response-head validation before polling.** Successful status classification and known-length mode preflight MUST precede `ResponseBodySpec::validate`, and validation MUST complete before the first response-body poll.

**API-RESPSPEC-004 — Buffered codec adaptation.** Every `DecodeBody` codec MUST adapt through the one generic buffered response-specification implementation and MUST receive only complete bounded bytes.

**API-RESP-002 — Universal successful metadata wrapper.** `ApiResponse<T>` MUST be the only successful metadata-wrapper implementation, direct await MUST return `T`, `.response()` MUST return `ApiResponse<T>` for every response mode, `DecodedResponse<T>` MUST be only an alias, and no separate stream metadata wrapper MAY remain.

**API-STREAM-001 — Media-independent bounded byte stream.** `ByteStream` MUST be a metadata-free and media-free lazy stream of bounded post-decompression bytes, MUST withhold the first excess chunk, MUST safely classify native read errors, and MUST cancel by dropping its owned native body.

**API-RECORD-001 — Incremental record decoder.** `RecordDecoder` MUST provide the exact public validation, construction, incremental decode, EOF decode, and positive finite staging-bound contract defined in this section without native or transport access.

**API-RECORD-002 — Records response and record stream.** `Records<D>` MUST select `Streaming`, return one `RecordStream<D>` over one decoder and one `ByteStream`, and surface post-return body or decoder failures as safe stream items.

**API-RECORD-003 — Record batching adapter.** `RecordStream::batches` MUST provide a nonzero-size `RecordBatches<D>` consumer adapter without adding a response mode or changing one-record streaming as the primitive.

**API-RECORD-004 — Typed CSV decoder.** Under `csv`, `Csv<T>` MUST implement the exact typed, header-driven, incremental CSV dialect and 1 MiB staging policy defined in this section.

**API-RECORD-005 — Typed NDJSON decoder.** Under `ndjson`, `NdJson<T>` MUST implement the exact typed, line-delimited, incremental JSON behavior and 1 MiB staging policy defined in this section without enabling the general `json` endpoint feature.

**API-RECORD-006 — Safe record errors.** `RecordError` formatting and recursive sources MUST exclude record bytes, parser-authored unsafe text, response headers, filenames, JSON values, CSV fields, and body values.

## 13. Pagination API

### 13.1 Page-response capability

```rust
pub trait PageResponseSpec:
    ResponseBodySpec<Mode = Buffered>
{
}
```

Core provides a blanket implementation for buffered codecs whose output implements `IntoIterator`.
A downstream custom buffered `ResponseBodySpec` can implement `PageResponseSpec` explicitly.
Generated pagination requires both `R: PageResponseSpec` and `R::Output: IntoIterator`; it does not
classify the response type by name. `Bytes`, `NoContent`, `Stream<M>`, and `Records<D>` do not
accidentally implement the capability, and record streams are not paginated.

### 13.2 Page limits

```rust
pub enum PageLimit {
    Take(u32),
    HardCap(NonZeroU32),
}
```

```rust
impl From<u32> for PageLimit {
    // converts to Take
}
```

The runtime validates every nonzero requested limit against its configured maximum page count before constructing a pagination controller.

### 13.3 Pagination error

```rust
pub struct PaginationError { /* private */ }

impl PaginationError {
    pub fn new(kind: PaginationErrorKind) -> Self;

    pub fn with_safe_message(
        kind: PaginationErrorKind,
        message: impl Into<String>,
    ) -> Self;

    pub fn kind(&self) -> PaginationErrorKind;
}
```

It implements safe `Display`, `Debug`, and `Error` and has a structured `PaginationErrorKind` including controller, arithmetic, repeated-request, hard-cap, configured-page-ceiling, collected-item-ceiling, and identity-storage-ceiling failures.

### 13.4 Controller

```rust
pub trait PaginationController<Page>: Sized + Send + 'static {
    fn validate(&self) -> Result<(), PaginationError> {
        Ok(())
    }

    fn next(
        self,
        page: &Page,
    ) -> Result<Option<Self>, PaginationError>;

    fn short_page_size(&self) -> Option<NonZeroUsize> {
        None
    }
}
```

`validate`, `next`, and `short_page_size` are synchronous, deterministic, non-blocking, and externally side-effect-free. They perform no transport or external I/O and execute without a core lock. Validation runs before each page request. `next` consumes the current state, and `short_page_size` selects count-based natural termination. The controller needs neither `Clone` nor `Sync`; it remains local to one pagination operation while the pagination future remains `Send`.

Controller state influences a request only through fields declared with DSL `bind`. There is no public pagination request patch, controller factory, endpoint-generic controller factory, binder protocol, or short-page callback hierarchy.

### 13.5 Cursor page support

```rust
pub trait HasNextCursor<C> {
    fn next_cursor(&self) -> Option<&C>;
}
```

### 13.6 Built-in controllers

```rust
pub struct OffsetPagination {
    pub offset: u64,
    pub page_size: NonZeroU32,
}

pub struct PagePagination {
    pub page: NonZeroU64,
    pub page_size: NonZeroU32,
}

pub struct CursorPagination<C> {
    pub cursor: Option<C>,
}
```

Stable constructors and accessors:

```rust
impl OffsetPagination {
    pub fn new(
        offset: u64,
        page_size: NonZeroU32,
    ) -> Self;

    pub fn offset(&self) -> u64;
    pub fn page_size(&self) -> NonZeroU32;
}

impl PagePagination {
    pub fn new(
        page: NonZeroU64,
        page_size: NonZeroU32,
    ) -> Self;

    pub fn page(&self) -> NonZeroU64;
    pub fn page_size(&self) -> NonZeroU32;
}

impl<C> CursorPagination<C> {
    pub fn new(cursor: Option<C>) -> Self;
    pub fn cursor(&self) -> Option<&C>;
}
```

Built-ins use the same public trait and generated binding path as application controllers. Offset and page-number progression uses checked addition, and each reports its exact requested size through `short_page_size`. Cursor pagination reports no count termination and may continue after an empty decoded item page when `HasNextCursor` supplies a continuation.

**API-PAGE-001 — Iterable pages.** Pagination page outputs MUST use `IntoIterator`; no Concord-specific item extraction trait is public.

**API-PAGE-002 — Controller purity.** `PaginationController::validate`, `PaginationController::next`, and `PaginationController::short_page_size` MUST be synchronous, deterministic, side-effect-free, non-blocking, perform no transport or external I/O, and execute without a core lock.

**API-PAGE-003 — Controller error construction.** Custom pagination controllers MUST be able to construct typed, cursor-safe `PaginationError` values.

**API-PAGE-007 — Declared controller-state visibility.** Every controller field named by a pagination declaration MUST be accessible to the generated ordinary Rust struct literal and every bound field MUST be readable for generated per-page binding under ordinary Rust visibility rules.

**API-PAGE-008 — Controller-selected count termination.** `short_page_size() == Some(n)` MUST select count termination exactly when the decoded item count is less than `n`; `Some(1)` MUST therefore select empty-page termination, while `None` MUST disable item-count termination.

**API-PAGE-009 — Binding-only request influence.** A pagination controller MUST influence ordinary request preparation only through declared bound endpoint or scope fields and MUST receive no public query/header patch, body, credential, authentication plan, native request or URL, Reqwest value, transport, or shared-client mutation capability.

**API-PAGE-010 — Request-local controller bounds.** `PaginationController<Page>` MUST require exactly `Sized + Send + 'static` at the trait level, MUST NOT require `Clone` or `Sync`, and its controller value MUST remain local to one pagination operation whose future remains `Send`.

**API-PAGE-011 — Explicit page-response capability.** Generated pagination MUST require `R: PageResponseSpec` and `R::Output: IntoIterator`; buffered codecs with iterable output MUST receive the core blanket capability, custom buffered response specifications MAY implement it explicitly, and bytes, discard, streaming, and records specifications MUST NOT gain it accidentally.

## 14. Rate limiting

### 14.1 Rate-limit error

```rust
pub struct RateLimitError { /* private */ }

impl RateLimitError {
    pub fn new(kind: RateLimitErrorKind) -> Self;

    pub fn with_safe_message(
        kind: RateLimitErrorKind,
        message: impl Into<String>,
    ) -> Self;

    pub fn kind(&self) -> RateLimitErrorKind;
}
```

`RateLimitError` implements safe `Display`, `Debug`, and `Error` and never reproduces resolved key values.

### 14.2 Value types

Stable public types include:

```rust
pub struct RateLimitId { /* private */ }
pub struct RateLimitKeyPart { /* private */ }
pub struct RateLimitKey { /* private */ }
pub struct RateLimitWindow { /* private */ }
pub struct RateLimitRule { /* private */ }
pub struct RateLimitPlan { /* private */ }
pub struct RateLimitPermit { /* opaque */ }
```

A custom limiter creates a successful permit through:

```rust
impl RateLimitPermit {
    pub fn granted() -> Self;
}
```

Advanced constructors validate structural invariants. The configured aggregate key-byte bound is enforced when the rule is attached to a prepared request and again by the built-in limiter before state insertion.

```rust
impl RateLimitId {
    pub fn new(value: impl Into<String>) -> Result<Self, RateLimitError>;
}

impl RateLimitKeyPart {
    pub fn new(
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self;
}

impl RateLimitKey {
    pub fn global() -> Self;

    pub fn from_parts<I>(
        parts: I,
    ) -> Result<Self, RateLimitError>
    where
        I: IntoIterator<Item = RateLimitKeyPart>;
}

impl RateLimitWindow {
    pub fn new(
        capacity: NonZeroU32,
        duration: Duration,
    ) -> Result<Self, RateLimitError>;
}

impl RateLimitRule {
    pub fn new(
        id: RateLimitId,
        key: RateLimitKey,
        cost: NonZeroU32,
        windows: impl IntoIterator<Item = RateLimitWindow>,
    ) -> Result<Self, RateLimitError>;
}

impl RateLimitId {
    pub fn as_str(&self) -> &str;
}

impl RateLimitKeyPart {
    pub fn name(&self) -> &str;
    pub fn value(&self) -> &str;
}

impl RateLimitKey {
    pub fn parts(&self) -> &[RateLimitKeyPart];
    pub fn is_global(&self) -> bool;
    pub fn total_bytes(&self) -> usize;
}

impl RateLimitWindow {
    pub fn capacity(&self) -> NonZeroU32;
    pub fn duration(&self) -> Duration;
}

impl RateLimitRule {
    pub fn id(&self) -> &RateLimitId;
    pub fn key(&self) -> &RateLimitKey;
    pub fn cost(&self) -> NonZeroU32;
    pub fn windows(&self) -> &[RateLimitWindow];
}

impl RateLimitPlan {
    pub fn rules(&self) -> &[RateLimitRule];
    pub fn is_empty(&self) -> bool;
}
```

### 14.3 Limiter request

```rust
pub struct RateLimitRequest<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub safe_url: &'a str,
    pub host: Option<&'a str>,
    pub page_index: u32,
    pub plan: &'a RateLimitPlan,
}
```

### 14.4 Limiter response

```rust
pub struct RateLimitResponse<'a> {
    pub request: RateLimitRequest<'a>,
    pub status: StatusCode,
    pub headers: SanitizedHeaders<'a>,
}
```

### 14.5 Limiter trait

```rust
pub trait RateLimiter:
    Send + Sync + 'static
{
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<
        'a,
        Result<RateLimitPermit, RateLimitError>,
    >;

    fn observe<'a>(
        &'a self,
        response: RateLimitResponse<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}
```

Observation cannot replace the current HTTP result.

### 14.6 Cooldown

```rust
pub enum CooldownScope {
    CurrentBuckets,
    Host,
    Client,
}

pub struct Cooldown {
    pub duration: Duration,
    pub scope: CooldownScope,
}
```

Scope semantics:

- `CurrentBuckets`: every exact rule/key in the current effective plan; when the plan is empty, the current host;
- `Host`: every later request to the current logical host using this limiter;
- `Client`: every later request using this limiter object.

A non-positive duration stores no wait. Positive durations are clamped to the configured cap. Extending an existing scope keeps the later expiration and never shortens a longer cooldown.

### 14.7 Observer response head

```rust
pub struct RateLimitResponseHead<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub safe_url: &'a str,
    pub host: Option<&'a str>,
    pub page_index: u32,
    pub plan: &'a RateLimitPlan,
    pub status: StatusCode,
    pub headers: SanitizedHeaders<'a>,
}
```

### 14.8 Observer

```rust
pub trait RateLimitObserver:
    Send + Sync + 'static
{
    fn observe(
        &self,
        response: &RateLimitResponseHead<'_>,
    ) -> Option<Cooldown>;
}
```

### 14.9 Built-ins

```rust
pub struct GovernorRateLimiter { /* private */ }
pub struct DefaultRateLimitObserver;
pub struct NoopRateLimiter;
```

```rust
impl GovernorRateLimiter {
    pub fn new() -> Self;

    pub fn observer(
        self,
        observer: Arc<dyn RateLimitObserver>,
    ) -> Self;

    pub fn quota_capacity(
        self,
        entries: usize,
    ) -> Result<Self, RateLimitError>;

    pub fn cooldown_capacity(
        self,
        entries: usize,
    ) -> Result<Self, RateLimitError>;

    pub fn idle_ttl(
        self,
        duration: Duration,
    ) -> Result<Self, RateLimitError>;

    pub fn key_limit(
        self,
        bytes: usize,
    ) -> Result<Self, RateLimitError>;
}

impl NoopRateLimiter {
    pub fn new() -> Self;
}
```

`DefaultRateLimitObserver` returns a cooldown only for final `429 Too Many Requests` responses with a valid positive `Retry-After` value. It accepts decimal delta-seconds and HTTP dates, ignores surrounding whitespace, ignores malformed values, treats past dates as zero, targets `CurrentBuckets`, and falls back to `Host` when the effective plan is empty. The governor clamps the duration to the configured cooldown cap.

A custom observer installed through `GovernorRateLimiter::observer` replaces this default observer rather than composing with it automatically.

`NoopRateLimiter` authorizes immediately and ignores response observation.

**API-RATE-001 — Visible-attempt contract.** A custom limiter MUST be called per Concord attempt, not per Reqwest-internal attempt.

**API-RATE-002 — No response replacement.** `observe` MUST NOT return an error or alternative response.

**API-RATE-003 — Extension result construction.** Custom limiters MUST be able to construct an opaque granted permit and typed, key-safe `RateLimitError` values.

**API-RATE-004 — Plan inspection.** Custom limiters and observers MUST be able to inspect rule identifiers, resolved key parts, costs, and windows without access to request bodies or credentials.

## 15. Hooks and debugging

### 15.1 Debug level

```rust
pub enum DebugLevel {
    None,
    Basic,
    Headers,
}
```

### 15.2 Hook error

```rust
pub struct HookError { /* private */ }
```

A safe constructor is provided:

```rust
impl HookError {
    pub fn new(
        message: impl Into<String>,
    ) -> Self;
}
```

### 15.3 Operation identity

```rust
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct OperationId { /* private */ }
```

`OperationId::Debug` is opaque and contains no route, credential, cursor, or rate-limit data. The same identifier is used for every event belonging to one direct endpoint or page operation. A page operation also carries its aggregate pagination operation identifier as `parent_operation_id`. Operation identifiers are distinct for simultaneously live operations, including operations created by different clients that share the same hook object.

### 15.4 Before-send context

```rust
pub struct BeforeSend<'a> {
    pub operation_id: OperationId,
    pub parent_operation_id: Option<OperationId>,
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub safe_url: &'a str,
    pub page_index: u32,
    pub recovery_index: u8,
    pub headers: SanitizedHeaders<'a>,
    pub body_kind: BodyKind,
    pub known_body_length: Option<u64>,
}
```

### 15.5 Response-head event

```rust
pub struct ResponseHeadEvent<'a> {
    pub operation_id: OperationId,
    pub parent_operation_id: Option<OperationId>,
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub safe_url: &'a str,
    pub page_index: u32,
    pub recovery_index: u8,
    pub status: StatusCode,
    pub headers: SanitizedHeaders<'a>,
}
```

### 15.6 Request/page completion event

```rust
pub struct CompletionEvent<'a> {
    pub operation_id: OperationId,
    pub parent_operation_id: Option<OperationId>,
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub safe_url: &'a str,
    pub page_index: u32,
    pub recovered_authentication: bool,
    pub elapsed: Duration,
    pub final_status: Option<StatusCode>,
    pub outcome: CompletionOutcome,
}
```

### 15.7 Aggregate pagination completion event

```rust
pub struct PaginationCompletionEvent<'a> {
    pub operation_id: OperationId,
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub pages_completed: u32,
    pub items_collected: usize,
    pub recovered_authentication: bool,
    pub elapsed: Duration,
    pub final_status: Option<StatusCode>,
    pub outcome: CompletionOutcome,
}
```

For request/page completion, `final_status` is the final application response status received by that operation. For aggregate pagination completion, it is the final completed page operation's application response status when at least one page response existed; it is `None` for zero-page success or failure before any page response. `elapsed` covers first poll through result fixation and excludes time awaiting the completion hook. The aggregate event contains no continuation value, repeated request identity, partial item collection, body, or rate-limit key.

### 15.8 Completion outcomes

```rust
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionOutcome {
    Success,
    StreamOpened,

    Configuration,
    RequestEncoding,
    RequestBody,

    AuthenticationUnavailable,
    AuthenticationProvider,
    AuthenticationRejected,

    RateLimit,
    Hook,

    Timeout,
    Connect,
    Transport,

    HttpStatus,

    ResponseBody,
    ResponseDecoding,
    ResponseContract,

    Pagination,
    Internal,
}
```

`Success` means a buffered, bytes, no-content, raw-success, or completed page result has been fully produced. `StreamOpened` means the response head was accepted and a bounded lazy stream was returned; it does not claim that later stream consumption completed successfully.

The outcomes contain no error source, response body, cursor, request identity, or rate-limit key.

`on_complete` and `on_pagination_complete` observe an already-fixed result outside the operation
deadline. They cannot replace or mutate that result, request a retry, or restart the deadline. They
are awaited rather than detached and may delay result delivery. A normal timeout result invokes
`on_complete` with `CompletionOutcome::Timeout`. Extension authors own completion-hook boundedness;
cancellation or panic may prevent completion delivery.


### 15.9 Hook trait

```rust
pub trait RuntimeHooks:
    Send + Sync + 'static
{
    fn before_send<'a>(
        &'a self,
        context: BeforeSend<'a>,
    ) -> BoxFuture<
        'a,
        Result<(), HookError>,
    > {
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(
        &'a self,
        context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    fn on_complete<'a>(
        &'a self,
        context: CompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    fn on_pagination_complete<'a>(
        &'a self,
        context: PaginationCompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}
```

**API-HOOK-001 — Sanitized inputs.** Hooks MUST receive no body bytes, credentials, native URL, or raw transport error.

**API-HOOK-002 — Concurrent use.** Hook implementations MUST support concurrent calls.

**API-HOOK-003 — Aggregate pagination visibility.** One pagination collection MUST emit one aggregate pagination completion callback in addition to per-page completion callbacks.

**API-HOOK-004 — Stream-open distinction.** Successful creation of a lazy stream MUST be represented as `StreamOpened`, not as full stream-consumption success.

**API-HOOK-005 — Aggregate outcome category.** A pagination aggregate completion MUST report `Success` for collection success, the ordinary returned category for a page-operation failure, and `Pagination` for cycle, controller, cap, or aggregate-ceiling failure.

**API-HOOK-006 — Operation correlation.** Every before-send, response-head, and request/page completion event for one operation MUST carry the same opaque `OperationId`; page events MUST also carry their aggregate pagination parent identifier.

**API-HOOK-007 — Completion result-fixation metadata.** Completion events MUST expose monotonic elapsed duration from first poll through result fixation, excluding completion-hook time, and the final application HTTP status when a response head existed. Completion hooks MUST observe but MUST NOT alter the fixed result and MUST run outside the operation deadline during normal continued polling, including for timeout results.

**API-HOOK-008 — Live identifier uniqueness.** Two simultaneously live Concord operations, including operations from different clients that can report to the same hook object, MUST NOT share an `OperationId`.

## 16. Sanitized metadata

### 16.1 Sanitized header value

```rust
pub enum SanitizedHeaderValue<'a> {
    Text(&'a str),
    Redacted,
    NonUtf8,
}
```

### 16.2 Header view

```rust
#[derive(Clone, Copy)]
pub struct SanitizedHeaders<'a> { /* private */ }
```

Required methods:

```rust
impl<'a> SanitizedHeaders<'a> {
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn contains_key(&self, name: &HeaderName) -> bool;

    pub fn get(
        &self,
        name: &HeaderName,
    ) -> Option<SanitizedHeaderValue<'a>>;

    pub fn iter(
        &self,
    ) -> impl Iterator<
        Item = (&'a HeaderName, SanitizedHeaderValue<'a>),
    > + 'a;
}
```

### 16.3 Owned sanitized map

```rust
pub struct SanitizedHeaderMap { /* private */ }
```

Required methods:

```rust
impl SanitizedHeaderMap {
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn contains_key(&self, name: &HeaderName) -> bool;

    pub fn get(
        &self,
        name: &HeaderName,
    ) -> Option<SanitizedHeaderValue<'_>>;

    pub fn iter(
        &self,
    ) -> impl Iterator<
        Item = (&HeaderName, SanitizedHeaderValue<'_>),
    >;
}
```

Used by retained status errors.

**API-SAFE-001 — No raw conversion.** Sanitized header wrappers MUST NOT provide access to hidden original sensitive values.

## 17. HTTP and runtime configuration

### 17.1 `HttpConfigBuilder`

```rust
pub struct HttpConfigBuilder { /* private */ }

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRetry {
    ProtocolRecovery,
    Disabled,
}

impl HttpConfigBuilder {
    pub fn connect_timeout(
        self,
        duration: Duration,
    ) -> Self;

    pub fn read_timeout(
        self,
        duration: Duration,
    ) -> Self;

    pub fn pool_idle_timeout(
        self,
        duration: Duration,
    ) -> Self;

    pub fn pool_max_idle_per_host(
        self,
        count: usize,
    ) -> Self;

    pub fn tcp_keepalive(
        self,
        duration: Duration,
    ) -> Self;

    pub fn tcp_nodelay(
        self,
        enabled: bool,
    ) -> Self;

    pub fn https_only(
        self,
        enabled: bool,
    ) -> Self;

    pub fn http1_only(self) -> Self;

    #[cfg(feature = "http2")]
    pub fn http2_prior_knowledge(self) -> Self;

    pub fn proxy(
        self,
        proxy: SafeProxy,
    ) -> Self;

    pub fn retry_reqwest_default(self) -> Self;
    pub fn retry_off(self) -> Self;

    pub fn provider_retry(
        self,
        retry: ProviderRetry,
    ) -> Self;

    #[cfg(feature = "default-tls")]
    pub fn add_root_certificate_pem(
        self,
        pem: impl Into<Vec<u8>>,
    ) -> Self;

    #[cfg(feature = "default-tls")]
    pub fn identity_pem(
        self,
        pem: impl Into<Vec<u8>>,
    ) -> Self;

    #[cfg(feature = "gzip")]
    pub fn disable_gzip(self) -> Self;

    #[cfg(feature = "brotli")]
    pub fn disable_brotli(self) -> Self;

    #[cfg(feature = "deflate")]
    pub fn disable_deflate(self) -> Self;

    #[cfg(feature = "dangerous-custom-retry")]
    pub fn dangerous_retry(
        self,
        builder: concord_core::dangerous::ReqwestRetryBuilder,
    ) -> Self;
}
```

PEM inputs are parsed during client construction. Invalid material produces `ClientBuildError` without reproducing PEM contents. Additional root certificates apply to both managed application and provider clients. `identity_pem` configures only the managed application client identity; the provider client does not present it. Concord 0.1 has no provider-specific client-identity setting.

`https_only(true)` is retained as effective managed-application metadata so public preflight can
reject every HTTP application request, including an unauthenticated request, before native Reqwest
execution.

### 17.2 `RuntimeConfigBuilder`

```rust
pub struct RuntimeConfigBuilder { /* private */ }

impl RuntimeConfigBuilder {
    pub fn request_body_limit(
        self,
        bytes: NonZeroUsize,
    ) -> Self;

    pub fn response_body_limit(
        self,
        bytes: NonZeroUsize,
    ) -> Self;

    pub fn stream_response_limit(
        self,
        bytes: NonZeroUsize,
    ) -> Self;

    pub fn status_body_limit(
        self,
        bytes: NonZeroUsize,
    ) -> Self;

    pub fn disable_status_body_capture(
        self,
    ) -> Self;

    pub fn pagination_max_pages(
        self,
        pages: NonZeroU32,
    ) -> Self;

    pub fn pagination_max_items(
        self,
        items: NonZeroUsize,
    ) -> Self;

    pub fn pagination_identity_limit(
        self,
        bytes: NonZeroUsize,
    ) -> Self;

    pub fn hooks(
        self,
        hooks: Arc<dyn RuntimeHooks>,
    ) -> Self;

    pub fn rate_limiter(
        self,
        limiter: Arc<dyn RateLimiter>,
    ) -> Self;

    pub fn debug(
        self,
        level: DebugLevel,
    ) -> Self;

    pub fn auth_failure_cooldown_cap(
        self,
        duration: Duration,
    ) -> Self;

    pub fn rate_limit_cooldown_cap(
        self,
        duration: Duration,
    ) -> Self;

    pub fn provider_request_limit(
        self,
        bytes: NonZeroUsize,
    ) -> Self;

    pub fn provider_response_limit(
        self,
        bytes: NonZeroUsize,
    ) -> Self;
}
```

All application and provider body limits remain positive and finite. Pagination page, item, and identity-storage ceilings also remain positive and finite. No ordinary runtime method disables these bounds.

### 17.3 Safe proxy

```rust
pub struct SafeProxy { /* private */ }

impl SafeProxy {
    pub fn parse(
        value: impl AsRef<str>,
    ) -> Result<Self, ConfigurationError>;
}
```

A safe proxy is a credential-free HTTP(S) origin with no path, query, or fragment.

**API-HTTP-001 — No raw builder.** Safe configuration MUST NOT expose `reqwest::ClientBuilder` or a raw Reqwest client.

**API-HTTP-002 — Native retry opt-in.** Safe application HTTP configuration MUST begin with native retry disabled and MUST enable Reqwest default retry only through `retry_reqwest_default()`.

**API-HTTP-003 — Effective HTTPS restriction.** Enabling `https_only` MUST remain available to public request preflight and MUST reject every HTTP application request before native execution.

**API-HTTP-004 — Managed TLS material scope.** Additional root certificates MUST apply to application and provider managed clients, while `identity_pem` MUST apply only to the application client.

**API-RUNTIME-001 — Finite ordinary limits.** `RuntimeConfigBuilder` MUST provide no ordinary method that disables request, response, stream, provider, or pagination ceilings.

**API-RUNTIME-002 — Finite status-body capture.** Status-body capture MUST remain finite; disabling capture MAY discard terminal bodies but MUST NOT create an unbounded mode.

## 18. Prepared endpoints

### 18.1 Purpose

Prepared endpoints define hand-written endpoints while retaining the complete generated-client runtime.

They are executed through:

```rust
client.request(endpoint)
```

### 18.2 Generic specification selection

```rust
pub struct PreparedEndpointBuilder<
    R = NoContent,
> { /* private */ }

pub struct PreparedEndpoint<
    R = NoContent,
> { /* private */ }
```

Construction starts as a no-content endpoint:

```rust
impl PreparedEndpoint<NoContent> {
    pub fn builder(
        endpoint: &'static str,
        method: Method,
    ) -> PreparedEndpointBuilder<NoContent>;
}
```

The builder supports:

```rust
impl<R> PreparedEndpointBuilder<R> {
    pub fn path_segment<T>(
        self,
        value: T,
    ) -> Self
    where
        T: Display;

    pub fn query<T>(
        self,
        name: impl Into<String>,
        value: T,
    ) -> Self
    where
        T: Display;

    pub fn query_optional<T>(
        self,
        name: impl Into<String>,
        value: Option<T>,
    ) -> Self
    where
        T: Display;

    pub fn query_values<T, I>(
        self,
        name: impl Into<String>,
        values: I,
    ) -> Self
    where
        T: Display,
        I: IntoIterator<Item = T>;

    pub fn header<T>(
        self,
        name: HeaderName,
        value: T,
    ) -> Self
    where
        T: Display;

    pub fn body(
        self,
        body: RequestBody,
    ) -> Self;

    pub fn body_spec<B>(
        self,
        input: B::Input,
    ) -> Self
    where
        B: RequestBodySpec;

    pub fn body_codec<C>(
        self,
        value: C::Input,
    ) -> PreparedEndpointBuilder<R>
    where
        C: EncodeBody;

    pub fn auth(
        self,
        auth: Auth,
    ) -> Self;

    pub fn timeout(
        self,
        duration: Duration,
    ) -> Self;

    pub fn disable_timeout(self) -> Self;

    pub fn rate_limit(
        self,
        rule: RateLimitRule,
    ) -> Self;

    pub fn response<S>(
        self,
    ) -> PreparedEndpointBuilder<S>
    where
        S: ResponseBodySpec;

    pub fn response_bytes(
        self,
    ) -> PreparedEndpointBuilder<Bytes>;

    pub fn response_no_content(
        self,
    ) -> PreparedEndpointBuilder<NoContent>;

    pub fn response_stream<M>(
        self,
    ) -> PreparedEndpointBuilder<Stream<M>>
    where
        M: MediaType;

    pub fn build(
        self,
    ) -> Result<PreparedEndpoint<R>, ConfigurationError>;
}
```

`body_spec<B>` is the generic request path. `body(RequestBody)` and `body_codec<C: EncodeBody>`
delegate to it. `response<S>` is the generic response path; `response_bytes`,
`response_no_content`, and `response_stream<M>` forward to `Bytes`, `NoContent`, and `Stream<M>`.
There are no parallel prepared response-marker types.

Every body-selection method replaces an earlier body selection. Every response-selection method replaces the response-specification type and consumes the previous builder. Repeated public header names replace the earlier value case-insensitively. Query calls append ordered pairs. Authentication and rate-limit calls append in call order. Timeout methods replace the earlier Concord operation-deadline selection.

### 18.3 Path and query safety

Every `path_segment` is encoded as exactly one segment. The prepared endpoint API has no raw path-fragment method in 0.1.

Query methods preserve insertion order and repeated values.

### 18.4 Authentication and rate limits

Multiple `.auth(...)` and `.rate_limit(...)` calls append in call order. Ordinary collision and limiter-plan validation remains active.

### 18.5 Execution

A `PreparedEndpoint<R>` is an owned value accepted by the generated client's `request` method. Its direct-await output is `R::Output` and `.response()` returns `ApiResponse<R::Output>` through the same request specification, response specification, sealed acquisition mode, body lifecycle, executor, limits, authentication, limiter, hooks, status handling, recovery, cancellation, and completion path as generated endpoints.

Prepared endpoints do not support pagination in 0.1.

### 18.6 Restrictions

Prepared endpoints provide no access to:

- native Reqwest client;
- native request builder;
- raw unvalidated path fragments;
- transport replacement;
- status bypass;
- body-limit bypass;
- auth-recovery bypass;
- limiter bypass.

**API-PREP-001 — Runtime containment.** Prepared endpoints MUST use the same request specification, response specification, body lifecycle, sealed acquisition, validation, executor, auth, limiter, hook, status, limit, recovery, cancellation, and completion runtime as generated endpoints.

**API-PREP-002 — Segment-safe routes.** Prepared endpoints MUST expose encoded path segments rather than raw path-fragment insertion.

**API-PREP-003 — Deterministic builder composition.** Prepared endpoint generic body and response selections and their conveniences, header, query, auth, rate-limit, and timeout calls MUST compose with the deterministic replacement/append rules defined in this section and delegate to one semantic path.

**API-PREP-004 — Prepared generic specification selection.** `PreparedEndpointBuilder<R = NoContent>` and `PreparedEndpoint<R = NoContent>` MUST support arbitrary `RequestBodySpec` and `ResponseBodySpec` selection through `body_spec<B>` and `response<S>` without response-marker taxonomy, prepared-specific executor, or pagination.

## 19. Dangerous API

### 19.1 Retry builder

Under `dangerous-custom-retry`:

```rust
pub use reqwest::retry::Builder
    as ReqwestRetryBuilder;
```

The exact re-export is part of the 0.1 compatibility contract because DSL custom retry factories return it.

### 19.2 Raw response

Under `dangerous-raw-response`, `RawResponse` and generated raw terminals are available.

### 19.3 Reduced guarantees

Dangerous custom retry may observe native method, status, transport error, and materialized URL, including query authentication.

Dangerous raw response exposes actual successful headers and body bytes.

**API-DANGER-001 — Explicit opt-in.** Dangerous APIs MUST be absent without their corresponding features.

## 20. Intentionally non-public concepts

**API-PRIVATE-002 — Prohibited stable surfaces.** The following concepts MUST NOT be stable application APIs:

- production transport or executor traits;
- Reqwest request or response objects;
- request execution plans;
- response decoder plans;
- authentication plans;
- prepared authentication applications;
- credential versions and leases;
- credential slot states;
- waiter and operation types;
- pagination runtime adapters;
- generated variable/context storage;
- body replayability internals;
- native request materialization helpers.

**API-PRIVATE-001 — No integration leakage.** A documented advanced workflow MUST NOT require an application import from `__private`.
