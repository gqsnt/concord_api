# ADR 0009 — Exact optional setter, `with`, `omit`, and `reset`

- **Status:** Superseded by ADR 0012 before release

## Decision

For `x?: T`, generate `set_x(Option<T>)`, `with_x(T)`, and `omit_x()`. Add `reset_x()` only when initialized. Rename v1 `clear_x` to `omit_x`.

## Property earned

- exact setter composes with configuration and nested optional payloads;
- `with` keeps the common present-value call direct;
- `omit` names the outer declaration transition;
- `reset` restores retained initialization.

## Risk

`with` increases generated method count and autocomplete/compile surface.

## Revisit

Use the 10–20 optional-field DX and large-contract method-count study. Keep or remove `with` before release; do not publish aliases indefinitely.
