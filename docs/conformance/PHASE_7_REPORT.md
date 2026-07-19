# Phase 7 Integrated Conformance Report

## Verdict

`PHASE_7_COMPLETE_WITH_LIMITATIONS`

All Phase 7-owned rate-limiting exit criteria pass at the immutable final source/test commit. The
remaining limitations are clauses of compound Concord 0.1 requirements whose unimplemented
behavior is explicitly owned by Phases 8-10. No Phase 7 limiter, governor, cooldown, DSL, public
API, generated construction, lifecycle, security, feature, or local-integration criterion is
deferred.

`PHASE_8_NOT_STARTED`

This report does not claim full Concord 0.1 conformance. The normative status remains
`Normative Implementation Candidate`.

## Immutable evidence identity

- Starting HEAD: `6972d0e93fc52205d433c7b4abb3e5e0e6d32086`
- Branch: `master`
- Initial working tree: clean
- Prior immutable Phase 6 source/test commit:
  `e4c1cca94962cf0ee6855bab3c252c1caba19202`
- Prior Phase 6 evidence/report/state commit:
  `a4e4a004a6868f1d5dd462fef077cc948fd5109f`
- Prior Phase 6 evidence anchor:
  `c19f8cd976bae31180e797736c4d02ce1beb54bb`
- Initial Phase 7 source/test commit:
  `fa85a77da74566e68db0a59954c1172daf75d574`
- Source-correction commits:
  `12691b5d5a5eec34507cb3e968fc35a8fe5c3a9d`,
  `52e4604fff4824f0061d78d38177d815e204ec78`
- Immutable final tested Phase 7 source/test commit:
  `52e4604fff4824f0061d78d38177d815e204ec78`
- Phase 7 evidence/report/state commit: `PENDING_PHASE_7_EVIDENCE_STATE`
- Phase 7 evidence anchor: the subsequent documentation-only
  `docs: anchor phase 7 evidence` commit; its object identity is external to its own content
- Complete Phase 7 executable patch SHA-256:
  `2d6eda8bb2d60bb91ed032061fa421221c980d687b9d43d777e297ccd68d6b87`
- Authoritative validation date: `2026-07-19`
- Rust edition: 2024
- MSRV exercised: Rust 1.85.0
- Cargo resolver: 3
- Managed HTTP baseline: Reqwest 0.13.4 with default features disabled

The ignored contract map is `git_diff/phase-7-contract-map.md`, the validation ledger is
`git_diff/phase-7-validation-log.md`, and the complete executable patch is
`git_diff/phase-7-rate-limiting.patch`. The contract map contains exactly 98 unique requirement
IDs in 14 trace groups. The effective command policy forbids every `git worktree` invocation,
including the otherwise read-only `git worktree list`; its unavailability is recorded as a policy
limitation and was not bypassed.

## Commit and correction boundary

| Unit | Commit | Subject | Durable result |
|---|---|---|---|
| Phase 7 source/test | `fa85a77da74566e68db0a59954c1172daf75d574` | `feat: implement phase 7 rate limiting` | Added the complete executable DSL, public API, generated integration, governor, fallback, cooldown, lifecycle, security, and validation surface. |
| Source correction 1 | `12691b5d5a5eec34507cb3e968fc35a8fe5c3a9d` | `test: align generated contract version` | Corrected the maintained generated-contract fixture from version 7 to the synchronized version 8. |
| Source correction 2 | `52e4604fff4824f0061d78d38177d815e204ec78` | `test: account for workspace feature unification` | Made the generated default-limiter fixture assert the resolved feature graph under both isolated no-governor and workspace-unified governor builds. |
| Evidence/report/state | `PENDING_PHASE_7_EVIDENCE_STATE` | `docs: record phase 7 conformance evidence` | Adds this report and prospective actual evidence and reconciles coverage, planning, controller state, and current-state documentation. |
| Evidence anchor | external identity | `docs: anchor phase 7 evidence` | Replaces pending evidence-state/time markers only; changes no executable artifact, classification, evidence ID, or count. |

