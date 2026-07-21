# CONCORD_CORE_ARCHITECTURE_0.1

**Version:** 0.1  
**Status:** Internal Implementation Candidate  
**Authority:** Intended runtime implementation architecture  
**Depends on:** All normative Concord 0.1 specifications  
**Supersedes:** None

## 1. Purpose

This document describes one clean implementation architecture satisfying the Concord 0.1 normative specifications.

It does not redefine public grammar, public signatures, execution ordering, limits, or security guarantees.

Normative references use requirement IDs such as `PROD-CORE-001`, `EXEC-ORDER-001`, and `SEC-URL-001`.

## 2. Architectural objectives

The core architecture is optimized for:

- concrete Reqwest ownership;
- compile-time typed endpoint output;
- cheap immutable client/facade snapshots;
- narrow dynamic-dispatch boundaries;
- one application execution engine;
- independent stateful auth and limiter subsystems;
- deterministic testing without production transport generics;
- straightforward security review.

The architecture deliberately avoids:

- a public transport abstraction;
- runtime endpoint output erasure;
- a global client context trait;
- a general middleware chain;
- a general retry loop;
- a cache engine;
- public request/auth/pagination plans.

## 3. Crate and module responsibilities

### 3.1 `concord_core`

The runtime crate owns:

- `ClientCore` construction and cloning;
- safe request-head preparation primitives;
- typed request/response execution;
- body adaptation and limits;
- credential slots and provider HTTP;
- rate-limit invocation and built-in governor;
- pagination runtime;
- hooks and built-in debug dispatcher;
- errors and redaction;
- generated-private integration traits.

### 3.2 `concord_macros`

The macro crate owns the declarative API compiler. It generates implementations against `concord_core::__private` but does not know Reqwest internals.

### 3.3 `concord_examples`

Examples are application consumers, not implementation helpers.

## 4. Dependency direction

Recommended logical dependency graph:

```text
error, redaction, config
        ↓
routing, headers, timeout
        ↓
codec, body, response primitives
        ↓
auth, rate_limit, hooks
        ↓
request head + execution engine
        ↓
client core and generated integration
```

Rules:

1. Authentication does not depend on generated client types.
2. Rate limiting does not depend on credential-state internals.
3. Codecs do not depend on client or Reqwest request objects.
4. Pagination depends on ordinary request execution but not native HTTP.
5. The generated-private layer adapts generated values into stable core primitives; core feature modules never depend on macro AST or IR.
6. Error and redaction modules sit below every subsystem.

## 5. Core client ownership

Recommended type:

```rust
#[derive(Clone)]
pub struct ClientCore {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    base_url: Url,
    application_http: ApplicationHttp,
    provider_http: ProviderHttpInner,
    hooks: Arc<dyn RuntimeHooks>,
    limiter: Arc<dyn RateLimiter>,
    limits: RuntimeLimits,
    debug: DebugConfig,
}
```

`ClientInner` is immutable after construction (`PROD-STATE-001`).

The private application wrapper retains the resolved `https_only` bit in addition to applying it
to Reqwest, so later public preflight can reject every HTTP URL without relying on a native send.
Additional configured roots are installed on both private managed clients. The configured client
identity belongs only to the application client; the provider client has an independent pool and
does not present application mTLS identity.

It does not own generated client values or one global credential registry. Generated credentials live in generated credential storage and are referenced by prepared auth requirements.

Advantages:

- a core client is shared by clones of one generated client snapshot;
- hand-written prepared endpoints can use the same `ClientCore`;
- credential source shape remains generated-client-specific without contaminating core ownership;
- no mutable global variable bag is needed.

## 6. Generated client ownership

Recommended generated shape:

```rust
#[derive(Clone)]
pub struct ExampleApi {
    core: ClientCore,
    vars: Arc<ExampleApiVars>,
    credentials: Arc<ExampleApiCredentials>,
}
```

### 6.1 Public values

One outer `Arc` stores the complete immutable client snapshot:

```rust
struct ExampleApiVars {
    tenant: String,
    locale: Option<String>,
    page_size: u32,
}
```

Cloning the generated client clones the outer `Arc` and therefore requires no `Clone` bound on individual user values.

