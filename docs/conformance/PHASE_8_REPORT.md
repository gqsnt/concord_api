# Phase 8 Integrated Conformance Report

## Verdict

`PHASE_8_COMPLETE_WITH_LIMITATIONS`

Every Phase 8-owned pagination exit criterion, including the corrected resolved-rate-key and
actual-logical-host integration, is implemented and directly proved at the immutable corrected
source/test commit. The 29 partial rows retain only exact Phase 9 prepared-endpoint/final-extension
or Phase 10 release-final clauses.

`PHASE_9_NOT_STARTED`

This report does not claim full Concord 0.1 conformance. The normative status remains
`Normative Implementation Candidate`.

## Immutable identities

- Original Phase 8 starting HEAD: `10c43969cc86ff27df1fc61b273a1dc860f78b5f`
- Correction starting HEAD: `cefedf4659ab2080d150eb0f48bb2b93b3bf07cd`
- Branch: `master`
- Initial tree: clean
- Previous immutable source/test commit: `c5071c0eee2b1292d2762c669ff2a7d99cd6b3c9`
- Previous evidence/report/state commit: `19ed74ba4ba64f772733096ea5ad16c8551e4a9e`
- Previous evidence anchor: `1c1a1da8739daf13dacc43ea259ab8163e8835b3`
- Original Phase 8 source/test commit: `8101a3ab8f281fe37aa9132327cc6c06f2c88d47`
- Phase 8 source/test correction commit and final immutable tested source:
  `515b33ebd345a5eae06a6f2681cbfa4ff403ab9e`
- Corrected Phase 8 evidence/report/state commit: `ccf569a3072344b345d79d2d551ccb0c0ebce57d`
- Corrected Phase 8 evidence anchor: `b86eb2e7516dcae4b26ac5d53f444351ebe15654`
- Compatible post-anchor metadata descendant: the final documentation-only commit with subject
  `docs: record corrected phase 8 anchor identity`; it records the real anchor identity and does
  not replace it
- Authoritative corrected-source validation recorded: `2026-07-19T13:05:36Z`
- Focused correction patch SHA-256:
  `5089677710ce487050ea8781a43440d05c09fa6be70192a0bbd837b6fa0bc886`
- Corrected cumulative executable patch SHA-256:
  `eac2687443d863b53b0bc7c64d4cbc44fe3aaf58706e068b438e518905ad3ead`

The ignored contract map is `git_diff/phase-8-contract-map.md` and contains exactly 122 unique
requirement IDs: the original 115 Phase 8 traces plus seven preserved rate-limit authorities owned
by this correction. The ignored correction ledger is `git_diff/phase-8-correction-validation.md`;
the focused and cumulative patches are `git_diff/phase-8-rate-key-correction.patch` and
`git_diff/phase-8-corrected-cumulative-source.patch`. The corrected semantic map contains 124
one-requirement evidence rows, including five catalog/state audits, and the independent claim-atom
map contains 191 atoms: 162 proved and 29 precisely unresolved.

## Commit outcome

| Unit | Commit | Durable result |
|---|---|---|
| Original source and tests | `8101a3ab8f281fe37aa9132327cc6c06f2c88d47` | Replaced proof-only pagination with the production public/generated contracts, typed request-local page loop, macro DSL/IR/generation, and deterministic focused proof. |
| Source/test correction | `515b33ebd345a5eae06a6f2681cbfa4ff403ab9e` | Removes the pagination rate-key placeholder, supplies the actual final logical host privately to rate limiting, and adds exact generated/core/governor/key-boundary/redaction proof. |
| Corrected evidence/report/state | `ccf569a3072344b345d79d2d551ccb0c0ebce57d` | Rebuilds all Phase 8 evidence at claim-atom level, reconciles 122 current coverage rows, records corrected validation, and advances controller state without executable changes. |
| Corrected evidence anchor | `b86eb2e7516dcae4b26ac5d53f444351ebe15654` | Finalizes the corrected evidence-state identity and audit time only; it changes no executable artifact, evidence ID, or classification. |
| Anchor metadata descendant | `docs: record corrected phase 8 anchor identity` | Records the real corrected anchor hash in this report and controller state while retaining `anchor-must-be-an-ancestor`. |

