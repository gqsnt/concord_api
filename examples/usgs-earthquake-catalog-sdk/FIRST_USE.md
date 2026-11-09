# First use — USGS Earthquake Catalog SDK (fresh consumer)

H06 migration: historical `Usgs::new(http).with_base_url(origin)` calls below now use `Usgs::for_fixture(http, origin)` (bare HTTP on a literal loopback IP). Both generated controls are crate-visible.

Role: fresh consumer (wave 1, SDK23), Claude session. Worktree
`work/w1-sdk23-usgs-earthquake-catalog`, HEAD `17aaf0e1` (the author commit), clean tree at start.
Live provider calls: **NOT_RUN** — every call below goes to `examples/testserver` on loopback.

## What I read before the first working call

Observable actions, in order (2026-11-07):

1. The capsule (`work/roles/w1-sdk23-consumer-opus/prompt.md`).
2. `examples/usgs-earthquake-catalog-sdk/README.md` and its `Cargo.toml` (dependency list only).
   `ls` of the SDK directory showed `fixtures/`, `src/`, `tests/{common,protocol.rs,workflows.rs}`,
   `FRICTION.md`, `PROVENANCE.md`; none of those files was opened.
3. Generated rustdoc (`cargo doc --locked --offline -p … --no-deps`), rendered to text with a
   throwaway HTML stripper in the session scratchpad:
   - `concord_sdk_usgs_earthquake_catalog`: crate page, `Usgs`, `UsgsCatalog`,
     `UsgsCatalogWindow`, `UsgsCatalogWindowGeojson`, `UsgsCatalogWindowCsv`,
     `UsgsCatalogEvent`, `Policy`, `Response`, `Error`, `HttpKind`, `HttpFailure`, `CsvExport`,
     `CsvRows`, `CsvRow`, `CatalogPages`, `CatalogCsv`, `CatalogCsvProfile`, `Window`,
     `Timestamp`, `EventId`, `Magnitude`, `PageSize`, `FeatureCollection`, `Feature`,
     `Metadata`, `Point`, `Properties`, the four limit constants.
   - `concord_example_testserver` (all items), `concord_pagination::{Pages, Page, PagesError,
     Incomplete, Progress, Unfinished}`, `concord_codecs::stream::{Completion, ReadError,
     ReadErrorKind, Position}`, `concord_codecs::csv::MediaPolicy`, `concord_codecs::Json`,
     `concord_core::{ResponseMeta, Operation, Error, ErrorKind}`, and the `concord` facade's
     item list (to confirm `concord::ErrorKind` is re-exported).
   - `docs/*.md`: only a `grep` for `ResponseMeta`.
