# Provenance and observable authoring record

This is **example-assisted** SDK21 author work at frozen Concord commit
`56a16ee34f8aa714bd51dca67f744e5870e813b6`. The only owned source directory is
`examples/open-library-sdk/`; root changes add its workspace member and Cargo's lock entry.
No shared crate, check, script or other example was edited. No commit was attempted.
One Codex author session performed this work; no independent consumer/reviewer run is claimed.
No private reasoning transcript is recorded here.

**Live provider calls: NOT_RUN.** All endpoint execution uses the shared
`examples/testserver` loopback harness, through `TestServer::start(|request: &Recorded| -> Reply)`.
Only read-only documentation URLs were requested. Linked `.json` examples on those pages
were not opened. Documentation retrieval date is **2026-11-07 (Europe/Paris)**; the local
session crossed no date boundary during this work (UTC diagnostic: 2026-11-06T23:28:10Z).

## Provider documentation

| Exact page | Read date | Facts used |
|---|---|---|
| https://openlibrary.org/dev/docs/api/search | 2026-11-07 | `/search.json`, `q`, one-based page/limit, work search by default; sample bare and `/works/` keys; both count spellings; common fields; explicit schema-stability limitation. |
| https://openlibrary.org/dev/docs/api/authors | 2026-11-07 | `/search/authors.json?q=…`, sample author key and fields; canonical author keys; `/authors/{id}/works.json`, default 50, optional limit/offset; remove human slug before JSON addressing. |
| https://openlibrary.org/dev/docs/api/books (Works API section) | 2026-11-07 | `/works/{id}.json`, remove human title/slug; work versus edition distinction. No legacy Books API behavior was implemented. |

A read of https://openlibrary.org/dev/docs/api was attempted; the web tool reported it
inaccessible. It supplies no contract evidence. The two required endpoint pages and the Books
Works API section were available. None supplies an exhaustive error-code/envelope contract;
the HTTP and body-failure cases below are authored robustness fixtures, not observed responses.

## Fixture derivation

Every file below is **authored synthetic**, not a captured provider response. Endpoint and field
vocabulary comes from the pages above. Titles, names, descriptions, counts, unknown fields and
response combinations are invented. Example OLIDs `OL27448W`, `OL45804W` and `OL23919A` are
reused as identifiers only; the fixtures assert no real metadata association with those IDs.
No expectation or fixture was generated from SDK requests, serialization, model code or output.
Wire expectations were written as literal targets from the documented routes and inputs.

| Fixture | Documentation basis and synthetic variation | Purpose |
|---|---|---|
| `work-search.json` | Search response envelope/common fields; canonical key, invented page-2 metadata and extra field. | Search → selected work, camel count. |
| `work-search-bare.json` | Search's earlier example uses bare keys and `num_found`; fields set null/absent. | Bare-key normalization, snake count, sparse hit. |
| `empty-work-search.json` | Search envelope with invented empty later page and nonzero reported total. | Empty page succeeds; no automatic follow-up. |
| `author-search.json` | Authors search envelope and named fields; invented author metadata. | Search → author works. |
| `empty-author-search.json` | Authors envelope, invented zero matches. | Empty author selection remains safe. |
| `author-works.json` | Authors work-collection route; synthetic `entries`/`size` projection with two sparse work records and a hostile unused link. The required docs do not specify a full collection schema. | Batch retrieval; null fields; no link following. |
| `work-minimal.json` | Books Works route; invented record containing only its canonical key. | Required missing-optional-fields failure case. |
| `work-rich.json` | Books Works route; synthetic description object/subjects/unknown field, without asserting their schema is guaranteed. | Small work projection; variable description JSON. |
| `error.json` | No documented error envelope; invented nested opaque JSON with a secret sentinel. | HTTP status handling and redaction. |
| `malformed.json` | Intentionally invalid JSON with a secret sentinel. | Syntax failure and safe error formatting. |
| `truncated.json` | Intentionally unfinished JSON array, delivered as a complete HTTP body. | JSON truncation distinct from HTTP transfer loss. |
| `wrong-shape.json` | Invented `docs` string where this SDK requires an array. | Shape failure distinct from an empty page. |
| `missing-envelope.json` | Invented response lacking `docs`. | Do not hide malformed collection envelopes. |
| `wrong-media.html` | Authored HTML with a secret sentinel, unrelated to any provider response. | Strict JSON media rejection/error-document failure. |

Inline fixtures in `tests/protocol.rs` are also **authored synthetic**: keyless hit envelopes;
empty/`null` documents; a valid empty page padded to the exact success cap; `{}` padded to the
exact error cap; over-cap whitespace bodies (fixed length and chunked); a complete JSON prefix
with a missing HTTP chunk terminator; stalled bodies; missing/repeated/malformed/non-UTF-8 media
headers; hang-up before a head; HTTP 301/400/401/403/404/408/422/429/500/503 with invented
Set-Cookie, Location and Retry-After headers. These test the package's failure semantics and do
not claim those statuses, headers or body shapes are guaranteed by Open Library.

