# Closure C1 bounded-construction foundation

Phase:
Closure C1 — Build the finite-construction foundation.

Started from commit:
`f5ce4bb2ed58215055b6a986fa76d97c4951728d`
(`docs: establish Concord refactor closure baseline`) on branch `master`.

Finished at commit:
The C1 commit containing this evidence, with message
`core: add canonical finite construction budgets`. Its exact hash will be
backfilled in the closure ledger by C2.

Files changed:

- `CONCORD_REFACTOR_CLOSURE_PLAN.md`;
- `crates/concord-core/src/advanced.rs`;
- `crates/concord-core/src/construction.rs`;
- `crates/concord-core/src/endpoint.rs`;
- `crates/concord-core/src/lib.rs`;
- `crates/concord-core/src/limits.rs`;
- `crates/concord-core/src/rate_limit/mod.rs`;
- `crates/concord-core/src/request/prepared.rs`;
- `crates/concord/src/lib.rs`;
- `crates/concord/tests/public_namespaces.rs`;
- `docs/contributor/evidence/closure-c0-baseline.md`;
- `docs/contributor/evidence/closure-c1-bounded-foundation.md`;
- `docs/reference/limits.md`;
- `fixtures/dependency/renamed_facade/src/lib.rs`;
- `tools/xtask/src/main.rs`.

Design decisions:

- Core now owns ten literal hard limits in `src/limits.rs`. Existing runtime
  endpoint ceilings remain unchanged: 256 top-level components, 65,536
  aggregate public bytes, and 64 authentication placements.
- One formatted public value is capped at 65,536 UTF-8 bytes. Matching the
  existing runtime aggregate ceiling avoids imposing a second, lower limit on
  a currently valid single-value runtime shape.
- The rate-limit structural ceilings are 256 ID bytes, 32 key parts, 128
  key-part-name bytes, 65,536 aggregate key bytes, 16 windows per rule, and
  64 rules per plan. The 256-byte ID cap accommodates the 128-byte endpoint
  identity plus the generated `.rate_limit` suffix and bounded path
  headroom. The nested caps bound one plan to at most 1,024 windows and at
  most 4,194,304 aggregate key bytes before lower limiter-specific policy.
- `DEFAULT_RATE_LIMIT_KEY_BYTES` remains the 1,024-byte operational default;
  C3 will enforce that selected limiter preparation limits cannot exceed the
  65,536-byte hard cap.
- `StructuralBudget` performs checked, non-mutating-on-error addition and
  replacement accounting. `collect_bounded` obtains no more than the
  remaining allowed items plus one withheld first excess. `ErrorLatch`
  prevents later conversion work after a failure.
- The capped formatter implements `fmt::Write`, checks UTF-8 byte length
  before reserving or retaining each chunk, detects a `Display`
  implementation that ignores a formatting error, and returns only
  value-free structural errors.
- The formatter, collector, latch, and replacement-specific budget methods
  are intentionally internal foundation code until their C2–C4 consumers
  migrate; narrow `dead_code` allowances keep phase commits warning-clean and
  will be removed as consumers adopt them.
- The limits table is mechanically generated from both runtime-default and
  hard-limit source constants. No value is duplicated in xtask logic.

Audit findings closed:

- F-16's canonical-owner requirement is closed: exact endpoint and rate-limit
  structural limits are core-owned, façade-reexported, and mechanically
  checked against the reference table.
- F-01, F-02, and F-18 are not yet fully closed. Their shared budget,
  first-excess, error-latch, and formatting foundations are complete; C2–C4
  own consumer enforcement.

Tests added/changed:

- Five core unit tests cover checked addition/replacement/overflow,
  canonical and UTF-8 formatting boundaries, ignored formatting errors,
  withheld excess chunks, exact/first-excess/infinite guarded iterators, and
  latch short-circuiting.
- Structural and formatting error `Display`/`Debug` assertions prove sentinel
  text is absent.
- The façade namespace test proves every hard constant is reachable from
  `concord::endpoint` or `concord::rate_limit` and checks the hard/default
  ordering at compile time.
- The renamed-façade fixture references all ten constants through the renamed
  dependency.
- Existing runtime endpoint boundary tests now consume the canonical public
  constants instead of private duplicates.
- The documentation generator now verifies literal values from both
  `runtime/defaults.rs` and `limits.rs`.

Commands executed:

```text
rg -n "\.to_string\(\)|\.collect\(\)|collect::<" crates/concord-core/src/request/prepared.rs crates/concord-core/src/request/head.rs crates/concord-core/src/rate_limit/types.rs crates/concord-extras/src/rate_limit/governor.rs crates/concord-extras/src/auth/oauth2_client.rs crates/concord-macros/src/emit -g '*.rs'
cargo fmt --all
cargo fmt --all -- --check
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord --test public_namespaces --no-default-features --locked
cargo test -p concord --test public_compile --no-default-features --locked
cargo run --locked -p concord-xtask -- docs
cargo rustdoc --locked -p concord --all-features -- -D warnings
cargo test -p concord --test rustdoc --all-features --locked
cargo +1.85.0 check -p concord-core --all-targets --all-features --locked
cargo +1.85.0 check -p concord --tests --no-default-features --locked
cargo +stable clippy -p concord-core -p concord -p concord-xtask --all-targets --all-features --locked -- -D warnings
git diff --check
```

Additional read-only inventories searched existing constants, formatter
implementations, public exports, tests, the limits generator, and arbitrary
iterator collection sites with `rg`, `rg --files`, and `Get-Content`.

Exact results:

- The final `cargo fmt --all -- --check` exited 0.
- The first core-lib invocation failed to compile because three test
  references still used the retired private
  `MAX_RUNTIME_PUBLIC_COMPONENTS` name. They were changed to the canonical
  public constant. The final core-lib invocation exited 0: 334 passed,
  including five new construction tests.
- Core bounds exited 0: 12 passed. An intermediate successful run emitted
  dead-code warnings for foundation methods not yet used by C2; narrow
  internal allowances were added, and the final run was warning-free.
- Façade public namespaces exited 0: 2 passed.
- Façade public compile exited 0: 7 passed, including the renamed dependency
  proof. The existing localized MSVC linker message remained a non-fatal
  `linker_messages` warning.
- Documentation checks exited 0 after the exact generated table matched both
  core source owners.
- Warning-denied façade Rustdoc exited 0; the rendered Rustdoc owner also
  exited 0 with 2 passed.
- Rust 1.85 core all-target/all-feature check exited 0.
- Rust 1.85 façade no-default test-target check exited 0.
- The first combined format/Clippy attempt exited 1: formatting had not yet
  been applied to one new assertion, and Clippy rejected two runtime
  assertions whose values were compile-time constants. Formatting was
  applied and the relationships were moved to const assertions. The final
  warning-denied stable Clippy command exited 0. The localized MSVC
  `linker_messages` warning is explicitly exempt from `-D warnings` by the
  compiler lint.
- `git diff --check` exited 0 before evidence/ledger creation and is rerun
  before commit.

Unexecuted checks:

- No benchmark was run; C1 does not yet route production formatting through
  the new formatter, and the checked build-time validation remains linear in
  the same retained structure.
- C2–C4 consumer-specific lifecycle and no-overconsumption matrices were not
  executed early.
- No graphical editor, remote Actions, or non-Windows platform check was
  executed in C1.

Security/redaction review:

- `CappedString` performs checked byte addition and limit comparison before
  `try_reserve_exact` or `push_str`; a rejected chunk is not retained.
- A user `Display` implementation can still consume arbitrary CPU, block,
  panic, or ignore `fmt::Error`. This trust boundary is now explicit in the
  canonical limits reference. Ignoring the error cannot make Concord accept
  or retain excess output.
- `BoundedFormatError` and `StructuralLimitError` contain no source value,
  byte prefix, key, identifier, or safe-message escape hatch. Sentinel tests
  cover both `Display` and `Debug`.
- The guarded infinite iterator panics if polled after the first excess; the
  bounded collector returns before that poll and withholds the excess item.
- The error latch test proves an already failed state does not create or poll
  a later iterator.
- No native transport, authentication material, lifecycle ordering,
  cancellation, response acquisition, or completion behavior changed.

Performance impact:

- Public consumer behavior is unchanged in C1. Existing runtime endpoint
  final validation now uses checked budget calls rather than ad hoc checked
  sums; both are linear and retain the same limits.
- The capped formatter is not yet on a production path. Its maximum retained
  output is 65,536 bytes and it does not preallocate from an iterator size
  hint.
- No benchmark result is claimed.

Deviations:
None.

Remaining blockers:

- C2 must apply the error latch, checked budget, bounded formatter, and
  no-overconsumption collector to every runtime endpoint mutator.
- C3 must make rate-limit constructors fallible/incremental and validate
  limiter preparation limits against the hard cap.
- C4 must apply the capped formatter to generated/public-head dynamic capture
  and complete the documented trust-boundary enforcement.
