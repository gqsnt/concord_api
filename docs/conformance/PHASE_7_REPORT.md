# Phase 7 Corrected Integrated Conformance Report

## Verdict

`PHASE_7_COMPLETE_WITH_LIMITATIONS`

Every Phase 7-owned rate-limiting criterion is implemented and directly proved at the corrected
immutable source/test commit. The remaining limitations are only clauses explicitly owned by
Phases 8-10.

`PHASE_8_NOT_STARTED`

This report does not claim full Concord 0.1 conformance. The normative status remains
`Normative Implementation Candidate`.

## Corrected immutable identities

- Correction starting HEAD: `1290ebfba618c3394ed13964bb3f355a03035588`
- Phase 7 implementation baseline: `6972d0e93fc52205d433c7b4abb3e5e0e6d32086`
- Branch: `master`
- Initial correction tree: clean
- Original implementation: `fa85a77da74566e68db0a59954c1172daf75d574`
- Original source corrections: `12691b5d5a5eec34507cb3e968fc35a8fe5c3a9d` and
  `52e4604fff4824f0061d78d38177d815e204ec78`
- Superseded evidence/report/state: `377b4cde2196725447f37a0737bcc80721ba3d82`
- Superseded documentation anchor: `1290ebfba618c3394ed13964bb3f355a03035588`
- Key-limit source/test correction: `8e3b1589ebf598c1850f273070cbc528b2dce66e`
- Deterministic-validation source correction: `c5071c0eee2b1292d2762c669ff2a7d99cd6b3c9`
- Corrected immutable tested source/test commit:
  `c5071c0eee2b1292d2762c669ff2a7d99cd6b3c9`
- Corrected evidence/report/state commit: `PENDING_PHASE_7_EVIDENCE_STATE`
- Corrected evidence anchor: `PENDING_PHASE_7_CORRECTED_ANCHOR`
- Compatible post-anchor metadata descendant: the later documentation-only commit that records the
  real corrected anchor identity; it does not replace that anchor
- Authoritative source validation completed: `2026-07-19T06:03:21Z`
- Correction patch SHA-256:
  `dc0c22a7ec75abc987a754e3679e307ed6e451ebec94fbe2c59ef3d99b3a491b`
- Corrected cumulative Phase 7 executable patch SHA-256:
  `9c5dbc1da3dd3362565aef6ff9682af043e07a05e086677a4fd8fb19a2a3e4f3`

The ignored Phase 7 product contract map is `git_diff/phase-7-contract-map.md` and traces 98
Phase 7 coverage requirements. The correction's ignored semantic map is
`git_diff/phase-7-semantic-evidence-map.tsv` and contains exactly 100 unique evidence-to-clause
traces. The ignored validation ledger is `git_diff/phase-7-correction-validation-log.md`.

## Correction chain

| Unit | Commit | Durable result |
|---|---|---|
| Accepted Phase 7 source | `52e4604fff4824f0061d78d38177d815e204ec78` | Preserved the accepted DSL, API, generated integration, governor, custom/no-op/fallback, cooldown, lifecycle, and security behavior. |
| Key-limit correction | `8e3b1589ebf598c1850f273070cbc528b2dce66e` | Unified configured governor key limits at protected preparation and governor insertion; added exact lifecycle, API, parser, privacy, and redaction tests. |
| Validation correction | `c5071c0eee2b1292d2762c669ff2a7d99cd6b3c9` | Serialized shared-directory Trybuild cases without skipping, retrying, or weakening a test. |
| Corrected evidence/state | `PENDING_PHASE_7_EVIDENCE_STATE` | Replaces all 100 Phase 7 rows with clause-accurate traces, reconciles 98 coverage rows, this report, planning, and controller state. |
| Corrected anchor | `PENDING_PHASE_7_CORRECTED_ANCHOR` | Finalizes the known evidence-state identity and audit time only; changes no executable artifact or evidence classification. |
| Anchor metadata descendant | later documentation-only commit | Records the real corrected anchor hash in the controller/report and retains `anchor-must-be-an-ancestor`. |

The first authoritative `just test-all` at `8e3b1589` passed 504 of 505 tests but exposed a real
pre-existing Trybuild race: separate Nextest cases in one `ui` binary concurrently replace files
inside the same generated per-crate package directory. `.config/nextest.toml` now assigns
`binary(ui)` to a one-thread test group with retries still disabled. The complete 62-command
focused set and all 16 maintained recipes were then rerun from the beginning at `c5071c0e`.

No executable artifact changed after `c5071c0eee2b1292d2762c669ff2a7d99cd6b3c9`.

## Configured key-limit defect and repair

