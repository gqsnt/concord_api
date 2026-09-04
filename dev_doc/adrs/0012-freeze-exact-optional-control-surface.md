# ADR 0012 — Freeze the exact optional control surface

- **Status:** Selected for the release candidate; Gate 3 human study remains externally blocked
- **Supersedes:** ADR 0009

## Decision

For `x?: T`, generate only `set_x(Option<T>)` and `omit_x()`. Generate
`reset_x()` only when the slot has a retained initializer. Remove the
provisional `with_x(T)` convenience and reject the retired `clear_x()` name.

For `x?: Option<U>`, the exact setter takes `Option<Option<U>>`: the outer
option controls Concord presence and the inner option remains the caller's
opaque Rust payload.

## Evidence state

The release mandate requires unfamiliar-human task evidence before Gate 3 can
pass. No human participants are accessible from this environment. The mandate
also says mixed or unavailable evidence must select the smaller exact surface,
so the release candidate removes `with_x` now rather than carrying a
provisional alias. Compile-fail fixtures prove both retired conveniences are
absent. The ready-to-run study package must still test whether the exact surface
is understood; this ADR does not claim the missing human evidence.

## Property earned

- one exact assignment operation preserves nested optionality;
- `omit` names the outer presence transition;
- `reset` remains distinct from omission;
- generated method count and autocomplete noise are reduced;
- the candidate contains no provisional optional-control method.

## Revisit

Only new measured unfamiliar-human evidence showing a material correctness or
task-time gain may justify a future additive design. Do not restore `with_x` as
an unmeasured compatibility alias.