## Files and examples consulted

Required workspace guidance read: `README.md`, `docs/QUICKSTART.md`,
`docs/AUTHOR_GUIDE.md`, `docs/CHEATSHEET.md`, `docs/RECIPES.md`; root `Cargo.toml` and the
lockfile/diff were inspected. There is no AGENTS.md or CLAUDE.md inside this worktree or its
`examples`/`crates` directories. Ancestor handoff `AGENTS.md` and `CLAUDE.md` were read, plus
their `START_HERE.md`, `DESIGN_CHARTER.md`, `PROMPT.md`, `AUTHORITY.md`,
`contracts/PHASES.md` and `contracts/RESOURCES.md`. The user's bounded capsule supersedes the
broader campaign work: no integration, other SDK work or shared edits were attempted.

Rustdoc was read in source for the Concord facade (`crates/concord/src/lib.rs`), core
(`crates/concord-core/src/{lib,prepared,response,error,value}.rs`), codec facade/JSON
(`crates/concord-codecs/src/{lib,json}.rs`), compatibility facade
(`crates/concord-extras/src/lib.rs`) and macro crate (`crates/concord-macros/src/lib.rs`).
The relevant public contracts were generated `api!` usage, sealed input conversions,
`Endpoint`/`SendWith`, bounded receive/decode/document, response metadata and safe errors.
`crates/concord/Cargo.toml` and `crates/concord-codecs/Cargo.toml` were read for feature selection.
Generated rustdoc for `OpenLibrarySearchWorks::prepare` was also inspected after `cargo doc`;
it documents I/O-free construction and build errors, without naming the send-policy bypass.

Dependency rustdoc was read from the cached Cargo registry at
`/home/canarit/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` (no external documentation
fetch): `serde-1.0.229/src/lib.rs` and `src/core/de/mod.rs` (derive/Deserialize),
`serde_json-1.0.154/src/lib.rs` and `src/value/mod.rs` (typed/untyped JSON and Value),
`tokio-1.54.1/src/lib.rs` (runtime/features), and `reqwest-0.13.5/src/lib.rs` plus
`src/async_impl/client.rs` (reuse, redirect/retry/proxy/timeout configuration). The cached
serde/serde_json/tokio Cargo manifests were inspected for license metadata. No registry file
was modified and no documentation example was executed.

The complete example-file consultation ledger follows. “Excerpt only” means the listed file
was not opened directly; its embedded code in a required guide was read. Merely linked files
whose contents were not displayed are not counted as consulted.

| Example file | Access | What was taken |
|---|---|---|
| `examples/testserver/src/lib.rs` | Direct source/rustdoc | `Recorded` request inspection, `Reply`/`Body` construction, Script, loopback lifecycle, explicit shutdown assertions. |
| `examples/testserver/Cargo.toml` | Direct | Package name/path and dev-dependency integration. |
| `examples/quickstart/src/lib.rs` | README/Quickstart excerpts only | `api!`, generated navigation/setters/send, base replacement and any-status bounded document handling. |
| `examples/quickstart/src/main.rs` | Quickstart excerpt only | Local stand-in execution concept; no main/binary copied. |
| `examples/shapes/tests/wire.rs` | Quickstart excerpts only | Transport remains caller-owned; its subdomain/DNS setup is unnecessary for this SDK. |
| `examples/native/src/native_edit.rs` | Recipes excerpt only | Native exit obligations considered; no request editing was needed or copied. |
| `examples/compat/src/lib.rs` | Recipes excerpt only | Multi-segment resource handling considered; canonical OLIDs instead normalize into one generated segment. |
| `examples/openai-sdk/src/lib.rs` | Author Guide multipart excerpts only | Multipart remains out of slice; no implementation adopted. |
| `examples/google-drive-sdk/src/upload.rs` | Author Guide excerpt only | Runtime multipart media remains out of slice; no implementation adopted. |
| `examples/frankfurter-sdk/src/lib.rs` | Author Guide send-policy excerpt only | HTTP status remains authoritative when a bounded error document fails; reuse that generic control flow with opaque JSON. |

The Author Guide also embeds `checks/phase-a/src/lib.rs`, `checks/phase-a/tests/received.rs`,
`checks/phase-a/tests/send_policy.rs` and `crates/concord/tests/forms.rs` snippets; they were read
only as guide excerpts. They informed send-policy and response-boundary understanding; none
of those files was opened or changed.

## Dependencies and execution

