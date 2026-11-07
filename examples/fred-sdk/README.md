# concord-sdk-fred

A bounded FRED v1 slice, maintained against frozen Concord baseline `e1995ff7`.
**Live provider calls: NOT_RUN.** Every test uses `concord-example-testserver` on loopback.
This is documentation/example-assisted work; the complete reading record is in
[PROVENANCE.md](PROVENANCE.md) and [FRICTION.md](FRICTION.md).

The slice supports searching series, selecting an ID and retrieving its metadata,
reading observations as default XML or explicit JSON, and exporting `file_type=csv`
as a bounded ZIP. Both connected workflows are exercised in `tests/workflows.rs`.

## Calling the SDK

The declaration generates `Fred → series(series_id) → metadata/observations/export_csv`.
Search belongs directly to `Fred`. The endpoints keep Concord's generated setters,
`prepare()` and ordinary `send()`; the SDK's send policy returns `Result<T, Error>`.

```no_run
use concord_sdk_fred::{Fred, FredConfig, SearchLimit, ObservationLimit, http_client};

async fn workflow(local_fixture_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let api = Fred::new(http_client()?, FredConfig {
        api_key: "synthetic-local-key".into(),
    }).with_base_url(local_fixture_url)?;

    let found = api.search("synthetic activity")
        .set_limit(SearchLimit::new(10)?).send().await?; // XML by default
    let chosen = found.seriess.first().ok_or("no matching series")?;
    let series = api.series(&chosen.id);
    let metadata = series.metadata().send().await?;
    let xml = series.observations()
        .set_limit(ObservationLimit::new(100)?).send().await?;
    let json = series.observations_json()
        .set_limit(ObservationLimit::new(100)?).send().await?;
    let export = series.export_csv().send().await?;
    std::fs::write("selected-series.csv", export.csv())?;
    let _ = (metadata, xml, json, export.readme());
    Ok(())
}
```

`search_json()` and `metadata_json()` provide JSON alternatives. Observation values
are always `String`: `.`, `NA`, empty strings, precision and trailing zeros are not
converted. `CsvExport::csv()` returns the exact uncompressed member bytes, without
parsing cells or assuming a header layout. `readme()` exposes the optional text
README; `into_csv()` transfers ownership for application-controlled export.
There is no automatic extraction to archive-supplied paths.

## Observation controls

XML, JSON and zipped-CSV requests expose the same `set_units`, `set_frequency`
and `set_aggregation_method` setters. Their closed types accept only documented
codes; `new(code)` or `code.parse()` returns `Error::InvalidParameter` for an
unknown, differently cased or whitespace-padded value, before any request.
The error identifies the parameter without retaining the input.

```no_run
use concord_sdk_fred::{AggregationMethod, Frequency, ObservationOptions, Units};

// Reuse one selection for observations(), observations_json() or export_csv().
fn annual_changes<E: ObservationOptions>(request: E) -> E {
    request.set_units(Units::PercentChange)
        .set_frequency(Frequency::Annual)
        .set_aggregation_method(AggregationMethod::EndOfPeriod)
}

let weekly = Frequency::new("weth")?;
assert_eq!(weekly, Frequency::WeeklyEndingThursday);
# Ok::<(), concord_sdk_fred::Error>(())
```

Each control is initially absent; `maybe_units(None)`, `maybe_frequency(None)`
and `maybe_aggregation_method(None)` clear it. FRED defaults to `lin`, the native
frequency and `avg`; an aggregation method alone has no effect. Pagination keeps
the selected controls when cloning the request; use the same controls on resume.

