# ADR 0004 — Endpoint-owned status and three effect classes

- **Status:** Selected

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
