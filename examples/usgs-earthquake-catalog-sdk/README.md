# USGS Earthquake Catalog SDK

A bounded slice of the public FDSN event service: query earthquakes as GeoJSON,
retrieve a selected event, and export the same filter as incremental CSV across
bounded pages. **Live provider calls: NOT_RUN.** Every executed workflow uses
`examples/testserver` on loopback. This is an **example-assisted** implementation;
see `PROVENANCE.md` and `FRICTION.md` for sources, assistance, and limitations.

The public hierarchy is `Usgs → catalog → window → geojson/csv`, with
`catalog → event` for lookup. Each operation uses generated `send()` and the SDK's
`Policy`; no native request editing or custom CSV parser is used.

```no_run
use std::num::{NonZeroU64, NonZeroUsize};
use concord_sdk_usgs_earthquake_catalog::{
    concord::reqwest, concord_codecs::stream::Completion,
    Magnitude, PageSize, Usgs, Window,
};

# async fn demo(local_base: &str) -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(30))
    .redirect(reqwest::redirect::Policy::none())
    .retry(reqwest::retry::never())
    .build()?;
// Supply the TestServer URL, without /fdsnws/event/1 (the declaration owns it).
let api = Usgs::new(http).with_base_url(local_base)?;
let catalog = api.catalog();
let filter = Window::new("2014-01-01T00:00:00Z", "2014-01-02T00:00:00Z")?
    .min_magnitude(Magnitude::new(4.0)?)
    .with_page_size(PageSize::new(500)?);
let budget = NonZeroUsize::new(4).unwrap();
let mut pages = catalog.pages(&filter, NonZeroU64::MIN, budget);
let mut selected = None;
while let Some(page) = pages.next_page().await? {
    if selected.is_none() {
        selected = page.items.first().map(|event| event.id.clone());
    }
    // Process each bounded GeoJSON page here.
}
if let Some(id) = selected {
    let event = catalog.event(id).send().await?;
    println!("{}: {:?}", event.data.id, event.data.properties.mag);
}
let mut export = catalog.export(&filter, NonZeroU64::MIN, budget);
while let Some(row) = export.next().await? {
    // Each row arrives before the whole response or window has been collected.
    println!("{}: {}", row.id, row.place);
}
assert_eq!(export.completion(), Completion::EndOfInput);
# Ok(()) }
```

For one page, use `catalog.query(&filter).geojson().send().await?` or
`catalog.query(&filter).csv().send().await?`. Both return `Response { data, meta }`.
The CSV `data` is an unread `CsvRows`: call `next().await`, handle every
`Some(Result<CsvRow, Error>)`, and check `completion()`. A single full page does
not establish that the window is exhausted; use `pages`/`export` for that.
Set a one-based offset with the endpoint's `set_offset(NonZeroU64)`.

Inputs accept RFC 3339 instants with explicit offsets and normalize them to UTC.
Bounds are inclusive and ordered, with at most 31 days between them. That maximum
is this SDK's scope limit, not a USGS limit. Page sizes are 1–20,000 (default 500),
minimum magnitude must be finite, and event IDs are nonempty opaque strings up to
256 bytes without controls. Queries fix `orderby=time-asc`, `eventtype=earthquake`,
and `nodata=404`. The filter is declared once on the window scope; only `format`
and the response type differ between GeoJSON and CSV. Event lookup sends no
window, sorting, magnitude, page-size or offset parameters. Detail products are
retained as JSON; their links and the event's `detail` URL are never fetched.

A full page advances by its actual row/feature count; a short page or 204/404
ends traversal. An exact multiple requires a further empty/no-data request.
`pages` uses `concord-pagination::Pages`: a budget or failed page returns a typed
`PagesError` with continuation/counters. `collect_complete()` retains its prefix
on failure. `export` holds one CSV reader and returns `Error::PageBudget` with the
first unrequested offset, or the actual failure. After failure its later reads
return `ExportFailed`, never a clean end. Dropping either traversal stops local
admission. Rows already processed are not rolled back.

