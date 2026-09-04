# Planning, descriptions, observation, and errors

Concord exposes several inspection/evidence products. They share one semantic contract but have different authority and runtime state.

## Four products

1. **Static contract description** — value-free declarations and provenance.
2. **Effective endpoint description** — inherited normalized endpoint contract.
3. **Request plan** — one bound credential-free public request projection.
4. **Operation evidence** — runtime identities, stages, waits, and outcomes.

Do not conflate them or call all of them “request metadata.”

## Static and effective descriptions

Generated entry points:

```rust
CatalogApi::contract()
request.description()
intent.description()
```

Descriptions include:

- client/scope/endpoint tree;
- docs/deprecation and stable/derived identity;
- exact value types, presence, defaults, sensitivity, and controls;
- local and effective request/response representation plus origin;
- route/query/header/auth/rate patches and provenance;
- status, effect, timeout, body/response capabilities;
- pagination bindings and metadata interests;
- available terminals;
- opaque custom behavior marked as opaque rather than guessed.

Because representation defaults are inherited, the effective representation and owner must appear near every endpoint signature in generated Rustdoc and tooling.

## Request plan

```rust
let plan = request.plan()?;
let plan = client.plan(&unbound_intent)?;
```

A plan is inert, credential-free, redaction-aware, and not universally safe for unrestricted disclosure.

It may include:

- stable endpoint ID and owner path;
- method and logical origin;
- actual public path/query/header structure;
- ordinary public values through explicit accessors;
- sensitive values as structural redaction markers;
- private auth target/source structure without material;
- body category/representation, not content;
- exact output and caller-lifecycle category;
- response representation and negotiation metadata;
- status, effect, timeout;
- rate schemas without resolved key values;
- pagination and terminals;
- local/effective provenance.

Planning performs bounded public atom encoding/validation. It does not invoke body preparation, provider, limiter, retry, observer, native HTTP, endpoint-backed storage, or completion.

## Plan versus execution

Both consume the same semantic shaping program. `RequestAtom` promises deterministic output for an unchanged semantic value. Within each pass, real and sanitized views derive from one atom emission.

Planning and later execution may perform separate passes; exact byte identity is not promised for an extension that violates its deterministic contract. A public frozen resolved request remains deferred.

## Information classes

```text
ordinary public protocol data
sensitive public protocol data
private credential data
body/content data
operator-only bounded diagnostics
```

### Sensitivity propagation

A `#[sensitive]` source redacts complete derived atoms, including `concat` and every `each` occurrence. It still affects the real request and internal pagination identity.

No plan, observer, error, or debug path may downgrade sensitivity.

## Operation observer

```rust
pub trait OperationObserver: Send + Sync + 'static {
    fn observe(&self, event: OperationEvent<'_>);
}
```

Properties:

- synchronous and borrowed;
- returns unit;
- cannot mutate or veto request/result/retry/auth/rate;
- prompt, finite, non-blocking, no external I/O, no panic by contract;
- event views are bounded and sanitized;
- core contains observer panics and checks the deadline before and after every
  intermediate callback, but cannot preempt synchronous Rust code;
- time spent in an intermediate callback consumes the remaining budget after
  it returns; if the deadline expired, no later remote work starts;
- completion observation runs after the semantic result is fixed, so a slow
  final callback can delay delivery beyond the nominal deadline but cannot
  replace that result.

Applications needing asynchronous export use an extras/application-owned bounded queue with explicit drop/backpressure/shutdown policy. Core does not own a telemetry worker.

## Event vocabulary

The active event set is:

```text
AttemptStarted
ResponseHeadObserved
OperationFinished
PaginationFinished
StreamFinished
```

Transport-private physical retries appear only as bounded structural evidence,
not independent visible attempts.

Events may expose:

- endpoint/operation/page/attempt identities;
- method and effect;
- credential-sanitized logical URL and sanitized public request headers on
  `AttemptStarted`;
- request body category and known length;
- status, safely parsed content length, and standard server delay on
  `ResponseHeadObserved`;
- final outcome, timing, attempt/retry/recovery counts, and stream lifecycle.

They never expose response-header values, bodies, output values, credentials,
private auth, raw rate keys, or native objects. `DebugLevel::Headers` adds only
sanitized public request headers; response output remains typed and value-free.

## Error model

Stable top-level categories:

```rust
pub enum ErrorCategory {
    Configuration,
    RequestEncoding,
    RequestBody,
    AuthenticationUnavailable,
    AuthenticationProvider,
    AuthenticationRejected,
    RateLimit,
    Connect,
    Transport,
    Timeout,
    HttpStatus,
    ResponseBody,
    ResponseDecoding,
    ResponseContract,
    Pagination,
    Internal,
}
```

Each error exposes or permits bounded access to:

- stable endpoint ID;
- operation ID;
- page/attempt ordinal;
- failure stage;
- dispatch knowledge;
- status when observed;
- policy-retry/auth-recovery counts;
- bounded structural response-header names with every value redacted;
- protected source only through explicit trusted access when safe.

Do not mirror every dependency error type in the public enum.

## Dispatch knowledge

```rust
pub enum DispatchKnowledge {
    NotDispatched,
    PossiblyDispatched,
    ResponseObserved,
}
```

A timeout or transport failure after dispatch does not prove remote rejection. `PossiblyDispatched` should direct callers toward application reconciliation using their own domain/idempotency identity.

## No universal retry boolean

Retry depends on effect, dispatch knowledge, body replayability, deadline, budgets, application workflow, and reconciliation. Expose the facts rather than one `is_retryable()` answer.

## Codec and provider diagnostics

Use stable codes and bounded structural locations. Raw third-party text is not
copied into public errors or automatic diagnostics.

Example:

```text
ResponseDecode {
  format: Json,
  location: Field("items").AnyIndex.Field("created_at"),
  reason: TypeMismatch,
}
```

## Semantic diff

Compare effective leaf contracts and classify:

```text
CallerApi
DomainMeaning
WireShapeOrRepresentation
SecurityOrAuthority
RuntimePolicyOrResource
Capability
OperationalIdentity
DocumentationOnly
OpaqueUnknown
```

A parent representation/query/header/auth/rate/timeout change expands to every affected leaf. Source moves must report inherited changes, not only generated Rust names.