The first correction was found by the required maintained `just test` run. The second was found on
the restarted maintained run when Cargo workspace feature unification enabled the governor through
the core package. The isolated `cargo nextest run -p concord_macros --test phase7_generated` command
continues to exercise the no-governor graph directly, while `just test` exercises the unified graph.
No source, test, fixture, manifest, lockfile, generated contract, or executable validation
configuration changed after `52e4604fff4824f0061d78d38177d815e204ec78`; the complete focused and
maintained matrix was rerun from the beginning against that exact commit.

## Authority and phase boundary

The normative index and all eight normative owners were reviewed before implementation, followed
by the current implementation program, Phase 6 report and chain, accepted decisions, core and macro
architecture, repository guide, source, test architecture, and maintained recipes. No material
authority conflict was found.

The implementation preserves ADR-002, ADR-013, ADR-014, ADR-017, ADR-018, ADR-030, ADR-035, and
ADR-041. It extends the existing request/head/executor path and does not add another request,
prepared head, executor, transport, retry loop, operation identity, deadline, authentication
lifecycle, application client, or provider client.

Phase 8 pagination execution, Phase 9 prepared endpoints and extension certification, and Phase 10
positive dangerous raw-success and release closure remain absent.

## Implemented DSL and normalization

The canonical macro compiler now supports:

- client `rate_limits` blocks with multiple named templates, multiple positive windows, default or
  explicit positive costs, and one optional `Default + RateLimitObserver` declaration;
- exactly the dimensions `host`, `endpoint`, `method`, `vars.NAME`, `scope.NAME`, and `ep.NAME`,
  with order preserved and `by []` producing a global key;
- only required, non-secret, non-optional dynamic values with a generated `Display` bound and no
  unnecessary `Clone` bound;
- client, scope, and endpoint attachments; ordinary append; one leading `replace`; exclusive
  `off`; and direct local rules;
- exact client/scope/endpoint clause positions without adding pagination syntax;
- deterministic IDs, names, ordering, inheritance, and collision diagnostics;
- a normalized IR with no unresolved append, replace, off, or attachment operations.

Focused parser, resolver, UI, generated-runtime, renamed-dependency, nested-invocation, and minimal
bounds tests cover positive and negative syntax, every dimension, optional/secret/undefined and
invalid namespace rejection, costs/windows, composition, order, collisions, and normalized plans.

## Public and generated API surface

`concord_core::advanced` now exports the approved stable contracts:

- safe `RateLimitError` and the existing `RateLimitErrorKind` variants;
- opaque `RateLimitId`, `RateLimitKeyPart`, `RateLimitKey`, `RateLimitWindow`, `RateLimitRule`,
  `RateLimitPlan`, and `RateLimitPermit` values with the exact specified constructors/accessors;
- body-free and credential-free `RateLimitRequest`, `RateLimitResponse`, `RateLimiter`,
  `RateLimitResponseHead`, `RateLimitObserver`, `Cooldown`, and `CooldownScope` contracts;
- `GovernorRateLimiter`, `DefaultRateLimitObserver`, and `NoopRateLimiter` with their approved
  constructors and finite configuration methods;
- `RuntimeConfigBuilder::rate_limiter(Arc<dyn RateLimiter>)` while retaining the existing finite
  `rate_limit_cooldown_cap`.

No limiter type was added to the ordinary prelude. Native requests/responses, Reqwest types, native
URLs, clocks, schedulers, maps, buckets, entries, waiters, transports, and executors remain private.
Negative UI and rendered Rustdoc tests prove the privacy boundary and the absence of key `Debug`.

Generated contract version 8 is synchronized between macro and core. Every generated request builds
one canonical immutable plan before protected side effects. Generated `.runtime(...)` remains the
only path for supplying a custom limiter or explicit no-op limiter. A DSL observer is constructed
once per independent build only when the generated default governor is selected. Client clones
share the selected limiter; independent default builds do not; pending requests retain their
original client snapshot.