No executable artifact changed after `515b33ebd345a5eae06a6f2681cbfa4ff403ab9e`.

## Implemented product and public surface

The ordinary prelude now exports the exact finite page limit:

```rust
pub enum PageLimit {
    Take(u32),
    HardCap(NonZeroU32),
}
```

`From<u32>` produces `Take`. The stable advanced namespace exports `PaginationError`, the retained
`PaginationErrorKind`, `PaginationRequest<'_>`, `PaginationController<Page>`, `HasNextCursor<C>`,
`OffsetPagination`, `PagePagination`, and `CursorPagination<C>` with the approved constructors,
accessors, bounds, and private fields. `PaginationError` has safe `Display`, `Debug`, and recursive
error behavior; custom controllers can construct exact typed safe errors.

`PaginationRequest` exposes only set/remove query and set/remove header. Host labels, paths, bodies,
credentials, authentication plans, native URLs/requests, transports, and generated endpoint
storage are inaccessible. Controllers remain synchronous, typed, deterministic, and do not run
under a core lock. Built-in offset/page arithmetic is checked; cursor absence remains distinct from
an empty present cursor.

The existing runtime setters remain exact and positive:

```text
pagination_max_pages(NonZeroU32)
pagination_max_items(NonZeroUsize)
pagination_identity_limit(NonZeroUsize)
```

Defaults remain 1,000 pages, 100,000 items, and 4 MiB of canonical identity bytes. Configured small
positive ceilings are honored at the exact first-excess boundary and no ordinary API disables them.

## DSL and generated API

The normalized compiler accepts all approved declarations:

```text
paginate OffsetPagination { offset = ep.offset, page_size = ep.limit }
paginate PagePagination { page = ep.page, page_size = ep.per_page }
paginate CursorPagination<String> { cursor = ep.cursor }
paginate custom crate::pagination::vendor_cursor(ep.cursor, scope.region);
```

Built-in assignments must be complete, direct `ep.FIELD` bindings of the exact compatible type.
Missing blocks/assignments, duplicates, additions, indirect expressions, undefined fields,
incompatible types, and trailing commas fail with reviewed diagnostics. Custom paths and arguments
use the existing restricted public-expression contract. Pagination is final after `rate_limit`,
bodyless, buffered/decoded, and incompatible with stream, bytes, or no-content responses.

Eligible requests conditionally expose:

```rust
pub fn paginate<L>(self, limit: L) -> PaginatedRequest<E>
where
    L: Into<PageLimit>;
```

The opaque wrapper is directly awaitable as `Result<Vec<<Page as IntoIterator>::Item>, ApiError>`.
It has no `.collect()`, no item-extraction trait, no item `Clone` bound, and no loop-protection
opt-out. Nonpaginated, body-bearing, and incompatible response endpoints have no method. Direct
await and `.response()` behavior remain unchanged. Generated contract version 9 is synchronized
between core and macros, and mismatch diagnostics pass.

Nested scopes, explicit endpoints, aliases, collision cases, repeated/nested expansions, and a
renamed `concord_core` dependency compile or fail deterministically without exposing generated
private bridges in public signatures or rendered Rustdoc.

## Corrected rate-limit integration

Generated rate-limit plan construction now formats every approved `vars.NAME`, `scope.NAME`, and
`ep.NAME` dimension from its actual resolved value, including pagination-bound endpoint fields.
The former `"<pagination>"` substitution is removed. Across two generated pages the selected
limiter observes the exact progressed endpoint values in declaration order; the first and second
page values differ exactly according to controller progression.

