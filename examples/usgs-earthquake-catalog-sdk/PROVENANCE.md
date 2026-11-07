# Provenance and observable evidence

Baseline: `56a16ee34f8aa714bd51dca67f744e5870e813b6` (Concord 4.2.0).
Authoring/retrieval date: **2026-11-07**. Package: `concord-sdk-usgs-earthquake-catalog`.
**Live provider calls: NOT_RUN.** Documentation research only; no USGS query,
count, application metadata, version, event-detail data URL, or feed was requested.
All executed workflows use `TestServer::start(|request: &Recorded| -> Reply)`.
Fixtures prove behavior against the stated contract, not live-service compatibility.

This work is **example-assisted**, including embedded examples in mandatory
Concord guides. One delegated read-only contract/code reviewer was used because
the ancestor `AGENTS.md`/`CLAUDE.md` explicitly request independent roles. It used
the inherited session model/settings; no model override or separate fresh-user
claim was made. It ran no cargo commands and edited no files. Review identified
the header-only schema hole and the ambiguous UTF-8 test; both were corrected.
The focused follow-up review found both corrections addressed without regressions.
No private reasoning transcript is retained.

Official documentation read (exact pages):

| Page | Read date | Facts used |
|---|---|---|
| https://earthquake.usgs.gov/fdsnws/event/1/ | 2026-11-07, author and reviewer | GET query parameters, GeoJSON/CSV media, inclusive time bounds, eventid, one-based offset, limit 1–20,000, ordering, eventtype, nodata 204/404, deleted-event 409 |
| https://earthquake.usgs.gov/earthquakes/feed/v1.0/geojson.php | 2026-11-07, author and reviewer | FeatureCollection/Feature, metadata count, nullable properties and Point coordinate order |
| https://earthquake.usgs.gov/earthquakes/feed/v1.0/geojson_detail.php | 2026-11-07, author and reviewer | Single-event Feature and products object; product contents not fetched |
| https://earthquake.usgs.gov/earthquakes/feed/v1.0/csv.php | 2026-11-07, author and reviewer | Documented field names; named mapping does not require their published display order |
| https://www.fdsn.org/webservices/FDSN-WS-Specifications-1.0.pdf | 2026-11-07, reviewer; error/status sections pp. 3–4 | 400/401/403/413/414/500/503; plain-text failures may echo submitted URLs |

Provider facts above are separate from SDK decisions: 31-day windows, default
500 rows, byte limits, no automatic retry, required-column presence, and strict
completion behavior are authored scope/policy. 429, 302, 206 and 418 responses,
all malformed data and secret sentinels are synthetic resilience cases. The
mutable-pagination warning comes from the supplied author capsule; the duplicate
fixture is an authored example, not a captured USGS response.

Fixture derivation (all values and identifiers are synthetic; none is a captured
earthquake or a generated SDK output):

| File under `fixtures/` | Derivation and purpose |
|---|---|
| `window.geojson` | Authored three events using the GeoJSON summary schema; time window follows the catalog documentation's 2014-01-01/2014-01-02 query example, with explicit UTC times |
| `page-1.geojson` | Synthetic first two events from that authored window; one-based offsets/limit from catalog docs |
| `page-2.geojson` | Synthetic third event, short final page |
| `page-mutated.geojson` | Synthetic repeated event B on a later short page, illustrating catalog mutation |
| `empty.geojson` | Synthetic zero-feature collection using summary schema |
| `event.geojson` | Synthetic event A plus a minimal products object shaped by the detail documentation |
| `window.csv` | Manually authored documented CSV columns and the same three events; comma, doubled quote, embedded LF, UTF-8, blank optional numbers, final record without newline |
| `page-1.csv` | Authored CSV partition after event B; two rows, including the multiline row |
| `page-2.csv` | Authored CSV partition containing event C; no final newline |
| `header-only.csv` | Documented column names with zero rows |
| `bad-header-only.csv` | Synthetic incomplete `id,place` header with no rows; schema regression |
| `reordered-header-only.csv` | Synthetic reversed documented columns plus `futureColumn`; compatible schema evolution |
| `bad-number.csv` | Synthetic valid first row, then a full-width bad magnitude with secret sentinel, then another row that must never be silently reached |
| `bad-width.csv` | Synthetic valid first row followed by a three-field row under the full header |
| `unclosed-quote.csv` | Synthetic valid first row then a place field cut inside quotes after an embedded LF |
| `malformed.geojson` | Synthetic mismatched JSON delimiters and a secret sentinel |
| `truncated.geojson` | Synthetic JSON prefix ending inside an array |
| `error.txt` | Synthetic plain-text service error echoing a fake submitted URL and sentinel, based on FDSN error-document shape |