## Governor, cooldown, and fallback behavior

The repository-owned governor is bounded best-effort process-local pacing, not strict or
distributed enforcement. Its default finite configuration is:

| Bound | Default |
|---|---:|
| Cooldown cap | 60 seconds |
| Quota-entry capacity | 4096 |
| Cooldown-entry capacity | 4096 |
| Idle quota-entry lifetime | 15 minutes |
| Aggregate resolved-key limit | 1024 bytes |

Configuration rejects zero quota or cooldown capacities and invalid idle TTL/key limits. A zero key
limit accepts global keys and rejects nonempty keys without reproducing their values.

Each acquisition prunes expired/idle state, refills applicable windows with bounded arithmetic,
evaluates client/host/current-bucket cooldowns, computes the complete wait, releases its mutex
before sleeping, reevaluates after waking, and reserves every effective cost atomically only when
all buckets are available. Equivalent bucket contributions are aggregated, so repeated rules cannot
undercount. Waiting consumes no quota or partial subset. Cancellation before grant leaves quota
available; a granted reservation is never refunded. State is bounded with deterministic
least-recently-used-equivalent eviction, and an evicted key receives a fresh local burst when seen
again. Poisoned or impossible internal state fails closed.

The default observer considers only final 429 responses, parses decimal delta seconds and HTTP
dates with whitespace handling, ignores malformed/past/non-positive results, chooses current
buckets or the empty-plan host fallback, and clamps positive results to the runtime cap. Later
cooldowns extend but never shorten state. Observation pressure or failure cannot alter the current
HTTP result.

`NoopRateLimiter` explicitly grants every plan and ignores observation. Without
`rate-limit-governor`, stable custom and no-op contracts remain available; the private fallback
authorizes an empty plan and rejects a nonempty plan before send. A generated nonempty plan fails
construction unless a custom functional limiter or explicit no-op limiter is supplied. JSON does
not enable the governor.

## Architecture and request lifecycle

The direct Phase 7 attempt path is:

```text
public head and body metadata
→ canonical immutable rate-limit plan and key-bound validation
→ before-send hook
→ body materialization
→ complete credential preflight
→ one deadline-bounded limiter acquisition
→ credential finalization
→ auth-free native request construction
→ safe request debug
→ native-only authentication materialization
→ managed Reqwest execution
→ response debug
→ response-head hook
→ body-free limiter observation
→ authentication challenge or status/body/decode
→ completion outside the already-fixed result deadline
```

Acquisition occurs once per Concord attempt, not construction and not hidden Reqwest physical
attempts. Hook veto, body failure, credential-preflight failure, limiter failure, and key overflow
stop at their exact boundaries. Limiter waiting uses the remaining original deadline and holds no
governor lock across `await`. Credential clear, replacement, rejection, or expiry during waiting is
seen by finalization. Authentication recovery retains identity/deadline and visibly reacquires real
limiter authority. Provider HTTP remains outside application limiting.

Response observation sees only endpoint, method, safe URL, logical host, page index zero, immutable
plan, status, and sanitized headers. It precedes body polling and challenge mutation and cannot
replace or retry the response. Final 429 remains `HttpStatus`; `ApiError::retry_after()` remains an
independent safe and capped metadata path.

## Security, redaction, cancellation, and state bounds

- High-entropy sentinels cover rate keys, dynamic dimensions, credentials, auth query/header data,
  request/response/status bodies, controller placeholders, and native authenticated URLs.
- Recursive checks cover `Display`, `Debug`, sources, hooks, debug/completion events, built-in
  limiter errors, safe URLs, sanitized headers, and rendered Rustdoc.
- Key sentinels appear only in controlled custom-limiter fixtures proving intentional advanced key
  inspection; built-in output remains key-free.
