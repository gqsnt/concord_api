# Maintenance record: publication reference list

Ticket: callers need a publication's reference list (`GET /{source}/{id}/references`, with
`page`, `pageSize` and the JSON/XML `format`), read by page number until the hit count is
reached, with a page cap and a resume point, one page size for every page, the SDK's JSON and
XML modes with their media and size rules, completed pages kept when a later page fails, and
search and citations unchanged. Maintainer: an independent role that did not write the SDK.
**Live provider calls: NOT_RUN.** Every executed request goes to the loopback test server.

## Prior exposure

Before starting I had read the role brief only. While attempting the ticket, in this order:

1. `git status`, `git log --oneline -3` and `git show --stat` of the two package commits. The
   commit messages are short summaries of the author's and the consumer's results. The
   consumer summary names "a numeric pubYear in JSON citations" as an unverified risk and
   "journal_title None for citations and core records". I saw both before writing any code;
   the first one informed the `pubYear` tolerance below.
2. Package: `Cargo.toml`, `README.md`, `PROVENANCE.md`, `fixtures/README.md`, every fixture
   payload, all of `src/`, `tests/support/mod.rs`, `tests/workflows.rs`, `tests/protocol.rs`;
   lines 1–60 of `tests/first_use.rs` and a grep of its `Error`/`SearchFormat`/`publication(`
   uses (to check that no exhaustive match would break if the API grew).
3. Shared docs and sources (read only): `docs/CHEATSHEET.md`; `docs/AUTHOR_GUIDE.md` §1, §2,
   §5.7, §9–§11; `crates/concord-pagination/src/{lib.rs, offset.rs}`, `types.rs` lines 1–90,
   `pages.rs` lines 240–390 and 680–770 plus its public-item list; the public-item list of
   `examples/testserver/src/lib.rs`; a grep of `crates/concord/src/lib.rs`. During a rustdoc
   diagnosis (below): `crates/concord-macros/src/emit.rs` lines 1030–1075, 1122–1152,
   1385–1410, 1460–1480 and 1690–1705.
4. Official documentation, below.

`FRICTION.md` and `FIRST_USE.md` were read only after the implementation, its tests, clippy and
rustdoc were green (2026-11-07 07:30 UTC). They prompted two documentation edits (the
endpoint docs now state the pinned `format` values; the README says which `PagesError` variant
carries `InvalidPage`), nothing else. No other SDK,
campaign file or worktree was opened.

## Provider documentation (read 2026-11-07)

Copies are under `target/tmp/docs/` (not part of the package).

