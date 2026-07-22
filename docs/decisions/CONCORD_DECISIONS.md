# CONCORD_DECISIONS
**Version:** 0.1  
**Status:** Informative decision log  
**Authority:** Architectural rationale; normative behavior remains in the specification documents

## How to use this log

Each decision records why the specification selected one design and rejected alternatives.

A decision does not override a normative requirement. If a decision and specification disagree, the specification wins and the decision must be updated.

Decision statuses:

- **Accepted** — selected for Concord 0.1;
- **Superseded** — replaced by another recorded decision;
- **Rejected** — considered and explicitly not selected.

---

## ADR-001 — Production transport is concrete Reqwest


**Context:** A public transport trait would require generic generated clients, a transport-neutral body/stream/error model, and would expose final authenticated requests to arbitrary transport implementations.

**Decision:** Production execution uses private managed Reqwest clients. A deterministic executor seam exists only under testing/development configuration and does not alter generated client types.

**Consequences:**

- simpler endpoint and response types;
- no duplicate transport error taxonomy;
- Reqwest retry, TLS, proxy, and body semantics remain authoritative;
- replacing Reqwest is not a supported application extension.

**Rejected alternatives:** Public `Transport` trait; generic `ApiClient<T>`; Tower service as the primary public transport.

**Affected requirements:** `PROD-CORE-001`, `FEAT-CORE-002`, `CONF-DET-001`.

---

## ADR-002 — No Concord general retry loop

**Status:** Accepted

**Context:** A Concord retry loop would coexist with Reqwest-native retries and complicate request counts, hooks, body replay, credentials, pagination, and limiter accounting. Even Reqwest's native default retry may create more physical attempts than one Concord limiter reservation.

**Decision:** Concord application native retry is disabled by default. Applications may explicitly opt into Reqwest default retry or the feature-gated dangerous custom Reqwest retry. Concord separately owns one authentication recovery.

**Consequences:** With the default, one limiter reservation corresponds to one physical native attempt. Explicit native retry weakens physical-attempt limiter accounting and remains invisible to Concord lifecycle hooks.

**Rejected alternatives:** Safe status-retry profiles; endpoint retry policies; custom Concord backoff engine; enabling Reqwest default retry implicitly.

**Affected requirements:** `PROD-GUAR-008`, `PROD-GUAR-013`, `DSL-RETRY-*`, `EXEC-NATIVE-*`, `EXEC-COUNT-*`.

## ADR-003 — HTTP caching is outside core


**Context:** Correct caching requires `Vary`, authorization partitioning, freshness, revalidation, stale behavior, and response transformation rules.

**Decision:** Concord core 0.1 contains no response cache.

**Consequences:** The request runtime remains focused. A future cache may be a separate crate and require a contract revision.

**Rejected alternatives:** Built-in memory cache; generic cache backend; stale-on-error in the core execution loop.

**Affected requirements:** Product non-goals.

---

## ADR-004 — Built client values are fixed immutable snapshots

**Context:** Mutable or derived client values create clone-sharing ambiguity and can reuse tenant-, region-, host-, or audience-sensitive credentials under a different public snapshot.

**Decision:** Client-level public values are selected during independent construction and remain fixed. Cloning preserves the exact same client snapshot and shares its credential handles. Per-operation variability belongs in scope values and endpoint parameters. A different client-level snapshot requires an independently built client with independent credential state.

**Consequences:** Credential validity never silently crosses changed client-level values. Same-snapshot clones remain cheap and concurrency-safe. Concord 0.1 does not need credential partition-key declarations or post-build `with_*` client methods.

**Rejected alternatives:** Shared mutable client values; derived clients sharing credentials; automatic credential partition maps in 0.1.

**Affected requirements:** `PROD-MODEL-*`, `PROD-STATE-*`, `GEN-CLONE-*`.

---

## ADR-005 — Credentials remain explicitly mutable through typed handles


**Context:** Credential rotation, session acquisition, invalidation, and clearing are genuine runtime lifecycle operations.

**Decision:** Credentials are shared only by clones of the same fixed client snapshot and mutated only through credential-specific handles. Independently built clients own independent credential handles.

**Consequences:** Client values and credentials have deliberately different state models. Compound static input replacement is atomic, and changing client-level routing/audience values cannot reuse an existing credential slot.

**Rejected alternatives:** Credentials embedded in immutable client snapshots; generic secret setters; public credential-slot mutation.

**Affected requirements:** `PROD-STATE-002`, `GEN-CRED-*`, `API-HANDLE-*`.

---

## ADR-006 — No `ClientContext` god trait


**Context:** A context combining route, values, auth registry, state initialization, policy, and recovery makes advanced clients implement internal orchestration.

**Decision:** Generated immutable storage and `ClientCore` replace a public context trait. Advanced users create prepared endpoints against `ClientCore` behavior through stable builders.

**Consequences:** Core feature traits remain narrow. Generated-private integration is sealed.

**Rejected alternatives:** Hand-written `ClientContext`; separate public route/auth context traits; application-implemented core client spec.

**Affected requirements:** `API-PRIVATE-001`, Core Architecture.

---

## ADR-007 — Endpoint outputs remain statically typed


**Context:** Generated endpoints already know their body and response types at compile time.

**Decision:** `EndpointSpec` has static body and response associated types. No normal response decoder returns `Box<dyn Any>`.

**Consequences:** Better compiler errors, no runtime downcast failure, and simpler pagination bounds.

**Rejected alternatives:** Type-erased response plans; dynamic endpoint descriptors; boxed decoded values.

