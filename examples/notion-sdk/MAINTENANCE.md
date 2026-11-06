# SDK18 Notion maintenance: unknown variant and second block page

Date: 2026-11-06. Starting SDK HEAD: `e76187ff`; frozen Concord library:
`47a25fbf`. This session performed maintenance, not SDK authorship. Requested
runner: Codex gpt-6-astra (xhigh); the session tools did not independently expose
the active model/effort setting. No agent delegation was used.

Ticket: add a new unknown block/property type and a second page of children;
preserve supported types, surface unknowns, and prevent concurrent version-header
leakage. The existing generic implementation already supported the perturbation.
The maintenance change adds fresh fixture evidence and clarifies that contract;
it does not introduce a new typed variant or require caller changes.

## First attempt and results

1. Inspected the SDK's README, declaration, client adapter, models, renderer,
   error types, fixtures, and all three existing
   integration test files. The initial worktree was clean. Baseline verification
   passed: 27 integration tests and 5 doctests.
2. Added hand-authored `future_progress` property and `future_panel` block
   fixtures, distinct from the earlier `future_metric`/`future_callout` cases.
   The new children list has three items on page one and five on page two,
   covering all seven supported block types as well as the new unknown.
3. Added four wire tests using the existing public SDK. The first targeted run
   compiled and passed all four without intervention. No runtime repair was
   needed. No generated operation name was guessed or handwritten; there was
   no generated-name or compile surprise.
4. Added source rustdoc and README clarification, fixture provenance, and this
   record. Final full verification passed: 31 integration tests (including all
   6 unchanged first-use tests) and 5 doctests, zero failures. Clippy with warnings
   denied and the package formatting check also passed.

The main new test gates both initial children responses together with two page
retrievals, then gates both second children responses. Both version contexts
therefore have outstanding requests at each boundary; no sleep or assumed task
ordering establishes overlap. A lazy traversal is created before its originating
client is consumed by `with_version`, and still sends its captured version on
both pages. Each observed request has exactly one expected version and bearer
header. A different synthetic response version remains metadata only.

The tests compare entire unknown JSON objects, common identity/has_children
fields, every supported variant and its rendered text, encoded property IDs,
and retained links. An unknown parent can still be expanded. The cursor includes
whitespace, a literal percent escape, plus, reserved characters and Unicode;
both its raw request-target encoding and decoded value are checked. A page cap
retains it for resume; a final page at the cap exhausts successfully. A second-page
429 retains the collected prefix, typed code, actual status and Retry-After. A
malformed supported to_do after the new unknown fails decoding and emits none
of that second page. All requests were loopback-only with synthetic auth.

## Edits by kind

| Kind | Edits |
| --- | --- |
| Declaration | None. `src/api.rs` already declares the version header and optional GET start_cursor correctly. |
| Adapter | Documentation only in `src/client.rs`: clones and lazy traversals retain their captured request context; response headers do not change it. The owned client, cursor validation/projection and typed error path are unchanged. |
| Model | Documentation only in `src/models.rs`: arbitrary new discriminators require no registration, common fields still decode, and unknown blocks remain ordered expandable items. Existing deserializers and renderer are unchanged. |
| Tests and fixtures | New `tests/maintenance.rs` with four wire tests; new `maintenance-page.json`, `maintenance-children-1.json`, and `maintenance-children-2.json`. Earlier fixtures/tests are unchanged. Existing `.gitattributes` already covers the new files; `git check-attr` confirms text normalization is unset for all three. |
| Docs | README documents retention and traversal context plus the new evidence. PROVENANCE maps every new fixture to official shapes and identifies invented data. This file records the maintenance result. |

## Independent descriptions and locality

**Zero independently maintained runtime descriptions required semantic changes.**
There are already two relevant unknown fallback arms (property and block) in the
model and two corresponding generic marker arms in the renderer. None enumerates
the invented names, so none needed editing. The existing declaration and adapter
also needed no new description of continuation or version ownership.

The new examples do have intentionally independent evidence: each invented type
is named in its response fixture, the test's expected variant/marker, and the
provenance text (three authored artifacts). The cursor has three literal forms:
fixture JSON, a decoded test expectation, and a wire-encoded test expectation.
The README and source comments explain the existing generic behavior. These
are test oracles/documentation, not multiple runtime implementations that had
to be kept in sync for one provider semantic change.

The change felt local: all nine changed/added files are in this SDK, with no new
dependencies, no library patches, no endpoint changes, and no caller migrations.
The substantive work was stronger combined wire coverage because unknown
retention, pagination and client ownership were already implemented separately.

## Reading and friction

Besides this SDK's source/tests/fixtures and public docs, consulted the repository
README, relevant response/pagination sections of `docs/RECIPES.md` and
`docs/CALLER_GUIDE.md`, `crates/concord-pagination/README.md`,
`crates/concord-extras/README.md`, generated Concord rustdoc for Prepared,
Received and ResponseMeta, and the testserver's Recorded/Reply/Gate interfaces.
Parent AGENTS guidance and its required START_HERE/PROMPT/AUTHORITY files were
read for scope; the user ticket's frozen-library and held-out restrictions were
preserved. The cloud runtime skill was read; its environment-status tool and
executor policy file were unavailable, and no credential inspection was needed.

Official Notion block, property, pagination, introduction and versioning references
were rechecked; exact links and fixture authorship are in [PROVENANCE.md](PROVENANCE.md).
Fixtures are hand-authored synthetic examples, not provider responses or a claim
of live conformance. No other provider SDK supplied this maintenance solution.
Neither this SDK's FRICTION/FIRST_USE records nor other SDK role records or
`v4/sdk20/` campaign records were read, before or after the first attempt.

No Concord friction entry was necessary: no private API reach-through, native
escape, rebuilt execution, handwritten operation identity, or library workaround
was introduced. All runtime behavior under test uses the existing public
Concord declaration/receive boundary and concord-pagination traversal.

## Commands

All cargo invocations used the required shared cache and five-job prefix:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo doc -p concord --no-deps --features csv,jsonl
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-notion --test maintenance
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-notion
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo clippy --locked -p concord-sdk-notion --all-targets -- -D warnings
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo fmt -p concord-sdk-notion -- --check
```

All commands passed. The new test file was formatted with the pinned rustfmt;
`git diff --check` also passed. The frozen library, other examples and existing
first-use workflows were left unchanged.
