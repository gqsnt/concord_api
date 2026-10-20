# ADR 0004 — Endpoint-owned status and three effect classes

- **Status:** Retained in v3.0.0-beta.1 (v3 specification §10.1); extended by [ADR 0019](0019-head-only-expected-absence.md) with a separate expected-absence status set. v3 additionally forbids automatic authentication replay of a non-idempotent request after a response (RETRY-02).
- **v2.1 status (historical):** Selected

## Decision

Accepted status is finite endpoint data. Endpoint effect is `safe`, `idempotent`, or `non_idempotent` with method defaults.

## Property earned

Status, response conversion, body replay, retry, and pagination no longer share hidden ownership. Safe retrieval and idempotent mutation remain distinguishable.

## Consequences

- auth challenge precedes status;
- rejected status may enter policy retry;
- accepted conversion failure is terminal by default;
- ambiguous retry requires safe/idempotent plus other gates;
- ordinary pagination requires safe;
- idempotency-header presence does not infer effect.

## Rejected

- status callback in response spec;
- binary `repeatability` as the complete effect model;
- pagination on merely idempotent mutations.