**Affected requirements:** `PROD-CORE-002`, `GEN-TERM-*`.

---

## ADR-008 — Narrow private credential erasure is allowed


**Context:** One request may contain heterogeneous credential material and providers, while endpoint outputs do not require erasure.

**Decision:** Credential preparation/materialization may use a private erased trait. Versions and prepared material remain private.

**Consequences:** Auth plans can contain mixed API-key/bearer/Basic/custom sources without exposing internal types publicly.

**Rejected alternatives:** Entire request type parameterized over credential tuples; public auth plan traits; runtime endpoint output erasure.

**Affected requirements:** `API-PRIVATE-001`, Core Architecture auth boundary.

---

## ADR-009 — Page-based pagination only


**Context:** Item caps require generic runtime knowledge of page item counts, truncation, and proof pages.

**Decision:** Pagination supports soft page count and hard page cap only.

**Consequences:** Page execution remains the unit of network boundedness. Applications may truncate returned vectors when needed.

**Rejected alternatives:** `TakeItems`; `HardItemCap`; generic item-count hints.

**Affected requirements:** Product pagination non-goals, `API-PAGE-*`, `EXEC-PAGE-*`.

---

## ADR-010 — Pagination pages use `IntoIterator`


**Context:** A custom `PageItems` trait duplicates standard item extraction and creates count-consistency obligations.

**Decision:** Decoded page output must implement `IntoIterator`. `Vec<T>` works directly.

**Consequences:** Custom cursor pages implement ordinary Rust iteration. The controller inspects `&Page` before consumption.

**Rejected alternatives:** `PageItems`; boxed iterator callbacks; runtime page downcasting.

**Affected requirements:** `API-PAGE-001`, `GEN-PAGE-001`.

---

## ADR-011 — Pagination is directly awaitable


**Context:** `.paginate(...).collect().await` implies a stream/iterator abstraction that does not otherwise exist.

**Decision:** `.paginate(limit).await` returns `Vec<Item>`.

**Consequences:** One terminal, simpler generated API, no separate collection builder.

**Rejected alternatives:** `.collect()` terminal; async page stream in core 0.1.

**Affected requirements:** `GEN-PAGE-002`.

---

## ADR-012 — Pagination loop protection and aggregate ceilings are always enabled

**Context:** Repeating a logical page request can create unbounded network activity, while a caller-selected `u32` page count alone is not a practical memory or work bound.

**Decision:** Every pagination operation tracks canonical prior request identities, rejects cycles before resend, and enforces finite configured ceilings for requested pages, collected items, and retained identity bytes.

**Consequences:** There is no loop-protection opt-out. Collection remains all-or-error. The runtime fails closed before exceeding any aggregate ceiling.

**Rejected alternatives:** Optional progress keys; adjacent-page comparison; public opt-out; relying on integer width as the only bound; mandatory incremental pagination in 0.1.

**Affected requirements:** `EXEC-PAGE-*`, `BOUND-PAGE-*`.

---

## ADR-013 — Body metadata is validated before hooks; expensive materialization follows the veto hook

**Context:** Invalid body ownership and known impossible sizes should fail early, but expensive codec/factory work should not occur for an operation vetoed by application policy.

**Decision:** Public validation → body metadata validation → before-send hook → body materialization and finite-size enforcement → credentials → limiter.

**Consequences:** Hook veto performs no provider, limiter, or expensive body work. Encoding failure still performs no provider or limiter work. Direct request streams remain unpolled until native execution.

**Rejected alternatives:** Fully materialize before hook; credentials before body validation; limiter before encoding; native middleware mutation.

**Affected requirements:** `EXEC-ORDER-001`, `EXEC-BODYMETA-*`, `EXEC-BODY-*`.

---

## ADR-014 — Before-send hook runs before provider and limiter


**Context:** A veto policy should reject a prepared public operation without causing token acquisition or quota consumption.

**Decision:** The hook receives validated body metadata before expensive body materialization, credentials, and limiter acquisition.

**Consequences:** Hook context remains credential-free. Hook errors have minimal protected side effects.

**Rejected alternatives:** Hook after limiter; hook after auth materialization; general middleware mutation.

**Affected requirements:** `EXEC-HOOK-001`.

---

## ADR-015 — Static rejection requires explicit challenge attribution

**Status:** Accepted

**Context:** A bare `401` cannot safely identify every credential in a multi-credential plan. Permanently rejecting all applied credentials can destroy a valid API key when only a bearer token expired.

**Decision:** Built-in static and endpoint-backed placements default to `ChallengePolicy::Never`; built-in OAuth2 bearer defaults to `Unauthorized`; custom providers require an explicit challenge policy. One effective auth plan may have at most one challenge-enabled placement. An explicitly challenge-enabled static credential enters persistent rejected state until complete source replacement.

**Consequences:** Ambiguous responses mutate at most one credential. Static material is never persistently rejected merely because it was present on a request whose bearer failed.

**Rejected alternatives:** Reject every applied credential on `401`; general challenge groups in 0.1; scheme inference for custom headers/query credentials.

**Affected requirements:** `DSL-AUTH-*`, `API-AUTH-*`, `EXEC-RECOVER-*`.

## ADR-016 — Endpoint-backed credentials require explicit reacquisition


**Context:** Automatically calling an application auth endpoint during challenge handling introduces hidden request trees and endpoint dependency cycles.

**Decision:** Endpoint-backed credentials use invalidate-only recovery. The application explicitly executes the acquisition terminal.