No new third-party crate was added. All resolved dependencies already existed in Cargo.lock.
The new package is version 0.0.0 with the workspace MIT OR Apache-2.0 license. Direct resolved
dependencies: Concord 4.2.0 (local), serde 1.0.229 (MIT OR Apache-2.0), serde_json 1.0.154
(MIT OR Apache-2.0). Dev dependencies: local concord-example-testserver 0.0.0 and tokio 1.54.1
(MIT). The facade's `json` feature selects the shared concord-codecs implementation;
`reqwest-defaults` supplies the existing TLS-capable transport. No retry/pagination library or
extra representation feature was added for explicit one-page reads.

The first command was `CARGO_BUILD_JOBS=4 cargo build --offline -p concord-sdk-open-library`
without `--locked`, so Cargo could add this package. It passed in 15.23s. The only lockfile
change was the new package entry; all preexisting package records stayed identical.
All subsequent compilation/check/documentation commands used `--locked`; Cargo formatting
does not resolve dependencies. No CARGO_TARGET_DIR was set; output is the worktree's `target/`.

Resource observations before building: `df -h .` reported 366 GiB available on
`/dev/nvme0n1p2`; target did not yet exist. `free -h` reported 30 GiB RAM, 24 GiB available,
8 GiB swap (4.9 GiB used); `nproc` reported 20. `rustc --version` reported
`rustc 1.99.1 (58cf8f9c3 2026-10-13)`. Jobs were capped at four, with one cargo command at a
time. After the first tests, target was 877 MiB and free disk 363 GiB. No cleanup was needed.
This is a focused package gate, not a full campaign or cross-platform gate.

Validation results and final counts are recorded in [VALIDATION.md](VALIDATION.md).

## Independent maintenance addendum — 2026-11-07

The historical author record above is preserved. Maintenance started at
`ba6ac11b0e990a701b806e15f5d1ab5b0d8a155e`; see [MAINTENANCE.md](MAINTENANCE.md) for
the new session's exposure, edit ledger and observed checks. No dependency was added or changed;
the existing workspace member and lock entry were already present. The initial unlocked offline
build left Cargo.lock byte-for-byte unchanged. **Live provider calls: NOT_RUN**; research fetched
documentation/schema/source pages only, never search/record endpoints or data dumps.

All sources below were read on **2026-11-07 (Europe/Paris)**:

| Exact page | Maintenance use |
|---|---|
| https://openlibrary.org/dev/docs/api/search | Four explicitly named sort values (`new`, `old`, `random`, `key`), default relevance, lexicographic key ordering, page/limit boundary. |
| https://openlibrary.org/dev/docs/api/authors | Author search and works routes; full author lookup exists at the provider but is outside the existing SDK slice. |
| https://openlibrary.org/dev/docs/api/books | Works API section: selected work retrieval by canonical ID with `.json`. |
| https://openlibrary.org/developers/dumps | Format of JSON records section links official work/author schemata and their shared text-block definition; no dump downloaded. |
| https://openlibrary.org/type/work | Work description has `/type/text`. |
| https://openlibrary.org/type/author | Author bio has `/type/text`; inspected for applicability only, since the SDK has no bio field. |
| https://github.com/internetarchive/openlibrary-client/tree/master/olclient/schemata | Official schema directory linked by the dumps page. |
| https://raw.githubusercontent.com/internetarchive/openlibrary-client/master/olclient/schemata/work.schema.json | `description` references `text_block`. |
| https://raw.githubusercontent.com/internetarchive/openlibrary-client/master/olclient/schemata/author.schema.json | `bio` references `text_block`; no author lookup was added. |
| https://raw.githubusercontent.com/internetarchive/openlibrary-client/master/olclient/schemata/shared_definitions.json | Text object uses `/type/text` and string `value`. The schema documents the object form only; plain-string compatibility comes from the ticket and existing synthetic fixture. |
| https://openlibrary.org/dev/docs/restful_api | Read-only content and collection examples corroborate JSON resource addressing; no write examples executed. |
| https://raw.githubusercontent.com/internetarchive/openlibrary/master/openlibrary/plugins/worksearch/schemes/works.py | Corroborates new/old publication-year semantics and contains additional server sort facets not included in this bounded SDK. |

`https://openlibrary.org/dev/docs/api/works` was attempted but inaccessible; the Books API's
Works section supplied route evidence instead. Two web searches for text handling returned no
usable primary-source evidence; unrelated results were not adopted or opened. No page above is
treated as an exhaustive stable schema guarantee.

`tests/maintenance.rs` adds authored synthetic inline fixtures, not provider captures: raw and
text-object descriptions, Unicode/whitespace/markup, empty/missing/null text, unrecognized
shapes, a selectable work search, and an author work batch. Literal sort wire expectations
come from the search page, not from SDK output. Unknown-shape retention is an SDK compatibility
choice, not a provider guarantee. Existing fixtures and all existing assertions remain intact.
