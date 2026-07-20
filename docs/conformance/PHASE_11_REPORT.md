# Phase 11 Conformance Report

## Verdict

`PHASE_11_COMPLETE_WITH_LIMITATIONS`

Phase 11 — Uniform Typed Pagination State is complete. The limitations are only the explicitly
unimplemented work assigned to Phases 12, 13, and 14. The current normative package remains
`Normative Implementation Candidate`; only Phase 14 may restore `Normative Release Final`.

Phase 12 — Generic Body/Response Specifications and Integrated Streaming Records is now active with
status `NOT_STARTED`. No Phase 12, Phase 13, or Phase 14 product behavior was implemented.

## Corrected closure

This focused correction started from clean `master` at
`462faa033b5252c729e17f1c28fbd156b169376c`. It supersedes the earlier Phase 11 source
`1e5d3c9e4567cde54a1b27acae5d48bd7b5d8607`, evidence state
`22868a9de9bd55daa975dbcd484f085033a52217`, and anchor
`bec42412043d24b8bab01bd7049b430abfb8a46a` as correction history without rewriting or deleting
those commits.

The correction addressed two evidence defects. `P11-CONF-001` had cited only downstream
compilation for a compile-and-runtime requirement; no public-only downstream generated pagination
operation had been awaited. `P11-DSL-002`, `P11-API-002`, and `P11-SEC-002` also bound positive or
multi-surface claims to commands that did not independently execute every claimed atom. All 22
Phase 11 rows were therefore re-audited against their active normative clauses instead of carrying
forward the earlier counts by assumption.

The corrected standalone fixture is implemented by:

- `concord_macros/tests/fixtures/extension_certification/src/lib.rs`;
- `concord_macros/tests/fixtures/extension_certification/tests/pagination_runtime.rs`;
- `concord_macros/tests/fixtures/extension_certification/Cargo.toml`;
- `concord_macros/tests/compile.rs`; and
- the unpublished isolated Cargo/Nextest helper in `concord_test_support/src/cargo_fixture.rs`.

It imports Concord product APIs only through `concord_core::prelude` and
`concord_core::advanced`. An ephemeral loopback proxy scripts two ordinary successful pages. Direct
`.paginate(10_u32).await` proves initial `cursor=initial`, controller transition to
`cursor=continuation`, generated binding into page two, ordered items `[1, 2, 3]`, exactly one
construction initializer evaluation, two validations, and two consuming `next` calls. The
downstream controller required no `concord_macros` source change and no private, development,
endpoint-specification, execution-plan, response-handler, ClientCore, Reqwest, or native import.

## Identities and Git boundary

- Starting HEAD: `6f7c3b69ec7f86a3eef4a9b70d94cf1ce449e52a`
- Starting branch: `master`
- Starting tree: clean
- Focused correction starting HEAD: `462faa033b5252c729e17f1c28fbd156b169376c`
- Focused correction starting branch/tree: clean `master`
- Phase 10 immutable source/test baseline: `9317a94cf020d81dbd216ed2a7dc999550586898`
- Phase 10 evidence/report/state: `5b157225b387387664a202d4cbfe5f85ba8c5289`
- Phase 10 evidence anchor: `e0648e36ea9450741724ac067cfe184500e724c6`
- Phase 10 compatible metadata descendant: `6f7c3b69ec7f86a3eef4a9b70d94cf1ce449e52a`
- Final immutable Phase 11 source/test commit:
  `1ee2ce99800af671e173679efeb573cb06b0c713`
- Corrected Phase 11 evidence/report/state commit: `EVIDENCE_COMMIT_PENDING`
- Corrected Phase 11 evidence anchor: `ANCHOR_COMMIT_PENDING`
- Corrected anchor-metadata descendant: the final two-file commit titled
  `docs: record corrected phase 11 anchor`; its own identity is intentionally not self-embedded.
- Head policy: `anchor-must-be-an-ancestor`

No branch was created. No remote Git, fetch, pull, push, merge, rebase, amend, tag, publication,
deployment, credential access, or history rewrite occurred.

## Authority and roadmap commits

The authority amendment was established before production implementation. Focused documentation
corrections preserved that separation:

| Commit | Subject and outcome |
|---|---|
| `9416d8516506b7413d8da8d6f5a5f017a0d4198d` | `docs: establish uniform pagination contract` — normative contract, requirement-change record, ADR-044, architecture, candidate status, and Phases 11–14 roadmap. |
| `c27f8f52325b6f58ce8a581ed12fa493f9de0921` | `docs: normalize active coverage states` — distinguishes active planned coverage from superseded historical IDs. |
| `6273c454340d3675acdd7ad3444cd87c673e4583` | `docs: correct historical evidence blob audit` — makes the prospective audit byte-check the historical catalog owner. |
| `808afc209b81a1400492162f94dfbc9442f48835` | `docs: follow finalized Phase 10 anchor bytes` — uses the finalized Phase 10 historical baseline without changing it. |
| `e4ef2a546486fa1c7654fdd58f61a8643ad850e7` | `docs: align lineage audit wording` — aligns exact requirement-change record wording and checks. |
| `5df5b0913e05acf16bccbe431bedb7ae4669e20f` | `docs: correct uniform pagination examples` — corrects authority examples before source implementation. |

ADR-044, “Pagination uses one typed state struct and inline bindings,” supersedes ADR-028. ADR-009,
ADR-010, ADR-011, ADR-012, ADR-027, and ADR-029 remain in force except for the pagination wording
narrowly superseded by ADR-044. The core and macro architecture documents now describe one typed
controller path and reject public binding protocols, endpoint-generic factories, controller request
patches, controller categories, and separate runtime factory/binder/short-page callbacks.

## Source and correction commits

| Commit | Outcome |
|---|---|
| `539faa7816fbb20ad7ed4a73d58bde5df2aa59ff` | Implements the uniform DSL, controller trait, built-ins, generated associated-controller integration, core loop, contract 12, examples, UI/fixture/runtime/public/Rustdoc tests, and removes `PaginationRequest`. |
| `fcdf3c0b10229b18b9ae99d2b68e25672c2d341f` | Replaces a pre-existing generated-timeout wall-clock oracle with a deterministic request barrier and shared body gate after the all-feature matrix exposed a 550/551 failure. |
| `1e5d3c9e4567cde54a1b27acae5d48bd7b5d8607` | Updates the maintained release artifact audit from the removed `paginate custom` marker to the uniform application-controller struct-and-`bind` example. |
| `f73cd8fb5f4bba290f85d1d00b4303b753a9c166` | Adds the stable-public downstream controller, deterministic two-page loopback runtime, exact import audit, and locked/offline isolated Nextest harness without changing product or macro implementation source. |
| `a197858b32f374b021ad852fcdcf4e109d7f459a` | Corrects Source-mode registry auditing for a non-rewritten correction source: the immutable 1,531-row Phase 0-10 prefix remains byte-checked while exactly 22 inherited Phase 11 rows are validated as historical inputs rather than new-source proof. |
| `1ee2ce99800af671e173679efeb573cb06b0c713` | Isolates nested fixture Nextest from an outer `NEXTEST_PROFILE=ci` after the first maintained all-feature attempt exposed profile inheritance before downstream test selection. |

The final source commit includes every executable source, test, fixture, example, generated contract,
and executable validation-script correction. No executable artifact changed after authoritative
validation restarted against `1ee2ce99800af671e173679efeb573cb06b0c713`.

## Requirement lineage

The tracked owner is
`docs/normative/09_CONCORD_REQUIREMENT_CHANGE_RECORD_0.2.md`. Historical evidence remains valid for
its recorded source commit and is not reinterpreted as current Phase 11 proof.

Retained with explicit version change:

- `DSL-PAGE-002`
- `API-PAGE-002`
- `SEC-PAGE-001`
- `SEC-PAGE-002`
- `EXEC-PAGE-001`
- `EXEC-PAGE-005`
- `EXEC-PAGE-007`

Exact supersessions:

| Historical ID | Current replacement |
|---|---|
| `DSL-PAGE-004` | `DSL-PAGE-007` |
| `DSL-PAGE-005` | `DSL-PAGE-008` |
| `DSL-PAGE-006` | `DSL-PAGE-009`, `DSL-PAGE-010` |
| `API-PAGE-004` | `API-PAGE-007` |
| `API-PAGE-005` | `API-PAGE-008` |
| `API-PAGE-006` | `API-PAGE-009` |
| `EXEC-PAGE-008` | `EXEC-PAGE-015` |
| `EXEC-PAGE-014` | `EXEC-PAGE-016` |

New Phase 11 IDs are exactly:

- `DSL-PAGE-007`, `DSL-PAGE-008`, `DSL-PAGE-009`, `DSL-PAGE-010`
- `API-PAGE-007`, `API-PAGE-008`, `API-PAGE-009`, `API-PAGE-010`
- `GEN-PAGE-004`
- `EXEC-PAGE-015`, `EXEC-PAGE-016`, `EXEC-PAGE-017`
- `SEC-PAGE-003`
- `COMPAT-MACRO-004`
- `CONF-PAGE-002`

Current coverage contains 511 active requirements and eight separately marked superseded historical
IDs. Exactly the 22 Phase 11-owned active requirements above are `implemented` and have direct
passing Phase 11 evidence.

## Public pagination contract

The stable public contract is exactly:

```rust
pub trait PaginationController<Page>: Sized + Send + 'static {
    fn validate(&self) -> Result<(), PaginationError> {
        Ok(())
    }

    fn next(
        self,
        page: &Page,
    ) -> Result<Option<Self>, PaginationError>;

    fn short_page_size(&self) -> Option<NonZeroUsize> {
        None
    }
}
```

Controllers require neither `Clone` nor `Sync`. State is moved between pages and remains local to
one `Send` pagination future. Methods are synchronous and receive neither request bodies,
credentials, authentication plans, native requests/URLs, Reqwest values, transports, nor shared
client mutation capabilities. `PaginationRequest`, its public query/header patch methods, and its
ordinary-controller execution role are absent.

`OffsetPagination`, `PagePagination`, and `CursorPagination<C>` implement this same trait. Their
state fields are available to generated complete struct construction under ordinary Rust visibility.
Offset and page progression use checked addition and return their exact requested page size from
`short_page_size`; cursor pagination returns `None`. `HasNextCursor<C>` remains only a convenience
trait used by the cursor controller and is not recognized by the macro.

## DSL, generated integration, and compatibility

The parser accepts one arbitrary `syn::Type` followed by one mandatory struct-shaped block. Each
field has either a restricted stable initializer or direct `bind ep.FIELD` / `bind scope.FIELD`.
Trailing commas, qualified/imported/generic/application types, bindings-only declarations,
mixed stable/bound fields, nested scopes, repeated expansions, and renamed core dependencies compile.
Duplicate fields/destinations, old assignment/factory syntax, missing blocks, shorthand/rest, calls,
methods, closures, async/arbitrary blocks, formatting, protected sources, indirect binding, and
non-request-local destinations fail with focused diagnostics. Ordinary Rust checking owns missing,
unknown, inaccessible, wrong-typed, non-controller, and non-`Send` cases.

Generated-private integration uses one sealed `PaginatedEndpoint` capability with associated
`Item` and `Controller` types, complete initial controller construction, binding into a fresh
endpoint clone, and one call into the core-owned loop. It is semantically equivalent to:

```rust
trait PaginatedEndpoint: EndpointSpec + Clone {
    type Item: Send + 'static;
    type Controller:
        PaginationController<<Self::Response as ResponseHandler>::Output>;

    fn initial_controller(&self) -> Result<Self::Controller, PaginationError>;

    fn bind_controller(
        endpoint: &mut Self,
        controller: &Self::Controller,
    ) -> Result<(), PaginationError>;
}
```

The exact trait stays generated-private and sealed. Public Rustdoc exposes no factory, binder, patch,
callback hierarchy, binding protocol, endpoint-generic controller, or generated integration trait.
There is no runtime `Any`, controller registry, or erased controller. Generated pagination remains
directly awaitable and conditionally available only for paginated endpoints.

Phase 11 performs the synchronized generated contract `11 -> 12`. Core and macro constants are 12;
the incompatible-core fixture proves a focused mismatch diagnostic. Phase 10 history correctly
remains at generated contract 11.

