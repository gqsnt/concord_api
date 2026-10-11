# Gate 10 reviewer tasks and rubric

**Status:** Part of the v2.1 Gate 10 packet described in [`README.md`](README.md); not revised for v3.

Using public/generated documentation and supplied normal entry points only:

1. Find the common client-construction path, including required values and
   explicit runtime/limiter configuration where applicable.
2. For one required value, optional value, and defaulted value, identify the
   generated controls and distinguish `set`, `omit`, and `reset` availability.
3. Find one endpoint's effective response representation and its client,
   named-scope, or endpoint provenance.
4. Explain why request and response representation inheritance are independent
   directions.
5. Explain an `explicit` representation fence and what a new descendant
   endpoint must declare.
6. Predict every affected endpoint when a parent representation changes.
7. Read the resulting semantic diff and classify caller, wire, security, and
   runtime changes.
8. Identify whether a pagination call returns a prefix or requires complete
   natural termination within its bound.
9. Distinguish successful `ByteStream`/`ItemStream<T>` acquisition from later
   stream completion, failure, cancellation, and early drop.
10. Identify which URL/header metadata is explicit caller authority through
    `RequestPlan`/`ApiResponse` and which exact values are absent from automatic
    observers, debug output, and limiter request context.

For each task record correctness, elapsed seconds, path/pages/types opened,
search terms, corrections, confidence from 1–5 before feedback, and whether
macro/core implementation source was consulted.

Classify a finding as:

- **Critical:** a semantic, security, compatibility, completeness, or lifecycle
  misunderstanding;
- **Major:** a requested fact cannot be found normally or requires
  implementation reading;
- **Minor:** navigation or naming friction without semantic error.

Gate 10's optional public-freeze human review passes only when all ten facts are findable through
public docs/descriptions, no Critical misunderstanding remains, provenance and
parent-change consequences require no macro-source reading, and every
release-relevant Major finding has been fixed and reviewed again.