**Consequences:** Hidden login flows do not occur. Error recovery is visible at the application level.

**Affected requirements:** `EXEC-RECOVER-004`, `GEN-AUTHFLOW-001`.

---

## ADR-017 — Limiter observation cannot replace the current response


**Context:** A successful HTTP response should not become an internal error because local cooldown storage is full or a custom observer fails.

**Decision:** `RateLimiter::observe` returns `()` and may affect future acquisition only.

**Consequences:** Response semantics remain server-authoritative. Limiter implementations must contain observation errors internally.

**Rejected alternatives:** Fallible response callback that suppresses auth/status/decode; current-response retry instruction.

**Affected requirements:** `API-RATE-002`, `EXEC-RESPHEAD-003`.

---

## ADR-018 — Built-in limiter reservations are atomic


**Context:** Reserving one bucket while waiting for another consumes quota for requests that may never send and complicates cancellation.

**Decision:** The built-in governor calculates all waits and subtracts all rule/window costs in one short locked transition only when all are available.

**Consequences:** Cancellation while waiting consumes no quota. Locking is simpler and fairer at the request level.

**Rejected alternatives:** Sequential Governor cell waits; compensating refunds; partial reservations.

**Affected requirements:** `EXEC-RATE-002`, `EXEC-RATE-003`.

---

## ADR-019 — No-content accepts all successful statuses and ignores body


**Context:** Many APIs return `200` with an ignorable body or use different successful codes for semantically bodyless operations.

**Decision:** `NoContent` accepts any `2xx` and drops the body unpolled.

**Consequences:** Tolerant SDK behavior; strict status contracts can be represented separately in a future feature if required.

**Rejected alternatives:** Require `204`/`205`; reject a body by content length; decode empty bytes through a codec.

**Affected requirements:** `EXEC-NOCONTENT-001`.

---

## ADR-020 — One structured hook model; debug is built-in presentation


**Context:** Separate hook and debug sink taxonomies duplicate lifecycle meaning and can diverge.

**Decision:** Applications implement `RuntimeHooks`. Built-in debug levels render the same lifecycle milestones internally.

**Consequences:** One extension lifecycle. No custom `DebugSink` trait in 0.1.

**Rejected alternatives:** Separate async hooks and synchronous debug sink; generic event bus with request mutation.

**Affected requirements:** `API-HOOK-*`, debug sections of Generated/Public API.

---

## ADR-021 — Terminal status bodies use finite complete-only explicit access

**Status:** Accepted

**Context:** Status and headers alone discard protocol information needed for validation, conflict handling, remediation, and machine-readable API errors. Raw or automatically formatted error bodies would undermine Concord's diagnostic safety.

**Decision:** A final non-success application response attempts to retain one small finite complete body in an opaque `StatusBody`. Known oversized bodies remain unread; unknown overflow discards every collected byte; capture failure preserves the HTTP status. Bytes are available only through explicit access or explicit decoding with an existing codec and never through automatic formatting, hooks, completion events, limiter contexts, or source chains.

**Consequences:** Applications can handle bounded protocol error data without a generated per-status error type system. Recoverable challenge bodies remain unread; terminal challenge bodies may use the same bounded capture path.

**Rejected alternatives:** Drop every non-success body; expose truncated prefixes; expose a native or lazy error response; generate typed error enums in 0.1.

**Affected requirements:** `PROD-GUAR-011`, `API-ERR-004` through `API-ERR-007`, `SEC-RESP-*`, `BOUND-BODY-009`, `BOUND-BODY-010`, `EXEC-STATUS-*`.

## ADR-022 — Specialized protocols live outside core

**Status:** Superseded by ADR-045

**Context:** SSE, WebSocket, record streaming, and multipart response parsing have distinct state machines and reconnection/framing semantics.

**Decision:** Core provides generic bounded byte streams. Specialized protocols belong in separate future crates.

**Consequences:** Smaller core feature matrix and execution model.

**Rejected alternatives:** Core response modes for every protocol.

---

## ADR-023 — Exact limits are centralized


**Context:** Repeating default numbers across subsystem docs risks contradictory changes.

**Decision:** Exact default limits are owned only by the Security and Limits Specification.

**Consequences:** Execution and public API docs reference `BOUND-*` requirements instead of repeating numbers.

**Affected requirements:** `DOC-DUP-001`.

---

## ADR-024 — Generated-private integration is not an extension API


**Context:** Procedural macro output compiled downstream requires public Rust visibility for some integration items.

**Decision:** Such items live in `__private`, are protected by a generated contract marker, and are unsupported for application code.

**Consequences:** Public visibility does not create semver promises. Stable advanced workflows require no private imports.

**Affected requirements:** `API-NS-002`, `COMPAT-GEN-*`.

## ADR-025 — No provider invalidation callback


**Context:** A provider invalidation callback races with proactive refresh, concurrent challenge responses, cancellation, and exact-version local rejection. Ordinary token renewal requires the challenged material to reach `refresh(current)`, not a separate revocation phase.

**Decision:** Concord 0.1 performs exact local rejection and coordinated `refresh(current)` recovery. `CredentialProvider` has no invalidation or remote-revocation callback.

**Consequences:** The credential state machine is smaller, one exact-version refresh operation owns challenge recovery, and remote revocation remains an explicit application workflow outside the protected request path.

**Rejected alternatives:** Provider invalidation before refresh; best-effort invalidation callback; callback on manual clear.

**Affected requirements:** `API-PROV-*`, `EXEC-RECOVER-*`, auth state-machine architecture.

---