## Core loop and state ownership

Core owns the only pagination loop. For nonzero work it validates the page ceiling, constructs the
controller once, then for each page validates current state, clones the original endpoint, writes
bound controller fields into that fresh clone, performs ordinary preparation and canonical cycle
preflight, and executes the ordinary page pipeline.

After decode, except on the final allowed soft page, core snapshots `short_page_size`, consumes the
current controller through `next`, retains its result, accepts items in order with pre-append ceiling
checks, applies controller-selected count termination before a retained `next` error, stops on
`None`, applies the hard cap, and only then commits the next controller. `Take(0)` constructs and
calls nothing. A nonzero page-ceiling failure also constructs nothing. The final soft page does not
call `next`.

There is no universal empty-page stop. Count termination is exactly `Some(n)` with decoded item
count less than `n`; `Some(1)` supplies empty-page termination and `None` supplies no count stop.
Tests prove short count termination suppresses a retained `next` error, while a non-short page
propagates it; hard cap succeeds on natural stop and fails without requesting an excess page when a
next controller remains.

The original endpoint owns stable configuration. Bound endpoint/scope values are copied into the
controller exactly once. Controller state then owns every evolution and is written before each page
into one ephemeral clone; no clone is copied back. Unbound fields initialize once and are never
rebound. Original client, facade, endpoint, and scope snapshots remain unchanged.

## Uniformity, shaping, and security proof

One built-in controller and one application controller traverse the same AST, normalized IR,
generated binding, public trait, and core loop. The application controller required no controller-
specific macro source change. Source/runtime proof rejects semantic branches for
`OffsetPagination`, `PagePagination`, `CursorPagination`, and custom pagination, as well as separate
runtime factory, binder, and short-page callbacks.

A controller that is `Send`, not `Clone`, and not `Sync` paginates successfully. Exact counters prove
one construction, validation before every attempted page, one initial bound-value transfer, one
fresh endpoint clone and binding per page, and move rather than clone semantics.

Across multiple pages, current bound state shapes dynamic host, path, ordered query, public header,
the existing generated rate-limit key, and full canonical identity consistently. Protected bound
continuations remain absent from safe errors, recursive sources, `Debug`, hooks, debug events, page
completion, aggregate completion, and generated Rustdoc. Only `bind` slots are classified as
continuation state; stable unbound controller configuration is not. Phase 13 lexical rate-limit
behavior was not introduced.

Cursor pagination continues after an empty decoded item page when metadata supplies a next cursor.
An application controller with `short_page_size() == None` likewise continues after an empty page
when `next` returns `Some`. Offset, page-number, and application controllers can independently select
the same count policy without core or macro privilege.

## Preserved lifecycle guarantees

Focused and full-matrix tests reprove one active page at a time, all-or-error results, page/item/
identity ceilings, full canonical cycle equality, deliberate digest-collision handling, page-local
deadlines, child and aggregate operation IDs, authentication recovery retaining page identity/state,
limiter reacquisition, cancellation preventing later pages, page and aggregate completion, and
ordinary `IntoIterator` item extraction. Prepared pagination, `.collect()`, async streams, prefetch,
parallel pages, partial success, and item-trait requirements remain absent.

All independently validated Phase 0–10 guarantees not expressly changed by the approved pagination
contract remain covered by the complete maintained matrix.

## Validation

Observed tools were rustc/cargo 1.85.0, LLVM 19.1.7, cargo-nextest 0.9.140, just 1.46.0,
cargo-deny 0.20.2, PowerShell 7.6.3, and Git 2.45.1.windows.1. All commands ran from
`F:\projects\concord_api` at the immutable source commit.

Two pre-final attempts are correction diagnostics, not final evidence. Source audit at
`f73cd8fb5f4bba290f85d1d00b4303b753a9c166` exposed the inherited-evidence chronology defect and
triggered `a197858b32f374b021ad852fcdcf4e109d7f459a`. The first `just test-all` at that source ran
551/552 tests before the outer `ci` profile caused nested Nextest to reject an unavailable fixture
profile. After `1ee2ce99800af671e173679efeb573cb06b0c713` removed that inherited test-process setting,
every focused command and all 16 maintained commands were restarted and passed against that exact
final source.