- Plan construction excludes body and authentication data and enforces checked aggregate byte
  counts before hooks or protected work.
- Quota/cooldown maps, waits, idle entries, key bytes, cooldown duration, arithmetic, and state
  insertion are finite.
- Deterministic barriers and paused/controlled time, not timing sleeps, prove wait/refill,
  cancellation, atomicity, cooldown, pruning, eviction, and future-call-only behavior.
- No governor lock is held across `await` or user code; no detached cleanup task or unsafe block was
  added.

## Exact executable change surface

The executable delta from the verified baseline through the final source commit contains exactly
these 61 paths:

```text
concord_core/src/__private.rs
concord_core/src/advanced.rs
concord_core/src/client.rs
concord_core/src/error/transport.rs
concord_core/src/execute/auth_tests.rs
concord_core/src/execute/mod.rs
concord_core/src/execute/tests.rs
concord_core/src/generated_contract.rs
concord_core/src/http/build.rs
concord_core/src/http/mod.rs
concord_core/src/http/test_support.rs
concord_core/src/lib.rs
concord_core/src/rate_limit/error.rs
concord_core/src/rate_limit/governor.rs
concord_core/src/rate_limit/limiter.rs
concord_core/src/rate_limit/mod.rs
concord_core/src/rate_limit/types.rs
concord_core/src/request/head.rs
concord_core/src/request/prepare.rs
concord_core/src/runtime/config.rs
concord_core/tests/config_methods.rs
concord_core/tests/fixtures/config_features/Cargo.toml
concord_core/tests/fixtures/config_features/src/bin/absence_governor.rs
concord_core/tests/fixtures/config_features/src/bin/presence_governor.rs
concord_core/tests/generated_contract.rs
concord_core/tests/public_rustdoc.rs
concord_core/tests/rate_limit_api.rs
concord_core/tests/rate_limit_generated.rs
concord_core/tests/ui.rs
concord_core/tests/ui/fail/rate_limit_context_exclusions.rs
concord_core/tests/ui/fail/rate_limit_context_exclusions.stderr
concord_core/tests/ui/fail/rate_limit_key_debug_absent.rs
concord_core/tests/ui/fail/rate_limit_key_debug_absent.stderr
concord_core/tests/ui/fail/rate_limit_opaque_values.rs
concord_core/tests/ui/fail/rate_limit_opaque_values.stderr
concord_core/tests/ui/fail/rate_limit_private_state.rs
concord_core/tests/ui/fail/rate_limit_private_state.stderr
concord_macros/src/ast.rs
concord_macros/src/codegen.rs
concord_macros/src/model.rs
concord_macros/src/parse.rs
concord_macros/src/resolve.rs
concord_macros/tests/compile.rs
concord_macros/tests/fixtures/renamed_core/src/lib.rs
concord_macros/tests/phase7_generated.rs
concord_macros/tests/ui.rs
concord_macros/tests/ui/fail/phase7_rate_clause_order.rs
concord_macros/tests/ui/fail/phase7_rate_clause_order.stderr
concord_macros/tests/ui/fail/phase7_rate_display_required.rs
concord_macros/tests/ui/fail/phase7_rate_display_required.stderr
concord_macros/tests/ui/fail/phase7_rate_invalid_cost.rs
concord_macros/tests/ui/fail/phase7_rate_invalid_cost.stderr
concord_macros/tests/ui/fail/phase7_rate_off_exclusive.rs
concord_macros/tests/ui/fail/phase7_rate_off_exclusive.stderr
concord_macros/tests/ui/fail/phase7_rate_optional_dimension.rs
concord_macros/tests/ui/fail/phase7_rate_optional_dimension.stderr
concord_macros/tests/ui/fail/phase7_rate_secret_dimension.rs
concord_macros/tests/ui/fail/phase7_rate_secret_dimension.stderr
concord_macros/tests/ui/fail/phase7_rate_undefined_attachment.rs
concord_macros/tests/ui/fail/phase7_rate_undefined_attachment.stderr
concord_test_support/src/tls.rs
```