## ADR-026 — Ordinary application I/O limits remain finite

**Context:** A normal method disabling request, buffered-response, or stream-response limits contradicts the product guarantee that ordinary I/O is bounded.

**Decision:** Concord 0.1 exposes only positive finite application I/O limits. No ordinary no-limit method or dangerous unbounded-I/O feature is included.

**Consequences:** Applications may select larger limits but cannot accidentally make ordinary execution unbounded. Provider limits also remain finite.

**Rejected alternatives:** Weaken the guarantee to bounded-by-default; normal `no_*_limit` methods; unbounded I/O in 0.1.

**Affected requirements:** `PROD-GUAR-004`, `BOUND-BODY-*`, `API-RUNTIME-001`.

---

## ADR-027 — Endpoint timeout is a Concord operation deadline

**Context:** A generic `timeout` name is misleading when only native Reqwest transport is bounded and hooks, credentials, provider HTTP, limiter waits, and authentication recovery remain outside it.

**Decision:** DSL and request-level `timeout` define one endpoint/page operation deadline. Remaining budget is propagated through asynchronous Concord phases and native transport. Pagination has a separate deadline per page and no aggregate collection deadline in 0.1.

**Consequences:** Connect/read configuration remains available at the managed-client level. Synchronous extension work cannot be preempted but is checked immediately after returning.

**Rejected alternatives:** Keep ambiguous transport-only timeout; expose two per-request timeout concepts in 0.1; whole-pagination deadline.

**Affected requirements:** `DSL-TIME-*`, `GEN-REQ-*`, `EXEC-DEADLINE-*`.

---

## ADR-028 — Custom pagination uses factories and a safe request patch

**Status:** Superseded by ADR-044

**Context:** Requiring `Default` and generated writes to public controller fields prevents invariants and freezes field layout as an extension ABI.

**Decision:** Custom pagination is created by one typed factory expression. Controllers keep private state and may modify only a safe `PaginationRequest` patch containing public query/header pagination inputs.

**Consequences:** Construction validates controller invariants atomically. Custom pagination receives no native request, auth, body, host, or path mutation capability in 0.1.

**Rejected alternatives:** `Default` plus public fields; runtime adapter objects; arbitrary native request mutation.

**Affected requirements:** `DSL-PAGE-*`, `API-PAGE-*`, `EXEC-PAGE-*`.

---

## ADR-029 — Stream opening and aggregate pagination have distinct completion outcomes

**Context:** Returning a stream does not prove full body consumption, and successful page events do not prove aggregate collection success.

**Decision:** Request completion reports `StreamOpened` when a bounded stream is safely returned. Pagination additionally emits one aggregate completion event covering final collection outcome.

**Consequences:** Metrics do not label an unopened/partially consumed stream as fully downloaded, and controller/cycle/cap failures remain observable. Terminal stream-consumption hooks are deferred.

**Rejected alternatives:** Treat stream construction as full success; page-only pagination observation; asynchronous terminal stream hooks in 0.1.

**Affected requirements:** `API-HOOK-*`, `EXEC-COMPLETE-*`.

---

## ADR-030 — Built-in rate limiting is bounded best-effort pacing

**Context:** Bounded LRU state may forget quota history after eviction, so the built-in governor cannot be strict authoritative quota enforcement for unbounded key cardinality.

**Decision:** The built-in governor is explicitly process-local best-effort pacing. Applications needing strict capacity or distributed authority provide a custom limiter.

**Consequences:** State remains bounded and an evicted key may receive a fresh local burst. Concord 0.1 does not add a second strict governor mode.

**Affected requirements:** `BOUND-RATE-*`, `BOUND-RATE-*`.

---

## ADR-031 — Dependency-renamed macro expansion is supported

**Context:** Hardcoded `::concord_core` paths fail when downstream crates rename dependencies or expand macros in nested modules.

**Decision:** The macro discovers the caller-visible core crate path through `proc_macro_crate` or an equivalent validated mechanism and generates a unique private alias.

**Consequences:** Renamed dependencies, nested modules, and multiple API declarations are compile-tested.

**Affected requirements:** `COMPAT-MACRO-*`, Macro Architecture.

---

## ADR-032 — Concord 0.x uses an explicit pre-1.0 compatibility policy

**Context:** Concord 0.1 intentionally defines a broad surface before implementation experience, and the dangerous retry API is coupled to Reqwest.

**Decision:** Patch releases preserve documented normal and advanced 0.1 signatures. A later `0.y.0` may make documented breaking changes. The generated contract is versioned, and dangerous Reqwest-coupled APIs may change with Concord minor releases.

**Consequences:** The specification is a firm implementation baseline without pretending to be a 1.0 semver freeze.

**Affected requirements:** `COMPAT-SURF-*`, `COMPAT-REQWEST-*`, `COMPAT-GEN-*`.

---

## ADR-033 — Automatic typed status schemas remain outside 0.1

**Status:** Accepted; supersedes the earlier body-discard wording through ADR-021

**Context:** Endpoint-specific typed status schemas would alter generated endpoint error types, prepared endpoints, pagination, and the uniform `ApiError` model.

**Decision:** Concord 0.1 provides bounded complete-only raw terminal status-body access and explicit decoding through existing codecs, but does not generate per-status error enums or automatic error-codec routing.

**Consequences:** Protocol information is available without expanding every endpoint's future error type. Typed generated schemas remain a possible later layer over `HttpResponseFailure`.

**Rejected alternatives:** Complete body discard; generated typed status schemas in the initial release.

