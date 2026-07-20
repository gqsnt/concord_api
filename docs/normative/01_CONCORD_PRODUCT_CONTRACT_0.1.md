# CONCORD_PRODUCT_CONTRACT_0.1

**Version:** 0.1  
**Status:** Normative Implementation Candidate
**Authority:** Highest-level product contract  
**Requirement prefix:** `PROD`  
**Depends on:** `CONCORD_SPECIFICATION_INDEX_0.1`  
**Supersedes:** None

## 1. Product definition

Concord is a generated, strongly typed HTTP API client built on private managed Reqwest clients.

Concord transforms a compile-time API declaration into:

- concrete generated client types;
- scope facades;
- typed endpoint requests;
- typed request and response bodies;
- managed credential and authentication behavior;
- optional rate limiting;
- explicit page-based pagination;
- safe observability;
- narrow application extension points.

**PROD-CORE-001 — Concrete managed transport.** Production execution MUST use private managed Reqwest clients.

**PROD-CORE-002 — Typed endpoint results.** Normal endpoint outputs MUST remain statically typed through request preparation and response decoding.

**PROD-CORE-003 — One execution runtime.** Generated endpoints, explicit generated endpoint values, and advanced prepared endpoints MUST use the same core execution runtime.

**PROD-CORE-004 — No runtime API tree.** Concord MUST NOT construct or mutate a runtime representation of the declared API tree.

## 2. Target use cases

Concord 0.1 is intended for:

- generated SDKs for HTTP and REST-style APIs;
- authenticated service APIs;
- regional and tenant-scoped APIs;
- JSON and text APIs;
- custom buffered wire formats;
- bounded binary downloads;
- bounded lazy response streams;
- streamed and multipart uploads;
- offset, page-number, cursor, and application-specific pagination;
- APIs with local pacing or custom rate-limit coordination.

Concord is appropriate when a compile-time declaration can represent the public request contract and generated Rust types can represent request and response values.

## 3. User mental model

### 3.1 Generated client

A generated client owns one immutable client-value snapshot. Cloning the client preserves that exact snapshot and shares the managed runtime and credential state.

Changing a client-level value requires constructing another independent client. Values that vary per operation belong in scopes or endpoint parameters.

### 3.2 Facade

A facade represents one scope and stores immutable scope-value snapshots.

### 3.3 Endpoint request

Calling an endpoint creates an owned request value. Fluent setters modify only that request value.

### 3.4 Credential handle

Credential handles provide explicit rotation, set, clear, and presence operations. Credential state is shared only by clones of the same independently built client snapshot.

### 3.5 Response

Direct `await` returns the semantic endpoint result. Optional terminals expose successful metadata, explicit pagination, or dangerous raw response data.

**PROD-MODEL-001 — Owned request snapshots.** A pending request MUST own or share immutable references to every public value needed for execution.

**PROD-MODEL-002 — No mutable public client state.** Public client and scope values MUST NOT be mutated in place after construction.

**PROD-MODEL-006 — Fixed client snapshot.** Client-level public values MUST remain fixed for the lifetime of one independently constructed generated client and its clones.

**PROD-MODEL-007 — Credential isolation boundary.** Credential material, provider failures, rejected state, and in-flight credential operations MUST be shared only by clones of the same independently built client snapshot.

**PROD-MODEL-005 — Request-local pagination.** Pagination controller state, request identities, and collected items MUST be local to one pagination operation.

## 4. Canonical workflows

### 4.1 Construct a client

```rust
let client = Api::builder()
    .tenant(tenant)
    .api_key(api_key)
    .http(|http| http.retry_off())
    .runtime(|runtime| {
        runtime.debug(DebugLevel::Basic)
    })
    .build()?;
```

### 4.2 Execute an endpoint

```rust
let user = client
    .users(region)
    .by_id(user_id)
    .await?;
```

### 4.3 Obtain successful response metadata

```rust
let response = client
    .users(region)
    .by_id(user_id)
    .response()
    .await?;
```

### 4.4 Consume a response stream

```rust
let mut stream = client.downloads().archive(id).await?;
while let Some(chunk) = stream.next().await {
    consume(chunk?);
}
```

### 4.5 Paginate

```rust
let items = client.items().list().paginate(10).await?;
```

### 4.6 Rotate a credential

```rust
client
    .credentials()
    .application_key()
    .replace(new_key)
    .await?;
```

### 4.7 Acquire an endpoint-backed credential

```rust
client
    .auth()
    .login(input)
    .acquire_as_session()
    .await?;
```

## 5. Capability inventory

### 5.1 Request shaping

Concord supports:

- validated base URLs;
- validated dynamic host labels;
- encoded path segments;
- ordered repeated query pairs;
- validated public headers;
- inheritance, replacement, removal, and clearing where defined;
- per-request timeout selection.

### 5.2 Request bodies

Concord supports:

- bodyless requests;
- buffered request codecs;
- reusable bytes;
- one-shot byte streams;
- reconstructible body factories;
- request-side multipart.

