# CONCORD_GENERATED_API_SPEC_0.1

**Version:** 0.1  
**Status:** Normative Implementation Candidate
**Authority:** Exact generated Rust API specification  
**Requirement prefix:** `GEN`  
**Depends on:** Specification Index, Product Contract, DSL Specification, Public API Specification  
**Supersedes:** None

## 1. Scope

This document defines the public Rust API generated from a valid Concord DSL declaration.

It owns:

- generated type and module organization;
- constructor and builder signatures;
- facade and endpoint method naming;
- optional/default setter naming;
- request terminals;
- endpoint-backed credential helpers;
- explicit generated endpoint usage;
- generated ownership and thread-safety behavior;
- generated Rustdoc content.

Internal generated traits and storage layouts are not public compatibility unless explicitly described here.

## 2. Generated module and client

For:

```text
client ExampleApi { ... }
```

Concord generates a public type:

```rust
pub struct ExampleApi { /* private */ }
```

The client and its builder are generated directly at the macro invocation scope:

```rust
pub struct ExampleApi { /* private */ }
pub struct ExampleApiBuilder { /* private */ }
```

A public snake-case support module is generated at the same scope:

```rust
pub mod example_api {
    pub mod prelude {
        // generated endpoint request extension traits
    }

    pub mod facades {
        // public facade types
    }

    pub mod endpoints {
        // public explicit endpoint modules and types
    }

    pub mod credentials {
        // public credential facade and source-specific handles
    }
}
```

Nested scopes produce nested modules under `facades` and `endpoints` following the DSL scope path.
The generated `prelude` re-exports the public sealed request extension traits needed for
endpoint-specific setters. Generated-private integration items are not placed in these public
modules.

**GEN-TYPE-001 — Concrete client.** A generated client MUST be a concrete type with no public transport or executor generic.

**GEN-TYPE-002 — Private fields.** Generated client, facade, endpoint, request, and credential-storage fields MUST be private.

**GEN-TYPE-003 — Cheap clone.** Generated clients and facades MUST implement `Clone` without duplicating Reqwest connection pools or credential state.

## 3. Client construction

### 3.1 Convenience constructor

Every generated client exposes:

```rust
impl ExampleApi {
    pub fn new(
        REQUIRED_INPUTS,
    ) -> Result<Self, ClientBuildError>;
}
```

Argument order is:

1. required client `var` values in DSL declaration order;
2. required secrets in credentials-block declaration order;
3. custom credential providers in credential declaration order.

Optional and defaulted client values are not constructor arguments.

Built-in credential declarations do not add provider arguments.

Custom provider declaration:

```text
credential vendor = custom crate::auth::VendorToken;
```

adds a constructor argument equivalent to:

```rust
vendor_provider:
    impl CredentialProvider<Material = VendorToken>
```

`new` delegates to the canonical builder and has identical behavior.

**GEN-CTOR-001 — Fallible construction.** Generated client construction MUST return `Result` and MUST NOT defer fixed-base configuration failures to the first request.

**GEN-CTOR-002 — Stable argument order.** Constructor argument order MUST follow the ordering defined in this section.

### 3.2 Canonical builder

Every generated client exposes:

```rust
impl ExampleApi {
    pub fn builder() -> ExampleApiBuilder;
}
```

The builder also exposes:

```rust
impl ExampleApiBuilder {
    pub fn new() -> Self;
}
```

`ExampleApi::builder()` and `ExampleApiBuilder::new()` create equivalent empty builders.

The builder is the only generated construction mechanism for configuring HTTP or runtime behavior.

There are no specialized retry, HTTP, TLS, hook, limiter, or raw-Reqwest constructors.

## 4. Builder value methods

### 4.1 Required client value

DSL:

```text
var tenant: String;
```

Generated:

```rust
pub fn tenant(
    self,
    value: String,
) -> Self;
```

The exact declared type is used.

Calling the setter more than once replaces the earlier builder value.

### 4.2 Optional client value

DSL:

```text
var locale: Option<String>;
```

Generated:

```rust
pub fn locale(
    self,
    value: String,
) -> Self;

pub fn clear_locale(
    self,
) -> Self;
```

`locale(value)` stores `Some(value)`.