The accepted implementation kept two disconnected values: the runtime preparation snapshot always
used 1,024 bytes, while `GovernorRateLimiter::key_limit(bytes)` affected only later acquisition.
That made lower and zero limits too late and prevented increased limits from authorizing otherwise
valid keys.

The correction uses a crate-private blanket metadata supertrait. It gives the runtime builder a
safe `Any` view of the already-selected limiter object. Only an actual built-in
`GovernorRateLimiter` is downcast; its immutable configured bound is copied into the same
`RuntimeConfig` snapshot as the selected `Arc<dyn RateLimiter>`. External custom and explicit
no-op limiters retain the approved 1,024-byte preparation bound. The governor independently
revalidates its same configured value immediately before state contribution or insertion.

No public `RateLimiter` method was added. The approved `RateLimiter::acquire`/`observe`,
`RuntimeConfigBuilder::rate_limiter(Arc<dyn RateLimiter>)`, and
`GovernorRateLimiter::key_limit(self, usize)` method signatures are unchanged. External custom
implementations compile with default and no-default features, the private metadata methods do not
appear in rendered Rustdoc, no unsafe inspection is used, and generated contract version 8 is
unchanged.

Exact deterministic lifecycle evidence proves:

- default aggregate boundaries accept 1,024 bytes and reject 1,025 during preparation;
- a configured limit of 16 accepts 16 and rejects 17 during preparation;
- a configured limit of zero accepts a global key and rejects a nonempty key structure during
  preparation;
- a configured limit of 2,048 permits a key above 1,024 through acquisition and native execution,
  and rejects 2,049 during preparation;
- clones share the selected limiter and copied bound, independent default builds retain 1,024 and
  independent state, and an already-created pending request retains its originating bound;
- a preparation-time failure performs no before-send hook, body materialization, credential
  acquisition or refresh, limiter acquisition, governor insertion, request debug, authentication
  materialization, or native send; completion still occurs once;
- `Display`, `Debug`, recursive sources, hooks, debug records, and completion diagnostics never
  reproduce the high-entropy key sentinel;
- direct governor acquisition rejects the same oversize key before inserting quota state.

## Preserved Phase 7 implementation

The correction leaves the accepted Phase 7 architecture and behavior intact:

- DSL templates, windows, costs, every approved dimension, global keys, observers, attachments,
  append, replace, off, direct rules, ordering, diagnostics, and fully normalized plans;
- exact advanced public value/error/context/limiter/observer/cooldown contracts and ordinary-prelude
  exclusion;
- static generated plans, `.runtime(...)` limiter selection, observer ownership, clone sharing,
  independent default-governor state, pending-request snapshots, and generated contract version 8;
- bounded best-effort process-local governor behavior with atomic multi-rule/multi-window
  reservation, duplicate-bucket accounting, cancellation-safe waiting, pruning, deterministic
  eviction, finite cooldowns, and fail-closed internal errors;
- custom limiter, explicit no-op, and no-governor empty/fail-closed behavior without feature
  leakage;
- one request/head/executor/application-client/provider-client/authentication/deadline path, one
  acquisition per Concord attempt, recovery reacquisition, hidden-Reqwest-retry containment,
  response-head observation, and unchanged current responses;
- body-free and credential-free built-in contexts, key-safe automatic diagnostics, and isolated
  provider HTTP.

Defaults remain a 60-second cooldown cap, 4,096 quota entries, 4,096 cooldown entries, 15-minute
idle quota lifetime, and 1,024 aggregate key bytes. Configured capacities remain finite and reject
zero; configured key limit zero retains its normative global-key exception. No production
dependency, manifest, lockfile, unsafe block, public transport/state/clock API, second executor, or
later-phase capability was added.

## Exact correction change surface

The correction relative to `1290ebfba618c3394ed13964bb3f355a03035588` changes exactly:

```text
.config/nextest.toml
concord_core/src/execute/auth_tests.rs
concord_core/src/execute/tests.rs
concord_core/src/rate_limit/governor.rs
concord_core/src/rate_limit/limiter.rs
concord_core/src/rate_limit/mod.rs
concord_core/src/runtime/config.rs
concord_core/tests/public_rustdoc.rs
concord_core/tests/rate_limit_api.rs
concord_macros/src/parse.rs
```

The corrected cumulative executable Phase 7 delta contains 62 files, 7,046 insertions, and 187
deletions. The key-limit correction itself adds no dependency, feature, manifest, lockfile, or
generated-contract change.

## Focused validation

All 62 unique clause-proof commands recorded in the corrected Phase 7 evidence rows ran from
`F:\projects\concord_api` against
`c5071c0eee2b1292d2762c669ff2a7d99cd6b3c9`, with the exact features written in each command and
exit status 0. They comprise:

- 33 exact core-library filters covering key limits, cutoff/redaction, governor bounds and math,
  atomicity, cancellation, deadlines, cooldowns, authentication mutation/recovery, provider
  isolation, hidden native retry, and response immutability;
- 12 exact core integration/UI/Rustdoc/generated/config/contract commands, including all-feature,
  no-default, and JSON-only no-governor configurations;
- 15 exact macro parser/resolver/UI/compile/generated commands, including no-default generated
  ownership and renamed dependency;
- `just deny` and the exact macro normal/build dependency-tree exclusion assertion.

The exact command text, UTC start/finish, working directory, feature configuration, tested commit,
exit status, and concise output for every command are retained in the ignored validation ledger and
the exact command text is durable in `CONFORMANCE_EVIDENCE_0.1.csv`. Key correction results were
4/4 for the combined boundary/snapshot/governor proof, 1/1 for the complete protected-side-effect
cutoff, 5/5 for all-feature advanced API, 4/4 for no-default advanced API, 1/1 for rendered
Rustdoc privacy, 7/7 for generated Phase 7 behavior in each applicable graph, and 21/21 for the
serialized macro UI binary.

## Complete maintained validation

Every maintained command ran from `F:\projects\concord_api` against the same immutable commit:

| Exact command | Configuration | Result |
|---|---|---|
| `just fmt-check` | workspace formatting | exit 0; passed |
| `just check` | workspace, all targets, defaults | exit 0; passed |
| `just check-all` | workspace, all targets, all features | exit 0; passed |
| `just check-no-default` | core, all targets, no defaults | exit 0; passed |
| `just clippy` | workspace defaults, `-D warnings` | exit 0; passed |
| `just clippy-all` | workspace all features, `-D warnings` | exit 0; passed |
| `just test` | workspace default Nextest | exit 0; 501/501 passed |
| `just test-all` | workspace all-feature CI Nextest | exit 0; 505/505 passed |
| `just test-no-default` | maintained no-default HTTP smoke | exit 0; 1/1 passed, 252 skipped |
| `just doctest` | workspace default doctests | exit 0; passed |
| `just doctest-all` | workspace all-feature doctests | exit 0; passed |
| `just doc` | each library, default Rustdoc, `-D warnings` | exit 0; passed |
| `just doc-all` | each library, all-feature Rustdoc, `-D warnings` | exit 0; passed |
| `just deny` | advisories, bans, licenses, sources | exit 0; passed |
| `just diff-check` | whitespace/diff validation | exit 0; passed |
| `just release` | complete repository-owned local matrix | exit 0; passed in 65.2 seconds |

The final release command repeated the applicable default, all-feature, no-default, dangerous
feature, doctest, Rustdoc, dependency-policy, and diff matrices. No maintained command was
unavailable. One obsolete exploratory JSON-only full-library invocation had previously selected TLS
tests without a crypto provider; it is not an applicable maintained command and is not recorded as
passing evidence. One superseded dependency-proof command had Windows quoting corrected before the
final 62-command rerun.

## Clause-level evidence correction

All 100 current `P7-*` rows were audited as:

```text
requirement ID
→ exact normative clause
→ exact source behavior
→ exact observable test assertion or focused command
→ direct or precise later-phase partial classification
```

Every row's clause statement, exact proof, immutable commit, and source/test paths were corrected,
so the corrected set is all of:

```text
P7-DSL-001..013
P7-API-001..008
P7-GEN-001..006
P7-SEC-001..005
P7-BOUND-001..008
P7-EXEC-001..025
P7-FEAT-001..006
P7-COMPAT-001..006
P7-PROD-001..005
P7-CONF-001..013
P7-AUDIT-001..005
```

The three classification changes are `P7-EXEC-004` direct to partial because Phase 8 owns
pagination-controller construction, `P7-EXEC-005` partial to direct because client construction's
network-free clause is fully proved, and `P7-FEAT-002` direct to partial because Phase 9 owns the
public dynamically introduced prepared-endpoint plan clause.

Final totals are 100 rows: 61 direct and 39 partial. The 95 immutable source/test rows are 57
direct and 38 partial. The five evidence-state audits are four direct and one partial. The 98
Phase 7 coverage rows are 59 direct and 39 partial. The full catalog remains 1,068 rows, and all
968 Phase 0-6 actual-evidence rows remain byte-identical and in their original order. Coverage
still maps all 504 normative IDs exactly once and keeps `PLAN-*` planning distinct from actual
passing evidence.

The semantic audit additionally checks exact commit ancestry, approved labels, artifact existence,
test-function presence in recorded paths, partial owners, coverage agreement, counts, report,
program, and controller state. Audit rows are finalized by the corrected documentation anchor
under the established evidence-state self-reference procedure.

## Requirement trace groups

