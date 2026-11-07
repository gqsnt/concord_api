# Provenance and observable authoring record

Read date for all sources below: **2026-11-07**. Frozen worktree HEAD was verified with
`git rev-parse HEAD`: `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`.
Live provider calls: **NOT_RUN**. Web access was limited to reading the two allowed documentation
pages; no linked API endpoint or sandbox was opened. Executed HTTP requests target loopback
`TestServer::start(|request: &Recorded| -> Reply)` responders only.

## Official documentation

| Exact page read | Read date | What informed the SDK |
|---|---|---|
| https://www.mediawiki.org/wiki/API:Continue/en | 2026-11-07 | Modern continuation object, category-members example, generic and module fields, exhaustion on absence, whole-object merge in the Python example. |
| https://www.mediawiki.org/wiki/API:Query | 2026-11-07 | `action=query`, page IDs, search/category/revision modules, formatversion=2 page arrays and missing/invalid flags, continuation requests and distinction from batch completion. |

No verbatim provider payload is copied. All fixture values and prose are authored synthetic.
In particular, the two permitted pages do not fully specify the search result or revision-slot
schema. The chosen search and `rvprop=ids|content&rvslots=main` shapes are explicit synthetic
contract assumptions, exercised locally; they are not claimed to be verified live responses.
The extra `futureoffset` field deliberately tests forward compatibility, not a documented
category-module parameter. Generic continuation preservation is supported by the cited examples.

## Fixture derivation table

| Fixture | Derivation and purpose |
|---|---|
| `fixtures/search-first.json` | Synthetic search schema, two selectable IDs; numeric offset plus generic continuation, using the Query/Continue envelope pattern. |
| `fixtures/search-last.json` | Synthetic final search page with no continuation. |
| `fixtures/content.json` | Synthetic latest-revision main-slot text; page-array structure follows Query, revision-slot fields are the explicit assumption above. |
| `fixtures/category-first.json` | Synthetic adaptation of Continue example 2: invented IDs/title/token, `batchcomplete` alongside continuation, added numeric future field. |
| `fixtures/category-empty.json` | Synthetic empty intermediate result, new complete token; tests progress despite zero members. |
| `fixtures/category-last.json` | Synthetic terminal category result, including category/file namespaces; no continuation. |
| `fixtures/api-error.json` | Synthetic open-ended code/info application-error envelope; sentinel detail exercises redaction. |
| `fixtures/warnings.json` | Synthetic warning with echoed sentinel; SDK conservatively refuses partial results. |
| `fixtures/missing-query.json` | Synthetic inconsistent envelope, rather than an empty list. |
| `fixtures/missing-page.json` | Synthetic ID/title; missing flag derived from Query example 3, adapted to an ID request. |
| `fixtures/hidden-content.json` | Synthetic unavailable main slot, with a suppression marker and absent content. |
| `fixtures/malformed.json` | Authored invalid JSON containing the diagnostic sentinel. |
| `fixtures/truncated.json` | Authored syntactically unfinished JSON over otherwise complete HTTP framing. |
| `fixtures/wrong-shape.json` | Authored valid JSON with a string where a list is required. |
| Inline protocol bodies | All synthetic: blank/null/empty documents; invalid continuation maps/scalars/null/fraction/collision/empty key; missing pages/revisions, invalid flag, wrong page ID, empty content. Size cases generate spaces above a literal limit. Interrupted framing uses TestServer `FailAfter`. |
| `tests/support/mod.rs::category_service` | Authored state oracle: missing any expected continuation field replays the first fixture. Both correct traversal and deliberate generic-field-loss control use it. |

Expectations were authored from this capsule, the two cited pages and the explicitly labelled
synthetic assumptions, independently of the SDK implementation. Wire expectations are literal
name/value lists checked against `Recorded`; they are not generated from the declaration.

## Local files read

Required guides: root `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CHEATSHEET.md`, `docs/RECIPES.md`. Read root `Cargo.toml`/`Cargo.lock` for workspace and
dependency constraints. Filename inventories under crates/docs/examples/checks did not imply
reading their contents; no `AGENTS.md` was found within the worktree.

Read all five shared capability READMEs before selecting: `crates/concord-pagination/README.md`,
`crates/concord-retry/README.md`, `crates/concord-auth/README.md`,
`crates/concord-ratelimit/README.md`, `crates/concord-codecs/README.md`.

Rustdoc/source read: `crates/concord/src/lib.rs` and its manifest;
`crates/concord-core/src/{lib,prepared,response,error,value}.rs` for generated endpoint/send
traits, bounded response decoding, response heads, redaction and dynamic pairs;
`crates/concord-pagination/src/{lib,pages,types}.rs` and its manifest for `Pages`, `Resume::At`,
page bounds and incomplete results; `crates/concord-codecs/src/{lib,json}.rs` for strict JSON.
The cargo registry crate-level rustdoc was also read for existing locked dependencies
`reqwest 0.13.5`, `serde 1.0.229`, `serde_json 1.0.154` and test runtime `tokio 1.54.1`.
No campaign/parent directory, other worktree, skill file or external research source was read.