The SDK forwards and validates these controls identically in all three forms.
**FRED documents frequency aggregation only for XML/JSON**, so CSV query parity
does not promise aggregated CSV results. Native-frequency compatibility remains
a provider check. The SDK performs no local transformation, aggregation or numeric
conversion: `"."` and numeric-looking strings remain exact, including inside CSV.
Source: [FRED observations, units/frequency/aggregation_method](https://fred.stlouisfed.org/docs/api/fred/series_observations.html#units),
read **2026-11-07**. Local fixtures establish request behavior, not service support.

## Bounded pagination

Search limits are 1–1000; observation/export limits are 1–100000. Constructors
reject values outside these ranges before a request exists. Offset is a `u64`.
Single `send()` calls return one page/export, not every result. Lower the page size
when a page exceeds the body bound.

```no_run
use std::num::NonZeroUsize;
use concord_sdk_fred::{Fred, SearchLimit, pages, concord_pagination::Resume};

async fn search_all(api: &Fred) -> Result<(), Box<dyn std::error::Error>> {
    let template = api.search_json("synthetic activity")
        .set_limit(SearchLimit::new(10)?);
    let mut walk = pages(template, Resume::Start, NonZeroUsize::new(3).unwrap());
    while let Some(page) = walk.next_page().await? {
        for series in page.items { println!("{}", series.id); }
    }
    Ok(())
}
```

The same `pages` adapter accepts either XML/JSON observations operation. It checks
response offset, item count, nonzero limit, arithmetic overflow and progress, then
advances by the actual number of records received. `PagesError::Bounded` carries a
`Resume::At(offset)`; provider failure carries the failed offset, and earlier
pages remain with the caller. Resume with the same query, series and page size.
Offsets are not snapshots: concurrent provider changes can produce gaps or duplicates.
Advanced real-time/vintage selectors are outside this slice; it does not promise a
consistent multi-page snapshot. CSV exports do not supply a pagination envelope;
use their generated limit/offset setters explicitly.

## Failure semantics and limits

Generated `send()` makes one logical attempt. `http_client()` disables redirects,
reqwest retries and referrers and sets a 30-second timeout; these protections must
also be configured on an application-supplied transport. The SDK performs no
automatic retry, sleep or rate admission. FRED's documented rate-limit response
is surfaced as HTTP 429; coordinate shared-key traffic in the application.
`HttpError::retry_after` uses `concord-retry` to parse all Retry-After field lines,
including HTTP dates, and distinguishes invalid/repeated/overflowing fields.

| Failure | Observable result |
|---|---|
| Invalid page size | `Error::InvalidLimit`; nothing sent |
| Invalid observation control code | `Error::InvalidParameter`; nothing sent |
| Build or connection/body transport failure | `Error::Concord`, kind `Build` or `Transport` |
| Any non-2xx, including documented 400/404/423/429/500 | `Error::Http`; HTTP status stays authoritative |
| Missing, repeated, malformed or wrong Content-Type / unsupported charset | Concord kind `Metadata`, before reading the body |
| Malformed, truncated, wrong-shape XML/JSON | Concord kind `Decode` |
| HTTP success body over **1 MiB**, or error document over **16 KiB** | Concord kind `Limit`, stopped while reading |
| Invalid ZIP/CRC, missing or duplicate CSV, unsupported/encrypted member | Concord kind `Decode`; no partial export |
| More than **8 ZIP entries** or **4 MiB aggregate extracted bytes** | Concord kind `Decode`, with a static limit label |
| Inconsistent offset envelope | `Error::Pagination` through the shared traversal |

XML accepts `text/xml` and `application/xml`, UTF-8/US-ASCII only, with DTDs disabled
and at most 100000 XML nodes. JSON uses Concord's strict `Json` codec. ZIP accepts
`application/zip`, `application/x-zip-compressed` and `application/octet-stream`;
`text/csv` is refused. A CSV-looking body under a ZIP media type is also refused.
Supported archives use stored or DEFLATE compression, one basename ending `.csv`
and optionally `README.txt` (case-insensitive). Directories, paths, symlinks,
other member types and ambiguous duplicates are rejected. Both advertised and
actual extracted lengths are checked; reads are capped and CRC-validated.

HTTP error documents can be XML or JSON. The official errors page labels a JSON
example as `text/xml`; this SDK permits body selection for error documents only
after accepting an XML/JSON media type. Error status survives a missing, malformed,
wrong-media, oversized or interrupted problem document (`HttpError::document`).

The API key is marked `#[sensitive]`. Generated config/handle/prepared-request
Debug and SDK error Debug/Display withhold it. Provider messages can echo a key:
`Problem` never formats its message; only `message_unredacted()` reveals it.
Native request/URL/response accessors and provider data are deliberate raw access.
The client declares `limit BODY_LIMIT;`, so finite success bodies remain bounded
even through `prepare()?.send()`. That native path bypasses the SDK's provider-error
interpretation; generated `send()` also decodes error documents with the explicit 16 KiB cap.

Excluded: FRED v2 bulk release archives, XLSX parsing, CSV cell/schema parsing,
authentication registration/rotation, advanced search,
real-time/vintage selectors and automatic retry/rate scheduling.

## Verification

Run from the workspace root, with its default `target/`:

```sh
CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-fred
CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-fred --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-fred --no-deps
```

See [EVIDENCE.md](EVIDENCE.md) for executed commands, initial diagnostics, resource
measurements and scope checks. No claim of live provider compatibility is made.
See [MAINTENANCE.md](MAINTENANCE.md) for the observation-control change and checks.