| Source | Method | What it establishes |
| --- | --- | --- |
| <https://dev.europepmc.org/RestfulWebService> | `curl` 07:17 UTC (65,560 bytes, sha256 `f243f645…`) and a browser render | "Web Service Overview": reference lists are a documented content type ("Reference lists for more than 19.4 million publications"); "Output response formats can be XML or JSON". "Format": XML is the default when `format` is unspecified. |
| same page, §6 "Request Methods" | browser render; page source | **Did not render.** The section is an empty Swagger container filled by script from `https://www.ebi.ac.uk/europepmc/webservices/api/swagger.json`. In the browser that request was blocked (console: no `Access-Control-Allow-Origin`); a direct fetch returned HTTP 404. The page script names `/{source}/{id}/references` only in a commented-out operation sorter. The method's parameter table was therefore **not observed** on this date. |
| [Webinar slides (2020)](https://www.ebi.ac.uk/training/online/courses/embl-ebi-programmatically/wp-content/uploads/sites/128/2020/11/Webinar-slides-Europe-PMC-programmatically_2020.pdf) | `curl` 07:18 UTC (4,736,096 bytes, sha256 `09147e2e…`); pp. 13, 32–37 rendered and read | p. 13: the "references" module, "publications referenced in a given publication". pp. 32/35: the sibling `GET /{source}/{id}/citations` with `id`, `source`, `page`, `pageSize`. p. 33: source codes. pp. 36–37: the citations XML envelope `responseWrapper/hitCount/request(id, source, offSet, pageSize)/citationList/citation(id, source, citationType, title, authorString, journalAbbreviation, pubYear, …)`, with `hitCount` annotated "Total number of citations found". |

Facts by status:

- **From the ticket (owner contract, not re-observed):** the route, `page` and `pageSize`,
  JSON/XML `format`, and a response carrying the references and a hit count.
- **Observed:** XML default; both formats; the citations sibling's parameters and envelope.
- **Inferred, unverified:** the envelope names `referenceList/reference` (by analogy with
  `citationList/citation`); `page` is one-based (the existing citations call sends `page=1`;
  the citations echo shows `offSet` 0); a reference the service did not match to a record has
  no `id`/`source` (prior knowledge of the service, so identity is optional and either reality
  decodes); a JSON `pubYear` may be a number (prior knowledge plus the commit summary); a
  zero-hit response may omit `referenceList` (tolerated, unverified). The Web Service
  Reference Guide PDF linked from the page was not read (outside the two allowed documents).

## What changed for developers

```rust
let references = api.publication(key.source, key.id).references(PageSize::new(25)?);
let pages = references.pages(SearchFormat::Xml, Resume::Start, NonZeroUsize::new(4).unwrap())?;
let one = references.json().set_page(PageNumber::new(2)?).send().await?;
```

- New generated scope `publication(..).references(page_size)` (type
  `EuropePmcPublicationReferences`) with endpoints `json()` and `xml()`, each taking an optional
  `page` (default `PageNumber::FIRST`) and pinning `format=json` / `format=xml`, decoded by
  `Json` / the new `ReferencesXml`. The page size is a scope value, so one traversal cannot mix
  sizes; this follows the existing search design (plan in the scope, position in the endpoint).
- `EuropePmcPublicationReferences::pages(format, Resume<PageNumber>, cap)` returns
  `ReferencePages = Pages<Reference, PageNumber, u64, Error>`: the shared paginator, the same
  1..=20 cap as search, hit count as page metadata. It requests pages 1, 2, … and ends on the
  page that reaches the hit count (no extra request). Each page goes through generated
  `send()` and therefore the existing `SendPolicy` (single attempt, bounded diagnostics,
  `pageSize`-bound record check).
- Continuation rule (`next_reference_page`): the shared `concord_pagination::next_offset` with
  `offset = (page - 1) * size` and `total = hitCount`, plus "a continuing page must be full",
  because page `n + 1` starts at `n * size` whatever page `n` returned; `next_offset`
  deliberately allows short continuing pages, which is right for offsets and would silently
  skip records for page numbers. The hit count must equal the traversal's first page. Any
  contradiction is `Error::InvalidPage` through `PagesError::Provider`, before the page is
  emitted, so `Incomplete.collected` holds exactly the completed pages and `resume()` is the
  failed page. No `Error` variant was added (adding one breaks callers' exhaustive matches).
- New models: `Reference` (optional `id`/`source`, `title`, `author_string`,
  `journal_abbreviation`, `pub_year` accepting text or an unsigned JSON number, redacted
  `Debug`, `key() -> Option<RecordKey>`), `ReferenceList`, `ReferencesPage` (`hit_count`
  required; absent list reads as empty). `PageNumber` (one-based, `NonZeroU32`).
- Unchanged for existing callers: every search/citations declaration, model and behaviour.
  `SearchXml` was refactored internally to share the XML wrapper parsing and media check with
  `ReferencesXml`; its protocol tests and the consumer suite pass unchanged. `SearchFormat`
  is reused for references (doc wording broadened); its name is now slightly narrow.

## Files and locations edited

| File | Location | Edit |
| --- | --- | --- |
| `src/lib.rs` | crate doc; `pub use` lines; `SendPolicy::send`; new `requested_page_size`; `api!` scope `publication` | exports; page-size read-back extracted to one helper; nested scope `references` with two endpoints |
| `src/input.rs` | import; after `PageSize` | `PageNumber` |
| `src/model.rs` | after `CitationsPage`; `Document` impls | `Reference`, `text_or_unsigned`, `ReferenceList`, `ReferencesPage`, `Document for ReferencesPage` |
| `src/xml.rs` | whole file | `ReferencesXml`; shared `check_xml_media`, `wrapper` (root, `hitCount`); `reference` mapper |
| `src/pagination.rs` | imports; `SearchFormat` doc; `check_cap`; new impl | cap check shared; `ReferencePages`; references `pages`; `next_reference_page` |
| `tests/references.rs` | new | 11 regression and connected tests |
| `tests/references_use.rs` | new | 3 consumer tests |
| `fixtures/references-{1,2,3}.{json,xml}` | new | authored pages |
| `README.md`, `fixtures/README.md`, `PROVENANCE.md` | sections | caller docs, fixture contract, pointer addendum |

Root `Cargo.toml` and `Cargo.lock`: unchanged (the package was already a member; no new crate).

Facts that had to be written in several places:

- **Format pairing** (literal + decoder): twice in the declaration (json/xml), again in the
  traversal's `match format`, and in tests. Nothing ties the `SearchFormat::Xml` arm to the
  `xml()` endpoint except the code; mutation m07 below shows only tests catch a mix-up.
- **Envelope field names**: once as serde names (JSON) and once as XML element strings for
  each of `referenceList`, `reference`, `id`, `source`, `title`, `authorString`,
  `journalAbbreviation`, `pubYear`. Same split as the existing search model.
- **Page size**: declared as a scope value, then recovered from the prepared URL in two places
  (send policy, traversal) because handles have no getters; the 100 cap lives in both
  `PageSize::new` and `validate_count` (pre-existing).
- **Count rules**: `validate_count` (per response: ≤ requested, ≤ hit count) and
  `next_reference_page` (per traversal: exact page arithmetic) are two related checks.
- **The 1..=20 cap**: `MAX_PAGES`, the `check_cap` message, README; now one function instead
  of the search method's inline copy.

## Did the compiler or the tests catch an omitted edit?

Fourteen deliberate single-edit mutations, each applied to the source, the full package suite
run, then restored (`target/tmp/mut/run.sh`; sources diffed identical afterwards):

| Mutation | Caught by |
| --- | --- |
| m01 accept a short continuing page | tests (1: contradictions) |
| m02 skip the changed-hit-count check | tests (1) |
| m03 omit `format=json` on the references JSON endpoint | tests (7, incl. 2 consumer) |
| m04 XML endpoint declared `response Json` | tests (11) |
| m05 `ReferencesXml` without the media check | tests (1: XML mode refuses JSON) |
| m06 `Document::validate` for references made a no-op | tests (1: overfull page) |
| m07 traversal's XML arm calls `json()` | tests (6) |
| m08 `pubYear` without the number tolerance | tests (10) |
| m09 `referenceList` required | tests (3) |
| m10 XML mapper reads the wrong element for `id` | tests (5) |
| m11 `PageNumber` not re-exported | compiler (`E0425` inside the declaration) |
| m12 page offset off by one page | tests (9) |
| m13 references `pages` without the cap check | tests (1) |
| m14 no `Document` impl for `ReferencesPage` | compiler (`E0277`: the send policy's bound) |

The compiler catches structural omissions (an export, the send policy's trait bound). Every
semantic omission — a pinned literal, a decoder choice, a media check, a count rule — is caught
only by tests; m03 is caught because the wire test asserts the query and the stand-ins read
`format`, while against the real service the media check would turn it into a typed
`Metadata` failure. Several rules are
caught by a single test each (m01, m02, m05, m06, m13).

Other diagnostics during the work: one test-only compile error (a local `api` binding shadowed
the support helper). `RUSTDOCFLAGS="-D warnings" cargo doc` failed after I wrote the backticked
name of the endpoint's response format in its own doc comment: `rustdoc::redundant_explicit_links`
pointing into the `concord::api!` expansion. I reproduced it on the unchanged search `xml`
endpoint by writing `SearchXml` in its doc, so it is a latent baseline trap, not caused by the
new code; the doc comment now avoids the name. Neither the compiler nor the tests catch this.

## Remaining generic duplication

- Two `Decode` impls with identical `ACCEPT`, `check_content_type` and `context` (three
  trivial methods each); the parser setup is now shared.
- JSON/XML endpoint pairs per resource (search and references), plus a `match format` in each
  traversal. The duplication is what keeps each decoder fixed, which FRICTION records as
  intentional.
- Traversal scaffolding repeated between search and references: cap check (now shared), scope
  clone, `Pages::new` closure, format dispatch (about 15 lines).
- Page-number semantics written in the SDK: "continuing pages are full" and "the total is
  stable" on top of `next_offset`.
- Reading `pageSize` back from the prepared URL, now at two call sites.

## Provider-specific responsibilities (stay in the SDK)

Route and scope placement; pinning `format`; refusing the XML default at the JSON media check;
the envelope names; optional identity for unmatched references; `pubYear` tolerance; treating
an absent list as empty only where no records remain; one-based pages; the SDK caps (100
records, 20 pages); failing on a changed hit count rather than reading a shifted list;
`Reference::key()` connecting a matched reference to `fetch_citation`/`publication`.

## Candidate Concord improvements (recorded, not applied: shared crates are frozen)

1. **Read-only value getters on generated handles** (already in FRICTION): the traversal needs
   the page size it was declared with; today it prepares a request and parses the URL.
2. **Page-number continuation in `concord-pagination`**: a `next_page_number(page, size, len,
   total)` (or a mode of `next_offset`) that requires full continuing pages, plus an opt-in
   "total must not change" check. Both are generic to page-number APIs.
3. **Resume plan identity** (already in FRICTION) matters more for page numbers than for
   cursors: a page number resumed at another page size silently addresses other records. A
   key type or checkpoint carrying the page size would turn that into a typed refusal. Not
   added here to keep `Resume<PageNumber>` symmetric with `Resume<Cursor>`.
4. **A finite XML codec helper** (already in FRICTION) would remove the two `Decode` impls'
   boilerplate; the envelope mapping stays in the SDK.
5. **Generated docs and `redundant_explicit_links`**: the macro emits `[`F`](F)` format links
   and allows the lint on the generated structs, yet a user doc that names `F` in backticks
   makes warning-denied rustdoc fail with a span into `concord::api!`. Emitting the allow on
   every item that carries the generated text (inferred: the navigation method lacks it), or
   plain code text, would remove the trap.

## Validation (2026-11-07)

`CARGO_TARGET_DIR` = the worktree's absolute `target/`, `CARGO_BUILD_JOBS=4`, one command at a
time, `--offline`:

- `cargo test --locked -p concord-sdk-europe-pmc`: 17 first_use, 13 protocol, 11 references,
  3 references_use, 6 workflows, 2 README doctests (compile-only). Baseline before the change:
  17 + 13 + 6 + 1 doctest.
- `cargo clippy --locked -p concord-sdk-europe-pmc --all-targets -- -D warnings`: clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-europe-pmc --no-deps`: clean.
- `rustfmt --edition 2024 --check` on the package's Rust files: clean.
- `git diff --check`: clean; `Cargo.toml` and `Cargo.lock` unchanged.

Existing tests and assertions are unmodified (`tests/first_use.rs`, `tests/protocol.rs`,
`tests/workflows.rs`, `tests/support/mod.rs` have no diff). The consumer tests were written by
this maintainer, not by an independent consumer, and the change has not been independently
reviewed. Nothing was committed.
