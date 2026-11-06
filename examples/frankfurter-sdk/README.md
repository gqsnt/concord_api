# concord-sdk-frankfurter

Small, unpublished SDK for Frankfurter **v2 JSON single-pair rates**, built on Concord's
public API. The contract is OpenAPI `2.1.1`, checked on 2026-11-06. It covers latest and
historical pair reads, plus the optional comma-separated `providers` filter. It excludes
v1, lists/time series, currency/provider catalogues, pagination, CSV, NDJSON, currency
conversion, exchange execution and claims about rates accuracy. Fixtures are offline
examples, **not vendor conformance evidence**. See `PROVENANCE.md`.

## Construction and workflows

The public instance needs no authentication or configuration record. Pass your configured
`concord::reqwest::Client`; timeouts, proxies, TLS, redirects and default headers belong
to that transport. The SDK adds no credentials. A private deployment's authentication
is outside this slice. `with_base_url(origin)` supports loopback or self-hosting; any
prefix in `origin` is retained, and the declaration appends `v2/rate/...`.

```no_run
use concord::reqwest;
use concord_sdk_frankfurter::{Date, Frankfurter};
use std::time::Duration;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder()
    .timeout(Duration::from_secs(10))
    .build()?;
let api = Frankfurter::new(http);
// Offline/self-hosted alternative: api.with_base_url("http://127.0.0.1:3000")?;

// A: latest available pair observation. This need not be today's rate.
let latest = api.latest_pair("EUR", "USD").send().await?;
println!("{}: {} EUR/USD", latest.rate.date, latest.rate.rate);

// B: a historical observation. Read the returned date; don't substitute the request date.
let date: Date = "2024-01-15".parse()?;
let historical = api.historical_pair("EUR", "USD", date).send().await?;
println!("{}: {} EUR/USD", historical.rate.date, historical.rate.rate);

// Optional filter; the two original callers above do not change.
let ecb = api.latest_pair("EUR", "USD").set_providers("ECB").send().await?;
assert!(ecb.meta.status().is_success());
# Ok(()) }
```

Currency and provider strings are sent as supplied. Concord encodes each currency as a
single path segment and the provider filter as one query value. It does not verify
coverage or normalize case. `Date` is `chrono::NaiveDate`, so calendar parsing errors
occur before a request is made. Callers should use the provider's `YYYY-MM-DD` date range;
chrono can represent dates beyond that range. Unknown response fields are ignored.

For example, a response may gain a nested `providers` attribution field without changing
either rate workflow or the four-field `Rate` model. The SDK ignores that field and returns
the top-level observation date and exact rate; it does not recompute them from attribution.
The request's optional `providers` filter remains independent. Ignoring an extra field still
requires valid JSON and does not relax validation of known fields. The offline attribution
fixture is a synthetic compatibility scenario based on the shared OpenAPI `Rate` schema;
the pair endpoint does not document an `expand` parameter, and the SDK does not send one.

## Decimal policy

`Rate::rate` is `Decimal` (a public alias for `bigdecimal::BigDecimal`). The field decoder
captures a validated JSON token using `serde_json::value::RawValue`, then parses it directly
as a decimal. There is no float intermediate, rounding or currency minor-unit truncation.
Positive integer, fractional and exponent notation are accepted, including precision beyond
`f64`. Strings, null, booleans, containers, zero, negative values and invalid JSON numbers
are rejected. BigDecimal's mantissa is arbitrary precision; its signed 64-bit decimal
scale is a representation limit, and unrepresentable exponents fail decoding. Equivalent
numeric spellings compare equal; original whitespace/exponent spelling is not retained.

This SDK only reads values. Arithmetic, formatting, precision contexts and any rounding
of later computations belong to the application. BigDecimal supplies the mature decimal
parser and representation; the SDK does not implement a decimal parser or general FX model.

## Errors and completion

Every request's `.send().await` returns `Result<RateResponse, Error>`, where `Error` is this
crate's error (the client declares `send SendPolicy;`). Only `with_base_url` and `.prepare()`
return `concord::Error`; signatures show both types as a bare `Error`, and this crate's
`Error` implements `From<concord::Error>`, so `?` converts. The cases:

- `Error::Provider` represents every non-2xx status. It retains the actual status, version,
  headers (including retry hints), and generated operation identity: `meta` is a
  `ResponseMeta`, Concord's `concord::ResponseMeta` re-exported (`status()`, `version()`,
  `headers()`, `operation()`). Its `document` is
  either a typed `Problem { status: Option<u16>, message }` or an `ErrorBodyError` distinguishing
  media refusal, body read/size failure and decode failure. The HTTP status wins over a
  contradictory JSON `status`. Provider bodies are capped at 16 KiB.
- `Error::Concord` keeps the original Concord error plus captured metadata when available.
  Build/transport errors before a head arrives have no invented status. On success-body
  read, media or decode errors, the real status and headers survive. Concord's unread
  response remains recoverable from `source` on success media-type refusals.

```no_run
use concord_sdk_frankfurter::{Error, Frankfurter};
# async fn handle(api: &Frankfurter) {
match api.latest_pair("EUR", "ABC").send().await {
    Ok(observation) => println!("{}", observation.rate.rate),
    Err(Error::Provider(error)) => {
        eprintln!("HTTP {}", error.meta.status());
        match &error.document {
            Ok(problem) => eprintln!("provider message: {}", problem.message),
            Err(cause) => eprintln!("error document unavailable: {cause}"),
        }
    }
    Err(error) => eprintln!("request failed: {error}"),
}
# }
```

One call performs one Concord execution. There are no SDK retries, caches, pagination,
background tasks or incremental rate results. A successful rate requires a complete body
and a valid JSON object: broken transfer, trailing garbage, wrong media type, missing fields
or invalid decimals return errors without a partial observation. Dropping the future
cancels local processing and produces no completion result. Success bodies are buffered
by Concord's finite JSON decoder and have **no SDK byte cap**; configure a transport
timeout and use a trusted endpoint. Error-body limits do not limit success bodies.

`.send()` is the one terminal method. `.prepare()` gives Concord's native request: use
`.prepare()?.receive()` to inspect the response head before consuming it. Its own
`.prepare()?.send()` yields only `Rate` and `concord::Error`; it does not run the SDK's typed
provider-error adapter. Errors and metadata redact header values and
provider messages from their own `Debug`/`Display`; explicit `Problem` and header access
is unredacted.

## Offline verification

`tests/protocol.rs` asserts HTTP method, target, headers and exact empty request bodies,
decimal policy, provider errors and failed body completion. `tests/workflows.rs` exercises
both public workflows and optional-filter/unknown-field compatibility against
`examples/testserver`, including nested provider attribution through both `.send()` and
`.prepare()?.send()`. No live provider requests or credentials are used by tests. The maintenance scope
and verification record are in [MAINTENANCE.md](MAINTENANCE.md).