`clear_locale()` stores an explicit `None`.

If neither is called, the value resolves to `None`.

### 4.3 Defaulted client value

DSL:

```text
var page_size: u32 = 100;
```

Generated:

```rust
pub fn page_size(
    self,
    value: u32,
) -> Self;

pub fn reset_page_size(
    self,
) -> Self;
```

The builder stores whether the explicit value or the DSL default is selected.

When an explicit builder value remains selected, the DSL default expression is not evaluated. When the default remains or becomes selected through `reset_page_size()`, it is evaluated once during `build()` and becomes the generated client's immutable value.

### 4.4 Required secrets

DSL:

```text
secret api_key: String;
```

Generated builder method:

```rust
pub fn api_key(
    self,
    value: impl Into<SecretString>,
) -> Self;
```

Secret builder methods use the secret declaration name.

### 4.5 Custom providers

DSL:

```text
credential session = custom crate::auth::SessionToken;
```

Generated:

```rust
pub fn session_provider<P>(
    self,
    provider: P,
) -> Self
where
    P: CredentialProvider<Material = SessionToken>;
```

The builder stores the provider behind shared ownership.

### 4.6 HTTP and runtime configuration

Generated:

```rust
pub fn http<F>(
    self,
    configure: F,
) -> Self
where
    F: FnOnce(HttpConfigBuilder)
        -> HttpConfigBuilder;

pub fn runtime<F>(
    self,
    configure: F,
) -> Self
where
    F: FnOnce(RuntimeConfigBuilder)
        -> RuntimeConfigBuilder;
```

Each method may be called at most once.

The closure is invoked immediately by `.http(...)` or `.runtime(...)`; it is never retained inside the generated builder. The returned configuration builder is stored as resolved builder state.

A second call:

- records a generated builder-state error returned by `build()`;
- does not invoke the second closure;
- does not modify the configuration produced by the first call.

The HTTP builder passed to `.http(...)` is preconfigured from the DSL retry declaration. When the DSL omits retry, the initial selection is disabled. Retry methods called by the closure replace that initial selection. When the closure replaces a DSL `retry custom` selection, the DSL factory is not invoked.

**GEN-CONFIG-001 — Immediate configuration closure.** `.http(...)` and `.runtime(...)` MUST invoke their closure synchronously during the method call and MUST NOT retain the closure.

**GEN-CONFIG-002 — Duplicate closure suppression.** A duplicate `.http(...)` or `.runtime(...)` call MUST record a build error without invoking the duplicate closure.

**GEN-CONFIG-003 — Disabled retry baseline.** A generated builder MUST initialize application native retry as disabled when the DSL contains no retry declaration.

### 4.7 Build

```rust
pub fn build(
    self,
) -> Result<ExampleApi, ClientBuildError>;
```

Missing required values, missing secrets, missing custom providers, invalid HTTP configuration, fixed TLS failure, and unavailable required limiter enforcement are construction errors.

When the DSL declares a rate-limit observer and the generated default governor remains selected, the observer type is constructed through `Default` once per independent client build and installed into that governor. When the runtime builder supplies another limiter, that limiter owns its own response observation and the DSL observer is not wrapped around it.

**GEN-BUILD-001 — One canonical builder.** All generated construction customization MUST be reachable through the canonical builder.

**GEN-BUILD-002 — Setter replacement.** Repeated value, secret, or provider setters MUST replace the previous builder entry.

**GEN-BUILD-003 — Configuration closure uniqueness.** `.http(...)` and `.runtime(...)` MUST reject duplicate calls rather than silently merge them.

**GEN-BUILD-004 — Observer ownership.** A DSL rate-limit observer MUST configure only the generated default governor; an explicitly supplied limiter MUST remain solely responsible for its observation behavior.

**GEN-BUILD-005 — HTTP override precedence.** Explicit `.http(...)` retry selection MUST override the DSL retry selection, and an overridden dangerous custom retry factory MUST NOT be invoked.

## 5. Client clone semantics

Generated client values are fixed after construction.

```rust
let clone = client.clone();
```

A clone:

- preserves the identical immutable client-value snapshot;
- shares the application and provider Reqwest clients;
- shares credential slots, rejected state, and credential-operation coordination;
- shares hooks and limiter state;
- preserves the same HTTP and runtime configuration.

Concord 0.1 generates no `with_*`, `without_*`, or client-level `reset_*` methods. To use different client-level values, the application constructs another client. Frequently varying values should be declared as scope or endpoint parameters.

**GEN-CLONE-001 — Same-snapshot clone.** Cloning a generated client MUST preserve the identical client-value snapshot and share that client's core runtime and credentials.

**GEN-CLONE-002 — No client derivation methods.** Generated clients MUST NOT expose post-construction methods that replace client-level public values.

**GEN-CLONE-003 — Independent construction.** Separately built generated clients MUST receive independent credential handles and credential-operation state even when their public values compare equal.

## 6. Credential access

Every generated client with credentials exposes:

```rust
pub fn credentials(
    &self,
) -> example_api::credentials::ExampleApiCredentials<'_>;
```

The returned facade borrows the generated client and provides typed credential accessors.

### 6.1 Static API key

DSL credential:

```text
credential application_key = api_key(secret.api_key);
```

Generated accessor:

```rust
pub fn application_key(
    &self,
) -> ApiKeyCredentialHandle;
```

Every generated source-specific handle exposes the underlying typed material handle for advanced prepared-endpoint authentication:

```rust
pub fn credential(
    &self,
) -> CredentialRef<Material>;
```

`Material` is the credential declaration's material type.

Handle operation:

```rust
pub async fn replace(
    &self,
    value: impl Into<SecretString>,
) -> Result<(), AuthError>;
```

### 6.2 Static bearer

Generated `BearerCredentialHandle` with the same one-value `replace` operation.

### 6.3 Basic

Generated `BasicCredentialHandle`:

```rust
pub async fn replace(
    &self,
    username: impl Into<SecretString>,
    password: impl Into<SecretString>,
) -> Result<(), AuthError>;
```

The replacement is atomic.

### 6.4 OAuth2 client credentials

Generated `OAuthClientCredentialsHandle`:

```rust
pub async fn replace_client_credentials(
    &self,
    client_id: impl Into<SecretString>,
    client_secret: impl Into<SecretString>,
) -> Result<(), AuthError>;
```

Token URL and scope are fixed by the DSL declaration in 0.1.

### 6.5 Endpoint-backed and custom credentials

A typed handle exposes:

```rust
pub async fn set(
    &self,
    value: M,
) -> Result<(), AuthError>;

pub async fn clear(
    &self,
) -> Result<(), AuthError>;

pub fn is_set(
    &self,
) -> bool;
```

Endpoint-backed credentials expose `set()`, `clear()`, and `is_set()`. Custom provider credentials expose `clear()` and `is_set()` but not direct `set()`. The generated credential-specific handle type controls method availability.

**GEN-CRED-001 — Typed handles.** Generated credential accessors MUST return credential-specific handle types rather than untyped slot handles.

**GEN-CRED-002 — Shared mutation.** Credential handle mutation MUST affect all related clients sharing that credential.

**GEN-CRED-003 — Atomic compound replacement.** Basic and OAuth client-secret replacement MUST update all required inputs atomically.

**GEN-CRED-004 — Advanced credential reference.** A generated source-specific credential handle MUST expose a typed read-only `CredentialRef<M>` for stable advanced authentication builders without exposing material values or generic mutation.

## 7. Facades

### 7.1 Scope entry

DSL:

```text
scope users(region: String) { ... }
```

Generated:

```rust
pub fn users(
    &self,
    region: String,
) -> example_api::facades::UsersFacade;
```

Required scope values are method arguments in declaration order.

### 7.2 Optional scope values

DSL:

```text
locale: Option<String>
```

Generated facade methods:

```rust
pub fn locale(
    self,
    value: String,
) -> Self;

pub fn clear_locale(
    self,
) -> Self;
```

### 7.3 Defaulted scope values

Generated:

```rust
pub fn page_size(
    self,
    value: u32,
) -> Self;

pub fn reset_page_size(
    self,
) -> Self;
```

The scope default is evaluated once when the facade is entered and retained by that facade snapshot. `reset_page_size()` restores the retained value without re-evaluation.