**Affected requirements:** Product error-body non-goals, `API-ERR-*`, `EXEC-STATUS-*`.

## ADR-034 — The initial MSRV is evidence-based Rust 1.85

**Context:** Rust 2024 and Reqwest 0.13.4 require Rust 1.85, while selecting the newest stable compiler would unnecessarily reduce compatibility.

**Decision:** Concord 0.1 targets Rust 1.85 unless an actually selected dependency or required language feature proves a higher floor. CI enforces the declared MSRV.

**Consequences:** An MSRV increase requires a compatibility-specification update and dependency evidence.

**Affected requirements:** `COMPAT-RUST-*`, `CONF-MSRV-*`.

## ADR-035 — Credential finalization follows limiter waiting

**Status:** Accepted

**Context:** Credential preflight before a potentially long limiter wait may retain material that is later cleared, replaced, rejected, or expired.

**Decision:** Concord preflights credential availability before limiter acquisition, then finalizes slot identity, version, rejection, clear state, and expiration immediately after limiter authority and before native request construction. Finalization is the send-selection linearization point.

**Consequences:** A clear or replacement completed before finalization prevents stale send. A race after finalization may not retract an already-selected native send. Finalization failure does not refund local limiter reservation.

**Rejected alternatives:** Use the preflight snapshot unconditionally; hold credential locks across limiter wait or native execution; require limiter reservation rollback.

**Affected requirements:** `EXEC-AUTH-009` through `EXEC-AUTH-013`.

## ADR-036 — Protected and provider requests require HTTPS

**Status:** Accepted

**Context:** The DSL permits HTTP for unauthenticated APIs, but credential placement over cleartext would contradict the security model.

**Decision:** Application requests with a non-empty effective auth plan and every provider HTTP request require HTTPS. No implicit localhost exception exists in 0.1.

**Consequences:** HTTP remains supported for unauthenticated application endpoints. Protected cleartext preflight is classified as `Configuration` / `InvalidAuthentication`. The managed application wrapper retains effective `https_only` metadata for later public preflight. Deterministic tests use the development executor rather than insecure credential transmission.

**Rejected alternatives:** Optional-only `https_only`; automatic localhost exception; ordinary dangerous insecure-auth switch.

**Affected requirements:** `PROD-GUAR-012`, `SEC-HTTP-008`, `SEC-HTTP-009`, `EXEC-HEAD-003`, `EXEC-PHTTP-005`.

## ADR-037 — Operation events carry opaque correlation identifiers

**Status:** Accepted

**Context:** Endpoint, URL, page index, and recovery index do not uniquely correlate concurrent identical operations.

**Decision:** Every request/page operation has an opaque `OperationId`; pagination pages carry a parent aggregate operation ID. Completion events expose monotonic elapsed time and final status when available.

**Consequences:** Hooks can build correct traces without receiving credentials, bodies, cursors, or native URLs.

**Rejected alternatives:** Infer correlation from endpoint and URL; expose internal task IDs; add full per-phase tracing metrics to core.

**Affected requirements:** `API-HOOK-006`, `API-HOOK-007`, `EXEC-OPID-001`, `EXEC-COMPLETE-005`.

## ADR-038 — Specification candidate and release evidence are distinct

**Status:** Accepted

**Context:** A mechanically complete design package is not a conforming release, and planned coverage rows are not passing evidence.

**Decision:** The current package is `Normative Implementation Candidate`. Coverage planning and actual conformance evidence use separate catalogs. `Normative Release Final` requires complete passing or approved waived actual evidence.

**Consequences:** Manual normative-document, coverage-plan, evidence-catalog, and release-command review establish governance without claiming implementation conformance prematurely.

**Rejected alternatives:** Mark the pre-implementation package final; store planned and passing evidence in one ambiguous table.

**Affected requirements:** `DOC-STATUS-*`, `CONF-TRACE-*`, `CONF-RELEASE-001`.

## ADR-039 — Direct transport dependencies are prohibited in production graphs

**Status:** Accepted

**Context:** Reqwest owns Concord's transport stack, while deterministic Reqwest contract probes
need to synthesize upstream HTTP/2 conditions such as `REFUSED_STREAM` that an ordinary loopback
HTTP server cannot request.

**Decision:** `concord_core` and `concord_macros` normal/build graphs have no direct `hyper`,
`hyper-util`, `tower`, or `h2` dependency. An isolated test probe may use `h2` as a dev dependency.

**Consequences:** The test-only type never appears in published runtime/generated code, public
APIs, or normal/build dependency graphs. Dependency-tree evidence must review the edge class.

**Rejected alternatives:** Remove the executable upstream retry probe; add `h2` to production;
retain wording that incorrectly prohibits a dev-only probe.

**Affected requirements:** `COMPAT-DEP-001`, `COMPAT-DEPS-001`, `CONF-REQWEST-001`.

## ADR-040 — Additional roots are shared but application identity is isolated

**Status:** Accepted

**Context:** Application and provider clients may need the same private trust roots, but presenting
an application mTLS identity to an authentication provider would cross client security domains.

**Decision:** Additional configured roots apply to both managed clients. `identity_pem` applies
only to the application client. Concord 0.1 defines no provider-specific identity configuration.

**Consequences:** Provider HTTPS can trust the configured authority without impersonating the
application client. A later provider identity requires an explicit contract revision.

**Rejected alternatives:** Apply one identity to both clients; add an undocumented provider
identity setting; select identity per request.

**Affected requirements:** `API-HTTP-004`, `SEC-HTTP-010`.

## ADR-041 — Completion observes fixed results outside the operation deadline