4. Not read: the SDK's `src/`, `tests/`, `fixtures/`, `FRICTION.md`, `PROVENANCE.md`. The
   official USGS page (https://earthquake.usgs.gov/fdsnws/event/1/) was **not fetched** in this
   session (it is on the provider host; I chose not to contact it). Provider facts below that
   go beyond the SDK README are my prior knowledge of the FDSN event service and are labelled
   *inferred*.

## Predictions

Written before any test code. "Wire" = what `examples/testserver`'s `Recorded` will show.

### Common setup

- Caller builds `reqwest::Client` (via `concord_sdk_usgs_earthquake_catalog::concord::reqwest`)
  with a timeout, `redirect::Policy::none()`, `retry::never()`, then
  `Usgs::new(http).with_base_url(server.url())?` (base without `/fdsnws/event/1`) and
  `api.catalog()`. No I/O until a `send()`/`next_page()`/`next()`.
- Filter: `Window::new(start, end)?.min_magnitude(Magnitude::new(2.5)?).with_page_size(PageSize::new(2)?)`.
  Start given as `2024-03-01T02:00:00+02:00` should normalize to UTC on the wire.
- No secrets exist in this public-data slice: no `authorization` header, no API key. The
  event ID is declared "sensitive", so it should be absent from `Debug`/`Display` of errors
  and endpoint values, but present on the wire.

### Workflow 1 — GeoJSON window, then one event

Calls: `catalog.pages(&filter, NonZeroU64::MIN, budget)`; `next_page().await` in a loop; pick
`page.items[k].id`; `catalog.event(id).send().await`.

Wire, per page:
- `GET /fdsnws/event/1/query?nodata=404&starttime=2024-03-01T00:00:00Z&endtime=2024-03-01T23:59:59Z&minmagnitude=2.5&limit=2&orderby=time-asc&eventtype=earthquake&offset=1&format=geojson`
  (parameter order taken from the rustdoc "Query (when present)" line). Unsure about the exact
  timestamp text: I predict RFC 3339 `…T00:00:00Z` without fractional seconds, not `+00:00`.
  `minmagnitude` predicted as `2.5`.
- Headers: `accept: application/json` (Json's fallback Accept), `host`, nothing secret; no
  `user-agent` unless reqwest adds one (predict none).
- Body: none.

Event lookup wire: `GET /fdsnws/event/1/query?nodata=404&eventid=<id>&format=geojson` — no
`starttime`, `endtime`, `minmagnitude`, `limit`, `orderby`, `eventtype`, `offset`.

Returns:
- `next_page()` → `Ok(Some(Page<Feature, NonZeroU64>))`. A full page (2 features with limit 2)
  sets `next = Some(offset + 2)`; a short page sets `next = None`, so the following call is
  `Ok(None)`. With 2 + 1 features: exactly 2 requests (offsets 1 and 3).
- If the window has an exact multiple (2 + 2) the SDK needs a 3rd request (offset 5); a 404 (or
  204) there ends cleanly with `Ok(None)` (no error surfaced).
- `event(id).send()` → `Ok(Response { data: Feature, meta })`, `meta.status() == 200`,
  `data.id == id`, `data.properties.mag == Some(..)`, `geometry.coordinates == [lon, lat, depth]`.
  The event endpoint is typed as one `Feature`, so I expect the provider body for
  `eventid=` to be a bare GeoJSON Feature (*inferred*: USGS returns a single Feature for an
  `eventid` query in geojson).

Failures:
- Unknown event → provider 404 → `Err(Error::Http(f))`, `f.kind == HttpKind::NoData`,
  `err.is_no_data()`, `f.meta.status() == 404`, `f.document_unredacted()` gives the text body.
  The event ID should not appear in `format!("{err:?}")` or `err.to_string()`.
- 503 on page 2 → `Err(PagesError::Provider { error: Error::Http(kind Server), failed: Resume::At(3), .. })`;
  the next call is `Err(PagesError::Ended { .. })`, never `Ok(None)`. Page 1 items stay with the caller.
- Budget 1 with a full first page → `Err(PagesError::Bounded { resume: Resume::At(3), .. })`.
- 200 with malformed JSON → `Error::Concord(e)` with `e.kind() == ErrorKind::Decode`;
  200 with `text/html` → `ErrorKind::Metadata`.
- Invalid input (end before start, > 31 days, page size 0, NaN magnitude, empty event ID) →
  `Error::InvalidInput` before any request (server sees 0 requests).

### Workflow 2 — the same window as incremental CSV

Calls: `catalog.export(&filter, NonZeroU64::MIN, budget)`; `while let Some(row) = export.next().await?`;
`export.completion()`, `rows_delivered()`, `pages_requested()`. For a single page:
`catalog.query(&filter).csv().send().await?` then `data.next().await`.

Wire: identical to the GeoJSON page request except `format=csv`; `accept: text/csv` (predict).

Returns: rows one at a time (`CsvRow`), header-mapped (USGS header
`time,latitude,longitude,depth,mag,magType,nst,gap,dmin,rms,net,id,updated,place,type,horizontalError,depthError,magError,magNst,status,locationSource,magSource`,
*inferred* from USGS). Quoted `place` values containing commas, doubled quotes and an embedded
newline arrive intact. Empty `nst`/`gap`/… cells are `None`. Paging same as GeoJSON: full page →
next offset; short page → `Ok(None)` and `completion() == Completion::EndOfInput`.
Response content type `text/csv` must be accepted (*inferred* USGS label: `text/csv`).

Failures:
- A row of the wrong width (or bad number) mid-page → `Err(Error::Stream(read_error))`,
  `read_error.kind() == ReadErrorKind::Format` (width) or `Decode` (number),
  `read_error.position().records` = rows already delivered in that response,
  `read_error.operation() == Some("Usgs.catalog.window.csv")`. Earlier rows are delivered,
  `completion() == Failed`, the next `next()` is `Err(Error::ExportFailed)`.
- Unclosed quote at end of body → `Stream`, kind `Format`.
- Connection dropped mid-body → `Stream`, kind `Transport`.
- Missing required header column → `Error::Protocol`.
- Budget reached on a full page → `Err(Error::PageBudget { next_offset })`.
- Single-response `CsvRows::next()` returns `Some(Err(..))` once, then `None`, with
  `completion() == Failed`.
- Provider update between pages (an event inserted before the current offset) → the SDK
  returns a duplicate row on the next page without deduplicating; the caller reconciles by ID.

## First working call

`tests/first_use.rs`, an initial `first_call` test (since replaced by the full file): a
`TestServer` answering every request with one hand-written FeatureCollection, a reqwest client
with `redirect::Policy::none()` and `retry::never()` (copied from the README), then
`Usgs::new(http).with_base_url(server.url())?.catalog().query(&Window::new(..)?).geojson().send().await?`.

- It compiled and passed on the **first attempt**; no source lookup and no extra imports beyond
  `{concord::reqwest, Usgs, Window}` and `concord_testserver::{Reply, TestServer}`.
- What it took: the README example plus the rustdoc of `Policy` to learn the concrete result type
  (`<Policy as SendWith<T>>::Output = Result<Response<T>, Error>`), which the endpoint's `send()`
  signature does not show.
- Observed raw target: `/fdsnws/event/1/query?nodata=404&starttime=2024-03-01T00%3A00%3A00Z&endtime=2024-03-02T00%3A00%3A00Z&limit=500&orderby=time-asc&eventtype=earthquake&offset=1&format=geojson`
  (no `minmagnitude` when unset; `limit=500` sent by default, as the scope docs say).

## The application (`tests/first_use.rs`)

Eight tests, all against loopback fixtures written by hand in the test file (the SDK's own
`fixtures/` were not used or read):

| Test | Caller task / failure |
|---|---|
| `workflow1_geojson_window_then_strongest_event` | Walk the M2.5+ window (2 + 1 features, limit 2), pick the strongest event, look it up; exact wire query of every request; lookup inherits nothing; 404 lookup → `Http(NoData)` with the echoed ID only in `document_unredacted()` |
| `workflow1_failures_keep_prefix_and_resume_point` | 503 + `Retry-After: 120` on page 2 → `PagesError::Provider` at `Resume::At(3)`, then `Ended`; resume from offset 3 after recovery; budget 1 → `Bounded`; exact multiple → a 3rd request whose 404 is a clean end; truncated JSON → `Concord(Decode)`; `text/html` 200 → `Concord(Metadata)` |
| `workflow2_csv_export_same_window_incrementally` | Same window as CSV, aggregate per network while streaming; first row observed while the server is still writing the first body (body split inside a quoted field, 300 ms gap); quoted commas, doubled quotes and CRLF survive; empty cells → `None`; `EndOfInput`; IDs equal the GeoJSON workflow's; only `format` differs on the wire |
| `workflow2_bad_row_is_terminal_never_skipped` | Unquoted comma in page 2's second row → `Stream(Format)` at record 1 of that response, operation `Usgs.catalog.window.csv`; three earlier rows delivered; the following good row never delivered; `ExportFailed` afterwards; no further page request |
| `workflow2_single_page_csv_failures` | `query().csv().send()` + `CsvRows::next()`: unclosed quote → `Format`, bad number → `Decode`, transfer cut → `Transport`; each once, then `None`, `Failed` |
| `workflow2_budget_and_shifting_offsets` | Budget 1 → `PageBudget { next_offset: 3 }`, `Failed`, then `ExportFailed`; a provider update shifts offsets so page 2 repeats a revised event: the SDK delivers 4 raw rows, the caller reconciles to 3 by ID + `updated` |
| `invalid_inputs_are_refused_before_io` | reversed / 32-day / date-only / offset-less windows, page sizes 0 and 20 001, NaN magnitude, empty and control-character event IDs, and a generated `set_endtime` that breaks the window → `InvalidInput`; server saw 0 requests |
| `missing_csv_column_is_never_accepted` | Header missing a column: never silently accepted, but classified differently by case (see M-3) |

Commands (worktree root, one cargo process at a time, `CARGO_TARGET_DIR=<worktree>/target`,
`CARGO_BUILD_JOBS=4`):

- `cargo test --locked --offline -p concord-sdk-usgs-earthquake-catalog --test first_use` → 8 passed.
- `cargo test --locked --offline -p concord-sdk-usgs-earthquake-catalog` → unit 0, `first_use` 8,
  `protocol` 13, `workflows` 9, doctest 1: all passed (author's suites run, not read).
- `cargo clippy --locked --offline -p concord-sdk-usgs-earthquake-catalog --all-targets -- -D warnings` → clean.
- `rustfmt --edition 2024 --config-path rustfmt.toml` applied to `tests/first_use.rs` only.
- `Cargo.lock` unchanged (the package was already a workspace member and locked).

Temporary probes (written into `first_use.rs`, run once, then removed): one printed the request
headers, `Timestamp`/`Window` Display/Debug, `CsvExport` completion after a budget stop and the
Display of `PagesError::Bounded`; one dropped each of the 22 CSV columns in turn, header-only and
with one data row. Their findings are folded into the tests and the sections below.

## Friction incidents, in order

1. **Result type hidden behind the policy projection.** Every endpoint's rustdoc shows
   `send() -> impl Future<Output = <Policy as SendWith<FeatureCollection>>::Output>`. The concrete
   `Result<Response<FeatureCollection>, Error>` is only on `Policy`'s `SendWith` impl. The README
   sentence "Both return `Response { data, meta }`" saved a lookup. (Generated usage.)
2. **Generated maintenance item first on the client page.** `Usgs`'s rustdoc opens with
   `validate_header_names()` (a long paragraph about native header constants) before `new()` and
   `with_base_url()`. Noise for a caller of a declaration with no native header names.
3. **Two ways into the same scope.** `catalog.window(starttime, endtime)` (generated, unvalidated
   pair) and `catalog.query(&Window)` (hand-written, validated) both return `UsgsCatalogWindow`;
   the generated doc says "Prefer `catalog.query(&Window)`". The names also cross: the *filter* is
   `Window`, the *scope* is `UsgsCatalogWindow`, and the method returning the scope is `query`.
   I used `query` everywhere; `window()` stayed unused.
4. **Macro artifact in rustdoc.** `impl Clone for UsgsCatalogWindowGeojson where for<'__concord> NonZeroU64: Clone`.
   Harmless, surprising.
5. **Generated text vs SDK policy.** Endpoint docs say "decoded from a 2xx response"; the README
   says only 200 enters the decoder and 204 is `NoData`. The policy wins, but the generated line
   reads as if a 204 would be decoded. (Not exercised: no 204 test.)
6. **Re-exported crates' docs need a separate, feature-aware `cargo doc`.** The SDK's `--no-deps`
   rustdoc does not link `concord_codecs::stream::{Completion, ReadErrorKind}` or
   `concord_pagination::{PagesError, Resume}`. Generating `concord-codecs` with default features
   omitted `stream`/`csv` entirely; I needed `--features json,csv`.
7. **Two vocabularies for one traversal concept.** GeoJSON `pages()` reports a budget as
   `Err(PagesError::Bounded { resume: Resume::At(3), .. })` and a later call as
   `PagesError::Ended`; CSV `export()` reports the same budget as `Err(Error::PageBudget { next_offset: 3 })`
   and a later call as `Error::ExportFailed`. A caller writing both workflows learns two error
   models, two resume encodings (`Resume<NonZeroU64>` vs `NonZeroU64`) and two counter APIs
   (`progress().admitted` vs `pages_requested()`).
8. **Inverted `next()` shapes.** `CsvExport::next() -> Result<Option<CsvRow>, Error>` but
   `CsvRows::next() -> Option<Result<CsvRow, Error>>`. Both documented; the loops differ
   (`while let Some(row) = export.next().await?` vs `while let Some(row) = rows.next().await { let row = row?; }`).
9. **Mandatory `NonZero` arguments for the ordinary path.** `pages`/`export` always take
   `(&Window, NonZeroU64, NonZeroUsize)`; "from the beginning" is `NonZeroU64::MIN` and every
   caller imports both `NonZero` types. Explicit and safe, but no shorter ordinary call exists.
10. **Testserver fixture friction (not SDK).** `Body::Chunks` takes `Bytes`, which the testserver
    does not re-export; `.into()` from `String` worked without naming the type. `Reply::json` sends
    `application/json; charset=utf-8` — my first assertion assumed bare `application/json`.
11. **Missing-column diagnostic depends on data and column type (M-3).** Header lacking
    `magSource`: header-only → `Protocol("CSV header lacks required catalog columns")`; with one
    data row → `Stream(ReadError { kind: Decode, label: "a CSV record does not decode into the requested row type", record 1 })`.
    The probe over all 22 columns: header-only, all 22 report `Protocol`. With a data row, the
    11 non-optional columns probed cleanly (`time`, `latitude`, `longitude`, `depth`, `magType`,
    `net`, `id`, `updated`, `status`, `locationSource`, `magSource`) report `Stream(Decode)`; the
    9 optional numeric ones (`mag`, `nst`, `gap`, `dmin`, `rms`, `horizontalError`,
    `depthError`, `magError`, `magNst`) report `Protocol`. (The `place`/`type` probe rows were
    malformed by my naive comma split and are not counted.) Never silent, no row delivered, `Failed` in every case — but the decode diagnostic
    blames record 1, not the header, and the README table promises `Protocol`.
12. **Update time in two representations.** The README tells consumers to reconcile "by ID and
    update time", but GeoJSON `Properties::updated` is `i64` Unix ms while `CsvRow::updated` is a
    provider `String`; the SDK's `Timestamp` is used for neither. My reconciliation compared CSV
    strings lexicographically, which only works because the provider format is fixed-width UTC.
    Cross-format reconciliation would need hand conversion.
13. **Traversal drops page metadata.** `CatalogPages = Pages<Feature, NonZeroU64, (), Error>`:
    `Metadata { generated, count, status }` is only visible through single-page
    `query().geojson()`. Not needed by my task; noted from rustdoc.

No source lookups were needed. No diagnostic was misleading except item 11.

## Predictions vs observed

| Prediction | Observed | Verdict |
|---|---|---|
| Page query order `nodata, starttime, endtime, minmagnitude, limit, orderby, eventtype, offset, format` | exactly that | match |
| `+02:00` start normalized to `2024-03-01T00:00:00Z`, no fraction, not `+00:00` | decoded value `2024-03-01T00:00:00Z`; raw target percent-encodes colons (`00%3A00%3A00Z`) | match (value); **mismatch M-1** (I predicted literal colons on the wire) |
| `minmagnitude=2.5`, `limit=2` | same | match |
| Headers: `accept: application/json` / `accept: text/csv`, `host`, no user-agent, no auth | exactly `[accept, host]` in both formats | match |
| Event lookup sends only `nodata, eventid, format` | same | match |
| 2 + 1 features → 2 requests, `next` = `[Some(3), None]`, then fused `Ok(None)` | same; `progress` admitted 2 / succeeded 2 / failed 0; no extra request | match |
| Exact multiple → 3rd request, 404 = clean end | offsets 1, 3, 5; `collect_complete` Ok with 4 items | match |
| Lookup 200 → `Response<Feature>`, status 200, products kept | same; response `content-type` preserved in `meta` | match |
| Unknown event → `Http(NoData)`, `is_no_data()`, ID withheld from Debug/Display | same; ID present only in `document_unredacted()` | match |
| 503 page 2 → `Provider { Http(Server), failed At(3) }`, then `Ended` | same; `Retry-After: 120` → 120 s; counters 2/1/1; no retry | match |
| Budget 1 → `Bounded { At(3) }` | same; `Incomplete.collected` = 1 page | match |
| Malformed JSON → `Concord(Decode)`; `text/html` → `Concord(Metadata)` | same; head status 200 kept on the decode error | match |
| Invalid inputs → `InvalidInput`, 0 requests | same, including the generated `set_endtime` case | match |
| CSV rows incremental | first row returned while `active_bodies() == 1` | match |
| Quoted commas / doubled quotes / embedded newline intact; empty optional → `None` | same (CRLF kept inside the field) | match |
| Short page → `EndOfInput`; counters | `rows_delivered` 3, `pages_requested` 2 | match |
| Bad width → `Stream(Format)`, `records` = rows already in that response, operation name, then `ExportFailed` | `records == 1`, `Some("Usgs.catalog.window.csv")`, `ExportFailed` twice, no more requests | match |
| Unclosed quote → `Format`; bad number → `Decode`; cut transfer → `Transport` | same, each once then `None` | match |
| Missing required column → `Protocol` | `Protocol` header-only and for optional columns; `Stream(Decode)` with a data row for non-optional columns | **mismatch M-3** (README table vs behaviour) |
| Budget on CSV → `PageBudget { next_offset: 3 }` | same; `completion() == Failed`, then `ExportFailed` | match (I had only predicted "not `EndOfInput`") |
| Shifted offsets → duplicate row delivered, no dedup | 4 raw rows, 3 after caller reconciliation | match |
| GeoJSON `content-type` I'd serve is `application/json` | testserver's `Reply::json` sends `application/json; charset=utf-8` | M-2: my fixture assumption, not SDK behaviour |

Not exercised (predictions neither confirmed nor refuted): 204, 400/401/403/409/413/414/429
classification, body/record/field limits, BOM handling, inconsistent `metadata.count`, excess
rows, offset overflow, dropped-traversal cancellation, TLS, and the live USGS service (NOT_RUN).

## What this says about Concord (core/macros/usage)

- The fixed hierarchy reads naturally: `Usgs → catalog → (query/window) → geojson|csv`, plus
  `catalog → event`. Declaring the filter once on the scope and varying only `format` and the
  output type held on the wire (incidents 3 and 7 aside).
- Generated rustdoc is complete about the wire (query names, sensitivity, defaults) and was
  enough to predict every request exactly. It is weaker at the caller's first question — "what
  does `send()` return?" — because the answer sits behind `<Policy as SendWith<T>>::Output`.
- Incremental CSV via a declared format (`CatalogCsv`) gave real streaming with precise,
  redacted diagnostics. The cross-page CSV traversal has no shared engine type, so the SDK's
  hand-written `CsvExport` and the `concord-pagination` `Pages` expose different error and
  resume vocabularies for the same concept (incident 7). A shared row-level traversal in
  `concord-pagination` (or `Pages::item_events`-style CSV support) is what I would change; no
  shared crate was edited.

## Assistance label

`docs-only` — README, generated rustdoc (SDK, `concord`, `concord-core`, `concord-codecs`,
`concord-pagination`, `concord-example-testserver`), and a `grep` of `docs/*.md`. The SDK's
`src/`, `tests/`, `fixtures/`, `FRICTION.md` and `PROVENANCE.md` were not read at any point. The
official USGS page was not fetched; provider-shape assumptions are marked *inferred*.