Concord 0.1 does not derive a new client by replacing one client value. A differently configured client is constructed independently and receives independent core and credential state.

### 6.2 Credentials

Generated credential storage contains typed handles or generated wrappers around typed handles:

```rust
struct ExampleApiCredentials {
    application_key: ApiKeyCredentialHandle,
    session: CredentialHandle<AccessToken>,
}
```

Credential state is shared only by cloning the same generated client snapshot. Separate builder executions create separate credential slots and provider-operation coordination.

## 7. Construction assembly

The generated builder stores resolved user inputs and configuration values, not user configuration closures:

```rust
struct ExampleApiBuilder {
    values: GeneratedBuilderValues,
    providers: GeneratedProviderValues,
    http: HttpConfigBuilder,
    runtime: RuntimeConfigBuilder,
    http_configured: bool,
    runtime_configured: bool,
    errors: Vec<ClientBuildError>,
}
```

`.http(F)` and `.runtime(F)` invoke `F` immediately. A duplicate call records an error without invoking the duplicate closure.

Build stages follow `EXEC-CONSTRUCT-*`:

1. resolve required/optional inputs and evaluate only selected client defaults;
2. build immutable `ExampleApiVars`;
3. validate base URL and fixed client-level declarations;
4. materialize safe HTTP configuration into a plain `HttpConfig`;
5. perform fixed TLS preflight;
6. build application and provider Reqwest clients;
7. materialize positive finite `RuntimeLimits` and `DebugConfig`;
8. construct or validate the limiter;
9. create independent generated credential slots and handles;
10. create `ClientCore` and the generated client.

The dangerous retry factory is invoked only after ordinary validation proves that it remains effective.

## 8. Facades and endpoint values

A facade is a generated immutable snapshot:

```rust
#[derive(Clone)]
pub struct UsersFacade {
    client: ExampleApi,
    region: Arc<String>,
    locale: Arc<Option<String>>,
}
```

Endpoint calls create an owned endpoint value:

```rust
struct ByIdEndpoint {
    vars: Arc<ExampleApiVars>,
    credentials: Arc<ExampleApiCredentials>,
    region: Arc<String>,
    user_id: Arc<String>,
    verbose: Arc<Option<bool>>,
}
```

The endpoint value does not contain a native URL, native headers, or Reqwest body.

## 9. Public request wrapper

Stable crate-root core wrappers:

```rust
pub struct Request<E> {
    core: ClientCore,
    endpoint: E,
    options: RequestOptions,
}

pub struct ResponseRequest<E> {
    request: Request<E>,
}
```

Their exact public paths are `concord_core::Request<E>` and
`concord_core::ResponseRequest<E>`; both are also exported from `concord_core::prelude`.
`Request<E>` owns every request-local input. It does not borrow its facade or client. Wrapper fields
remain private.

Generic setters are inherent core methods that update `RequestOptions`. Endpoint-specific setters
are methods of public generated sealed extension traits implemented for the exact
`Request<GeneratedEndpoint>` type. A generated support-module prelude re-exports every endpoint
trait under a client- and path-qualified deterministic alias, so one glob import enables all
endpoint setters for that client without exposing generated-private integration.

Each extension method calls one doc-hidden generated-private core bridge that consumes
`Request<E>`, maps `E -> E`, and reconstructs the same request while preserving `ClientCore` and
`RequestOptions`. The bridge neither polls the request nor prepares its head, body, or transport.
It adds no user-value `Clone` bound and is the only core operation needed by every generated
endpoint setter. Explicit endpoint values remain local generated types and use inherent setters
before they enter `Request<E>`.

The wrappers implement `IntoFuture` with a publicly nameable boxed future alias without allocating a
runtime endpoint descriptor. Generated-private `EndpointSpec`, `ClientCore`, and execution bounds
remain hidden from generated public signatures and public Rustdoc. Public `RequestBodySpec` and
`ResponseBodySpec` are intentional extension contracts; their native adapters and executor remain
private.

## 10. Generated-private endpoint integration

The core uses one small sealed trait to bridge generated endpoint values into the executor.

Recommended shape:

```rust
pub trait EndpointSpec: Send + 'static {
    type Body: RequestBodySpec;
    type Response: ResponseBodySpec;

    fn prepare_head(
        &self,
        base: &Url,
        options: &RequestOptions,
    ) -> Result<PreparedHead, ApiError>;

    fn body_metadata(
        &self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError>;

    fn prepare_body(
        self,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError>;
}
```

This trait is generated-private, not an application extension.

The endpoint mapping bridge is also generated-private. It is a structural ownership operation on
the existing pending wrapper, not an executor, endpoint plan, public mutation API, or second request
wrapper. Generated extension-trait signatures name only the stable `Request<E>` and declared value
types; they do not name `EndpointSpec`, `ClientCore`, `RequestOptions`, or `__private`.

Borrowed public-head preparation and request-specification metadata permit route, auth-collision,
TLS, content-type ownership, exact-length, and lower-bound validation before consuming or encoding
the body. `prepare_body(self)` dispatches the selected public `RequestBodySpec::prepare` exactly once
after hook approval and transfers the input into the one managed `RequestBody`. A bodyless endpoint
uses one core-private empty specification and exposes no public input.

Because the stable prepared type is parameterized only by response specification `R`,
`PreparedEndpoint<R>` stores its selected request specification behind one private owned adapter.
That adapter captures `B::Input` and delegates exactly to `B::metadata` and `B::prepare`; it has no
independent materialization, retry, native body, executor, or output-downcast behavior. This private
storage erasure preserves the one public request lifecycle without adding `B` to the prepared public
type.

One-shot bodies need no `Mutex<Option<T>>` workaround.

## 11. Prepared public head

Recommended private structure:

```rust
struct PreparedHead {
    meta: RequestMeta,
    method: Method,
    logical_url: Url,
    safe_url: Arc<str>,
    public_headers: HeaderMap,
    timeout: TimeoutSetting,
    auth: Arc<[AuthRequirement]>,
    rate_limits: RateLimitPlan,
}
```

The head contains no body and no prepared credentials.

`PreparedHead` is immutable and reused by authentication recovery (`EXEC-HEAD-002`).

## 12. Head builder

Generated code should call a compact generated-private `HeadBuilder` rather than construct internal
fields directly. Normal generated code emits lexical declaration IDs, static costs/windows, and
`RateLimitKey::global()`; it never formats request values into rate keys. Prepared/custom code may
continue to supply an explicit `RateLimitPlan` containing bounded key parts.

Responsibilities:

- append or replace validated host/path/query/header facts already normalized by the macro;
- convert request-shaping values through `Display` only where the ordinary route/query/header
  contract requires it;
- validate header ownership;
- accept static generated lexical rules with `RateLimitKey::global()`;
- preserve explicit bounded `RateLimitKey` values supplied by prepared/custom users;
- add typed credential handles as private erased requirements;
- produce safe/logical URLs;
- verify TLS capability.

The builder should not reproduce DSL inheritance; that is macro normalization work.

## 13. Typed response architecture

Normal responses are statically typed through the stable public `ResponseBodySpec`. Its associated
`Mode` selects one of three public markers whose `ResponseMode` implementation is sealed inside
core:

```text
Buffered  -> Bytes
Streaming -> ByteStream
Discard   -> ()
```

The executor owns successful status classification, mode-specific known-length preflight, native
response ownership, bounded acquisition, deadline checks, error reduction, and completion. It
constructs a borrowed logical `ResponseHead`, calls `R::validate` before the first body poll, and
calls `R::from_body` only after the selected mode body is ready. No application specification
receives a native response or can introduce a fourth mode.

One blanket adapter maps every `DecodeBody` codec to buffered `ResponseBodySpec`. `Text`, `Json<T>`,
`Bytes`, `NoContent`, `Stream<M>`, and feature-gated `Records<D>` use the same contract. Dangerous
raw success remains a separate private capability that reuses bounded execution but skips the
selected specification; it is not a public acquisition mode.

`ApiResponse<T>` is the single successful metadata wrapper for every mode.
`DecodedResponse<T>` is only a type alias and there is no `StreamResponse<M>` implementation.

## 14. Body architecture

### 14.1 Public request specification

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

