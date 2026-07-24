# Closure C2 runtime endpoint bounds

Phase:
Closure C2 — Make `RuntimeEndpointBuilder` incrementally bounded.

Started from commit:
`ee2efc9cba310f2952d5c180d30ee3f08260d315`
(`core: add canonical finite construction budgets`) on branch `master`.

Finished at commit:
The C2 commit containing this evidence, with message
`core: bound runtime endpoint construction incrementally`. Its exact hash will
be backfilled in the closure ledger by C3.

Files changed:

- `CHANGELOG.md`;
- `CONCORD_REFACTOR_CLOSURE_PLAN.md`;
- `MIGRATION.md`;
- `crates/concord-core/src/auth/placement.rs`;
- `crates/concord-core/src/construction.rs`;
- `crates/concord-core/src/rate_limit/types.rs`;
- `crates/concord-core/src/request/head.rs`;
- `crates/concord-core/src/request/mod.rs`;
- `crates/concord-core/src/request/prepared.rs`;
- `crates/concord/tests/endpoint_planning.rs`;
- `docs/contributor/evidence/closure-c1-bounded-foundation.md`;
- `docs/contributor/evidence/closure-c2-runtime-endpoint-bounds.md`;
- `docs/guide/07-explicit-and-runtime-endpoints.md`;
- `docs/reference/generated-api.md`.

Design decisions:

- `RuntimeEndpointBuilder<R>` now keeps non-response state in one
  `ErrorLatch<RuntimeEndpointParts, ConfigurationErrorKind>`. The response
  marker remains outside the latch, so every response type transition
  preserves an earlier failure.
- Each mutation works on a local copy of `StructuralBudget` and commits the
  budget and retained value together. A failed mutation drops its candidate
  and leaves only a value-free error category in the latch.
- Runtime formatting uses the lower of the canonical per-value cap and the
  endpoint's remaining aggregate byte budget. Concord therefore cannot
  temporarily retain a complete value that the aggregate budget would reject.
- Runtime query names now require `AsRef<str>`, an intentional breaking
  closure change needed to measure them before copying. Repeated query names
  use one underlying `Arc<str>` allocation while the logical public-shape
  budget still counts the name for every emitted pair.
- `query_values` calls `IntoIterator::into_iter` only while the builder is
  valid. It accepts values in order, consumes one withheld first excess, and
  returns immediately without polling the tail. An empty iterator copies no
  query name.
- Header-name reservation and reserved-header rejection occur before value
  formatting. Values become validated `HeaderValue`s during mutation.
  Replacement removes the old retained value from a temporary budget, formats
  only within the exact replacement allowance, and commits atomically without
  adding a component.
- `Auth` exposes only crate-private placement count and public placement-name
  byte length. It never exposes or formats credential material.
- `RateLimitRule` exposes only a checked retained-structure byte count.
  Runtime accounting includes rule ID plus key names and values, including
  operationally sensitive values for memory boundedness, while errors retain
  none of them. C3 remains the owner of making every rule producer itself
  structurally bounded.
- `build()` no longer performs the first component, aggregate-byte,
  authentication-count, or rate-rule-count validation. It retains only
  completed-structure work: public/auth collision checks, authentication
  challenge compatibility, immutable boxing/Arcs, safe description creation,
  and rate-plan assembly.
- Body selection stores application input without invoking metadata,
  encoding, factories, or streams. Rustdoc identifies arbitrary retained body
  input as an application extension trust boundary.

Audit findings closed:

- F-01 is closed: runtime path, query, repeated query, header,
  authentication, and rate-rule construction now reject incrementally, and
  infinite repeated-query iterators terminate at the finite component bound.
- The runtime enforcement portion of F-16 is closed against the canonical C1
  limits.
- The runtime endpoint portion of F-18 is closed. Generated/public-head
  formatted capture remains assigned to C4.
- F-02 is not claimed closed; C3 owns fallible bounded rate-limit semantic
  constructors and bounded limiter preparation.

Tests added/changed:

- Added the required named unit owners:
  `runtime_endpoint_query_values_stops_at_first_excess`,
  `runtime_endpoint_infinite_query_values_returns_configuration_error`,
  `runtime_endpoint_first_excess_component_is_not_retained`,
  `runtime_endpoint_first_excess_public_byte_is_not_retained`,
  `runtime_endpoint_errored_builder_does_not_consume_later_iterator`,
  `runtime_endpoint_header_replacement_updates_budget_exactly`,
  `runtime_endpoint_auth_first_excess_is_rejected_before_push`,
  `runtime_endpoint_rate_rule_first_excess_is_rejected_before_push`,
  `runtime_endpoint_bounded_format_does_not_retain_sentinel`, and
  `runtime_endpoint_build_remains_inert`.
- The guarded repeated-query iterator panics if polled beyond the withheld
  first excess and counts formatting calls, proving the rejected item is not
  formatted.
- Added focused auth-name, rate-key, header-replacement, and formatted-value
  redaction checks using sentinel text and credential material.
- A body specification whose metadata and preparation functions panic proves
  that mutation and `build()` remain inert.
- Renamed the existing façade planning proof to
  `runtime_endpoint_plan_remains_zero_effect`; it still verifies descriptions,
  timeout selection, and zero hooks/body/provider/limiter/transport/completion
  effects.
- Stable exact Trybuild owners, renamed-façade public compilation, runtime
  lifecycle, security, and all full façade owners passed unchanged.

Commands executed:

```text
cargo fmt --all
cargo check -p concord-core --lib --all-features --locked
cargo fmt --all
cargo check -p concord-core --lib --all-features --locked
cargo fmt --all
cargo test -p concord-core --lib --all-features --locked
cargo fmt --all
cargo test -p concord-core --lib --all-features --locked request::prepared::tests
cargo test -p concord --test endpoint_planning --all-features --locked
cargo test -p concord --test runtime_endpoint_lifecycle --all-features --locked
cargo test -p concord-core --test bounds --all-features --locked
cargo test -p concord-core --test lifecycle --all-features --locked
cargo test -p concord-core --test security --all-features --locked
cargo +stable clippy -p concord-core -p concord --all-targets --all-features --locked -- -D warnings
cargo test -p concord --all-features --locked
cargo +1.85.0 check -p concord-core --all-targets --all-features --locked
cargo +1.85.0 check -p concord --tests --all-features --locked
cargo bench -p concord-perf --bench concord_overhead --no-run --locked
cargo run --locked -p concord-xtask -- docs
cargo rustdoc --locked -p concord --all-features -- -D warnings
cargo test -p concord --test rustdoc --all-features --locked
git diff --check
cargo fmt --all -- --check
cargo test -p concord-core --lib --all-features --locked
cargo run --locked -p concord-xtask -- docs
cargo fmt --all -- --check
git diff --check
git status --short
```

Read-only inventories also used `rg`, `Get-Content`, `git status --short`,
`git diff`, and `git diff --stat` to inspect the interrupted patch, all
runtime builder call sites, body/auth/rate internals, documentation ownership,
and the final change set.

Exact results:

- The first core check exited 1 because the interrupted patch imported the
  newly split central header-name validator without reexporting it from the
  request module. The reexport was added.
- The next core check exited 0 but reported one dead-code warning for the C1
  `remaining_components` helper. The unused helper and its test assertion were
  removed; production code instead reserves the component transactionally.
- An ancillary `rg` command using
  `crates/concord-core/src/request/*.rs` exited 1 because PowerShell did not
  expand the wildcard. The explicit-path inventory returned the needed
  ownership information.
- The first complete core-lib run after the production refactor exited 0 with
  343 passed. The focused runtime endpoint run after the full named test set
  exited 0 with 17 passed. The final complete core-lib run exited 0 with 345
  passed.
- Core bounds exited 0 with 12 passed; lifecycle exited 0 with 6 passed;
  security exited 0 with 3 passed.
- Façade endpoint planning exited 0 with 5 passed, including
  `runtime_endpoint_plan_remains_zero_effect`.
- Façade runtime endpoint lifecycle exited 0 with 9 passed.
- Warning-denied stable Clippy exited 0. The localized MSVC
  `linker_messages` warning remained visible and is explicitly exempt from
  `-D warnings` by the compiler lint.
- The full all-feature façade suite exited 0. All integration owners,
  7 public-compile cases, 7 stable exact UI groups, 2 rendered Rustdoc tests,
  and doctests passed; no Trybuild snapshot changed.
- Rust 1.85 core all-target/all-feature check and façade all-feature test
  target check both exited 0.
- The benchmark no-run command exited 0 and produced
  `concord_overhead-d01aa542161bf3e9.exe`.
- Both documentation generation/check invocations, warning-denied façade
  Rustdoc, and the 2 rendered Rustdoc tests all exited 0.
- The final format check and the observed `git diff --check` exited 0.
- The pre-commit status listed exactly the 13 modified tracked files and the
  new C2 evidence file named under `Files changed`.

Unexecuted checks:

- No benchmark was executed, so C2 claims no numeric performance comparison;
  only the optimized benchmark target was compiled.
- No graphical editor or manual rust-analyzer acceptance session was
  available or attempted in C2.
- No remote GitHub Actions or independent Linux/macOS platform lane ran.
- No packaging, registry, legal metadata, or publication check ran in C2.
- C3 rate-limit producer/governor checks and C4 generated/OAuth2 formatting
  checks were not run early.

Security/redaction review:

- The latch stores only `ConfigurationErrorKind`; it cannot retain a rejected
  path/query/header value, query name, authentication placement name,
  credential, rate ID, or rate-key value.
- Component/name budgets are checked before `Display`, copying, Arc cloning,
  or vector push. Formatting is capped before allocation growth and rejects a
  complete first-excess chunk instead of retaining a prefix as a valid value.
- Later path/query calls after a reserved-header failure use panic sentinels
  and prove there is no later name inspection, formatting, `into_iter`, or
  iterator polling across a response type transition.
- Auth introspection matches only placement variants and public names.
  Credential material remains behind its private erased owner. Rate-key values
  participate only through a checked byte count.
- Sentinel assertions cover both `Display` and `Debug` for component,
  aggregate-byte, auth-name, rate-key, and formatted-capture failures.
- Build-time authentication collision validation remains value-free and
  canonical. No native request, provider, limiter, hook, operation identity,
  body method, stream, transport, or completion surface changed.

Performance impact:

- Successful scalar mutation adds checked constant-size budget arithmetic.
  Header replacement remains linear in the bounded header count.
- Repeated query construction is linear only through accepted values plus one
  first excess; it no longer collects an arbitrary or infinite iterator.
- Sharing the repeated query name with `Arc<str>` replaces one full name copy
  per retained pair with one underlying name allocation plus bounded Arc
  clones. Logical URL-size accounting still charges every emitted name.
- Failed formatting retains at most the lower of 65,536 bytes and the
  endpoint's remaining aggregate allowance.
- The optimized benchmark target compiles. No measured regression or
  improvement is claimed in C2.

Deviations:
None.

Remaining blockers:

- C3 must bound every public rate-limit ID, key part, key, rule, and plan
  constructor and validate limiter preparation limits against the hard cap.
- C4 must bound OAuth2 configuration/encoding and generated/public-head
  formatted capture.
- C5–C8 remain in their planned sequence, including public API/macro cleanup,
  package graph repair, executable validation, and final certification.