The final delta is 6522 insertions and 187 deletions. Cargo.lock and production manifests are
unchanged; the only manifest path is the existing test-only feature fixture. No direct production
dependency was added.

## Exact focused validation

Every final command below ran from `F:\projects\concord_api` against
`52e4604fff4824f0061d78d38177d815e204ec78` with exit status 0.

| Exact command | Configuration and result |
|---|---|
| `cargo nextest run -p concord_macros --lib` | Parser/resolver/codegen units; 26 passed |
| `cargo nextest run -p concord_macros --test phase7_generated` | Isolated generated/no-governor graph; 7 passed |
| `cargo nextest run -p concord_macros --test ui` | Trybuild/UI diagnostics and minimal bounds; 21 passed |
| `cargo nextest run -p concord_macros --test compile` | Downstream, renamed dependency, contract mismatch, Rustdoc; 8 passed |
| `cargo nextest run -p concord_core --lib rate_limit::governor::tests` | Deterministic governor; 18 passed, 290 filtered |
| `cargo nextest run -p concord_core --lib real_tls_` | Real local TLS recovery and provider isolation; 3 passed, 305 filtered |
| `cargo nextest run -p concord_core --test rate_limit_api` | Exact public API with defaults; 2 passed |
| `cargo nextest run -p concord_core --test rate_limit_api --no-default-features` | Stable API without governor; 1 passed |
| `cargo nextest run -p concord_core --test rate_limit_generated` | Ownership, retained reservation, loopback cooldown; 3 passed |
| `cargo nextest run -p concord_core --test ui --all-features` | Core privacy UI; 1 passed |
| `cargo nextest run -p concord_core --test config_methods` | Feature topology fixture; 1 passed |
| `cargo nextest run -p concord_core --test public_rustdoc` | Rendered Rustdoc exclusions; 1 passed |
| `cargo nextest run -p concord_core --no-default-features --features json --lib custom_limiter_context_and_response_observation_use_one_ordered_attempt` | JSON-only custom limiter; 1 passed, 255 filtered |
| `cargo nextest run -p concord_core --no-default-features --features json --lib private_fallback_rejects_a_nonempty_runtime_plan_before_send` | JSON-only no-governor fail-closed path; 1 passed, 255 filtered |
| `cargo check -p concord_core --no-default-features --features rate-limit-governor --all-targets` | Independent governor feature; passed |

## Complete maintained validation

| Exact command | Configuration | Exit/result |
|---|---|---|
| `just fmt-check` | `cargo fmt --all -- --check` | 0; passed |
| `just check` | workspace, all targets, defaults | 0; passed |
| `just check-all` | workspace, all targets, all features | 0; passed |
| `just check-no-default` | core, all targets, no defaults | 0; passed |
| `just clippy` | workspace defaults, `-D warnings` | 0; passed |
| `just clippy-all` | workspace all features, `-D warnings` | 0; passed |
| `just test` | workspace defaults, Nextest | 0; 493/493 passed |
| `just test-all` | workspace all features, CI Nextest profile | 0; 497/497 passed |
| `just test-no-default` | maintained no-default typed HTTP smoke | 0; 1/1 passed, 252 filtered |
| `just doctest` | workspace default doctests | 0; two compile-fail doctests passed |
| `just doctest-all` | workspace all-feature doctests | 0; two compile-fail doctests passed |
| `just doc` | each library, defaults, Rustdoc `-D warnings` | 0; passed |
| `just doc-all` | each library, all features, Rustdoc `-D warnings` | 0; passed |
| `just deny` | advisories, bans, licenses, sources | 0; passed with existing allowed duplicate-version warnings only |
| `just diff-check` | maintained diff/whitespace check | 0; passed |
| `just release` | complete repository-owned local release recipe | 0; passed in 69.4 seconds |

