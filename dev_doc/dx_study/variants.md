# Surface under study

**Status:** Part of the v2.1 Gate 3 packet described in [`README.md`](README.md); not revised for v3.

The Gate 3 final-surface study uses only the exact v2.1 candidate:

- `set_x(Option<T>)` for a Concord-present value;
- `omit_x()` for Concord-owned absence;
- `reset_x()` only for initialized/defaulted slots;
- no generated `with_x` or `clear_x` method;
- explicit `PageLimit::Prefix` or `PageLimit::CompleteWithin`.

This is not a variant comparison. Reopening `with_x` requires a separate
study-only branch and a paired, counterbalanced protocol with at least four to
six qualifying humans, identical unrelated docs/errors, complete regeneration,
and scale/API evidence. One anecdote or friction report cannot change v2.1.