Facade setters consume and return the facade.

Facades implement `Clone` where generated client and stored scope values support the generated representation.

**GEN-FACADE-001 — Snapshot semantics.** Facade setters MUST create modified facade snapshots and MUST NOT mutate the source facade.

## 8. Endpoint methods

### 8.1 Method name

Without alias:

```text
GET ById(...)
```

generates:

```rust
by_id(...)
```

An explicit `as NAME` alias is used exactly, subject to Rust identifier escaping and collision validation.

### 8.2 Required arguments

Required endpoint values become method arguments in declaration order.

### 8.3 Optional/default values

Optional and defaulted endpoint parameters are not endpoint method arguments.

They become request setters.

### 8.4 Body argument

When present, the body argument follows all required non-body endpoint arguments.

Example DSL:

```text
POST Create(
    tenant_id: String,
    body: Json<CreateUser>,
) -> Json<User> { ... }
```

Generated:

```rust
pub fn create(
    &self,
    tenant_id: String,
    body: CreateUser,
) -> Request<CreateEndpoint>;
```

### 8.5 Endpoint return

Endpoint methods return:

```rust
concord_core::Request<GeneratedEndpointType>
```

The wrapper is also available as `Request` from `concord_core::prelude`, so ordinary generated use
does not import generated-private integration. Generated public signatures and public Rustdoc never
display `concord_core::__private`.

The endpoint method performs no request execution.

**GEN-END-001 — Required argument order.** Required endpoint method arguments MUST preserve declaration order, with the body argument last.

**GEN-END-002 — Request creation only.** Calling a generated endpoint method MUST NOT execute HTTP or invoke body codecs, providers, hooks, or limiters.

**GEN-END-003 — Stable request-wrapper path.** Generated endpoint methods and `client.request(endpoint)` MUST return `concord_core::Request<E>`. Each generated client's `request` method MUST use a public, API-specific, sealed endpoint capability so an endpoint generated for another client, an arbitrary application value, or a downstream implementation is rejected at compile time. The public signature MAY name that API-specific capability, but MUST NOT expose its private seal, generated-private core endpoint bounds, or `concord_core::__private` in public signatures or public Rustdoc.

## 9. Endpoint request setters

Endpoint-specific setters on a facade-created request are methods of a generated public sealed
extension trait implemented only for the exact `concord_core::Request<Endpoint>` type. Application
code brings every such trait for one generated client into scope with one import:

```rust
use example_api::prelude::*;
```

The generated endpoint module may own a locally named `RequestExt` trait. The support prelude
re-exports each endpoint trait under a deterministic collision-resistant public alias containing
the client name, complete scope path, endpoint public name or alias, and `RequestExt`. Consequently,
preludes for several generated clients may be imported together, and endpoints in different scopes
may use the same setter names without a trait-name collision. The generated trait is sealed so
downstream crates cannot implement it for another type.

The trait implementation uses one generated-private core operation that consumes the pending
request, maps its endpoint value, and reconstructs the same `Request<E>` while preserving the client
core and generic request-local options. It performs no request preparation or execution.

### 9.1 Optional endpoint value

```rust
pub fn verbose(
    self,
    value: bool,
) -> Self;

pub fn clear_verbose(
    self,
) -> Self;
```

### 9.2 Defaulted endpoint value

```rust
pub fn limit(
    self,
    value: u32,
) -> Self;

pub fn reset_limit(
    self,
) -> Self;
```

The endpoint default is evaluated once when the pending request is created and retained by that request. `reset_limit()` restores the retained value without re-evaluation.

These endpoint-specific methods are unavailable until their generated trait is in scope. They
consume and return the exact same `concord_core::Request<E>`, so direct await and every terminal
supported by `E` remain unchanged.

### 9.3 Generic request setters

Every request exposes the following inherent core methods without a generated prelude import:

```rust
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
```

`timeout` selects the Concord operation deadline defined by the Execution Specification. It is distinct from connection and read timeouts configured through `HttpConfigBuilder`.

Request setters consume and return the request.

Explicit generated endpoint values use inherent optional/defaulted setters before
`client.request(endpoint)`. They do not require the generated request-extension prelude.

