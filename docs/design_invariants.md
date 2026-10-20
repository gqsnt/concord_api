# Concord v3 design invariants

These invariants are normative. Each names the implementation-specification
rule it restates.

## Language and projection

1. One `api!` invocation contains exactly one closed client-shaped domain
   tree; it is the unit of semantic validation (§1.4).
2. Rust payload types are opaque. The macro never assigns protocol semantics by
   recognizing `Option`, `Vec`, an alias or a qualified spelling (§3.1, REP-01).
3. `?` adds exactly one Concord-owned outer presence layer. A required
   `Option<U>` payload stays required; `x?: Option<U>` has two layers
   (VALUE-01).
4. `= V` and `#[default]` retain an initializer at the owner-snapshot boundary;
   reset restores it without re-evaluation (VALUE-02).
5. A bare name binds only the current owner; ancestors are explicitly
   qualified; redundant qualification of the current owner (including the
   retired v2 `endpoint.x` form) is rejected (LANG-03).
6. Direct placement is a normal introduction; explicit mappings of the same
   value coexist with it (LANG-01).
7. A plain key introduction never overwrites. `replace`, `when_present` and
   `remove` require an earlier potential owner; family `replace`/`off` are
   explicit fences (PATCH-02).
8. One atom expression is one scalar; only `each` expands an ordered sequence
   into repeated query/header occurrences (ATOM-01).
9. Paths are relative templates; DNS labels are typed single-label list atoms;
   the base owns the fixed origin (PATH-01, ORIGIN-01, ORIGIN-02).
10. Body input is exactly what the caller supplies; the result type is exactly
    what ordinary `.await` returns, except the explicit head-only absence branch
    (REP-01, OUTCOME-01).
11. Singleton clauses are errors when duplicated, never last-writer-wins
    (LANG-02).
12. Every ordinary input has a reachable semantic use (VALUE-06).
13. Generated direct, unbound, documentation, description, planning and
    runtime-bridge views derive from one resolved model (EXPLAIN-01).
14. Capability methods are present exactly when the contract grants the
    capability; unsupported capabilities are absent, not runtime errors
    (CALL-03).
15. Value controls have distinct names for payload set, exact slot
    replacement, conditional set, omission and reset; conversions are closed
    (VALUE-03, VALUE-04).

## Runtime and authority

16. Every request form uses one managed executor (PRODUCT-03).
17. Core alone owns native requests/responses and transport execution; a
    `RequestControls` implementation grants no execution authority (CALL-02).
18. Planning shares public selection and rendering and never prepares a body,
    obtains credentials, acquires a limiter or opens a connection (EXEC-03).
19. Suppressed public contributions are never encoded (PATCH-03).
20. Every operation has one positive finite absolute deadline starting at the
    first poll; retries, pages and traversals never reset it (CONTROL-01,
    PAGE-03).
21. Caps only narrow; `timeout(d)` is a preference under every cap
    (CONTROL-01).
22. Operation, traversal, page, visible attempt and transport-private physical
    try are distinct identities (EXEC-01).
23. Dispatch knowledge is a monotone join: `NotDispatched < PossiblyDispatched <
    ResponseObserved`; settled facts never regress (EXEC-01).
24. Mechanical body replayability never implies `safe` or `idempotent` (BODY-01,
    RETRY-01).
25. Policy retry, auth recovery, transport-private recovery and pagination
    continuation are separate finite authorities (RETRY-02).
26. Pagination requires `effect safe` and a buffered complete page; a page bound
    is never reported as exhaustion (PAGE-01, PAGE-02).
27. A declared auth placement is never silently omitted because material is
    unavailable; at most one placement attributes a challenge (AUTH-01).
28. Explicit credential storage follows the latest-started intent; installed
    version and intent epoch are separate counters (AUTH-03, AUTH-04).
29. Rate policies compose additively; descendants cannot disable an ancestor
    constraint; admission is atomic all-or-none (RATE-01, RATE-04).
30. A client with an effective rate policy requires an explicit limiter
    choice; no implicit no-op fallback exists (§7.3).
31. Rate response feedback influences future admission and cannot replace the
    current endpoint result (§12.6).
32. Observation is read-only, synchronous, bounded and runs after result
    fixation (EXEC-04).
33. Stream acquisition and stream completion/failure are distinct lifecycles;
    streams carry a finite lease (STREAM-01, STREAM-02).
34. Every growing dimension has an independent finite bound owned by core
    (LIMIT-01).

## Security and negative space

35. Secrets, credential material, bodies, outputs and raw rate keys never appear
    in plans, errors, observer events, contract descriptions or automatic debug
    output (PRIVACY-01).
36. Sensitive public values affect the real request while staying redacted in
    automatic observation; key sensitivity is sticky (PRIVACY-01).
37. Public/private/media/transport header ownership conflicts are rejected
    before presence is known (HEAD-01).
38. The logical origin is fixed by the contract. A proxy changes routing only;
    automatic redirects, cookies and upgrades are disabled (ORIGIN-01).
39. Runtime endpoints cannot gain greater origin, auth, native, retry or
    resource authority than generated endpoints (RUNTIME-01, RUNTIME-02).
40. Extensions receive only the facts and authority required by one semantic
    role (EXT-01).
41. Arbitrary native middleware, arbitrary origins, cookies, caching, `TRACE`,
    `CONNECT`, upgrades and WebSockets remain outside the product (§26.1).

## Evidence

42. Documentation is a tested product surface: DSL and Rust snippets are
    compiled or anchored to compiled examples, and the grammar, generated
    names, feature matrix and contract tools agree with the implementation
    (DOC-01).
43. Changing any invariant above requires a recorded specification correction
    with the affected tests, not an undocumented compromise (§28.1).