`HeadBuilder::rate_limit_host()` and the crate-private prepared-head integration now derive the
limiter host from the actual final logical URL. `RateLimitRequest::host` therefore matches each
page's actual logical host, while `RateLimitRequest::safe_url`, hooks, debug events, completion
events, errors, and Rustdoc continue to use sanitized data. No public API signature or generated
contract changed; generated contract version 9 remains exact.

The selected custom limiter is an intentional recipient of the actual resolved `RateLimitPlan`,
its ordered key parts, and the actual logical host. This access is distinct from Concord automatic
observability. Custom limiters remain responsible for their own handling of these values, while
Concord-provided governor diagnostics remain key-safe.

Prepared-page validation measures actual formatted key bytes. A value exactly at the configured
boundary succeeds, the first byte above it fails before hooks, credential/provider work, limiter
acquisition, debug events, authentication materialization, or send, and the rejected value does not
enter the safe error chain. The built-in governor revalidates the same actual bytes before state
insertion. Deterministic paused-time proof shows page-varying actual keys form distinct buckets and
equivalent actual keys continue to aggregate under the Phase 7 contract.

## One request-local pagination architecture

The production path is:

```text
Request<E>
→ opaque PaginatedRequest<E>
→ sealed typed generated pagination specification
→ request-local pagination future
→ cloned bodyless endpoint snapshot for one page
→ ordinary preparation and executor
→ decoded Page / typed controller transition / bounded Vec<Item>
→ exactly one aggregate completion
```

The former `#[cfg(test)]` reduced model was removed. The production state machine reuses the one
`Request<E>`, public-head preparation, native request construction, application Reqwest client,
transport, response handler/decoder, authentication, limiter, deadline, operation-ID allocator,
and completion path. It does not create an independent page executor or request representation.

On first poll the wrapper allocates one aggregate identity and timer. `Take(0)` returns an empty
vector, calls no controller factory, performs no page/planning/protected work, and still awaits one
zero-count aggregate success. A nonzero limit above the configured page ceiling returns
`PageCeilingExceeded` before controller construction or page effects.

For each nonzero page, the loop clones the original endpoint, applies typed built-in bindings or the
safe custom patch, prepares the existing public request head, creates and retains the canonical
identity, allocates a child operation identity, resolves a fresh page deadline, and invokes the
ordinary executor. Repetition and identity-byte excess fail before hooks, providers, limiter,
debug, native authentication, or send. Page completion remains awaited outside the page deadline.

## Limits, natural stop, and controller authority

`Take(n)` sends at most `n` pages; its final allowed page consumes items and never calls `next`.
`HardCap(n)` succeeds when page `n` terminates naturally and returns `HardCapExceeded` only when a
next controller still exists; page `n + 1` is never sent.

Successful page processing retains the candidate `next` result, consumes items in iterator order,
checks the first excess item before append, and applies precedence in this order:

1. empty page stops every controller and ignores a speculative `next` error;
2. sealed built-in offset/page-number short pages stop and ignore that error;
3. only then is a candidate error propagated or `None` accepted;
4. hard-cap failure is selected only when a next controller exists at the cap;
5. controller state and page index commit only after accepted page processing.

Cursor and custom short nonempty pages continue when `next` returns `Some`; no short-page inference
is applied. Absent cursor stops, an empty present cursor is a distinct continuation, and every
failure returns no partial item vector.

## Canonical identity and cycle protection

Every operation retains a bounded structural identity containing the generated stable endpoint
discriminator, exact method token, `url`-canonicalized logical URL after pagination patching, and
canonical final public headers before authentication. Header names are lowercase and sorted;
repeated value order and exact non-UTF8 value bytes are preserved. Query and repeated-query order,
default ports, and percent encoding follow `url` behavior.

Authentication query/header state, credentials, body data, and native URL state are excluded.
Digest matches are confirmed through full structural equality. Exact byte accounting includes only
the normative component bytes, and checked arithmetic prevents page, item, identity, offset, or
page-number wrapping. Immediate, two-state, and longer cycles fail deterministically; different
endpoint discriminators or methods remain distinct.