**GEN-REQ-001 — Request-local setters.** Request setters MUST affect only the returned request value.

**GEN-REQ-002 — Timeout distinction.** Timeout, disablement, and inheritance setters MUST remain semantically distinct.

**GEN-DEFAULT-001 — Selected default evaluation.** A client default MUST be evaluated only when selected at build time; scope and endpoint reset methods MUST restore the value resolved when their owning snapshot was created and MUST NOT re-evaluate the DSL default expression.

## 10. Direct await

`Request<E>` implements `IntoFuture` for every valid generated endpoint.

```rust
let value = request.await?;
```

Output mapping:

| DSL response | Direct-await output |
|---|---|
| `Json<T>` | `T` |
| `Text` | `String` |
| custom buffered codec `C` | `<C as DecodeBody>::Output` |
| `Bytes` | `bytes::Bytes` |
| `NoContent` | `()` |
| `Stream<M>` | `StreamResponse<M>` |

**GEN-TERM-001 — Direct semantic output.** Direct await MUST return the endpoint's semantic result without a redundant execution wrapper.

## 11. Metadata terminal

Buffered, bytes, and no-content requests expose:

```rust
pub fn response(
    self,
) -> concord_core::ResponseRequest<E>;
```

The wrapper is also available as `ResponseRequest` from `concord_core::prelude`.

Awaiting the returned value produces:

```rust
Result<DecodedResponse<T>, ApiError>
```

where `T` is the direct-await output.

For `NoContent`, `T` is `()`.

Streaming endpoints do not expose `.response()` because `StreamResponse<M>` already carries successful metadata.

**GEN-TERM-002 — Metadata availability.** `.response()` MUST be available only when it adds meaningful metadata beyond the direct output.

## 12. Dangerous raw response

Under `dangerous-raw-response`, buffered, bytes, text, JSON, custom-codec, and no-content requests expose:

```rust
pub fn raw_response(
    self,
) -> RawResponseRequest<E>;
```

Awaiting returns:

```rust
Result<RawResponse, ApiError>
```

The endpoint decoder is not invoked.

Streaming endpoints do not expose `raw_response()` in 0.1.

**GEN-TERM-003 — Feature-gated raw method.** `raw_response()` MUST be absent when `dangerous-raw-response` is disabled.

## 13. Pagination

A request exposes pagination only when its endpoint has a pagination declaration and its decoded output implements the required `IntoIterator` bound.

### 13.1 Pagination method

```rust
pub fn paginate<L>(
    self,
    limit: L,
) -> PaginatedRequest<E>
where
    L: Into<PageLimit>;
```

The public `Into<PageLimit>` implementations guarantee both canonical forms:

```rust
request.paginate(10_u32).await?;
request.paginate(PageLimit::HardCap(cap)).await?;
```

A `u32` converts to `PageLimit::Take`.

### 13.2 Await result

`PaginatedRequest<E>` implements `IntoFuture` with output:

```rust
Result<Vec<Item>, ApiError>
```

where `Item` is the decoded page's `IntoIterator::Item`.

There is no `.collect()` terminal and no loop-detection opt-out.

**GEN-PAGE-001 — Page-only output.** Pagination MUST return `Vec<Item>` using the decoded page's `IntoIterator` implementation.

**GEN-PAGE-002 — No collect method.** Generated pagination MUST NOT require or expose a separate `.collect()` terminal.

**GEN-PAGE-003 — Conditional availability.** Non-paginated or incompatible endpoints MUST NOT expose pagination methods.

Generated-private integration uses one sealed endpoint capability with an associated item type, an associated controller type, complete initial controller construction, and binding of the current controller into a fresh endpoint clone. Generated code invokes the one core-owned page loop exactly once and contains no page-loop algorithm. The synchronized private generated contract version is `12`.

**GEN-PAGE-004 — Associated typed controller integration.** A generated paginated endpoint MUST expose to core, through a generated-private sealed capability, one associated controller type implementing `PaginationController` for the decoded page, one complete initial-controller operation, one current-controller-to-fresh-endpoint binding operation, and one item type; it MUST NOT pass separate runtime factory, binder, or short-page callbacks, generate another page loop, erase the controller, or expose this integration publicly.