| Trace | Requirements | Evidence |
|---|---|---|
| DSL declarations and cross-cuts | `DSL-RATE-001`-`007` plus six Phase 7-owned compound clauses | `P7-DSL-001`-`013` |
| Public limiter API and cross-cuts | `API-RATE-001`-`004` plus four API clauses | `P7-API-001`-`008` |
| Generated integration | `GEN-BUILD-004` plus five generated clauses | `P7-GEN-001`-`006` |
| Security and arithmetic bounds | two dedicated security, seven dedicated bounds, and six shared clauses | `P7-SEC-001`-`005`; `P7-BOUND-001`-`008` |
| Execution and authentication preservation | three dedicated and 22 shared execution clauses | `P7-EXEC-001`-`025` |
| Feature and compatibility | two dedicated governor, four shared feature, and six compatibility clauses | `P7-FEAT-001`-`006`; `P7-COMPAT-001`-`006` |
| Product and state | five shared product/state clauses | `P7-PROD-001`-`005` |
| Conformance and catalog | 13 conformance clauses and five audit clauses | `P7-CONF-001`-`013`; `P7-AUDIT-001`-`005` |

The evidence catalog supplies the exact source-to-test-to-command mapping for each individual row;
broad `just release` support is never used as the sole direct proof.

## Remaining limitations and later owners

The 38 partial source rows and one partial audit row are the complete corrected limitation set:

- Phase 8 only: `PROD-GUAR-002`, `PROD-GUAR-008`, `PROD-GUAR-010`, `DSL-END-004`,
  `DSL-NORM-001`, `SEC-FAIL-001`, `BOUND-MATH-001`, `EXEC-BUILD-002`,
  `EXEC-DEADLINE-001`, `CONF-SEMA-001`, and `CONF-ORDER-001` retain pagination grammar,
  normalization, construction, cycle, deadline, arithmetic, diagnostic, and matrix clauses.
- Phase 9 only: `FEAT-GOV-002` retains the public dynamically introduced prepared-endpoint plan
  proof.
- Phases 8 and 9: `DSL-NAME-002`, `GEN-NAME-001`, `GEN-NAME-002`, `EXEC-CREATE-001`,
  `EXEC-CREATE-002`, `EXEC-ORDER-001`, `EXEC-PREC-001`, `EXEC-CANCEL-001`,
  `EXEC-COUNT-001`, `EXEC-COUNT-002`, `CONF-COMPILE-002`, `CONF-DOWNSTREAM-001`, and
  `CONF-DOWNSTREAM-002` retain pagination/prepared names, creation, ordering, precedence,
  cancellation, counts, downstream workflows, and final extension certification.
- Phases 8 and 10: `DSL-FEAT-001`, `GEN-DOC-002`, `SEC-TEST-001`, `CONF-PARSER-001`, and
  `CONF-SEC-001` retain later syntax, cursor/sentinel, security, and release-final clauses.
- Phases 8 through 10: `API-PRIVATE-001`, `API-PRIVATE-002`, `FEAT-DIAG-001`,
  `FEAT-DOC-001`, `CONF-COMPILE-001`, `CONF-COMPILE-003`, `CONF-REQWEST-001`, and
  `CONF-DOC-001` retain later public/private surfaces, diagnostics, examples, dependency probes,
  and release-wide closure.
- Phase 10: `CONF-TRACE-006` retains release-final actual-evidence completeness across the whole
  specification.

Phase 8 owns pagination execution and one acquisition per page. Phase 9 owns prepared endpoints and
final extension certification. Phase 10 owns positive dangerous raw success and release-wide
closure. No Phase 8 or later product capability was implemented by this correction.

## Final architecture and governance review

- One `ClientCore`, `Request<E>`, head/prepared-head path, executor, operation identity/deadline,
  application client, provider client, and authentication lifecycle remain.
- Hook/body/credential preflight, limiter acquisition, credential finalization, native-only auth,
  response observation, recovery, and completion ordering remain unchanged except that the correct
  configured key bound now fails at protected preparation.
- No governor lock spans an await or user callback; successful reservations are not refunded;
  waiting cancellation consumes no built-in quota.
- No key, body, credential, auth material, native URL, or native request enters automatic
  diagnostics, hooks, completion events, source chains, or generated documentation.
- Tests use deterministic barriers, paused/controlled time, explicit counters, and local protocol
  servers; no timing sleep is used as concurrency or cooldown proof.
- No test was deleted, ignored, retried away, weakened, or reclassified to hide a failure.
- Historical reports and Phase 0-6 evidence are unchanged.
- Phase 8 remains the active planning entry with status `NOT_STARTED`.

`PHASE_7_COMPLETE_WITH_LIMITATIONS`

`PHASE_8_NOT_STARTED`