**Status:** Accepted

**Context:** The operation deadline must bound asynchronous work capable of determining the
application result, while normal timeout results must still produce completion observation. An
already expired deadline cannot also provide a remaining budget in which an awaited timeout
completion hook finishes.

**Decision:** Concord fixes the result, completion outcome, final status, and elapsed duration before
completion observation. `on_complete` and `on_pagination_complete` are awaited outside the operation
deadline, are not detached, and cannot mutate the fixed result, request retry, or restart the
deadline. Elapsed time ends at result fixation and excludes completion-hook time. Extension authors
bound completion-hook work. Cancellation or panic may prevent delivery.

The stable pending request wrappers are `concord_core::Request<E>` and
`concord_core::ResponseRequest<E>`, also exported from `concord_core::prelude`. Generated-private
endpoint and execution traits remain absent from generated public signatures and public Rustdoc.

**Consequences:** Normal timeout results invoke completion with `CompletionOutcome::Timeout`.
Completion may delay delivery of an already-fixed result, but it creates no second timeout, queue,
detached task, executor surface, or alternate result path. Generated users can name request wrappers
without importing `__private`.

**Rejected alternatives:** Bound completion by an already exhausted operation deadline; detach
completion; add a completion-task queue; add a second completion timeout/configuration placeholder;
expose generated-private endpoint bounds in stable signatures.

**Affected requirements:** `API-REQ-001`, `API-REQ-002`, `API-HOOK-007`, `GEN-END-003`,
`EXEC-DEADLINE-001`, `EXEC-DEADLINE-004`, `EXEC-COMPLETE-001`, `EXEC-COMPLETE-005`,
`SEC-EXT-001`.

## ADR-042 — Endpoint request setters use generated sealed extension traits

**Status:** Accepted

**Context:** Generated facade methods must return the stable core-owned
`concord_core::Request<E>`, while optional and defaulted endpoint values require endpoint-specific
consuming methods whose names are known only in downstream macro expansion. Rust coherence forbids
downstream inherent implementations for the foreign wrapper, and stable Rust 1.85 does not support
`self: Request<Self>` as an arbitrary receiver. A generated wrapper would violate the single
pending-request contract; changing setter syntax or the toolchain would alter approved public
behavior.

**Decision:** Each endpoint with optional or defaulted request controls generates one public sealed
extension trait implemented only for `concord_core::Request<ThatEndpoint>`. The client support
module contains a public `prelude` that re-exports every endpoint trait under a deterministic alias
containing the client, complete scope path, endpoint public name or alias, and `RequestExt`.
Applications use one `use example_api::prelude::*;` import. A single doc-hidden core bridge consumes
a request, maps its endpoint value, and reconstructs the same wrapper while preserving its core and
generic request options. Explicit endpoint values keep inherent controls, and generic timeout/debug
setters remain inherent on `Request<E>`.

**Consequences:** The stable wrapper, endpoint ownership seal, response capabilities, direct await,
and one executor remain unchanged. Multiple client preludes and repeated endpoint names coexist
without trait-name collisions. Public Rustdoc documents the generated prelude and extension traits
but hides their seals, the endpoint mapping bridge, and all generated-private core bounds.

**Rejected alternatives:** Downstream inherent implementations; a generated request newtype;
closure-based or generic-name setters; invisible injected application imports; nightly arbitrary
self types; a Rust MSRV/toolchain change.

**Affected requirements:** `GEN-END-003`, `GEN-REQ-001`, `GEN-DEFAULT-001`, `GEN-NAME-001`,
`GEN-NAME-002`, `GEN-DOC-001`, `GEN-DOC-002`, `API-REQ-001`, `API-REQ-002`,
`CONF-COMPILE-001`, `CONF-COMPILE-003`, `CONF-DOWNSTREAM-001`, `CONF-MSRV-001`.

## ADR-043 — Conformance evidence types use a closed extended vocabulary

**Status:** Accepted

**Context:** The normative Conformance Plan enumerated 16 evidence types as the complete set allowed
in both catalogs. The 504-row coverage plan uses 23 types and the 771-row actual-evidence catalog
uses 33 types. Their union contains 19 additional labels, including the coverage-only
`runtime-and-compile` label for `CONF-CORE-002`. The rows otherwise retain exact artifacts,
commands, configurations, results, and immutable commits, so the literal enumeration contradicted
accurate historical catalogs.

**Decision:** Preserve every historical catalog row byte-for-byte and amend the normative
Conformance Plan in place. The existing 16 labels remain canonical primitive proof types. Every
currently used additional label is admitted as one precisely bounded alias, compound mapping, or
specialized procedural type. Planned labels continue to describe intended work only; actual labels
classify only the exact proof recorded by their row. The vocabulary is closed, and a future label
requires an approved normative amendment that names and defines it before catalog use.

**Consequences:** The permitted vocabulary contains 35 labels: 34 currently used labels plus the
intentionally retained supplemental `fuzz` primitive. Historical evidence identities,
classifications, artifacts, results, and commits remain unchanged. The clarification changes only
metadata interpretation; it changes no product behavior, public API, implementation phase, or
release verdict. The Conformance Plan remains the normative owner, and this decision remains
informative rationale.

**Rejected alternatives:** Reclassifying historical rows into the original 16 labels, which would
rewrite accurate evidence and discard useful procedural specificity; permitting arbitrary
free-form or catch-all labels; treating this decision log as normative authority; migrating the
unchanged 0.1 authority to a new path or version.