`metadata()` is structural and does not invoke codecs, body or multipart factories, streams, file
I/O, or native body construction. It reports request content-type ownership, body kind, and
trustworthy exact/lower-bound length facts; it cannot label replayability.

`prepare()` runs synchronously exactly once after before-send hook approval. Core validates its
result against metadata before protected phases. Built-in and downstream implementations all
return the one stable managed `RequestBody`; there is no parallel outgoing-body type or lifecycle.

### 14.2 Managed request-body forms

The public `RequestBody` has private representation. Its internal forms represent behavior:

```text
Empty
ReusableBytes
OneShotStream
StreamFactory
ReplayableMultipart
OneShotMultipart
MultipartFactory
```

Each form provides current-attempt native body construction, replayability inferred by core, safe
body-kind metadata, and known length. Metadata/produced-body validation rejects kind, exact or lower
bound, content-type ownership, generated ownership, body-presence, and impossible-state
contradictions before credentials, limiter, debug, authentication, native construction, or send.

### 14.3 Buffered codec

The generic `C: EncodeBody` request-specification adapter owns `C::Input`. `prepare` invokes
`C::encode`, validates returned bytes, and creates reusable `Bytes`. A complete custom codec
implements only `EncodeBody`.

### 14.4 Native body wrapper

All streaming native bodies pass through one private wrapper that counts data bytes, enforces exact length, captures safe producer errors, and exposes no body data to diagnostics.

### 14.5 Multipart

Reqwest owns multipart serialization. Concord wraps the resulting Reqwest body with the same aggregate byte-counting layer.

### 14.6 Byte and record streams

`ByteStream` privately owns exactly one native successful response body plus its cumulative
post-decompression counter and finite limit. It owns no response metadata or media marker, withholds
the first excess chunk, safely reduces native errors, and cancels by ordinary drop. It is the only
streaming acquisition body delivered to `ResponseBodySpec`.

Under `records`, `RecordStream<D>` owns one `ByteStream`, one `D: RecordDecoder`, one `BytesMut`
staging buffer, and explicit EOF/progress state. Decoder calls run synchronously without a core lock.
The independent positive staging bound is checked before append, so offending bytes and oversized
framed records are withheld. `RecordBatches<D>` is a consumer adapter around one record stream, not
another response mode or executor. `Csv<T>` and `NdJson<T>` are isolated decoder implementations
under their features and use a 1 MiB staging bound.

## 15. Execution engine structure

The executor is generic over the endpoint request and response specifications and follows the
normative ordering in `EXEC-ORDER-001`.

Suggested private decomposition:

```text
record_operation_start_and_id
prepare_public_head
compute_request_spec_metadata_and_known_checks
run_before_send_hook
prepare_request_spec_once
validate_metadata_against_request_body
materialize_managed_body_for_attempt
preflight_credentials
acquire_limiter
finalize_credentials
build_and_run_attempt
observe_response_head
handle_challenge
capture_terminal_status_or_classify_success
preflight_response_mode_length
validate_response_spec_before_poll
acquire_sealed_mode
convert_response_spec_and_check_deadline
emit_request_or_page_completion
```

Every awaited phase receives or derives the remaining operation-deadline budget. Authentication recovery reuses the original start/deadline rather than constructing a new budget.

These helpers are private and do not form a middleware API.

## 16. Private auth erasure boundary

Authentication requirements are heterogeneous, so private type erasure is appropriate at the credential materialization boundary.

Recommended private trait:

```rust
trait ErasedCredential: Send + Sync {
    fn id(&self) -> CredentialId;
    fn recovery_mode(&self) -> RecoveryMode;

    fn preflight<'a>(...)
        -> BoxFuture<'a, Result<PreparedCredential, AuthError>>;

    fn finalize<'a>(...)
        -> BoxFuture<'a, Result<FinalCredential, AuthError>>;

    fn reject_exact(...)
        -> ExactRejectionResult;

    fn recover_exact<'a>(...)
        -> BoxFuture<'a, Result<PreparedCredential, AuthError>>;
}
```

This is a valid dynamic-dispatch boundary because:

- endpoint output typing remains static;
- application code never implements the erased trait directly;
- material heterogeneity is intrinsic to one auth plan;
- the trait contains only credential preparation and exact-version rejection/recovery behavior.

