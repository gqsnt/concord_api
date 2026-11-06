# Open-Meteo hourly forecast SDK

`concord-sdk-open-meteo` is an unpublished Concord example for the public
`GET /v1/forecast` interface. One operation retrieves a single location's hourly
forecast, selecting JSON or CSV at runtime. Any documented numeric hourly
variable can be named; temperature, humidity, wind speed and precipitation have convenience
constructors. The optional timezone and forecast-day settings are supported.
Time is ISO-8601 and units use provider defaults, retained in the results.

This small slice excludes current/daily/15-minute data, multiple coordinates,
model selection, other query options, reserved commercial resources, historical
products, XLSX and meteorological validation. The public endpoint needs no
authentication or context record. No credentials are used in tests.

## Construction and workflow A: JSON

The caller supplies the transport, including timeouts, proxies and redirect
policy. `with_base_url` can target the local fixture server and preserves any
base path prefix. It returns `concord::Error`; forecast calls return this SDK's
`Error`, which implements `From<concord::Error>`.

```rust,no_run
use concord_sdk_open_meteo::{
    ForecastData, ForecastQuery, HourlyVariable, OpenMeteo, ResponseFormat,
};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let http = concord::reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build()?;
let client = OpenMeteo::new(http);
let query = ForecastQuery::new(52.52, 13.41, [HourlyVariable::temperature_2m()])?
    .with_timezone("Europe/Berlin")
    .with_forecast_days(1)?;
let response = client.forecast(&query, ResponseFormat::Json).await?;
let ForecastData::Json(forecast) = response.data else { unreachable!() };
let values = &forecast.hourly.variables["temperature_2m"];
assert_eq!(values.len(), forecast.hourly.time.len());
println!("{}", forecast.hourly_units["temperature_2m"]);
# Ok(()) }
```

## Workflow B: CSV rows at runtime

The same method sends `format=csv` and `Accept: text/csv`. CSV bytes received
from that HTTP response pass through Concord's CSV parser; they are never
decoded as JSON. The provider's location table and hourly table are preserved.

```rust,no_run
use concord_sdk_open_meteo::{
    ForecastData, ForecastQuery, HourlyVariable, OpenMeteo, ResponseFormat,
};

# async fn example(client: &OpenMeteo) -> Result<(), Box<dyn std::error::Error>> {
let variables = [
    HourlyVariable::temperature_2m(),
    HourlyVariable::wind_speed_10m(),
    HourlyVariable::precipitation(),
];
let query = ForecastQuery::new(52.52, 13.41, variables)?;
// This can equally be an application setting chosen at runtime.
let format = ResponseFormat::Csv;
let response = client.forecast(&query, format).await?;
assert_eq!(response.meta.headers()["content-type"], "text/csv; charset=utf-8");
let ForecastData::Csv(csv) = response.data else { unreachable!() };
println!("timezone: {}", csv.location["timezone"]);
for column in &csv.columns {
    println!("header: {} / unit: {:?}", column.header, column.unit);
}
for row in &csv.rows {
    println!("{}: wind {:?}, precipitation {:?}",
        row.time, row.values["wind_speed_10m"], row.values["precipitation"]);
    // Unselected columns retain their complete header and decoded string value.
    println!("additional columns: {:?}", row.unknown);
}
let original_csv_bytes = csv.bytes(); // Exact quoting, UTF-8 and line endings.
assert!(!original_csv_bytes.is_empty());
# Ok(()) }
```

Headers determine mapping, so reordered columns do not change the values.
Selected numeric variables require a unit-bearing header; unknown columns are
available as text in every row. `NaN` and empty selected CSV cells become `None`;
JSON `null` likewise becomes `None`. No unit conversion is performed. CSV
location header order, all location fields, exact hourly headers and units are
retained. JSON retains all numeric hourly arrays and the entire hourly-unit map.

`HourlyVariable::precipitation()` selects the documented `precipitation`
variable in either format: total rain, showers and snow during the preceding
hour, in millimeters by default ([hourly parameter definitions](https://open-meteo.com/en/docs#hourly-parameter-definition)).
CSV exposes it as `row.values["precipitation"]` when selected, with the returned
header and unit in `csv.columns`. If it is returned without being selected,
it remains text in `row.unknown["precipitation (mm)"]`, just like any other
unselected column. Adding a convenience constructor does not change that rule.
Existing selections and `HourlyVariable::named("precipitation")` continue to work.

## Errors and completion

`Error::Provider` keeps the actual HTTP head and a `Result<Problem,
ErrorBodyError>` for the bounded JSON error-document attempt. For example,
`Problem { error: true, reason: ... }` describes an invalid variable. A malformed
or non-JSON error body still yields a provider error, with its real status,
headers (including retry hints) and operation identity. `Error::Response`
distinguishes media, JSON and CSV validation failures. `Error::Concord` retains
build, transport and body-limit diagnostics. `error.meta()`, `error.status()`
and `error.operation()` work across these cases; no HTTP status is fabricated.
Error formatting withholds provider text and header values; accessing fields,
headers or an underlying unredacted codec source exposes response data.

Each call executes once, with no pagination or retries. It buffers at most
4 MiB of successful response bytes (16 KiB for error documents) and validates
the entire document before returning. CSV has at most 10,000 records, each at
most 256 KiB, and Concord's default 4,096-field limit. Parsed data also occupies
memory, so the byte cap is not a total heap cap. A valid EOF plus all structural
checks is required for `Ok`; malformed late rows, truncated HTTP bodies and
limits return an error with **no partial forecast**. Iterating `csv.rows` is
in-memory consumption of a complete download, not network streaming. Dropping
the request future cancels it and releases its owned response; there is no
background work or completion report to collect.

Missing/duplicate headers, wrong row widths, absent selected variables/units,
invalid selected numeric cells and inconsistent JSON array lengths are errors.
Time strings and meteorological values are not scientifically validated.
Unknown top-level JSON fields are ignored. CSV accepts LF and CRLF through
Concord and supports quoted commas, escaped quotes and embedded newlines.
Concord's shared CSV framing rejects an unmatched quote at EOF; other framing follows
Concord's parser and is not a full strict RFC 4180 validation pass.

## Evidence and implementation

Tests use only `examples/testserver` on loopback. Fixtures are hand-authored
documentation-derived examples and synthetic perturbations, **not vendor
conformance evidence**. See [PROVENANCE.md](PROVENANCE.md) for source pins and
transformations and [FRICTION.md](FRICTION.md) for reproducible observations.
The precipitation change and its verification are recorded in
[MAINTENANCE.md](MAINTENANCE.md).

Dependencies are `concord` (JSON and CSV features) and `serde` (JSON domain
models). The mature CSV implementation already behind Concord's facade handles
CSV quoting and framing; this SDK only handles Open-Meteo's two-table envelope
and header mapping. No vendor SDK or additional generic parser is used.

Reading material used: the root README, author/caller/custom-format guides,
crate READMEs and generated rustdoc; the existing Salesforce CSV adapter and
Frankfurter metadata/error adapter supplied public-API patterns. No existing
provider implementation supplied the Open-Meteo envelope or row mapping.

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-open-meteo
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo clippy --locked -p concord-sdk-open-meteo --all-targets -- -D warnings
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo fmt -p concord-sdk-open-meteo
```
