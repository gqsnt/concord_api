# Phase 8 Integrated Conformance Report

## Verdict

`PHASE_8_COMPLETE_WITH_LIMITATIONS`

Every Phase 8-owned pagination exit criterion is implemented and directly proved at the immutable
source/test commit. The 29 partial rows retain only exact Phase 9 prepared-endpoint/final-extension
or Phase 10 release-final clauses.

`PHASE_9_NOT_STARTED`

This report does not claim full Concord 0.1 conformance. The normative status remains
`Normative Implementation Candidate`.

## Immutable identities

- Starting HEAD: `10c43969cc86ff27df1fc61b273a1dc860f78b5f`
- Branch: `master`
- Initial tree: clean
- Previous immutable source/test commit: `c5071c0eee2b1292d2762c669ff2a7d99cd6b3c9`
- Previous evidence/report/state commit: `19ed74ba4ba64f772733096ea5ad16c8551e4a9e`
- Previous evidence anchor: `1c1a1da8739daf13dacc43ea259ab8163e8835b3`
- Phase 8 source/test commit: `8101a3ab8f281fe37aa9132327cc6c06f2c88d47`
- Phase 8 evidence/report/state commit: `PENDING_PHASE_8_EVIDENCE_STATE`
- Phase 8 evidence anchor: `PENDING_PHASE_8_ANCHOR`
- Compatible post-anchor metadata descendant: the later report/controller-only commit that records
  the real anchor identity and does not replace that anchor
- Authoritative source validation recorded: `2026-07-19T10:54:35Z`
- Cumulative executable patch SHA-256:
  `6a99cded39cfca77f2ec84b6f5dce19f0f088185448b77560360e7214925d319`

The ignored contract map is `git_diff/phase-8-contract-map.md` and contains exactly 115 unique
requirement IDs. The ignored validation ledger is `git_diff/phase-8-validation.md`; the cumulative
executable patch is `git_diff/phase-8-executable.patch`; and the ignored semantic map contains 117
one-requirement evidence rows, including five catalog/state audits.

## Commit outcome

| Unit | Commit | Durable result |
|---|---|---|
| Source and tests | `8101a3ab8f281fe37aa9132327cc6c06f2c88d47` | Replaces proof-only pagination with the production public/generated contracts, typed request-local page loop, macro DSL/IR/generation, and deterministic focused proof. |
| Evidence/report/state | `PENDING_PHASE_8_EVIDENCE_STATE` | Appends Phase 8 evidence, reconciles the 115 current coverage rows, records validation, completes Phase 8 planning, and advances controller state without executable changes. |
| Evidence anchor | `PENDING_PHASE_8_ANCHOR` | Finalizes the evidence-state identity and audit time only; it changes no executable artifact, evidence ID, or classification. |
| Anchor metadata descendant | later documentation-only commit | Records the real anchor hash in this report and `PROJECT_CONTROL.toml` while retaining `anchor-must-be-an-ancestor`. |

No executable artifact changed after `8101a3ab8f281fe37aa9132327cc6c06f2c88d47`.

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
sources, hooks, page and aggregate completion, built-in debug events, limiter contexts, wrapper
privacy, and public/generated Rustdoc.

Continuation values appear only in controlled native-request/controller assertions proving actual
application. They do not appear in safe URLs, sanitized headers, hooks, events, errors, recursive
sources, limiter contexts, wrapper debugging, or documentation. Request debug remains before
native-only authentication materialization.

## Exact executable change surface

The source commit changes exactly 55 files (6,245 insertions and 1,204 deletions):

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

No manifest, lockfile, dependency, feature, unsafe block, or validation configuration changed.
Generated contract version 9 is the private compatibility change required by the typed pagination
bridge. The one new private state machine is the normative production page loop and replaces the
proof-only model rather than existing beside it.

## Focused validation

Every command ran from `F:\projects\concord_api`, used no external service, exited 0, and tested
`8101a3ab8f281fe37aa9132327cc6c06f2c88d47`:

| Exact command | Result |
|---|---|
| `cargo nextest run -p concord_macros --lib -E 'test(/phase8/)'` | 4/4 parser/resolver units passed. |
| `cargo nextest run -p concord_macros --test ui -E 'test(phase8_pagination_diagnostics_are_source_only_and_focused)'` | 1/1 Trybuild group passed. |
| `cargo nextest run -p concord_macros --test phase8_generated` | 8/8 generated local-server tests passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(/pagination::/)'` | 20/20 core identity/runtime tests passed. |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(/paginated_/)'` | 3/3 real TLS authentication/provider tests passed. |
| `cargo nextest run -p concord_core --no-default-features --test pagination_api` | 2/2 no-default downstream API tests passed. |
| `cargo nextest run -p concord_core --test ui -E 'test(source_only_public_surface_failures_use_trybuild)'` | 1/1 core privacy UI group passed. |
| `cargo nextest run -p concord_core --test public_rustdoc` | 1/1 rendered Rustdoc inspection passed. |
| `cargo nextest run -p concord_macros --test compile -E 'test(generated_public_rustdoc_uses_stable_paths_without_private_integration_names) \| test(renamed_core_dependency_resolves_through_package_metadata) \| test(incompatible_core_contract_fails_during_compilation)'` | 3/3 generated-doc/renamed-core/contract checks passed. |
| `cargo nextest run -p concord_core -E 'test(=runtime::config::tests::defaults_are_exact_positive_finite_values) \| test(=runtime::config::tests::setters_preserve_positive_finite_types_and_zero_duration_caps) \| test(=downstream_runtime_configuration_requires_nonzero_ordinary_limits)'` | 3/3 runtime ceiling tests passed. |
| `cargo nextest run -p concord_core --test generated_contract -E 'test(generated_contract_version_is_nine)'` | 1/1 private contract check passed. |
| `powershell -NoProfile -File git_diff/validate-phase-8-manifest-dependencies.ps1 -SourceCommit 8101a3ab8f281fe37aa9132327cc6c06f2c88d47` | Manifest identity and locked dependency/feature ownership audit passed. |
| `cargo nextest run -p concord_core --lib -E 'test(safe_url_property_auth_values_never_survive_rendering)'` | 1/1 deterministic seeded property test passed. |

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
| `just test` | Nextest default workspace: 500/500 passed. |
| `just test-all` | Nextest CI all features: 504/504 passed. |
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

Phase 8 adds 117 unique evidence rows: 88 direct and 29 partial. The 115 requirement traces are 86
direct and 29 partial. Of those, 112 rows are immutable source/test evidence (84 direct and 28
partial), three are requirement-trace state audits (two direct and one partial), and two additional
direct catalog audits prove exact normative coverage and historical preservation.

The complete evidence catalog contains 1,185 unique evidence IDs. All 1,068 Phase 0-7 rows remain
byte-identical and in their original order. The coverage catalog still contains exactly one row for
each of 504 normative requirement IDs; exactly 115 rows now point to Phase 8 evidence with taxonomy
86 `P8-pagination`, 15 `P8-partial-P9`, and 14 `P8-partial-P10`. Planned `PLAN-*` identifiers remain
separate from actual passing evidence.

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