Runtime fixture transformations are also synthetic and are authored in tests:
`protocol.rs` supplies missing/wrong/repeated media with stalled bodies, invalid
UTF-8 **inside a full-width third CSV row**, connection loss before/after headers,
finite/record/header/total-byte overflow, bad/repeated/missing CSV columns,
inconsistent count, overflowed offsets, and HTTP statuses/Retry-After fields.
`workflows.rs` converts LF to CRLF, fragments CSV byte-by-byte, delays the suffix
for two seconds, serves no-data/failing second pages, and verifies page budgets.
The oversized payloads are generated locally to keep checked-in fixtures small.
Assertions use documented request fields and independently authored fixture
values; the implementation is not imported to produce expected wire requests or
fixture bodies. Imported SDK limit constants select over-limit test sizes only.

Every external example file consulted:

| File | Access | What was taken |
|---|---|---|
| `examples/testserver/src/lib.rs` | Direct source/rustdoc, author and reviewer | Recorded query/header inspection, Reply/Body constructors, delayed/fragmented/failing/stalled bodies, counters, shutdown |
| `examples/testserver/Cargo.toml` | Direct manifest | Package name and path dependency; existing Tokio feature support |
| `examples/quickstart/src/lib.rs` | Embedded snippets in README/QUICKSTART only | Declaration/navigation, supplied reqwest client, local base override, generated send, bounded error-document pattern |
| `examples/quickstart/src/main.rs` | Embedded QUICKSTART snippet only | Local-server workflow convention; not copied |
| `examples/shapes/tests/wire.rs` | Embedded QUICKSTART snippets only | Read as required; no subdomain/resolver technique used |
| `examples/frankfurter-sdk/src/lib.rs` | Embedded AUTHOR_GUIDE §5.11 snippet only | Generic policy returning typed data plus metadata, bounded failure document, boxed error payload |
| `examples/openai-sdk/src/lib.rs` | Embedded AUTHOR_GUIDE multipart snippets only | Read as required; nothing adopted (multipart excluded) |
| `examples/google-drive-sdk/src/upload.rs` | Embedded AUTHOR_GUIDE multipart snippet only | Read as required; nothing adopted |
| `examples/native/src/native_edit.rs` | Embedded RECIPES snippet only | Read as required; native query editing not adopted |
| `examples/compat/src/lib.rs` | Embedded RECIPES snippet only | Read as required; native multi-segment paths not adopted |

No other existing `examples/*-sdk` file was opened. References/links and a filename
inventory are not claimed as file-content consultation. Embedded
`checks/phase-a/tests/{send_policy,received}.rs` and `checks/phase-a/src/lib.rs`
snippets in AUTHOR_GUIDE supplied the public `SendWith`/stream syntax; those files
were not opened or edited. Review subsequently read this new package's source,
tests, README and fixtures, not another SDK implementation.

Repository guidance read:

- Ancestor `AGENTS.md`, `CLAUDE.md`, `START_HERE.md`, `DESIGN_CHARTER.md`, `PROMPT.md`,
  `AUTHORITY.md`, `contracts/PHASES.md`, `contracts/RESOURCES.md`, `BASELINE.json`.
  The user's later bounded capsule supersedes broader campaign/merge work.
