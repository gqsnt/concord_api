# Surface under study

**Status:** selected v3 surface; no variant experiment or human result has run.

The Gate 3 final-surface study uses only the exact v3 candidate:

- `set_x(payload)` for a Concord-present value;
- `replace_x(Option<T>)` for exact outer-slot replacement;
- `set_x_if_some(Option<T>)` for conditional update, preserving state on `None`;
- `omit_x()` for Concord-owned absence;
- `reset_x()` only for initialized/defaulted slots;
- no generated `with_x` or `clear_x` method;
- explicit `collect_prefix(u32)`, `collect_all(NonZeroU32)` or `pages(u32)`.

This is not a variant comparison. Reopening `with_x` requires a separate
study-only branch and a paired, counterbalanced protocol with at least four to
six qualifying humans, identical unrelated docs/errors, complete regeneration,
and scale/API evidence. One anecdote or friction report cannot change the selected v3 contract.
