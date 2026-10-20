# ADR 0012 — Freeze the exact optional control surface

- **Status:** Superseded for v3 by [ADR 0014](0014-exact-conditional-and-reset-value-controls.md): v3 moves exact outer-slot replacement to `replace_x` and adds `set_x_if_some`.
- **v2.1 status (historical):** Selected for the v2.1 integration candidate; human comprehension remains unknown for public freeze
- **Supersedes:** ADR 0009

## Decision

For `x?: T`, generate only `set_x(Option<T>)` and `omit_x()`. Generate
`reset_x()` only when the slot has a retained initializer. Remove the
provisional `with_x(T)` convenience and reject the retired `clear_x()` name.

For `x?: Option<U>`, the exact setter takes `Option<Option<U>>`: the outer
option controls Concord presence and the inner option remains the caller's
opaque Rust payload.

## Evidence state

No unfamiliar-human participants have completed the Gate 3 task protocol. The
selected conservative fallback therefore removes `with_x` rather than carrying
a provisional alias. Compile-fail fixtures prove both retired conveniences are
absent. The ready-to-run study package may inform public compatibility freeze,
but its missing evidence does not block local beta integration and this ADR
does not claim human comprehension.

## Property earned

- one exact assignment operation preserves nested optionality;
- `omit` names the outer presence transition;
- `reset` remains distinct from omission;
- generated method count and autocomplete noise are reduced;
- the beta contains no provisional optional-control method.

## Revisit

Only new measured unfamiliar-human evidence showing a material correctness or
task-time gain may justify a future additive design. Do not restore `with_x` as
an unmeasured compatibility alias.