Identity/controller/item state is collection-local, shared with no client clone or other request,
and drops at completion or cancellation.

## Page lifecycle and completion

Deterministic gates and local protocol servers prove one active page per collection and concurrent
independent collections. Each page receives a distinct child `OperationId` under the aggregate
parent, a zero-based page index, and a fresh deadline. Authentication recovery retains that same
page identity/index/deadline, performs the existing second limiter acquisition, and emits no second
page completion. Each Concord page attempt acquires the limiter once.

Hidden Reqwest retries do not repeat pagination preparation or Concord callbacks. Provider HTTP
enters no pagination lifecycle. Response-head hooks and limiter observation precede body polling.
Later pages wait for the ordinary page completion hook. Cancellation drops local state and prevents
later pages without rolling back completed external effects.

Each collection awaits exactly one aggregate completion after fixing success or failure and elapsed
time. It reports only the aggregate ID, endpoint/method, completed pages, accepted items, aggregate
recovery flag, fixation elapsed time, last completed status, and safe outcome. It runs outside all
page deadlines, cannot alter the fixed result, may delay delivery, and cannot request another page.
Ordinary page errors retain their exact categories and safe status; only controller/cycle/cap and
aggregate-ceiling/arithmetic failures use `ErrorCategory::Pagination`.

## Security and continuation secrecy

High-entropy sentinel tests cover cursor, custom controller state, repeated logical URL, pagination
query/header values, collected item, credential, authentication header/query, request/response
body, rate key, and native authenticated URL. Recursive inspection covers `Display`, `Debug`, error
sources, hooks, page and aggregate completion, built-in debug events and governor errors, wrapper
privacy, and public/generated Rustdoc.

Continuation values may intentionally appear in controlled native-request/controller fixtures and,
when declared as a resolved rate-limit dimension or final logical host, in the selected custom
limiter's exact plan/host inputs. They remain absent from safe URLs, sanitized headers, hooks,
automatic debug/events/completion, Concord errors and recursive sources, built-in governor errors,
wrapper debugging, and documentation. The tests audit intentional limiter fields separately from
automatic safe fields rather than concatenating both and claiming global absence. Request debug
remains before native-only authentication materialization.

## Exact executable change surface

The original Phase 8 source commit changes exactly 55 files (6,245 insertions and 1,204 deletions):

```text
concord_core/src/{__private.rs,advanced.rs,client.rs,lib.rs,prelude.rs}
concord_core/src/error/transport.rs
concord_core/src/execute/{mod.rs,auth_tests.rs}
concord_core/src/generated_contract.rs
concord_core/src/hooks/debug.rs
concord_core/src/http/{application.rs,development.rs}
concord_core/src/pagination/{controller.rs,error.rs,execute.rs,identity.rs,limits.rs,mod.rs,pending.rs,request.rs,runtime_tests.rs,tests.rs}
concord_core/src/pagination/model.rs (removed)
concord_core/src/redaction/headers.rs
concord_core/src/request/{head.rs,mod.rs,prepare.rs}
concord_core/src/response/{handler.rs,mod.rs}
concord_core/tests/{generated_contract.rs,pagination_api.rs,public_rustdoc.rs,ui.rs}
concord_core/tests/ui/fail/{pagination_private_state.rs,pagination_private_state.stderr}
concord_macros/src/{ast.rs,codegen.rs,model.rs,parse.rs,resolve.rs}
concord_macros/tests/{compile.rs,phase8_generated.rs,ui.rs}
concord_macros/tests/fixtures/renamed_core/src/lib.rs
concord_macros/tests/ui/fail/phase8_pagination_{bounds,method_absence,modes,syntax,types}.{rs,stderr}
concord_test_support/src/gate.rs
```

The correction commit changes exactly these seven source/test paths:

```text
concord_macros/src/codegen.rs
concord_macros/tests/phase8_generated.rs
concord_core/src/request/head.rs
concord_core/src/execute/mod.rs
concord_core/src/pagination/runtime_tests.rs
concord_core/src/rate_limit/governor.rs
concord_core/tests/pagination_rate_limit.rs
```

No manifest, lockfile, dependency, feature, public signature, unsafe block, or validation
configuration changed.
Generated contract version 9 is the private compatibility change required by the typed pagination
bridge. The one new private state machine is the normative production page loop and replaces the
proof-only model rather than existing beside it.

## Focused validation

Every command ran from `F:\projects\concord_api`, used no external service, exited 0, and tested
`515b33ebd345a5eae06a6f2681cbfa4ff403ab9e`:

| Exact command | Result |
|---|---|
| `cargo nextest run -p concord_macros --lib -E 'test(/phase8/)'` | 4/4 parser/resolver units passed. |
| `cargo nextest run -p concord_macros --test ui -E 'test(phase8_pagination_diagnostics_are_source_only_and_focused)'` | 1/1 Trybuild group passed. |
| `cargo nextest run -p concord_macros --test phase8_generated` | 8/8 generated local-server tests passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(/pagination::/)'` | 22/22 core identity/runtime tests passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(/rate_limit::/)'` | 27/27 core rate-limit tests passed. |
| `cargo nextest run -p concord_core --test pagination_rate_limit` | 1/1 generated actual-key boundary test passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(/paginated_/)'` | 3/3 real TLS authentication/provider tests passed. |
| `cargo nextest run -p concord_core --no-default-features --test pagination_api` | 2/2 no-default downstream API tests passed. |
| `cargo nextest run -p concord_core --test ui -E 'test(source_only_public_surface_failures_use_trybuild)'` | 1/1 core privacy UI group passed. |
| `cargo nextest run -p concord_core --test public_rustdoc` | 1/1 rendered Rustdoc inspection passed. |
| `cargo nextest run -p concord_macros --test compile -E 'test(generated_public_rustdoc_uses_stable_paths_without_private_integration_names) \| test(renamed_core_dependency_resolves_through_package_metadata) \| test(incompatible_core_contract_fails_during_compilation)'` | 3/3 generated-doc/renamed-core/contract checks passed. |
| `cargo nextest run -p concord_core -E 'test(=runtime::config::tests::defaults_are_exact_positive_finite_values) \| test(=runtime::config::tests::setters_preserve_positive_finite_types_and_zero_duration_caps) \| test(=downstream_runtime_configuration_requires_nonzero_ordinary_limits)'` | 3/3 runtime ceiling tests passed. |
| `cargo nextest run -p concord_core --test generated_contract -E 'test(generated_contract_version_is_nine)'` | 1/1 private contract check passed. |
| `powershell -NoProfile -File git_diff/validate-phase-8-manifest-dependencies.ps1 -SourceCommit 515b33ebd345a5eae06a6f2681cbfa4ff403ab9e` | Manifest identity and locked dependency/feature ownership audit passed. |
| `cargo nextest run -p concord_core --lib -E 'test(safe_url_property_auth_values_never_survive_rendering)'` | 1/1 deterministic seeded property test passed. |
| `cargo nextest run -p concord_macros --test phase8_generated -E 'test(=generated_pagination_preserves_resolved_rate_keys_and_actual_hosts)'` | 1/1 passed: two pages exposed exact ordered actual endpoint/host limiter values with no placeholder. |
| `cargo nextest run -p concord_core --test pagination_rate_limit -E 'test(=generated_pagination_key_limit_measures_actual_value_not_placeholder_text)'` | 1/1 passed: actual boundary accepted, first excess rejected before hook/send, and error chain redacted. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(=pagination::runtime_tests::selected_limiter_receives_actual_page_hosts_while_automatic_surfaces_stay_redacted) \| test(=pagination::runtime_tests::pagination_rate_key_limit_uses_actual_value_and_cuts_off_protected_work) \| test(=request::head::tests::pagination_host_is_actual_for_rate_limiting_and_redacted_for_safe_surfaces)'` | 3/3 actual-host, protected-cutoff, and automatic-redaction proofs passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(=rate_limit::governor::tests::page_varying_actual_keys_are_distinct_and_equivalent_keys_aggregate) \| test(=rate_limit::governor::tests::configured_key_limit_revalidates_before_governor_state_insertion)'` | 2/2 deterministic governor bucket and insertion-boundary proofs passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(=pagination::runtime_tests::page_completion_delay_is_outside_deadline_and_later_page_gets_a_fresh_deadline)'` | 1/1 exact page-local deadline proof passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(=pagination::tests::identity_accounting_counts_only_canonical_components_and_collisions_use_full_equality)'` | 1/1 exact canonical-accounting and deliberate full-equality collision proof passed. |
| `cargo nextest run -p concord_macros --test phase8_generated -E 'test(=generated_offset_pagination_is_directly_awaitable_and_stops_on_short_page)'` | 1/1 positive bodyless generated pagination proof passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(=execute::auth_tests::configured_key_limit_failure_cuts_off_every_protected_side_effect_and_redacts_key) \| test(=execute::tests::configured_key_limit_enforces_lower_zero_and_increased_boundaries) \| test(=execute::tests::default_key_limit_accepts_1024_bytes_and_rejects_the_first_excess)'` | 3/3 Phase 7 direct-request key-limit regressions passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(=execute::tests::custom_limiter_context_and_response_observation_use_one_ordered_attempt) \| test(=execute::tests::hidden_reqwest_retry_does_not_reenter_custom_limiter_or_lifecycle) \| test(=pagination::runtime_tests::hidden_reqwest_retry_does_not_repeat_pagination_preparation_or_page_lifecycle)'` | 3/3 custom limiter and hidden-retry containment proofs passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(=execute::auth_tests::paginated_tls_recovery_keeps_one_page_identity_index_and_completion)'` | 1/1 real TLS paginated recovery proof passed. |
| `cargo nextest run -p concord_core --no-default-features --test rate_limit_api` | 4/4 no-default selected-limiter API proofs passed. |
| `cargo nextest run -p concord_macros --test phase7_generated` | 7/7 direct rate-limit plan regression tests passed. |