The final release recipe repeated the 493 default and 497 all-feature suites, the no-default smoke,
both doctest/doc graphs, dependency policy, diff checks, and independent dangerous-feature suites:
default 2/2, custom retry 3/3, development tools 2/2, and raw flag 2/2.

One non-maintained exploratory broad all-target no-default Nextest invocation was not applicable to
the existing TLS/provider integration targets and failed before the final source boundary. It was
not substituted for the maintained `just test-no-default` recipe; exact no-governor, JSON-only,
public-API, fallback, and independent-governor commands above passed. A malformed exploratory
Nextest filter selected zero tests; the corrected `real_tls_` command passed 3/3. Neither
exploratory command is recorded as passing evidence.

The only required check unavailable was `git worktree list`, denied by effective repository policy.
No maintained validation command was unavailable.

## Requirement-to-source-to-test-to-command-to-evidence trace

| Trace | Requirements | Source/test owner | Evidence |
|---|---|---|---|
| T1 DSL declarations/composition | `DSL-RATE-001`-`007` | macro AST/parser/model/resolver, Phase 7 generated and UI suites | `P7-DSL-001`-`007` |
| T2 public limiter API | `API-RATE-001`-`004` | core rate-limit modules, advanced exports, API/UI/Rustdoc tests | `P7-API-001`-`004` |
| T3 generated observer ownership | `GEN-BUILD-004` | macro codegen, runtime/private build bridge, generated tests | `P7-GEN-001` |
| T4 confidentiality and bounds | `SEC-RATE-001`-`002`; `BOUND-RATE-001`-`007` | rate-limit error/types/governor and security/runtime tests | `P7-SEC-001`-`002`; `P7-BOUND-001`-`007` |
| T5 acquisition/observation | `EXEC-RATE-001`-`003` | prepared head and canonical executor tests | `P7-EXEC-001`-`003` |
| T6 governor feature/fallback | `FEAT-GOV-001`-`002` | feature-gated rate module/runtime, no-governor generated/executor tests | `P7-FEAT-001`-`002` |
| T7 product/state | five exact T7 requirements | client/runtime/pending ownership and executor/generated tests | `P7-PROD-001`-`005` |
| T8 DSL cross-cuts | six exact T8 requirements | canonical macro grammar, normalization, UI | `P7-DSL-008`-`013` |
| T9 API cross-cuts | four exact T9 requirements | safe errors, runtime selection, privacy UI/Rustdoc | `P7-API-005`-`008` |
| T10 generated cross-cuts | five exact T10 requirements | codegen/private bridge, generated/compile/Rustdoc tests | `P7-GEN-002`-`006` |
| T11 response/fail/sentinel/math | four exact T11 requirements | limiter/executor/security/governor tests | `P7-SEC-003`-`005`; `P7-BOUND-008` |
| T12 executor/auth preservation | 22 exact T12 requirements | one executor, auth slot, real TLS and deterministic runtime tests | `P7-EXEC-004`-`025` |
| T13 feature/compatibility | ten exact T13 requirements | manifests, contract v8, fixtures, compile/dependency/release tests | `P7-FEAT-003`-`006`; `P7-COMPAT-001`-`006` |
| T14 conformance/trace | 16 exact T14 requirements | compiler/runtime/UI/docs/release plus evidence-state audits | `P7-CONF-001`-`013`; `P7-AUDIT-003`-`005` |

## Evidence, coverage, and controller state

Phase 7 traces the 98 contract-map requirements through 95 immutable source/test rows (58 direct,
37 partial) and three requirement-trace audit rows (two direct, one partial). Two additional direct
catalog audits prove catalog identity and byte preservation of all earlier evidence. The Phase 7
total is therefore 100 rows: 62 direct and 38 partial. The complete catalog contains 1068 unique
evidence IDs: all 968 Phase 0-6 rows followed byte-for-byte by the 100 Phase 7 rows.

