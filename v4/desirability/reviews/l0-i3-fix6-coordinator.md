# I3 fix round 6 — coordinator acceptance notes (not a review)

Implementer: Claude subagent (Opus 5.5). Committed `1708a9ad` on `dz/l0-i3`, merged into
`dz/l0-instruments` at `05413782`. Coordinator re-ran the self-test after one change: ALL
EXPECTATIONS MET (138 expectations, 27 effect checks), output `work/wt/i3/target/tmp/st-coord/`.

Implementer decisions taken over (each reversible, all put to review 10):
1. `<T as Trait>::Name` projections refused (copying the rustdoc projection would get R2 credit
   without resolving the type; `__judge_off` reference shows 8 such copies would compile).
2. `extern crate alloc;` and `extern crate self as <crate>;` inside each check function, so the
   real `alloc::` and `<crate>::` spellings resolve.
3. Elided reference lifetimes and `'_` refused (inferred inside a function body).

Coordinator change: the refusals were not stated to readers (the same class of divergence as
review 6 #3), so `probe_reader.md` and the result-question line in `pack.py` now say "each type
written out in full: no `_`, `impl`, `Self`, elided lifetimes or `<T as Trait>::Name`
projections"; README notes it. Re-run `pack.py` before any reader run.
