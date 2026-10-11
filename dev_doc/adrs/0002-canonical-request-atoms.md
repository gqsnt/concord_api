# ADR 0002 — Canonical request atoms, not implicit `Display`

- **Status:** Retained unchanged in v3.0.0-beta.1 (v3 specification §5.1, ATOM-01).
- **v2.1 status (historical):** Selected and mechanically supported on Windows/Linux; human ergonomics remains unknown for public freeze. That platform evidence is v2.1 evidence; v3 was not run on Windows.

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