The coverage plan still has exactly one row for each of 504 normative IDs. Exactly 98 Phase 7 rows
now use current `P7-*` taxonomy, actual source/test paths, maintained/focused commands, evidence IDs,
and direct or explicit later-phase partial notes. Obsolete `M5-rate-limiting` ownership and stale
planned rate-limit paths are absent. Planned `PLAN-*` rows remain structurally separate from actual
passing evidence.

The controller records Phase 7 as the latest validated phase with this verdict and immutable source
identity. Phase 8 — Pagination becomes the active planning entry with status `NOT_STARTED`.

## Remaining limitations and later-phase owners

The 37 partial source/test rows and one partial evidence-state audit row are the complete 38 known
later-phase limitations in the Phase 7 inventory:

| Later owner | Exact partial requirements | Remaining clause |
|---|---|---|
| Phase 8 | `DSL-END-004`; `DSL-NORM-001`; `SEC-FAIL-001`; `BOUND-MATH-001`; `EXEC-ORDER-001`; `EXEC-DEADLINE-001`; `PROD-GUAR-002`; `PROD-GUAR-008`; `PROD-GUAR-010`; `CONF-SEMA-001`; `CONF-ORDER-001` | Pagination grammar/order/normalization, per-page lifecycle/deadline/counting arithmetic, diagnostics, retry containment, cycles, and side-effect matrix. |
| Phase 9 | `EXEC-CONSTRUCT-001` | Prepared-endpoint construction side-effect proof. |
| Phases 8-9 | `DSL-NAME-002`; `GEN-NAME-001`; `GEN-NAME-002`; `EXEC-CREATE-001`; `EXEC-CREATE-002`; `EXEC-PREC-001`; `EXEC-CANCEL-001`; `EXEC-COUNT-001`; `EXEC-COUNT-002`; `CONF-COMPILE-002`; `CONF-DOWNSTREAM-001`; `CONF-DOWNSTREAM-002` | Pagination/prepared names, creation, precedence, cancellation, counts, downstream workflows, and final extension certification. |
| Phases 8-10 | `DSL-FEAT-001`; `API-PRIVATE-001`; `API-PRIVATE-002`; `GEN-DOC-002`; `SEC-TEST-001`; `FEAT-DIAG-001`; `FEAT-DOC-001`; `CONF-PARSER-001`; `CONF-COMPILE-001`; `CONF-COMPILE-003`; `CONF-REQWEST-001`; `CONF-SEC-001`; `CONF-DOC-001` | Later syntax/features/private-surface/docs/security/Reqwest/downstream fixtures and release-final closure. |
| Phase 10 | `CONF-TRACE-006` | Release-final actual evidence completeness for every applicable normative requirement. |

Phase 8 owns one-acquisition-per-page behavior and pagination integration. Phase 9 owns prepared
endpoints and final extension certification. Phase 10 owns positive dangerous raw successful
responses plus release-wide grammar, platforms, documentation, security, performance, MSRV, and
evidence closure.

## Engineering and architecture review

- No unexplained work was overwritten; the baseline and final executable trees were clean.
- The complete 61-path executable delta and public/generated surface were reviewed.
- Two focused test corrections were committed without amendment; the complete matrix was restarted
  after each executable change.
- One `ClientCore`, `Request<E>`, `HeadBuilder`, `PreparedHead`, operation/deadline, authentication
  lifecycle, executor, application Reqwest client, and provider client remain.
- No lock is held across `await` or user callbacks; no quota subset is consumed while waiting.
- No test was deleted, ignored, retried away, or weakened. The feature-unification correction
  asserts both valid resolved graphs, and the isolated no-governor command remains direct evidence.
- No new production dependency, unsafe block, public transport/clock/scheduler/state entry,
  speculative API, silent fallback, second retry loop, or later-phase capability was added.
- Historical evidence rows are byte-identical and remain in their original order.

`PHASE_7_COMPLETE_WITH_LIMITATIONS`

`PHASE_8_NOT_STARTED`
