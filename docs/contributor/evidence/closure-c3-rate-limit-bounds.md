# Closure C3 rate-limit bounds

Phase:
Closure C3 — Bound rate-limit semantics and governor preparation.

Started from commit:
`1fe6351b61aa8eba96f5ae63fc2859189f88b23e`
(`core: bound runtime endpoint construction incrementally`) on branch
`master`.

Finished at commit:
`12a71f612dd0bfd2ccb877f401067a2bd33ad2f5`
(`core: enforce finite rate limit structures`).

Files changed:

- `CHANGELOG.md`;
- `CONCORD_REFACTOR_CLOSURE_PLAN.md`;
- `MIGRATION.md`;
- `crates/concord-core/src/client.rs`;
- `crates/concord-core/src/execute/auth_tests.rs`;
- `crates/concord-core/src/execute/tests.rs`;
- `crates/concord-core/src/pagination/runtime_tests.rs`;
- `crates/concord-core/src/rate_limit/limiter.rs`;
- `crates/concord-core/src/rate_limit/types.rs`;
- `crates/concord-core/src/request/head.rs`;
- `crates/concord-core/src/request/prepared.rs`;
- `crates/concord-core/src/runtime/config.rs`;
- `crates/concord-core/tests/public_types/rate_limit_api.rs`;
- `crates/concord-extras/src/rate_limit/governor.rs`;
- `crates/concord-extras/src/rate_limit/observer.rs`;
- `crates/concord-extras/tests/governor.rs`;
- `crates/concord-macros/src/lib.rs`;
- `crates/concord-macros/src/semantic/inheritance.rs`;
- `crates/concord-macros/src/semantic/mod.rs`;
- `crates/concord-macros/src/syntax/rate_limits.rs`;
- `crates/concord/tests/runtime_endpoint_lifecycle.rs`;
- `crates/concord/tests/rustdoc.rs`;
- `crates/concord/tests/ui/fail/rate_limits/rate_limit_rule_limit.rs`;
- `crates/concord/tests/ui/fail/rate_limits/rate_limit_rule_limit.stderr`;
- `crates/concord/tests/ui/fail/rate_limits/rate_limit_window_limit.rs`;
- `crates/concord/tests/ui/fail/rate_limits/rate_limit_window_limit.stderr`;
- `docs/contributor/evidence/closure-c2-runtime-endpoint-bounds.md`;
- `docs/contributor/evidence/closure-c3-rate-limit-bounds.md`;
- `docs/reference/limits.md`;
- `fixtures/features/tests/no_governor.rs`;
- `tools/xtask/src/main.rs`.

Design decisions:

- The C1 public hard limits are now enforced at every core semantic
  constructor. `RateLimitId::new` and `RateLimitKeyPart::new` inspect borrowed
  text before copying; keys, windows, and plans consume incrementally without
  consulting iterator size hints or collecting an unbounded input first.
- `RateLimitKeyPart::new` is fallible.
  `RateLimitPlan::try_from_rules` is the sole public nonempty plan
  constructor. The former infallible `from_rules` API is removed.
- One private mapped-construction path owns key validation for both already
  validated public parts and generated/public-head raw name/value pairs. It
  checks the part count before inspecting the withheld excess pair, then
  checks duplicate names and aggregate bytes before retention.
- One private mapped-construction path likewise owns window conversion for
  public `RateLimitWindow` values and generated raw capacity/duration pairs.
  It checks the count before conversion, validates cost as each window is
  consumed, and creates only the final bounded window allocation.
- Plan append validation is shared by public plan construction, generated
  head preparation, and runtime endpoint construction. It rejects the 65th
  rule and duplicate IDs before push. The internal finalizer is reachable
  only after those append checks.
- `RuntimeConfigBuilder` snapshots the limiter preparation bound as before,
  but `ClientCore` now rejects a selected bound above
  `MAX_RATE_LIMIT_KEY_BYTES` before resolving the immutable client. Generated
  limiter preparation performs the same validation. Zero remains valid for
  global-only plans.
