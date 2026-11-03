
---

## Supplementary scope for this review (Gate 2, written by the coordinator)

- Repository snapshot: detached worktree of branch `concord-v4` at commit `2639b39` (V4 lives in `crates/concord` and `crates/concord-macros`; V3 was removed from this branch in `391e252` and is preserved at tag `concord-v3-preserved`).
- Design record: `v4/DECISIONS.md` (D1–D10) and `v4/PRODUCT.md`. There is no `v4/SEMANTICS.md` or `v4/TEST_MATRIX.md` yet.
- Implemented: the whole macro (parser `syntax.rs`, resolution/composition `model.rs`, generation `emit.rs`, `template.rs`) and runtime (`__private.rs`, `prepared.rs`, `error.rs`, `repr.rs`, `value.rs`).
- Tested so far: only `crates/concord/tests/slice.rs` (7 loopback tests: M01–M05, path value, into_parts). Composition (query/header slots, override/remove/append), subdomains, sensitivity, bodies and compile-fail diagnostics are implemented but **not yet tested** — they are the next gate. Review that code as code: say what would break.
- You may run `cargo test --offline --test slice` or `cargo expand`-free experiments only if your sandbox permits; otherwise reason from source and say so.
- Please give an explicit opinion on decision D4's contested point (absent optional under `override` keeps the inherited value; exact replacement is `remove` + introduce) versus your blind-review proposal (exact `replace`).
