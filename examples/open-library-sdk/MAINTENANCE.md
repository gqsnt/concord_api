# SDK21 independent maintenance

Date: **2026-11-07, Europe/Paris**. Starting commit:
`ba6ac11b0e990a701b806e15f5d1ab5b0d8a155e`. Changes are uncommitted.
**Live provider calls: NOT_RUN.** All workflow requests used loopback fixtures through
`TestServer::start(|request: &Recorded| -> Reply)`; only documentation and source were fetched.

## Prior exposure and observable sequence

This session did not write the original SDK or its first consumer. Assistance label:
**source-assisted maintenance**, one agent, no delegated author/reviewer or independent review
claimed. Before inspecting the workspace, exposure consisted of the supplied maintenance
capsule and environment instructions.

Before the first implementation/test attempt, the following were read or inspected:

- Ancestor handoff `AGENTS.md`, `START_HERE.md`, `DESIGN_CHARTER.md`, `PROMPT.md`, `AUTHORITY.md`,
  `contracts/PHASES.md`, and `contracts/RESOURCES.md`. Their broad campaign instructions were
  bounded by this capsule's ownership and frozen-baseline rules. An initial read of root
  `RESOURCES.md` found no file; the referenced `contracts/RESOURCES.md` was then read.
- Root and SDK `Cargo.toml`, the existing lock entry, Git status/commit, available disk/RAM,
  target size and the absence of `CARGO_TARGET_DIR`. No nested `AGENTS.md` was found.
- SDK `README.md`, `PROVENANCE.md`, all four `src/*.rs` files, `tests/first_use.rs`,
  `tests/workflows.rs`, `tests/protocol.rs`, `tests/support/mod.rs`, and
  `fixtures/author-works.json`. Existing tests exposed raw-JSON compatibility obligations.
- Shared `crates/concord-core/src/value.rs` and matching lines in
  `crates/concord-macros/src/emit.rs` were inspected read-only for typed inputs and `Display`
  query formatting. No other provider SDK implementation or generated SDK source was used.