Private `PreparedCredential` contains:

- protected material representation retained only for preflight/finalization;
- selected slot-reference identity;
- exact applied token/version identity;
- recovery mode.

Private `FinalCredential` is produced only after limiter acquisition. It identifies the currently selected valid slot/version and is the only material accepted by native authentication insertion.

No protected value is exposed to hooks or the limiter.

## 17. Credential ownership

A public read-only `CredentialRef<M>` and any source-specific/mutable handle point to a replaceable slot reference:

```rust
struct CredentialSlotRef<M> {
    current: RwLock<Arc<CredentialSlot<M>>>,
}
```

Static compound input replacement constructs a new slot and atomically swaps the `Arc`.

Benefits:

- no partial Basic/OAuth input state;
- in-flight operations continue against their old isolated slot;
- stale old-slot completions cannot alter the replacement slot;
- public replacement semantics remain atomic (`GEN-CRED-003`).

The slot state machine is specified separately by `CONCORD_AUTH_STATE_MACHINE_0.1`.

## 18. Request-local auth preparation cache

One Concord attempt uses a private map:

```rust
HashMap<CredentialIdentity, PreparedCredential>
```

When multiple placements reference the same credential, preflight occurs once per attempt. After limiter acquisition, every entry is finalized against the current public slot reference. Finalization may reuse a newer valid version or reenter coordinated acquisition/refresh under the remaining deadline.

The map and finalized material are discarded after the attempt.

The identity is opaque and body/secret-free.

## 19. Provider HTTP architecture

`ProviderHttp` wraps a private `ProviderHttpInner` containing the provider Reqwest client and mandatory request/response limits. It shares configured additional trust roots with the application client but never receives the application client identity; Concord 0.1 has no provider-specific identity configuration. When invoked while preparing an application operation, it also receives the remaining parent operation-deadline budget and applies the earlier of that budget and the provider request timeout.

Provider bodies are limited to:

- empty;
- reusable bytes;
- replayable factory.

Direct arbitrary provider streams are intentionally excluded in 0.1 to keep provider operations bounded and retryable.

## 20. Rate-limit integration

The prepared head owns a canonical `RateLimitPlan`.

Normal generated plans are produced by lexical traversal: optional client declaration, outer-to-inner
scope declarations, then optional endpoint declaration. Each declaration has one deterministic ID
derived from the generated module path and complete lexical declaration path. Every normal generated
rule uses a global key, so scope and endpoint argument values, hosts, methods, and pagination
continuations do not partition buckets. A direct rate-limit declaration has no ancestor opt-out or
replacement operation. The advanced prepared/custom boundary remains the only source of explicit
`RateLimitKeyPart` values.

The executor invokes the selected `RateLimiter` after credential preflight and before credential finalization and native construction.

The built-in governor should use one short-held mutex around:

- pruning;
- token refill calculations;
- wait computation;
- atomic multi-rule reservation;
- cooldown insertion.

It releases the mutex before sleeping.

This architecture avoids partial reservation and quota refund complexity (`EXEC-RATE-002`). Bounded LRU state makes the built-in governor best-effort process-local pacing: eviction may permit a fresh local burst, while strict or distributed enforcement remains a custom-limiter responsibility.

Limiter response observation returns `()` and cannot replace the current response.

## 21. Pagination integration

Generated paginated endpoints implement one uniform sealed trait for every controller type.

Common typed facts are:

```rust
pub trait PaginatedEndpoint:
    EndpointSpec + Clone
where
    Self::Response: PageResponseSpec,
    <Self::Response as ResponseBodySpec>::Output:
        IntoIterator,
{
    type Item: Send + 'static;
    type Controller:
        PaginationController<
            <Self::Response as ResponseBodySpec>::Output
        >;

    fn initial_controller(
        &self,
    ) -> Result<Self::Controller, PaginationError>;

    fn bind_controller(
        endpoint: &mut Self,
        controller: &Self::Controller,
    ) -> Result<(), PaginationError>;
}
```