### 5.3 Responses

Concord supports:

- buffered decoded values;
- bounded bytes;
- no-content results;
- bounded lazy streams;
- successful metadata wrappers;
- feature-gated bounded raw successful responses;
- explicit access to a small finite complete terminal non-success body when capture succeeds.

### 5.4 Authentication

Concord supports:

- static API keys;
- static bearer tokens;
- Basic credentials;
- OAuth2 client credentials;
- endpoint-backed credentials;
- custom credential providers;
- header, query, bearer, and Basic placement;
- proactive refresh;
- one bounded authentication recovery.

### 5.5 Rate limiting

Concord supports:

- reusable declared rules;
- direct local rules;
- multiple windows;
- request costs;
- local governor pacing;
- custom limiters;
- safe response observation;
- future-call cooldowns.

### 5.6 Pagination

Concord supports:

- offset pagination;
- page-number pagination;
- cursor pagination;
- custom pagination controllers;
- soft page counts;
- hard page caps;
- always-on request-cycle protection;
- collection through `IntoIterator` page results.

### 5.7 Extensibility

Concord supports application-defined:

- request codecs;
- response codecs;
- media markers;
- body factories;
- credential materials and providers;
- pagination controllers;
- rate limiters and response observers;
- runtime hooks;
- prepared endpoints.

## 6. Global guarantees

**PROD-GUAR-001 — Logical URL safety.** Concord-managed query authentication MUST NOT become part of the logical URL exposed through ordinary public APIs.

**PROD-GUAR-002 — Body-free automatic diagnostics.** Error formatting and source chains, hooks, debug output, limiter contexts, completion events, and pagination diagnostics MUST NOT expose request or response body bytes; the explicit bounded terminal status-body accessor defined by `PROD-GUAR-011` is not an automatic diagnostic surface.

**PROD-GUAR-003 — Credential-free ordinary diagnostics.** Ordinary errors, hooks, debug output, limiter contexts, pagination diagnostics, and generated Rustdoc MUST NOT expose credential material.

**PROD-GUAR-004 — Bounded ordinary I/O.** Ordinary request bodies, buffered responses, streaming responses, and provider HTTP bodies MUST be subject to the bounds defined by the Security and Limits Specification.

**PROD-GUAR-011 — Bounded terminal status body.** A final non-success HTTP response MAY expose only a finite complete body through the explicit opaque status-response API defined by the Public API and Security specifications; ordinary formatting and callbacks MUST remain body-free.

**PROD-GUAR-012 — Protected HTTPS.** Concord MUST NOT materialize credential values onto a cleartext HTTP application or provider request.

**PROD-GUAR-013 — Retry-disabled default.** Application native retry MUST be disabled unless the application explicitly selects Reqwest retry behavior.

**PROD-GUAR-005 — Single auth recovery.** One protected page or endpoint operation MUST perform at most one authentication-recovery attempt.

**PROD-GUAR-006 — Explicit bounded pagination.** Multi-page execution MUST occur only after an explicit pagination call whose requested page count is finite and does not exceed the configured runtime pagination ceilings.

**PROD-GUAR-007 — No partial pagination result.** A failed pagination operation MUST NOT return a partial item collection.

**PROD-GUAR-008 — Hidden retry containment.** Reqwest-internal retries MUST NOT rerun Concord hooks, credential preparation, limiter acquisition, body factories, or pagination preparation.

**PROD-GUAR-009 — Provider isolation.** Provider HTTP MUST NOT use application hooks, application rate limiting, application pagination, endpoint codecs, or application custom retry.

**PROD-GUAR-010 — Fail closed.** Invalid auth placement, unavailable TLS, body-limit violations, unavailable required rate-limit enforcement, and pagination cycles MUST fail rather than silently weakening the declared behavior.

## 7. State and clone model

| State | Clones of one generated client | Separately built client |
|---|---|---|
| Application Reqwest client and pool | Shared | Independent |
| Provider Reqwest client and pool | Shared | Independent |
| Credential handles, slots, failures, and rejected state | Shared | Independent |
| Selected hook object | Shared | Independently selected |
| Selected limiter object | Shared | Independently selected |
| HTTP configuration and runtime limits | Immutable and shared | Independently selected |
| Public client values | Same immutable snapshot | Independent snapshot |
| Scope values | Immutable snapshot per facade | Independent |
| Endpoint values and request options | Request-local | Request-local |
| Pagination controller and collected items | Pagination-local | Pagination-local |

**PROD-STATE-001 — Immutable runtime selection.** HTTP behavior, limits, hooks, limiter selection, and debug defaults MUST be fixed when a client is built.

**PROD-STATE-002 — Credential mutability.** Credential replacement and manual credential state changes MAY occur after client construction through explicit typed handles.

**PROD-STATE-003 — Stable in-flight request.** A request already created MUST retain the public values and runtime configuration selected when that request was created.

