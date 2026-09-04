# ADR 0002 — Canonical request atoms, not implicit `Display`

- **Status:** Selected; trait ergonomics gate pending

## Decision

Default public scalar shaping requires `RequestAtom`. Placement-specific `via Encoder` adapters may supply alternative representations. `DisplayAtom` exists only as an explicit compatibility extra.

## Property earned

Human-facing formatting changes no longer silently migrate URL/query/header semantics, planning identity, or pagination identity.

## Rejected

- blanket `Display` protocol encoding;
- one universal representation trait spanning atoms, bodies, responses, and rate keys;
- type-name special cases.

## Boundaries

Atom produces one bounded scalar. DNS/path/query/header contexts validate and encode afterward. Omission and repetition remain placement concerns. Rate keys retain a separate canonical identity role.

## Revisit

If real domain types require excessive boilerplate and explicit adapters produce worse diagnostics than documented `Display`, evaluate derive/support tooling—not a silent blanket impl—using the DX and defect evidence.