The generated constructor emits one complete ordinary controller struct literal. For each declared
`bind`, it reads the original request-local endpoint or scope snapshot once. Before each page the
core validates the current controller, clones the original endpoint, and asks the generated sealed
capability to write each current bound controller field into that fresh clone. Ordinary endpoint
preparation then shapes host, path, query, public headers, current rate dimensions, and canonical
identity. Unbound controller fields remain controller-local and are never rebound.

The runtime owns the single loop, current typed controller, page index, configured ceilings, seen
logical identities and retained identity bytes, result vector, and aggregate completion counters.
It receives no factory, binder, or short-page callback. Count termination is read directly from the
controller; no empty-page or built-in policy exists independently in core.

No `Any`, state registry, public binding protocol, endpoint-generic public factory, controller-name
or response-name branch, or runtime downcast is used. `Bytes`, `NoContent`, `Stream<M>`, and
`Records<D>` have no accidental page capability. Controller methods execute outside core locks, and
the request-local controller needs `Send` but not `Clone` or `Sync`.

## 22. Canonical page identity

The implementation follows `EXEC-PAGE-009` exactly. Architecture does not redefine equality.

A private identity stores the full structural values required for equality and an optional precomputed hash. Hash matches are confirmed by full equality. Identity storage tracks retained bytes and fails closed before exceeding `BOUND-PAGE-009`.

Authentication is excluded. Identity values are never rendered in pagination errors.

## 23. Hooks and debug integration

`ClientInner` owns one `Arc<dyn RuntimeHooks>`.

The hook object is invoked by the execution engine only at normative lifecycle points, including one aggregate pagination completion callback per collection. Stream-return completion uses `StreamOpened`. Every event receives an opaque operation identifier; page events also receive their aggregate pagination parent identifier. Completion metadata uses the operation's monotonic start instant through result fixation and the final response status when available.

Result-determining hook work runs under the operation deadline. After fixing the result, outcome,
status, and elapsed duration, the executor awaits completion observation outside the deadline and
then returns the unchanged result. Completion is never detached, may delay result delivery, and is
bounded by the extension author rather than a second core timeout.

Built-in debug output is a separate internal dispatcher controlled by `DebugConfig`; it is not another application extension trait.

Why keep debug internal:

- `RuntimeHooks` already provides structured application observability;
- debug is a convenience presentation layer;
- one public callback taxonomy avoids duplicate lifecycle semantics.

Request-local debug override is stored in `RequestOptions` and resolved at execution.

## 24. Error architecture

`ApiError` should be a small public wrapper around private typed kinds.

Principles:

- immediate native error reduction;
- no raw Reqwest error retained;
- terminal non-success bodies stored only as finite complete `StatusBody` bytes inside `HttpResponseFailure`;
- no partial terminal body retained after limit overflow;
- terminal body capture failure preserves status and records structural capture state;
- subsystem kind accessors avoid message parsing;
- application-provided provider/limiter/controller/hook errors remain trusted inputs only where the public contract allows them.

Large internal error payloads may be boxed privately. Enum size is not a public contract.

## 25. Redaction architecture

Use one central redaction module for:

- sensitive-name classification;
- safe URL rendering;
- borrowed sanitized headers;
- owned sanitized status headers;
- wrapper `Debug` helpers;
- safe native error reduction.

Subsystems must not implement independent ad hoc sensitive-name lists.

## 26. Deterministic executor seam

Application and provider HTTP wrappers should contain a development-only optional executor branch behind `cfg(test)` or `dangerous-dev-tools`.

Production client and generated types remain unchanged.

The branch exists at the final Reqwest request execution boundary, after auth materialization, so tests exercise:

- public validation and request-specification metadata;
- before-send hooks;
- request-specification preparation, consistency validation, and managed-body materialization;
- credential preflight and post-limiter finalization;
- limiter;
- auth insertion;
- sealed response-mode processing and record streaming.

Ordinary deterministic capture remains safe; explicit fake-value expectations remain inside test infrastructure.

## 27. Locking rules

1. No lock across await.
2. Credential locks protect only state transitions and waiter registration/publication.
3. Slot-reference replacement lock is held only to clone or replace an `Arc`.
4. Governor lock is held only for local calculations and atomic reservation/update.
5. Client configuration requires no lock.
6. Request and pagination state is exclusively owned by one future.

