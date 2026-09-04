# Concord v2.1 design invariants

These invariants are normative.

## Language and projection

1. One `api!` invocation contains exactly one closed client-shaped domain tree.
2. Rust payload types are opaque. The macro never assigns semantics by recognizing `Option`, `Vec`, aliases, qualification, stream names, or other type spelling.
3. `?` adds exactly one Concord-owned outer presence layer.
4. `#[default]` adds retained initialization at the narrowest owner-snapshot boundary.
5. `#[sensitive]` classifies public protocol data for observation; it does not make the value a credential.
6. Every public value reference names an exact owner: `client`, a named visible scope, or `endpoint`.
7. One atom expression creates one scalar atom. `each` alone expands an ordered sequence into repeated query/header occurrences.
8. Public protocol atoms use `RequestAtom` or an explicit adapter, not implicit `Display`.
9. Body input is exactly what the caller supplies. The result type is exactly what ordinary `.await` returns.
10. Request and response representation, status, and effect have independent owners.
11. Generated direct, unbound, documentation, reflection, planning, and runtime bridge views derive from one canonical semantic graph.
12. Capability methods are present exactly when the contract grants the capability.

## Runtime and authority

13. Every request form uses one managed executor.
14. Core alone owns native request/response objects and transport execution.
15. Public shaping and observation happen before private credential placement.
16. Every generated operation has one positive finite absolute deadline.
17. Page and retry loops cannot reset or extend the operation deadline.
18. Operation, pagination aggregate, page, visible attempt, and transport-private physical try remain distinct identities.
19. Dispatch knowledge is conservative and monotonic: `NotDispatched`, `PossiblyDispatched`, `ResponseObserved`.
20. Mechanical body replayability never implies endpoint `safe` or `idempotent` semantics.
21. Policy retry, auth recovery, transport-private recovery, and pagination continuation are separate finite authorities.
22. Ordinary pagination requires `effect safe` and a complete page response.
23. A declared auth placement is never silently omitted because material is unavailable.
24. At most one effective credential placement may attribute a recoverable challenge.
25. Rate policies compose additively and descendants cannot disable an ancestor provider constraint.
26. A client with effective rate policies requires an explicit limiter choice.
27. Rate response feedback influences future admission and cannot replace the current endpoint result.
28. General observation is read-only, synchronous, bounded, and non-fallible for the operation result.
29. Lazy stream acquisition and later stream completion/failure are distinct lifecycles.
30. Every growing dimension has an independent finite bound or an explicit caller-owned post-return contract.

## Security and negative space

31. Secrets, credential material, private auth values, bodies, outputs, and raw rate keys never appear in general plans, errors, observer events, contract descriptions, or automatic debug output.
32. Sensitive public values affect the real request and internal correctness identities while remaining redacted in automatic observation.
33. Public/private/media/transport target ownership conflicts are errors; no last-writer precedence repairs them.
34. The logical origin is fixed by the contract. A proxy may change routing but not origin identity or credential trust.
35. Automatic redirects are disabled.
36. Runtime endpoints cannot gain greater origin, auth, native, retry, or resource authority than generated endpoints.
37. Extensions receive only the facts and authority required by one semantic role.
38. Unsupported native middleware, arbitrary origins, cookies, caching, TRACE, CONNECT, upgrades, and WebSockets remain outside the product.

## Evidence

39. Selected public syntax is not considered cognitively supported until unfamiliar-user tasks are run.
40. Representation traits, stream erasure, and large-contract metadata are not considered mechanically/performance supported until the release gates pass.
41. A private implementation may change to satisfy Rust coherence or performance; changing any invariant above requires an ADR, counterexample, and updated evidence.