- Root `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
  `docs/CHEATSHEET.md`, `docs/RECIPES.md`, workspace `Cargo.toml` and `Cargo.lock`.
- Concord rustdoc at source: `crates/concord/src/lib.rs`,
  `crates/concord-core/src/{lib,codec,prepared,response,error}.rs`.
- Codec rustdoc/source: `crates/concord-codecs/src/{lib,json,text}.rs`,
  `csv/{mod,options,read,record}.rs`, `stream/{error,records}.rs`;
  reviewer additionally inspected `csv/{frame,codec}.rs` and `stream/driver.rs`.
- Pagination rustdoc/source: `crates/concord-pagination/src/{lib,pages,types}.rs`.
- Retry rustdoc/source: `crates/concord-retry/src/http.rs`; relevant Concord,
  codec and retry manifests were inspected for feature/dependency choices.
- Registry rustdoc/source: chrono 0.4.45 `src/lib.rs`, `src/datetime/mod.rs`
  and its manifest; reqwest 0.13.5 `src/retry.rs` and
  `src/async_impl/client.rs` for native retry/redirect configuration;
  serde 1.0.229 and serde_json 1.0.154 `src/lib.rs`, Tokio 1.54.1
  `src/time/timeout.rs` for derive/value/timeout contracts.

Dependencies: **no new third-party crate and no existing package version changed**
in Cargo.lock. Only the new SDK package entry was added. Direct shared crates are
Concord/core/macros through `concord` (reqwest defaults enabled for TLS capability),
`concord-codecs` with `json,csv`, `concord-pagination`, and `concord-retry` for
Retry-After projection. Existing locked `chrono 0.4.45` (MIT OR Apache-2.0,
`std` only) supplies calendar-aware RFC 3339 validation/normalization without a
handwritten date parser; serde/serde_json supply models, Tokio and testserver the
local test runtime. No crypto, date, parser or HTTP dependency was newly resolved.

The initial unlocked command was offline and successful:
`CARGO_BUILD_JOBS=4 cargo test --offline -p concord-sdk-usgs-earthquake-catalog --no-run`.
Subsequent cargo builds/checks used `--locked`. All builds used this worktree's
default `target/`; no CARGO_TARGET_DIR was set, no cargo commands overlapped,
and no commit was attempted. Final command results are recorded in `FRICTION.md`.

## Maintenance addendum — 2026-11-07

Maintenance baseline: `9f59f9a6`, after the first consumer. The original evidence
above remains historical. This session read the implementation and consumer tests,
implemented the filter ticket and passed the focused and complete test suites
**before opening `FRICTION.md` or `FIRST_USE.md`**. No subagents were used.
The complete reading/edit/verification record is in `MAINTENANCE.md`.

Documentation retrieved read-only on **2026-11-07**:

- [USGS FDSN event API](https://earthquake.usgs.gov/fdsnws/event/1/), Query method
  Parameters → Other: magnitude bounds are decimals; `orderby` accepts `time`,
  `time-asc`, `magnitude`, `magnitude-asc`. The service defaults to `time`;
  retaining the SDK's `time-asc` default is a compatibility decision. This page
  gives no numeric magnitude interval or explicit magnitude min/max relation;
  the ticket's min ≤ max guard is implemented with equality allowed.
- [Linked FDSN specification v1.0](https://www.fdsn.org/webservices/FDSN-WS-Specifications-1.0.pdf),
  parameter table (p. 5) and event descriptions (p. 12), corroborate the
  magnitude parameter types and ordering spellings.

`tests/filters.rs` uses the existing synthetic fixtures. Its consumer rearranges
`window.geojson` features and `page-1.csv`/`page-2.csv` records into descending
time order C, B, A, repartitioned 2 + 1. It delays the second CSV record for two
seconds and fragments that record byte by byte. Its event B detail is authored
inline from the same synthetic values, without the SDK's model serializers.
Empty responses exercise the wire matrix without claiming provider sorting or
filter implementation. The bad-number fixture still includes an unreachable
later row, proving failure is terminal with the new filters too.

**Live provider calls: NOT_RUN.** Only documentation URLs were fetched; all
executed requests use the local TestServer. No new dependency, package version,
workspace member or lockfile change was needed: this SDK was already registered
and locked at the maintenance baseline.