## Complete maintained validation

All 16 maintained commands exited 0 against the same immutable source commit:

| Exact command | Configuration and result |
|---|---|
| `just fmt-check` | Workspace formatting check passed. |
| `just check` | Workspace/all targets/default features passed. |
| `just check-all` | Workspace/all targets/all features passed. |
| `just check-no-default` | Core/all targets/no defaults passed. |
| `just clippy` | Workspace/default features with warnings denied passed. |
| `just clippy-all` | Workspace/all features with warnings denied passed. |
| `just test` | Nextest default workspace: 505/505 passed. |
| `just test-all` | Nextest CI all features: 509/509 passed. |
| `just test-no-default` | Maintained core no-default filter: 1/1 passed. |
| `just doctest` | Maintained default Cargo doctests: 2/2 passed. |
| `just doctest-all` | Maintained all-feature Cargo doctests: 2/2 passed. |
| `just doc` | Three warning-denied default Rustdoc builds passed. |
| `just doc-all` | Three warning-denied all-feature Rustdoc builds passed. |
| `just deny` | Advisories/bans/licenses/sources passed; only the two established allowed duplicate-version warnings were reported. |
| `just diff-check` | Tracked diff integrity passed. |
| `just release` | Complete maintained umbrella matrix, including dangerous-feature isolation, exited 0. |

No applicable check was unavailable and no weaker substitute was used.

## Evidence and trace reconciliation