- `GovernorRateLimiter::key_limit` rejects values above the hard cap.
  Governor preparation also checks the cap defensively. It creates one
  deeply owned `RuleKey` per rule and shares it with `Arc` across that rule's
  windows instead of copying ID/key strings per window.
- Macro static validation mirrors only the three declaration-owned limits:
  ID bytes, windows per rule, and effective rules per plan. The documentation
  xtask mechanically compares those private literals with the core owners.
  Dynamic key limits remain entirely core-owned.
- Stable Rust owns the two exact new Trybuild snapshots. The 17th-window and
  65th-effective-rule outputs were inspected individually before their
  snapshots were added.
- Existing governor aggregation tests that previously formed invalid plans
  with duplicate IDs now express equivalent contributions as duplicate
  windows within one valid rule. Their undercount and fail-before-state
  assertions are unchanged.

Audit findings closed:

- F-02 is closed. IDs, key parts, keys, rules, and plans are independently
  hard-bounded before limiter acquisition; arbitrary iterators stop at the
  withheld first excess; and duplicate key names/rule IDs stop without tail
  consumption.
- The rate-limit consumer portion of F-16 is closed against the canonical C1
  constants, including mechanical macro-mirror ownership.
- The governor preparation portion of F-01/F-18 is closed. A plan can expand
  to at most 64 rules and 1,024 rule/window buckets, while one key is capped
  at 32 parts and 65,536 bytes before governor work.

Tests added/changed:

- Added every required named owner:
  `rate_limit_id_max_and_first_excess`,
  `rate_limit_key_part_max_and_first_excess`,
  `rate_limit_key_iterator_stops_at_first_excess`,
  `rate_limit_key_duplicate_stops_without_tail_consumption`,
  `rate_limit_rule_window_iterator_stops_at_first_excess`,
  `rate_limit_plan_rule_iterator_stops_at_first_excess`,
  `rate_limit_plan_rejects_duplicate_ids`,
  `rate_limiter_rejects_preparation_limit_above_hard_cap`,
  `governor_accepts_maximum_valid_plan`,
  `governor_rejects_first_excess_without_sensitive_output`,
  `generated_rate_limits_obey_same_structural_limits`, and
  `runtime_endpoint_rate_limits_obey_same_structural_limits`.
- Guarded key/window/rule iterators panic if polled past the withheld first
  excess. The generated raw-key guard also panics if the first excess pair is
  inspected through `AsRef`.
- The governor maximum test acquires a 64-rule plan with 16 windows per rule
  and observes exactly 1,024 quota entries.
- Macro unit tests cover the first excess static window, effective rule, ID
  byte, and duplicate effective ID.
- Stable compile-fail cases own the exact first-excess rule/window
  diagnostics. Existing exact snapshots were not changed.
- Public core type and rendered Rustdoc owners now require the fallible
  constructors and explicitly reject the removed `from_rules` method.

Commands executed:

```text
cargo fmt --all
cargo fmt --all -- --check
cargo check -p concord-core --lib --all-features --locked
cargo check -p concord-extras --all-targets --all-features --locked
cargo check -p concord-macros --all-targets --locked
cargo check -p concord --tests --all-features --locked
cargo check -p concord-xtask --all-targets --locked
cargo test -p concord-core --lib --all-features --locked rate_limit::types::tests --no-fail-fast
cargo test -p concord-core --lib --all-features --locked rate_limit::types::tests
cargo test -p concord-core --lib --all-features --locked generated_rate_limits_obey_same_structural_limits
cargo test -p concord-core --lib --all-features --locked runtime_endpoint_rate_limits_obey_same_structural_limits
cargo test -p concord-core --lib --all-features --locked rate_limiter_rejects_preparation_limit_above_hard_cap
cargo test -p concord-extras --lib --all-features --locked governor_accepts_maximum_valid_plan
cargo test -p concord-extras --lib --all-features --locked governor_rejects_first_excess_without_sensitive_output
cargo test -p concord-macros --lib --locked rate_limit
cargo test -p concord-macros --lib --locked rate_limit_normalization_rejects_the_first_excess_id_byte
cargo test -p concord-macros --lib --locked
cargo test -p concord --test ui --no-default-features --locked authentication_and_rate_limit_contracts_reject_invalid_declarations -- --exact --nocapture
cargo test -p concord --test ui --no-default-features --locked authentication_and_rate_limit_contracts_reject_invalid_declarations -- --exact
cargo test -p concord-core --lib --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord-core --test lifecycle --all-features --locked
cargo test -p concord-core --test security --all-features --locked
cargo test -p concord-core --test public_types --all-features --locked
cargo test -p concord-extras --all-features --locked
cargo test -p concord-extras --test governor --all-features --locked
cargo test -p concord --test runtime_endpoint_lifecycle --all-features --locked
cargo test -p concord --test generated_surface --all-features --locked
cargo test -p concord-examples --all-features --locked
cargo test -p concord --test rustdoc --all-features --locked
cargo test -p concord --all-features --locked
cargo +1.85.0 check -p concord-core -p concord-extras -p concord-macros -p concord --all-targets --all-features --locked
cargo +stable clippy -p concord-core -p concord-extras -p concord -p concord-macros -p concord-xtask --all-targets --all-features --locked -- -D warnings
cargo run --locked -p concord-xtask -- docs
cargo rustdoc --locked -p concord --all-features -- -D warnings
cargo rustdoc --locked -p concord-core -p concord-extras --all-features -- -D warnings
cargo rustdoc --locked -p concord-core --all-features -- -D warnings
cargo rustdoc --locked -p concord-extras --all-features -- -D warnings
cargo bench -p concord-perf --bench concord_overhead --no-run --locked
git diff --check
git status --short
```

Focused and phase-matrix commands were repeated after repairs and after the
shared raw-key/window finalizers were complete. Read-only inspection also used
`rg`, `Get-Content`, `git diff`, `git diff --stat`, and `git status --short`
to audit constructors, call sites, snapshots, documentation, and the exact
change set.

Exact results:

- The initial focused core-lib test compilation exited 1: the new negative
  tests used `expect_err`, which would require adding prohibited `Debug`
  implementations to key-bearing success types, and one test-only limit
  import was missing. The tests were changed to extract errors without a
  `Debug` bound and the import was added. The final core-lib gate exited 0
  with 354 passed.
- Core bounds exited 0 with 12 passed; lifecycle exited 0 with 6 passed;
  security exited 0 with 3 passed; public types exited 0 with 24 passed.
- The first full extras run exited 1 with 36 passed and 2 failed. Both failed
  during plan construction because old governor tests deliberately used
  duplicate rule IDs. They were converted to duplicate-window contribution
  cases. The final extras run exited 0: 38 unit tests, all integration tests,
  and the governor privacy doctest passed. The separate governor integration
  owner exited 0 with 1 passed.
- The first two targeted Trybuild runs exited 1 because the two deliberate new
  snapshots were absent. The emitted diagnostics were inspected, the
  deeply-nested rule fixture was flattened to keep the primary span at column
  9, and both snapshots were added with `apply_patch`. The final targeted
  stable owner exited 0: 4 authentication and 7 rate-limit compile-fail cases
  passed.
- Macro unit tests exited 0 with 65 passed.
- Runtime endpoint lifecycle exited 0 with 9 passed; generated surface exited
  0 with 1 passed.
- Examples exited 0: 1 construction test, 7 local-runtime tests, and 3 Riot
  contract tests passed; all example targets and doctests passed.