Focused results:

| Exact command | Result |
|---|---|
| `cargo nextest run -p concord_macros --lib -E 'test(/pagination/)' --no-tests fail` | 4/4 passed; 26 skipped |
| `cargo nextest run -p concord_macros --test ui -E 'test(=pagination_diagnostics_are_source_only_and_focused)' --no-tests fail` | 1/1 passed; 22 skipped |
| `cargo nextest run -p concord_macros --test generated_pagination` | 10/10 passed |
| `cargo nextest run -p concord_macros --test compile` | 11/11 passed, including downstream runtime and import audits |
| `$env:CARGO_TARGET_DIR='F:\projects\concord_api\target\cargo-fixtures\phase11-corrected-runtime-final'; cargo nextest run --offline --locked --color never --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-extension-certification --test pagination_runtime -E 'test(=downstream_generated_controller_constructs_binds_and_collects_two_pages)' --no-tests fail --success-output final` | 1/1 passed; sentinel `pages=2 items=3 construction=1 validation=2 next=2` |
| `$env:NEXTEST_PROFILE='ci'; cargo nextest run -p concord_macros --test compile -E 'test(=downstream_pagination_runtime_executes_generated_construction_and_binding)' --no-tests fail --success-output final` | 1/1 passed; 10 skipped; nested runtime selected under outer CI profile |
| `cargo nextest run -p concord_macros --test compile -E 'test(=complete_extension_fixture_has_no_private_native_or_integration_escape)' --no-tests fail` | 1/1 passed; 10 skipped |
| `cargo nextest run -p concord_macros --test generated_pagination -E 'test(=application_controller_selects_the_same_count_termination_without_macro_privilege)' --no-tests fail` | 1/1 passed; 9 skipped |
| `cargo nextest run -p concord_macros --test generated_pagination -E 'test(=cursor_and_custom_empty_pages_follow_controller_authority_and_redact_continuations)' --no-tests fail` | 1/1 passed; 9 skipped |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(/pagination/) \| test(/paginated/)' --no-tests fail` | 27/27 passed; 313 skipped |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(=pagination::runtime_tests::page_lifecycle_is_sequential_correlated_and_limited_once_per_page) \| test(=pagination::runtime_tests::controller_construction_and_validation_failures_are_page_side_effect_free) \| test(=pagination::runtime_tests::aggregate_completion_observes_a_fixed_result_and_excludes_its_hook_delay)' --no-tests fail` | 3/3 passed; 337 skipped |
| `cargo nextest run -p concord_core --all-features --lib -E 'test(=pagination::runtime_tests::pagination_rate_key_limit_uses_actual_value_and_cuts_off_protected_work) \| test(=hooks::tests::pagination_completion_debug_has_no_continuation_or_private_state)' --no-tests fail` | 2/2 passed; 338 skipped |
| `cargo nextest run -p concord_core --all-features --test generated_contract --test pagination_api --test pagination_rate_limit --test public_rustdoc --test ui` | 8/8 passed |
| `cargo nextest run -p concord_core --all-features --test public_rustdoc -E 'test(=stable_request_terminals_are_semantic_and_private_in_rendered_rustdoc)' --no-tests fail` | 1/1 passed |
| `cargo nextest run -p concord_core --no-default-features --test pagination_api` | 2/2 passed |
| `cargo nextest run -p concord_macros --test grammar_registry` | 1/1 passed |
| `pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode SelfTest` | 15 classes; 797 claims parsed |
| `pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode Source -SourceCommit 1ee2ce99800af671e173679efeb573cb06b0c713` | 511 active, 8 superseded, 1,531 historical rows, 22 inherited prior Phase 11 rows not assessed as correction proof; passed |

Complete maintained matrix:

| Command | Result |
|---|---|
| `just fmt-check` | passed |
| `just check` | passed |
| `just check-all` | passed |
| `just check-no-default` | passed |
| `just clippy` | passed with warnings denied |
| `just clippy-all` | passed with warnings denied |
| `just test` | 538/538 passed |
| `just test-all` | 552/552 passed, including the nested downstream runtime under CI profile |
| `just test-no-default` | 1/1 passed; 240 skipped |
| `just doctest` | 2/2 passed |
| `just doctest-all` | 4/4 passed |
| `just doc` | four libraries documented with warnings denied |
| `just doc-all` | four libraries documented with warnings denied |
| `just deny` | advisories, bans, licenses, and sources passed; accepted duplicate-version warnings only |
| `just diff-check` | passed |
| `just release` | passed in 151.1 seconds; all maintained gates and 9/9 benchmark categories |

The release wrapper also passed Rust 1.85.0 MSRV, dangerous-feature, feature-matrix, examples,
benchmarks, grammar/diagnostics, Reqwest assumptions, security/properties, Rustdoc, release naming,
release artifacts, and implementation-candidate registry audits. Validation used deterministic
barriers, controlled local protocol servers, finite waits only for diagnostic barriers, no arbitrary
sleep oracle, no external service, and no automatic retry.

## Evidence and historical preservation

Phase 11 contributes exactly 22 direct passing rows, one per Phase 11-owned requirement, zero partial
rows, zero waivers, and zero missing paths. The catalog therefore contains 1,553 evidence rows:
1,531 historical Phase 0–10 rows followed by 22 Phase 11 rows.

Those totals were recomputed from an independent 22-row claim-atom audit. In particular:

- corrected `P11-CONF-001` cites the direct standalone runtime, exact public/private/native import
  audit, outer CI-profile runtime harness, full compile matrix, and the five principal fixture,
  harness, and code-generation paths;
- corrected `P11-DSL-002` combines ordinary Rust field diagnostics with the generated application
  controller and downstream once-only construction probe;
- corrected `P11-API-002` separately binds negative field diagnostics, successful accessible
  construction, readable controller fields, and runtime page-two binding to commands that execute
  those claims; and
- corrected `P11-SEC-002` names exact selectors for safe errors, recursive sources, structural
  `Debug`, before-send and response-head hooks, request/response debug, page and aggregate
  completion, generated Rustdoc, and cursor/controller/logical-URL/query/header/item sentinels.

Every Phase 11 path exists at `1ee2ce99800af671e173679efeb573cb06b0c713`; every recorded exact
command appears in the corrected passing validation registry. No Phase 11-owned atom remains
partial or unresolved. The only limitations remain the explicitly unimplemented Phases 12–14.

The corrected validation registry contains 71 unique rows overall and 38 Phase 11 rows, all tied to
the corrected source with passing exit status zero. The corrected evidence, coverage, and validation
catalog SHA-256 values are respectively
`90d31f417690082af865850d6e1ce25331bca16c76f37c9338382b71df85a91f`,
`19d6c6d47003837674da042378e7f88166d54649c399300151a2c328eb958dba`, and
`35aed3c4e0f2fd2e29779aeaea421be3f6b93f1495b33470dec904a550fd961b`.

- Phase 10 report SHA-256 remains
  `36e144ba7fb866029e70efbffc89837a8e8e9021b453076e388e8417e7b029af`.
- The exact 1,531-row Phase 0–10 evidence owner is 1,081,762 bytes with SHA-256
  `aadd64241a2dc42bb19dd01643915724df1a55390debbe52f7d7becbdc71e091`.
- Every Phase 0–10 evidence byte remains unchanged; Phase 11 evidence is append-only.
- Superseded IDs remain resolvable through the requirement-change record and retain their exact
  historical evidence/source identities.
- The release-final requirement/claim closure remains a Phase 14 concern and is not falsely
  reclassified as Phase 11 completion.

## Limitations and next state

Only these authorized future phases remain:

1. Phase 12 — Generic Body/Response Specifications and Integrated Streaming Records — active,
   `NOT_STARTED`.
2. Phase 13 — Lexically Scoped Rate Limits — `NOT_STARTED`.
3. Phase 14 — Product Completion, Conformance Closure, and Release Finalization — `NOT_STARTED`;
   owns whole-product release-evidence closure and any restoration of `Normative Release Final`.

No Phase 12 body/response/record behavior, Phase 13 lexical rate-limit behavior, Phase 14 release
promotion, or Phase 15 work is present.