**Affected requirements:** `DOC-ID-003`, `DOC-CHG-003`, `DOC-LINT-001`, `CONF-CORE-002`,
`CONF-TRACE-001`, `CONF-TRACE-005`, `CONF-TRACE-006`.

**Affected documents:** `docs/normative/08_CONCORD_CONFORMANCE_PLAN_0.1.md`,
`docs/conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv`, and
`docs/conformance/CONFORMANCE_EVIDENCE_0.1.csv`; both catalogs are interpreted but remain
byte-identical.

---

## ADR-044 — Pagination uses one typed state struct and inline bindings

**Status:** Accepted

**Context:** The built-in/custom split gave macro-recognized controller categories privileged
binding and short-page semantics, while application controllers used factories and a separate safe
query/header patch. That split duplicated runtime roles, prevented one uniform typed state machine,
and required macro changes for controller-specific behavior.

**Decision:** Every pagination declaration names one arbitrary controller type and one complete
struct-shaped field block. `bind ep.FIELD` and `bind scope.FIELD` initialize controller fields from
request-local snapshots once and write current controller fields into a fresh endpoint clone before
each page. One public consuming `PaginationController<Page>` state-machine trait and one
generated-private associated-controller capability feed the single core-owned page loop. Controller
validation and count termination belong to the controller, not to macro-recognized categories.

This decision supersedes ADR-028. It preserves ADR-009, ADR-010, ADR-011, ADR-012, ADR-027, and
ADR-029 except for the narrow pagination wording required by uniform typed state and binding.

**Consequences:** Built-in and downstream controllers use the same parser, IR, generated binding,
trait, and runtime. Controller values need `Send` but not `Clone` or `Sync`. Bound state can shape
every ordinary endpoint-derived surface while remaining subject to existing secrecy, canonical
identity, limiter, deadline, recovery, and completion contracts. Stable unbound controller fields
are initialized once and are not classified as evolving continuation values.

**Rejected alternatives:** Controller factories; separate safe query/header controller patches;
macro-recognized built-in or custom categories; separate runtime binder or short-page callbacks;
public binding protocols; endpoint-generic public factories; erased controller registries;
generated page loops.

**Affected requirements:** `DSL-PAGE-002`, `DSL-PAGE-007` through `DSL-PAGE-010`, `API-PAGE-002`,
`API-PAGE-007` through `API-PAGE-010`, `GEN-PAGE-004`, `SEC-PAGE-001` through `SEC-PAGE-003`,
`EXEC-PAGE-001`, `EXEC-PAGE-005`, `EXEC-PAGE-007`, `EXEC-PAGE-015` through `EXEC-PAGE-017`,
`COMPAT-MACRO-004`, `CONF-PAGE-002`.

---

## ADR-045 — Generic body and response specifications use sealed core acquisition modes

**Status:** Accepted; supersedes ADR-022

**Context:** Macro-owned request and response categories tied extensibility, generated output,
prepared endpoints, pagination eligibility, media policy, and execution to a closed list of names.
The separate `StreamResponse<M>` metadata/stream primitive and prepared response markers duplicated
semantic taxonomies. Applications could extend complete codecs but could not define an arbitrary
body or response contract, and incremental CSV or NDJSON records had no integrated bounded seam.

**Decision:** Concord owns one stable `RequestBodySpec`, one stable `ResponseBodySpec`, and one
managed outgoing `RequestBody`. Request metadata is structural and computed before the hook;
preparation runs once after hook approval; core validates metadata against the produced body before
credentials, limiter, debug, authentication, native construction, or send. Replayability remains a
property inferred only from the produced managed body.

Response specifications select exactly one of three public-but-sealed core modes: `Buffered` yields
complete bounded `Bytes`, `Streaming` yields one bounded lazy metadata-free and media-free
`ByteStream`, and `Discard` yields `()`. Core retains native response ownership, performs successful
status classification and known-length preflight before specification validation, validates before
body polling, owns acquisition and completion, and checks synchronous extension work against the
deadline. `ApiResponse<T>` is the one universal successful metadata wrapper;
`DecodedResponse<T>` is only its compatibility alias and `StreamResponse<M>` is removed.

The macro stores arbitrary `syn::Type` body and response shapes and generates only structural trait
bounds. It recognizes no application or built-in semantic type name. Prepared endpoints select the
same generic specifications and executor. Pagination uses the explicit buffered
`PageResponseSpec` capability rather than response-name matching. Direction-specific request and
response media ownership replaces a universal media policy while retaining `MediaType` as an exact
helper for `Stream<M>`.

Incremental record framing is integrated in feature-gated `concord_core::records` over the same
`ByteStream`, with one `RecordDecoder`, `Records<D>`, `RecordStream<D>`, and batching adapter. CSV
and NDJSON are optional isolated decoders with independent 1 MiB undecoded staging bounds. Concord
adds neither a records crate, second executor, fourth response mode, native response access,
unbounded buffering, multipart response parsing, nor protocol reconnection behavior.

This decision supersedes ADR-022's placement of all record streaming outside core. It preserves the
valid intent of ADR-007 (type-owned endpoint results without runtime downcast), ADR-013 (metadata
before the hook and expensive preparation afterward), ADR-019 (all-success discard semantics),
ADR-021 (bounded explicit terminal status bodies), ADR-023 (central exact limits), ADR-024
(generated-private integration is not an application extension API), ADR-026 (ordinary application
I/O remains finite), ADR-029 (stream-opening and pagination-aggregate completion are distinct), and
ADR-039 (Reqwest transport ownership and prohibited production dependencies).