## Examples consulted — example-assisted

These are every pre-existing example file directly read for this task:

| File | What was taken |
|---|---|
| `examples/testserver/src/lib.rs` | `Recorded` wire/query inspection, `Reply::json`, complete/interrupted bodies, start/shutdown lifecycle. Used as the required fixture harness, not copied into the SDK. |
| `examples/testserver/Cargo.toml` | Actual package name `concord-example-testserver`, path dev dependency and runtime features. |

No existing `examples/*-sdk` implementation file was consulted. The required guides themselves
embed quickstart/native/provider excerpts; their declaration, `SendWith`, `document`, `limit`
and pagination patterns informed this implementation. Those referenced source files were not
opened. No sub-agents were used.

## Dependency and scope audit

All dependencies already existed in `Cargo.lock`. No new third-party crate was added; therefore
there is no new crate/license justification to supply. Concord's `json` and transport defaults
and `concord-pagination` without optional features are used. Serde derives the provider models;
serde_json handles extensible continuation scalars/warnings and independent test oracles.
Tokio and TestServer are test-only dependencies. Cargo added only this package's lock entry;
no existing package version changed.

Owned writes: this package, the single root workspace member, and Cargo's package lock entry.
No shared crate, check, script or other example was modified. Builds use default `target/`,
`CARGO_BUILD_JOBS=4`, and one cargo command at a time. No commit was attempted.

## Diagnostics and decisions

The first offline dependency resolution failed because the guessed TestServer package name
was wrong; the manifest was corrected to its actual name. The next compilation identified
`Resume::Key` as nonexistent; rustdoc showed `Resume::At`, which replaced it. These are SDK
author mistakes, not Concord defects. The first full protocol run caught `Option` accepting
a present null continuation as exhaustion; an explicit field deserializer now distinguishes
absence from invalid null, and the regression passes. No shared change was needed.

The final validation commands and outcomes are recorded in `VALIDATION.md`.

## Maintenance addition: batch content fetch (2026-11-07)

Added by the independent maintainer; details, file list and findings are in `MAINTENANCE.md`.
Live provider calls: **NOT_RUN**. The three pages below were fetched once each with `curl` on
2026-11-07 (06:36 UTC) and kept under the worktree's `target/tmp/docs/`; no API endpoint or
sandbox link was opened.

| Exact page read | Revision seen | What informed the change |
|---|---|---|
| https://www.mediawiki.org/wiki/API:Query | `oldid=7933095` (last edited 12 October 2025) | `pageids` is a list of integers, at most 50 values (500 for clients allowed higher limits); missing/invalid pages are listed with `missing`/`invalid` flags (Example 3); continued queries. |
| https://www.mediawiki.org/wiki/API:Revisions | `oldid=8361673` (last edited 1 May 2026) | Mode 1 "latest revision of a set of pages, by setting titles or pageids"; `rvlimit` "may only be used with a single page" (errors `rvmultpages`/`invalidparammix`); `rvprop=content` reports revision-deleted content as `texthidden`; `rvslots=main`; `rvcontinue`. |
| https://www.mediawiki.org/wiki/API:Continue/en | `oldid=8588859` (last edited 3 September 2026) | A continued property query returns the same set of pages with the next set of properties; the whole `continue` object is carried forward; absence of `continue` ends; `batchcomplete` concerns property data for the batch. |

Not on these pages, therefore labelled *inferred* (maintainer knowledge of MediaWiki output, not
verified live): a page ID unknown to the wiki is listed as `{"pageid": N, "missing": true}`
without `ns`/`title`; a result is continued when the wiki's own result-size limit is reached;
MediaWiki may attach a result-truncation warning to such a response (see `MAINTENANCE.md`).
The `invalid` row carrying a `pageid` and the `rvcontinue` value spelling are synthetic.

| Fixture | Derivation and purpose |
|---|---|
| `fixtures/batch-first.json` | Synthetic continued batch response: content for ID 12, a title-less missing ID 404, ID 7 listed without revisions (not answered yet), `texthidden` ID 31, an `invalid` ID 5, ID 8 without revisions; `continue` with `rvcontinue` and generic `||`. No `batchcomplete`, as property data is incomplete. |
| `fixtures/batch-last.json` | Synthetic final response: the same pages re-listed, only ID 7 with a revision (non-ASCII and `|` in its text); ID 8 still without a revision, so it becomes `NoContent`. |
| `tests/support/mod.rs::batch_service` | Authored oracle: only the complete continuation reaches the final response; anything else replays the first, as a restarting server would. |
| Inline bodies in `tests/batch.rs` / `tests/batch_consumer.rs` | All synthetic: unrequested, unattributable and duplicate rows, never-answered IDs, two revisions, untitled content, stalled and replayed continuations, warnings/API error/missing query mid-batch, colliding continuation keys, a 2 MiB content body, and an application's search hits plus bookmarks. |

No dependency was added; `Cargo.toml` and `Cargo.lock` are unchanged by this maintenance.