**PROD-STATE-004 — Independent build isolation.** Two independently built generated clients MUST NOT share credential slots or credential-operation coordination unless a future explicit shared-credential API is added by contract revision.

## 8. Safety classes

### 8.1 Normal generated surface

The standard generated client and prelude. It provides the strongest ordinary guarantees.

### 8.2 Advanced surface

Stable extension contracts that remain inside Concord's managed lifecycle. Advanced extensions receive only the data needed for their purpose, but application code remains responsible for its own logs, panics, storage, and external side effects.

### 8.3 Dangerous surface

Feature-gated capabilities that intentionally reduce ordinary guarantees:

- raw successful response data;
- Reqwest-native custom retry.

### 8.4 Generated-private surface

`__private` exists only for generated Rust integration and is not a supported application API.

### 8.5 Development-only surface

`__development` supports deterministic testing and is unstable.

**PROD-SURF-001 — Explicit dangerous access.** A capability reducing ordinary secrecy or request-count guarantees MUST be feature-gated and placed in the dangerous surface.

**PROD-SURF-002 — No private promotion by visibility.** Rust visibility required by downstream generated code MUST NOT make an item a supported application extension contract.

## 9. Explicit non-goals

### 9.1 Transport and middleware

Concord 0.1 does not provide:

- public production transport replacement;
- generated clients generic over transport;
- raw Reqwest clients or request builders;
- a general middleware chain;
- Tower integration;
- user-defined execution-phase ordering.

### 9.2 Retry

Concord 0.1 does not provide:

- a Concord-owned general retry loop;
- endpoint-level retry configuration;
- scope-level retry configuration;
- status retry profiles;
- physical-attempt counters;
- retry callbacks.

Reqwest owns any explicitly enabled native retry behavior. Application native retry is disabled by default. Concord owns only one authentication recovery.

### 9.3 Caching

Concord 0.1 does not provide:

- HTTP response caching;
- ETag or Last-Modified revalidation;
- stale-on-error;
- cache backends;
- authenticated cache partitioning.

### 9.4 Specialized protocols

Concord 0.1 does not provide:

- WebSocket sessions;
- server-sent event parsing or reconnection;
- CSV or record streaming;
- multipart response parsing;
- bidirectional streaming protocols.

### 9.5 Pagination

Concord 0.1 does not provide:

- item-based hard or soft caps;
- partial results;
- parallel page fetching;
- speculative prefetch;
- page callbacks, item streams, or page streams;
- pagination for request-body endpoints;
- loop-detection disablement;
- a separate `.collect()` terminal.

### 9.6 Error bodies

Concord 0.1 does not provide:

- automatic endpoint-specific decoding of non-success bodies;
- generated per-status error enums;
- unbounded non-success bodies;
- streaming or lazy non-success bodies;
- truncated-prefix exposure.

A final terminal status may retain one small finite complete body in an opaque non-formatting wrapper. Applications may explicitly inspect or decode that body through the documented status-response API.

### 9.7 Client mutation

Concord 0.1 does not provide post-construction replacement of:

- transport;
- HTTP configuration;
- hooks;
- limiter;
- retry selection;
- runtime limits;
- client-level public values.

A differently configured client is built independently. Per-operation variation is represented through scope values and endpoint parameters.

### 9.8 Authentication internals

Concord 0.1 does not expose:

- credential generations;
- credential leases;
- credential slot states;
- authentication plans;
- recovery plans;
- provider-operation ownership identifiers.

It also does not provide:

- more than one auth recovery;
- automatic endpoint-backed credential reacquisition;
- automatic immediate resend of an unchanged static credential;
- provider invalidation or remote token-revocation callbacks;
- per-request client-certificate selection;
- provider HTTP authenticated through another Concord credential handle.

### 9.9 Resource and security limitations

Concord does not guarantee:

- secret zeroization;
- encrypted process memory;
- protection from a debugger or memory dump;
- bounded decoded object memory;
- bounded application extension state;
- bounded extension latency;
- distributed rate-limit synchronization;
- transactional rollback after cancellation or later failure.

**PROD-NONGOAL-001 — No implied support.** A capability listed as a non-goal MUST NOT be treated as supported because a private implementation detail could be used to approximate it.

**PROD-NONGOAL-002 — Contract revision required.** Adding a non-goal as a supported capability requires a normative contract revision and conformance coverage.

## 10. Product conformance summary

A conforming Concord implementation must make the ordinary user model simple:

1. Build a concrete typed client.
2. Call a typed endpoint.
3. Concord validates and prepares a bounded request.
4. Concord obtains credentials and local rate-limit authority.
5. Concord sends through Reqwest.
6. Concord may recover authentication once.
7. Concord bounds and decodes the successful response or returns a bounded stream.

Application extension points must remain explainable as one-purpose contracts. If a normal extension requires transport adapters, runtime plans, response downcasts, credential generations, or pagination runtime machinery, the implementation is not conforming to the intended product boundary.