**Consequences:** Downstream request specifications, each sealed response-mode specification,
page-capable responses, and record decoders use public-only imports and need no macro modification.
Generated and prepared endpoints share one lifecycle, executor, limits, authentication, limiter,
hooks, status, recovery, cancellation, and completion behavior. Streaming direct and metadata
terminals both report `StreamOpened`; later byte or record errors are stream items and emit no
second completion. Feature-disabled built-ins fail through core type and trait availability.

**Rejected alternatives:** Keep macro body/response categories; expose native responses or a
public execution plan; let applications implement acquisition modes; add another outgoing body or
prepared executor; retain `StreamResponse<M>`; use runtime `Any` output; create a records workspace
crate or executor; make complete vectors the record primitive; add an unbounded mode; parse
multipart responses; add SSE, WebSocket, reconnection, or terminal stream-consumption hooks.

**Affected requirements:** Phase 12 retained-version rows and new requirements recorded by
`docs/normative/10_CONCORD_REQUIREMENT_CHANGE_RECORD_0.3.md`, including `PROD-MODEL-008`,
`API-BODYSPEC-*`, `API-RESPSPEC-*`, `API-RESP-002`, `API-STREAM-001`, `API-RECORD-*`,
`API-PREP-004`, `GEN-BODY-001`, `GEN-RESP-*`, `GEN-PAGE-005`, `GEN-PREP-001`,
`SEC-BODY-003`, `SEC-RESP-009`, `SEC-STREAM-001`, `SEC-RECORD-*`, `BOUND-RECORD-*`,
`EXEC-BODY-006`, `EXEC-BODY-007`, `EXEC-RESP-*`, `EXEC-STREAM-004`, `EXEC-RECORD-*`,
`FEAT-RECORDS-001`, `FEAT-CSV-001`, `FEAT-NDJSON-001`, `COMPAT-MACRO-005`,
`CONF-BODY-001`, `CONF-RESP-001`, and `CONF-RECORD-*`.

---

## ADR-046 — Generated rate limits use lexical ownership and global keys

**Status:** Accepted; Phase 13 authority

**Context:** The prior normal generated DSL modeled reusable templates, remote attachments,
dimensions, replacement, and opt-out. That model made generated rule identity and bucket scope
depend on request values and required rate-limit-only `Display` bounds. The approved Phase 13
directive instead makes quota ownership lexical and preserves the already-proven limiter lifecycle.

**Decision:** A client, scope, or endpoint may own one complete direct `rate_limit { ... }`
declaration. A client rule applies to every endpoint; a scope rule applies to its complete lexical
subtree; an endpoint rule applies only to that endpoint. Effective plans compose automatically in
client, outer-to-inner scope, endpoint order. Child declarations add rules and cannot replace,
disable, or modify ancestors. The normal DSL has no templates, remote named attachments, `by`
dimensions, `replace`, `off`, formatted key parts, hidden per-value partitioning, or discontiguous
quota grouping. A quota without a coherent lexical owner belongs to a custom limiter or advanced
prepared endpoint.

Every generated declaration receives a deterministic collision-resistant identifier based only on
the generated module path, canonical client name, complete lexical scope path, endpoint name when
applicable, and declaration kind. Every normal generated rule uses `RateLimitKey::global()` and
static cost/windows. Runtime request values never enter generated plans, IDs, keys, diagnostics,
or normal generated Rustdoc. The observer is an optional unique client-only
`rate_limit_observer PATH;` declaration and configures only the generated default governor.

The advanced prepared/custom boundary remains explicit: applications may construct
`RateLimitRule` with bounded `RateLimitKey::global()` or `RateLimitKey::from_parts(...)`, and may
inspect those key parts through a selected custom limiter. Normal generated declarations cannot
access that dimension mechanism. Generated contract 13 therefore becomes 14.

**Preserved decisions:** ADR-014 (hook veto before provider and limiter work), ADR-017 (response
observation cannot replace the response), ADR-018 (atomic multi-rule reservation), ADR-030
(bounded best-effort process-local governor semantics), and ADR-035 (credential finalization after
limiter waiting) remain accepted and unchanged.

**Consequences:** Independently built generated clients receive independent default governor state;
clones share their selected limiter; intentionally shared explicit limiter objects share their
namespace while deterministic declaration IDs prevent collisions. Pagination reuses the same
lexical plan and global keys for every page. No new public identity or hashing API, dependency,
feature, executor, or runtime state machine is introduced.

**Rejected alternatives:** Preserve templates through an adapter; encode lexical scope values as
hidden key parts; keep endpoint-local opt-out; add arbitrary generated key expressions; group
discontiguous branches in the normal DSL; rewrite the governor; expose a public generated identity
type.

**Affected requirements:** `PROD-MODEL-009`, `PROD-GUAR-010`, `PROD-GUAR-015`,
`PROD-NONGOAL-003`, `DSL-RATE-001`, `DSL-RATE-002`, `DSL-RATE-008` through `DSL-RATE-012`,
`API-RATE-004`, `API-RATE-005`, `GEN-BUILD-004`, `GEN-DOC-001`, `GEN-DOC-002`,
`GEN-RATE-001` through `GEN-RATE-006`, `SEC-RATE-001` through `SEC-RATE-003`,
`BOUND-RATE-005`, `EXEC-RATE-001` through `EXEC-RATE-005`, `FEAT-GOV-001`, `FEAT-GOV-002`,
`COMPAT-MACRO-006`, and `CONF-RATE-001` through `CONF-RATE-004`.