- The official endpoint pages, dumps/type/schema pages and REST documentation listed with
  exact URLs in [PROVENANCE.md](PROVENANCE.md#independent-maintenance-addendum--2026-11-07).

The initial offline unlocked build passed. Then `WorkSort`, the optional query declaration,
`description_text()` and four new tests were implemented. The focused maintenance test run
compiled and passed all four tests on its first run. **Only after that attempt** were
`FRICTION.md` and `FIRST_USE.md` opened, together with the historical `VALIDATION.md`.
The later read of the provider's work-search implementation corroborated sort meanings.
Historical records were left intact; new evidence is identified as maintenance evidence.

## Changed behavior and compatibility

The baseline already decoded both description shapes into `Option<serde_json::Value>`;
no baseline decoding crash was reproduced for either shape. The missing behavior was a
uniform plain-text caller interface. `Work::description_text() -> Option<&str>` now borrows
either the string or a `/type/text` object's string `value`. Missing/null yields `None`,
whereas either empty-string representation yields `Some("")`. It preserves Unicode,
whitespace and markup; it does not render markup or stringify arbitrary JSON.

Keeping `Work.description` unchanged preserves callers and **every existing assertion**,
including tests that inspect the raw object and raw string differently. Unknown shapes remain
raw and yield no text; callers needing to distinguish those from absent data can inspect the
raw field. This is a compatibility choice, not a claim about undocumented provider shapes.
The same method serves work lookup and `AuthorWorks.entries` without another decoder.
The slice has no individual-author endpoint/model and reads no `bio`, so the ticket's
conditional author-lookup change does not apply. Full author records remain excluded.

`search().works(q).set_sort(WorkSort::New)` is generated from one optional query declaration.
`New`, `Old`, `Random`, and `Key` encode the four values explicitly named on the
[Search API page](https://openlibrary.org/dev/docs/api/search), read **2026-11-07**.
Omission and `maybe_sort(None)` leave relevance ordering to the provider. No pagination or
sort option was added to another endpoint. The larger set of facets in linked server source
is outside this bounded enum.

The [Books API Works section](https://openlibrary.org/dev/docs/api/books#works-api) documents
work retrieval, and the [data dumps format page](https://openlibrary.org/developers/dumps#format-of-json-records)
links the [work schema](https://raw.githubusercontent.com/internetarchive/openlibrary-client/master/olclient/schemata/work.schema.json)
and [text-block definition](https://raw.githubusercontent.com/internetarchive/openlibrary-client/master/olclient/schemata/shared_definitions.json).
These were read **2026-11-07**. The schema supports the object representation; the requested
plain-string compatibility is stated in the ticket and represented by the pre-existing
synthetic author-works fixture. The [Authors API page](https://openlibrary.org/dev/docs/api/authors),
also read **2026-11-07**, grounds the unchanged second workflow. No exhaustive schema guarantee
or live-provider observation is inferred from this evidence.

## Complete edit ledger and repeated facts

All paths below are under `examples/open-library-sdk/`.

| File/location | Edit and reason |
|---|---|
| `src/models.rs`: `WorkSort`, its `Display`, `Work.description` docs, `impl Work` | Typed sort vocabulary/wire mapping; additive plain-text accessor and documented absence behavior. |
| `src/lib.rs`: models reexport, `search::works` declaration/docs | Expose `WorkSort`; add `query sort?: WorkSort` so Concord generates the optional setters/encoding. |
| `tests/maintenance.rs` (new) | Two focused model regressions and two public-API consumer workflows, using independently written literal wire expectations. |
| `README.md`: supported workflows, import/call example, query and description contracts, maintenance link | Make the new public behavior discoverable and document compatibility/limits. |
| `PROVENANCE.md`: maintenance addendum | Exact research URLs/read date, synthetic fixture derivation, unchanged dependencies and research limitations. |
| `MAINTENANCE.md` (new) | This maintenance receipt, costs, validation and candidate Concord changes. |

The sort type name appears in its definition, reexport, endpoint declaration and consumers.
The four wire spellings are repeated in `Display`, independent wire assertions and public
documentation. The text-object discriminator/value and absent-versus-empty distinction appear
in the accessor, fixtures/assertions and docs. Citation/date facts also occur in README,
provenance and this receipt. Tests intentionally use independent literals rather than derive
expected wire values from `Display`. No generated file was hand-edited, and neither endpoint
execution nor author-batch decoding needed its own text conversion.

No compiler error or test failure caught an omitted edit: the first implementation included
the type, export and declaration, and the first new test run passed. No omission/mutation
experiment was run. Compilation checks the accessor/import/setter call sites; runtime assertions
check all four emitted sort strings, option removal, both text representations and absence.
Documentation synchronization still required manual review. This is observed first-pass
success, not evidence that the compiler can detect a semantically wrong wire spelling.

## Verification and scope

All Cargo commands used `CARGO_BUILD_JOBS=4`, one at a time, default worktree `target/`, and
offline resolution. Only the initial build omitted `--locked`; Cargo formatting does not
resolve dependencies. Existing target artifacts were reused; no clean-build claim is made.

| Command (with `CARGO_BUILD_JOBS=4`) | Result |
|---|---|
| `cargo build --offline -p concord-sdk-open-library` | PASS; workspace/lock entries already existed, no lock change. |
| `cargo test --offline --locked -p concord-sdk-open-library --test maintenance` | PASS, first attempt: 4 new tests. |
| `cargo fmt -p concord-sdk-open-library` | PASS; formatted the owned package only. |
| `cargo test --offline --locked -p concord-sdk-open-library` | PASS: 2 unchanged first-use + 4 maintenance + 13 protocol + 3 workflow tests; 1 compiled README doctest; zero failures/ignored tests. |
| `cargo clippy --offline --locked -p concord-sdk-open-library --all-targets -- -D warnings` | PASS, no warnings. |
| `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-open-library --no-deps` | PASS, no warnings. |
| `cargo fmt -p concord-sdk-open-library -- --check` | PASS. |

Both required connected workflows ran locally, including the unchanged empty-page and
missing-optional-field cases. New consumers also check sorted search → selected work and
author search → mixed-text work batch. All old tests, assertions, fixtures, `FRICTION.md`,
`FIRST_USE.md`, and `VALIDATION.md` are unchanged. Git comparison confirmed no changes to
Cargo.toml, Cargo.lock, shared crates/checks/scripts or other examples. No dependency was
added and no package version changed. No commit was attempted.

Before building, available disk was 359 GiB and available RAM 24 GiB (30 GiB total).
The existing target was about 1.3 GiB, growing to about 1.4 GiB after checks; no cleanup was
needed. This is a focused local package gate, not a workspace, cross-platform or live-service gate.

## Remaining responsibilities and candidate Concord improvements

Provider responsibilities remain in the SDK: OLID resource rules, field projections, text
shape interpretation, sort vocabulary, one-based work-search pages versus author-work offsets,
and which response fields may be absent. Work/author search still repeat count aliases and
similar envelopes. That similarity alone does not justify a shared provider schema abstraction.
Application responsibilities remain selection, explicit next-page decisions and retaining
partial results after a later failure.

Generic duplication remains in `ReadPolicy`'s bounded success/error branch and `HttpError`'s
redacted metadata/document wrapper. New tests reuse the existing client helper, while the
independent first consumer retains its own transport and assertion helper. This ticket added
no custom codec, native request rewrite or per-endpoint execution wrapper.

Candidate shared changes, recorded only because shared code is frozen:

- **External capability:** evaluate a bounded JSON response-policy helper and a safely formatted
  `HttpDocumentError<D>` against another provider. Keep caps, success rules and error semantics
  configurable. The existing policy/error source is the local duplication witness; the later
  FRICTION read records the same opportunities. Neither is required to implement this ticket.
- **Macro docs:** generated `prepare()` documentation should identify the selected send policy
  and explain the lower-level bypass. This remains supported by the unchanged protocol test
  and historical first-use/friction records; maintenance did not reinspect generated HTML.
- **Guide/usage:** show a small typed `Display` query enum with `query sort?: WorkSort`, public
  reexport, `set_sort` and `maybe_sort(None)`. The current declaration handles this change with
  one new input; a provider-specific sorting DSL or core text-object type would add no benefit
  demonstrated by this ticket. No core/macro defect was observed.