Corrected Phase 8 contains 124 unique evidence rows: 95 direct and 29 partial. The 122 requirement
traces are 93 direct and 29 partial. Of those, 119 rows are immutable source/test evidence (91
direct and 28 partial), three are requirement-trace state audits (two direct and one partial), and
two additional direct catalog audits prove exact normative coverage and historical preservation.

The complete evidence catalog contains 1,192 unique evidence IDs. All 1,068 Phase 0-7 rows remain
byte-identical and in their original order with SHA-256
`8e8691ffca76c3a3941264649cbaf479b4ef82740caaaaf1ec0d3de389e5a66f`. The coverage catalog still
contains exactly one row for each of 504 normative requirement IDs; exactly 122 rows now point to
corrected Phase 8 evidence with taxonomy 93 `P8-pagination-correction`, 15 `P8-partial-P9`, and 14
`P8-partial-P10`. Planned `PLAN-*` identifiers remain separate from actual passing evidence.

The ignored claim-atom review represents every assertion in every Phase 8 row as 162 proved atoms
and represents each of the 29 partial rows with one additional unresolved atom naming its exact
Phase 9 or Phase 10 owner. Every proved atom names a source/test path and a command present in the
immutable-source correction ledger; direct rows contain no unresolved Phase 8 atom.

The evidence anchor runs the ignored semantic audit against the real evidence-state identity. It
checks normative/coverage equality, evidence-ID and approved-label uniqueness, exact paths and
passing commands, source/evidence ancestry, classification/owner agreement, counts, report/program/
controller reconciliation, and the unchanged historical evidence prefix.

## Remaining limitations and later owners

Phase 9 owns exactly these 15 partial requirements:

```text
DSL-NAME-002
GEN-NAME-001 GEN-NAME-002
EXEC-CREATE-001 EXEC-CREATE-002 EXEC-ORDER-001
EXEC-NATIVE-001 EXEC-NATIVE-002 EXEC-PREC-001 EXEC-CANCEL-001
EXEC-COUNT-001 EXEC-COUNT-002
CONF-COMPILE-002 CONF-DOWNSTREAM-001 CONF-DOWNSTREAM-002
```

They retain prepared-endpoint generated names, creation, ordering, native-attempt, precedence,
cancellation/count behavior, downstream workflows, and final extension certification.

Phase 10 owns exactly these 14 partial requirements:

```text
DSL-FEAT-001
API-PRIVATE-001 API-PRIVATE-002
GEN-DOC-002 SEC-TEST-001 FEAT-DIAG-001 FEAT-DOC-001
CONF-PARSER-001 CONF-COMPILE-001 CONF-COMPILE-003
CONF-REQWEST-001 CONF-SEC-001 CONF-DOC-001 CONF-TRACE-006
```

They retain release-final feature/namespace/documentation/diagnostic/security/dependency/evidence
closure and positive dangerous raw-success ownership. No failed Phase 8 criterion is classified as
a limitation.

## Final architecture and engineering review

- One request, head/prepared-head, executor, application HTTP client, provider HTTP client,
  transport, response decoder, authentication path, limiter path, operation-ID allocator, deadline
  mechanism, and generated runtime remain.
- No lock spans an await or controller/user callback; controller methods are invoked outside core
  locks; pages are sequential and aggregate completion is awaited rather than detached.
- Finite ceilings, checked arithmetic, earliest-error/natural-stop precedence, completion fixation,
  cancellation, and redaction are explicit and tested.
- No public type erasure, `Any` item/controller output, registry, generated field mutation, dynamic
  prepared-pagination API, raw-success API, item limit, partial result, prefetch, stream, callback,
  second terminal, or loop-protection opt-out was added.
- No test was deleted, ignored, retried away, weakened, or reclassified to hide a failure.
- Phase 0-7 source behavior and evidence remain preserved.
- No Phase 9 or Phase 10 capability was implemented.

## Next phase

Phase 9 — Prepared Endpoints and Extension Certification: `NOT_STARTED`.