Offsets are **not snapshots**. Provider updates may create duplicates or gaps,
including between the GeoJSON and CSV workflows. Continuations belong to the
same bounds, filters, ordering and page size; they are not durable snapshots and
are not bound cryptographically to a query. Consumers own reconciliation by ID
and update time. No automatic deduplication or completeness claim hides changes.

Only HTTP 200 enters the declared decoder. HTTP errors preserve status and
headers even if the bounded error document is invalid or interrupted:

| Condition | Result |
|---|---|
| Invalid input, including a scope setter making the window invalid | `InvalidInput`, before I/O |
| 204 or 404 | `Http(NoData)` for direct calls; exhaustion in traversals |
| 400 / 401 / 403 / 409 | `BadRequest` / `Unauthorized` / `Forbidden` / `Deleted` |
| 413 / 414 / 429 | `TooLarge` / `UriTooLong` / `RateLimited` |
| Any 5xx / any other non-200 | `Server` / `Unexpected` |
| Connection failure, wrong media, malformed/truncated GeoJSON, finite body limit | `Concord` with its stage and available response head |
| Bad CSV width, UTF-8, quoting, numeric value, transport or size | `Stream(ReadError)` with operation and record/byte position |
| Missing required CSV columns, inconsistent page counts, excess rows, offset overflow | `Protocol` |

429 coverage is defensive HTTP behavior, not a claim that USGS documents a fixed
throttling policy. `HttpFailure::retry_after(now)` uses `concord-retry`'s strict
singleton/date parser. The SDK never sleeps or retries and adds no authentication
or rate limiter to this public-data slice. The caller owns transport timeouts,
redirects and reqwest's own retry policy; the example and tests explicitly disable
both redirects and native retries. SDK page budgets count logical requests;
arbitrary caller-supplied transport settings can change wire-attempt counts.

Finite GeoJSON is limited to 8 MiB. Error text is limited to 16 KiB and decoded
as UTF-8 regardless of media label (so proxy text/HTML can be inspected); an
explicit unsupported charset is refused. CSV is limited per response to 16 MiB,
20,000 rows, 64 fields, 4 KiB of header field bytes and 64 KiB of unescaped field
bytes per data record. These bounds do not include the transport's own buffers
or rows retained by the application. Required catalog columns are checked even
for a header-only export; reordered and extra columns are accepted within the
field limit. Empty nullable numeric cells are `None`; invalid numbers are errors.

The shared CSV grammar preserves quoted commas, doubled quotes, LF/CRLF inside
fields, arbitrary network chunk boundaries and UTF-8. It accepts a final record
without a newline, strips one leading BOM, and ignores blank physical lines
outside quoted fields. It is not a strict RFC 4180 validator: stray quotes in
unquoted fields remain data. An unclosed quoted field, bad row or truncated HTTP
transfer is terminal. `CsvRows::next()` returns that error once and then `None`
with `completion() == Failed`; do not discard row errors with `filter_map(Result::ok)`.

SDK and Concord diagnostics withhold provider bodies, response header values and
the lookup request's event ID. `document_unredacted()`, native accessors, explicit
model formatting, and raw header access intentionally expose data. `prepare()`
is Concord's lower-level boundary: executing a prepared request directly bypasses
the SDK send policy's interval check, status mapping and finite-body limit.

Excluded: full QuakeML, scientific validation, product downloading, realtime feeds,
unbounded harvesting, snapshot consistency, credential flows and automatic retry.
Tests cover HTTP/1.1 loopback only; TLS/provider behavior was not exercised.

Reproduce from the workspace root, one cargo process at a time:

```sh
CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-usgs-earthquake-catalog
CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-usgs-earthquake-catalog --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-usgs-earthquake-catalog --no-deps
```
