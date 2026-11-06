# Nager.Holidays v4 SDK

`concord-sdk-nager-holidays` is an unpublished Rust 2024 crate for the Nager.Holidays
(Nager.Date) Community **v4** contract, rechecked on **2026-11-06**. It declares
`GET /api/v4/Holidays/{CountryCode}/{Year}` with `concord::api!` and decodes the
finite holiday JSON array. Country precedes year. It does not use the former v3
`PublicHolidays/{Year}/{CountryCode}` contract or its `global`, `counties`, and
`types` fields. Sources and byte-level fixture provenance are in [PROVENANCE.md](PROVENANCE.md).

The scope is country/year retrieval and local filtering by national status or holiday
type. There are no calendar calculations, holiday-date predictions, coverage guarantees,
or other provider endpoints. No generic pagination or streaming abstraction is needed.

## Construction

The hosted REST API requires no authentication or license key. There is no auth config
record and the SDK declares no authorization header. Supply an appropriately configured
Concord/reqwest transport; a client with application default headers will still use them.
Timeouts, redirects, proxies and TLS configuration belong to that transport.

```rust,no_run
use concord_sdk_nager_holidays::NagerHolidays;

fn configured(origin: Option<&str>) -> Result<NagerHolidays, Box<dyn std::error::Error>> {
    let http = concord::reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let api = NagerHolidays::new(http); // https://nagerholidays.com
    // The override is an origin, e.g. a loopback server URL, without /api/v4.
    Ok(match origin {
        Some(origin) => api.with_base_url(origin)?,
        None => api,
    })
}
```

In this workspace, depend on this crate by path. The HTTP/runtime dependency is
`concord`; consumers can use `concord::reqwest`. Tests disable proxies and redirects
and point exclusively to `examples/testserver` on loopback. Documentation examples
are compiled without execution; none of the tests contacts the provider.

## Workflow A: read holidays for one country and year

```rust,no_run
use concord_sdk_nager_holidays::{Date, NagerHolidays};

async fn austria(api: &NagerHolidays) -> Result<(), Box<dyn std::error::Error>> {
    let response = api.holidays("AT", 2026).fetch().await?;
    println!("HTTP {}", response.meta.status);
    for holiday in &response.holidays {
        let date: Date = holiday.date; // chrono::NaiveDate, no timezone
        println!("{date}: {}", holiday.name);
    }
    Ok(())
}
```

`Holiday` keeps the six v4 fields: `date`, `name`, `country_code`, `national_holiday`,
`subdivision_codes`, and `holiday_types`. Dates use Chrono's mature date parser instead
of SDK-written validation. All documented holiday types are typed enum variants;
unrecognized strings remain `HolidayType::Other(String)`. Unknown object fields are
ignored. JSON `null` or an omitted subdivision list is `None`, while `[]` is
`Some(vec![])`; the SDK does not infer national status from that list. Other documented
fields are required, including the `holidayTypes` array.

These compatibility rules apply to both `.fetch()` and generated `.send()`. If a
response adds an optional unknown field and changes a populated `subdivisionCodes`
list to `null`, callers keep every other known value and receive `None` for that
list. In particular, a regional holiday's `national_holiday: false` stays false;
no caller changes are needed. See [MAINTENANCE.md](MAINTENANCE.md) for the regression
and change record.

## Workflow B: another country/year and local filters

```rust,no_run
use concord_sdk_nager_holidays::{Holiday, HolidayType, NagerHolidays};

async fn compare(api: &NagerHolidays) -> Result<(), Box<dyn std::error::Error>> {
    let austria = api.holidays("AT", 2026).fetch().await?;
    let germany = api.holidays("DE", 2027).fetch().await?;

    let national: Vec<&Holiday> = germany.holidays.iter()
        .filter(|h| h.national_holiday).collect();
    let bank: Vec<&Holiday> = germany.holidays.iter()
        .filter(|h| h.holiday_types.contains(&HolidayType::Bank)).collect();
    let national_public: Vec<&Holiday> = germany.holidays.iter()
        .filter(|h| h.national_holiday && h.holiday_types.contains(&HolidayType::Public))
        .collect();
    println!("AT: {}; DE national: {}, bank: {}, national public: {}",
        austria.holidays.len(), national.len(), bank.len(), national_public.len());
    Ok(())
}
```