Poisoned-lock behavior should fail safely with an internal error rather than continue without enforcement.

## 28. Cancellation ownership

Cancellation behavior should emerge from ownership:

- provider operation guard owns rollback/publication responsibility;
- limiter sleeping future owns no reserved quota;
- one-shot body lives in the active native request;
- buffered response future owns body polling;
- returned `ByteStream` owns later native polling;
- returned record or batch stream owns decoder state, staging, and its `ByteStream`;
- pagination future owns result/controller/identity state.

No asynchronous work should be spawned merely to simplify cancellation unless the contract requires work to outlive the caller. Concord 0.1 has no such requirement.

## 29. Allocation strategy

- `Arc` for shared immutable client/facade values;
- `Bytes` for reusable body data;
- one small `Vec` for ordered query pairs;
- one `HeaderMap` for public request headers;
- one request-local auth preflight map plus short-lived finalized material;
- one pagination result vector, bounded identity set, and aggregate counters;
- one bounded `BytesMut` staging buffer per record stream;
- boxed futures only at dynamic extension boundaries;
- monomorphized request and response specifications for normal output paths.

Avoid cloning full request bodies or decoded pages except where user types explicitly do so.

## 30. Unsafe code

The runtime and macro crates should use:

```rust
#![forbid(unsafe_code)]
```

No architectural requirement justifies unsafe code in 0.1.

## 31. Suggested module layout

```text
concord_core/src/
├── lib.rs
├── client/
├── request/
├── execute/
├── http/
├── routing/
├── headers/
├── codec/
├── body/
├── response/
├── records/              # feature-gated, integrated over response/ByteStream
├── auth/
├── rate_limit/
├── pagination/
├── hooks/
├── error/
├── redaction/
├── __private.rs
└── __development.rs
```

Files should be split by responsibility, not merely by trait/type count.

No module should become a generic dumping ground named `context`, `plan`, `types`, or `utils` without a narrow domain responsibility.

## 32. Rejected alternatives

### 32.1 Public transport trait

Rejected because it leaks protected material, duplicates Reqwest abstractions, complicates streams/errors/retry, and makes generated types generic.

### 32.2 `ClientContext` god trait

Rejected because routing, public values, auth registry, state, and policy are distinct concerns. Generated immutable storage plus narrow core types is simpler.

### 32.3 Runtime `Any` response plans

Rejected because generated endpoint output is already statically known and downcasting converts compile-time errors into runtime failures.

### 32.4 General middleware chain

Rejected because execution ordering is security-sensitive and extension contracts are intentionally narrow.

### 32.5 General Concord retry loop

Rejected because Reqwest owns native retry and a second loop complicates request counts, hooks, limiter accounting, and body replay.

### 32.6 Cache in core

Rejected because correct HTTP caching is a separate protocol domain entangled with authorization, `Vary`, revalidation, and stale behavior.

### 32.7 Mutable public client values

Rejected because fixed independently built client snapshots prevent credential reuse across changed tenant/region/audience configuration and keep clone semantics straightforward.

### 32.8 Item-based pagination

Rejected because it requires generic runtime knowledge of page internals and creates truncation/proof semantics unrelated to page transport.

### 32.9 Unsealed or native response extensions

Rejected because a fourth acquisition mode or native response access could bypass status, bounds,
deadline, cancellation, completion, and Reqwest ownership.

### 32.10 Separate prepared or records runtime

Rejected because generic prepared endpoints and incremental record framing compose with the one
request/response lifecycle and executor; another runtime would duplicate security-critical order.

## 33. Architecture acceptance criteria

The implementation architecture is acceptable when:

- generated clients have no transport/executor generic;
- public advanced code needs no private import;
- normal endpoint outputs use no runtime downcast;
- arbitrary request and response specifications use public-only contracts and one executor;
- response acquisition has exactly three core-sealed modes and no native escape;
- public client values require no mutation locks;
- one managed request body and `ByteStream` enforce body consumption by ownership;
- record staging is independently positive and finite;
- auth type erasure is private and limited to heterogeneous credential materialization;
- pagination controller state and page-response capability are typed;
- one execution engine serves generated and prepared endpoints;
- all normative ordering/security/bounds pass their conformance tests.