- The first complete façade run exited 1 only because the rendered Rustdoc
  test still required `method.from_rules`. That owner was changed to require
  `method.try_from_rules` and forbid the old method. The subsequent complete
  all-feature façade run exited 0: all integration, 7 public-compile,
  7 exact-UI groups, rendered Rustdoc, and doctest owners passed.
- Warning-denied stable Clippy exited 0. The localized MSVC
  `linker_messages` output remained visible and is explicitly not promoted by
  `-D warnings`.
- The Rust 1.85 all-target/all-feature check for core, extras, macros, and
  façade exited 0.
- Documentation checks exited 0 and mechanically matched all three macro
  mirrors to core.
- Warning-denied façade, core, and extras Rustdoc commands each exited 0.
  One attempted combined core/extras `cargo rustdoc` invocation exited 1
  because Cargo rustdoc accepts only one `--package`; the two valid individual
  commands replaced it.
- The optimized benchmark no-run command exited 0 and produced
  `concord_overhead-d01aa542161bf3e9.exe`.
- An ancillary first attempt to read Trybuild `wip` files from the repository
  root failed because Trybuild placed them under `crates/concord/wip`; the
  corrected direct reads succeeded. One recursive snapshot search timed out
  after identifying the correct rule snapshot path. Neither incident changed
  source or accepted output.
- The final `cargo fmt --all -- --check`, documentation ownership check, and
  `git diff --check` each exited 0 immediately before the phase commit.

Unexecuted checks:

- No benchmark was executed, so no numeric performance comparison is claimed.
- No graphical editor or manual rust-analyzer acceptance session ran in C3.
- No remote GitHub Actions or independent Linux/macOS platform lane ran.
- No package, registry, legal-metadata, or publication check ran in C3.
- OAuth2 and remaining formatted public-head capture are intentionally
  unexecuted until C4.

Security/redaction review:

- Rate IDs, key-part names, key values, and rejected structural inputs never
  enter `RateLimitError`, `ApiError`, `ConfigurationError`, or
  `ClientBuildError`. Structural failures retain only typed categories.
- Key-containing semantic types still implement neither `Debug` nor
  `Display`. Tests were adapted around that boundary rather than weakening
  it.
- Exact-limit error tests use distinct ID/key sentinels and check both
  `Display` and `Debug` error rendering. Generated and governor errors also
  walk or check their safe source surfaces.
- The generated raw-pair guard proves the 33rd pair is consumed only as the
  withheld first excess and is never inspected or copied. Duplicate
  key/rule guards prove no iterator tail is consumed.
- Governor preparation checks key limits before state insertion. The
  first-excess test observes one acquisition attempt and zero quota entries.
- Macro diagnostics contain only structural counts and the source declaration
  span. No runtime key, protected URL, authentication value, or internal
  generated bridge text is emitted.
- Full lifecycle, security, stable UI, façade, and example owners passed; no
  transport, authentication recovery, completion, cancellation, response
  acquisition, or pagination ordering changed.

Performance impact:

- Plan shape is now bounded to 64 rules × 16 windows = 1,024 governor bucket
  contributions. Key shape is bounded to 32 parts and 65,536 aggregate bytes.
- Successful construction is linear except bounded duplicate checks:
  key-name checks are at most 32² comparisons and rule-ID checks at most 64².
- Generated raw windows now allocate only their final bounded vector rather
  than a temporary vector followed by a second rule allocation.
- Governor preparation performs one deep ID/key copy per rule and cheap
  `Arc` clones per window, eliminating the former per-window deep key copy.
- The optimized performance target compiles. No measured regression or
  improvement is claimed.

Deviations:
None.

Remaining blockers:

- C4 must bound OAuth2 builder inputs and checked encoding, expose the provider
  request limit semantically, and apply capped formatting to generated and
  pagination-shaped public-head capture.
- C5–C8 remain in their planned order for API/macro cleanup, package graph,
  executable validation/documentation truth, and final certification.