Filtering uses ordinary iterators after the full response is decoded, without extra
HTTP calls. Test DE entries are synthetic examples of the model, not a German calendar.

## Errors and response metadata

`.fetch()` returns `HolidayResponse` or the SDK's `Error`. `Error::kind()` distinguishes
HTTP 400 (`InvalidRequest`), other non-2xx statuses (`Http`) and Concord's build,
transport, metadata and decode stages (`Concord(concord::ErrorKind)`). The 400
classification follows HTTP status only; the official pages reviewed do not specify
an error-document schema, so no JSON problem type or provider message is invented.

`ResponseMeta` contains the actual `StatusCode`, HTTP version and all response headers,
including duplicate values. It is kept on both success and any failure after a head
arrives. For a pre-head transport or build failure, `Error::meta()` and `status()` are
`None`. A body decode/transport failure can therefore have status **200**. The original
operation identity, codec diagnostics and source are available through `concord()`.

```rust,no_run
use concord_sdk_nager_holidays::{ErrorKind, NagerHolidays};

async fn inspect_failure(api: &NagerHolidays) -> Result<(), Box<dyn std::error::Error>> {
    match api.holidays("XX", 0).fetch().await {
        Ok(response) => println!("{} entries", response.holidays.len()),
        Err(error) => {
            if error.kind() == ErrorKind::InvalidRequest {
                println!("Rejected with {:?}", error.status());
            }
            // Non-2xx and refused Content-Type responses remain unread.
            // This explicit escape leaves body reading/size limits to the caller.
            if let Some(response) = error.concord().response() {
                println!("Response status: {}", response.status());
            }
            return Err(error.into());
        }
    }
    Ok(())
}
```

To take an unread response, use `error.into_concord().into_response()`. Error
`Display`/`Debug` omit response header values, bodies and URLs; named metadata accessors
are unredacted. The generated `.send()` and `.prepare()` remain available for ordinary
Concord use; `.send()` returns `Vec<Holiday>`/`concord::Error` without the SDK's captured
metadata and HTTP classification. The documented SDK path is `.fetch()`.

## Completion and limitations

One fetch performs one Concord execution, with no SDK retry. A reqwest transport's
configured redirect behavior still applies. Success means the whole finite JSON array
was decoded; `[]` is a complete empty result. A missing/wrong media type, malformed JSON,
missing required field, invalid date or interrupted body returns an error and no partial
vector. Local filtering changes the caller's selection, not request completion. Separate
completed reads remain owned by the caller if a later read fails; there is no combined
transaction, cross-call rollback or resumable partial outcome.

There are no pages, cursors or incremental stream. The successful response is buffered
in memory by Concord's finite JSON decoder, with no SDK-specific byte cap. Country
strings and `u16` years are sent as supplied; coverage, year availability and semantic
consistency between returned fields are left to the provider/application. Each country
value is encoded as one segment; Concord rejects dot segments before sending. `Json`
requires the normal JSON response Content-Type. This crate is a scoped example SDK,
not a published package or a vendor conformance result.

## Offline verification

`tests/protocol.rs` checks wire method, country/year order, percent encoding, headers,
exact empty request bytes, documented success fields, synthetic HTTP errors, unknown
fields/types, nullable/empty/omitted subdivisions, invalid data and interrupted reads.
The maintenance regression compares an original and perturbed DE array over HTTP:
one populated list becomes null, unknown fields are added, and all other known
values remain equal. The AT perturbation also exercises generated `.send()`.
`tests/workflows.rs` exercises both public workflows and a failed second read.
`fixtures/** -text` in `.gitattributes` preserves fixture bytes across Windows checkouts.
Fixtures and loopback tests **are not vendor conformance evidence**.

Use the repository's Rust 1.99.1 toolchain and shared target directory:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-nager-holidays
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo clippy --locked -p concord-sdk-nager-holidays --all-targets -- -D warnings
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo fmt -p concord-sdk-nager-holidays
```