## 14. Endpoint-backed credential acquisition

For:

```text
credential session = endpoint auth::Login;
```

the referenced endpoint request exposes:

```rust
pub async fn acquire_as_session(
    self,
) -> Result<(), ApiError>;
```

The method:

1. executes the endpoint through the ordinary endpoint runtime;
2. decodes the endpoint output;
3. stores it into the generated `session` credential handle;
4. returns `()`.

If endpoint execution fails, the previous credential value remains unchanged.

If credential storage fails, the method returns an authentication error.

If one endpoint backs several credentials, one acquisition method is generated for each compatible credential.

**GEN-AUTHFLOW-001 — Explicit acquisition.** Endpoint-backed acquisition MUST occur only through an explicit acquisition terminal.

**GEN-AUTHFLOW-002 — Atomic success.** Failed endpoint execution MUST NOT replace an existing credential.

## 15. Explicit generated endpoints

Each endpoint has a public explicit endpoint type under:

```rust
example_api::endpoints::scope_path::EndpointName
```

Its constructor accepts, in order:

1. every required enclosing scope value from outermost scope to innermost scope, preserving lexical declaration order within each scope;
2. every required non-body endpoint argument in endpoint declaration order;
3. the request body last, when present.

For:

```text
scope regions(region: String) {
    scope users(tenant: String, locale: Option<String>) {
        GET ById(user_id: String, verbose: Option<bool>) -> Json<User> { ... }
    }
}
```

the explicit constructor is conceptually:

```rust
pub fn new(
    region: String,
    tenant: String,
    user_id: String,
) -> Self;
```

Public constructor parameter bindings use the sanitized DSL names shown above. `GEN-EXPL-002` and
`GEN-NAME-002` govern those public names: collision-resistant private field and helper bindings
remain implementation details instead of replacing ordinary DSL names in the public Rust
signature, and collisions between visible DSL namespaces use deterministic namespace-qualified
public bindings rather than private generated identifiers.

The explicit endpoint exposes the same optional/defaulted enclosing-scope and endpoint setters/resetters needed to construct the immutable snapshots used by facade execution:

```rust
let endpoint =
    example_api::endpoints::regions::users::ById::new(
        region,
        tenant,
        user_id,
    )
    .locale(locale)
    .verbose(true);

let user = client.request(endpoint).await?;
```

`client.request(endpoint)` returns the same `Request<E>` type used by facades. It supplies only the generated client's fixed client-value snapshot, credential handles, and core runtime; it does not infer or replace missing scope values. Every generated API owns a distinct sealed endpoint capability. Only that API's generated endpoint types implement the capability, downstream code cannot implement it, and facade methods construct an endpoint value and call the same capability-constrained request path. No runtime client identifier or dynamic ownership check is used.

The same `request` method accepts stable advanced `PreparedEndpoint<R>` values. Prepared endpoints use the generated client's base URL, core runtime, hooks, limiter, and limits, plus credentials explicitly attached to the prepared endpoint.

Explicit endpoint values do not expose private generated context or storage types.

**GEN-EXPL-001 — Runtime and ownership parity.** Explicit generated endpoints MUST execute through the same capability-constrained request path and runtime as facade-created endpoints and produce the same behavior. Passing an explicit endpoint to a different generated client MUST fail at compile time.

**GEN-EXPL-002 — Scope-complete construction.** An explicit endpoint constructor MUST accept every required enclosing scope value in lexical outer-to-inner order, followed by required endpoint arguments in declaration order and the body argument last when present. The explicit endpoint MUST expose the optional/defaulted scope controls needed for parity with facade-created execution.

**GEN-EXPL-003 — No inferred scope.** `client.request(explicit_endpoint)` MUST NOT infer a required scope value from unrelated client state.

## 16. Names and collisions

### 16.1 Rust name conversion

PascalCase declarations are converted to snake case for methods and modules unless an explicit alias is supplied.

Rust keywords use raw identifiers where that preserves an unambiguous public API.

### 16.2 Collision detection

The macro rejects collisions involving:

- generated client methods;
- builder setters;
- credential accessors;
- facade methods;
- endpoint setters;
- endpoint request extension trait names;
- generated-prelude trait aliases;
- request-wide setters;
- terminal methods;
- endpoint-backed acquisition methods;
- explicit endpoint module/type paths.

Reserved generated method names include at least:

```text
new
builder
build
http
runtime
credentials
request
response
raw_response
paginate
timeout
disable_timeout
inherit_timeout
debug
```

The macro may reserve additional names required by stable public APIs.

Every generated-prelude trait alias is derived deterministically from the client identity, full
scope path, endpoint public name or alias, and `RequestExt`. Alias generation participates in
collision validation, including collisions across multiple clients in one macro invocation.

**GEN-NAME-001 — Compile-time collisions.** Generated public name collisions MUST be rejected during macro expansion.

**GEN-NAME-002 — Deterministic normalization.** Name conversion and collision diagnostics MUST be deterministic.

## 17. Ownership and thread behavior

Generated clients and facades own `Arc`-backed immutable snapshots.

Endpoint and request values own their request-local data.

A request is `Send` when all stored application values, body values, and selected public extension objects satisfy their required `Send` bounds.

A generated client is `Send + Sync` when declared client values and custom providers satisfy the generated bounds.

The macro should not add unnecessary `Clone` bounds to user value types. Internal `Arc<T>` storage permits cloning generated clients and facades without cloning `T`.

**GEN-OWN-001 — No borrowed request lifetime.** Normal generated endpoint requests SHOULD own their state and SHOULD NOT borrow a facade or mutable client.

**GEN-OWN-002 — Minimal bounds.** Generated code MUST NOT require `Clone` solely to implement generated client or facade cloning when `Arc` storage can avoid that bound.

## 18. Generated Rustdoc

Generated public types and methods include Rustdoc describing:

- DSL endpoint identity;
- HTTP method;
- public route shape without resolved values;
- required arguments;
- optional/default setters;
- the generated support-prelude import required by endpoint-specific request setters;
- the endpoint-specific sealed request extension trait and its exact request type;
- request body mode;
- response mode;
- authentication placement categories without secret values;
- attached rate-limit rule names;
- timeout behavior;
- pagination availability and page-limit semantics;
- feature requirements;
- dangerous capability warnings.

**GEN-DOC-002 — Rustdoc secret exclusion.** Generated Rustdoc MUST NOT include:

- secret values;
- native URLs;
- credential material;
- request or response bodies;
- rate-limit key values;
- cursor values;
- custom retry internals;
- private generated traits or storage types.

The public generated request extension trait and its public prelude alias are supported generated
API and are documented. Their private seal, the core endpoint mapping operation, endpoint execution
traits, and private request fields are not rendered.

**GEN-DOC-001 — Public-only documentation.** Generated Rustdoc MUST describe only supported public behavior and APIs.

## 19. Canonical mapping table

| DSL | Generated Rust |
|---|---|
| `var tenant: String` | builder `.tenant(T)`; fixed on the built client |
| `var locale: Option<String>` | builder/facade/request `.locale(T)`, `.clear_locale()` according to ownership level |
| `var size: u32 = 100` | builder/facade/request `.size(T)`, `.reset_size()` according to ownership level |
| `secret api_key: String` | builder `.api_key(SecretString)` |
| `credential x = custom T` | builder `.x_provider(provider)` |
| `scope users(region: String)` | `client.users(region)` |
| `GET ById(id: String)` | `.by_id(id)` |
| endpoint `Option<bool>` | request `.field(value)`, `.clear_field()` |
| endpoint default | request `.field(value)`, `.reset_field()` |
| `body: Json<T>` | method argument `T` |
| `body: Text` | method argument `String` |
| `body: Stream<M>` | method argument `StreamBody` |
| `body: Multipart` | method argument `MultipartBody` |
| `-> Json<T>` | await `T` |
| `-> Text` | await `String` |
| `-> Bytes` | await `Bytes` |
| `-> NoContent` | await `()` |
| `-> Stream<M>` | await `StreamResponse<M>` |
| endpoint `paginate ...` | `.paginate(n).await -> Vec<Item>` |
| endpoint-backed credential | `.acquire_as_name().await -> Result<(), ApiError>` |
