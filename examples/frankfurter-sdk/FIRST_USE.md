# First use of `concord-sdk-frankfurter` (independent consumer)

Role: application developer adopting the SDK, not its author. Runner: Claude Opus subagent
(requested in the task capsule; the model is not re-derived here from harness metadata).
Date: 2026-11-06. Concord library frozen at `e04236ca`; SDK author commit `d66eb228`.
The SDK's `FRICTION.md`, `src/` and existing tests were not opened unless an intervention below
says so.

Command: `cargo test --locked -p concord-sdk-frankfurter --test first_use`.

## Predictions (written before the matching test code)

### Outcome A: latest pair rate (written before any test code)

Sources at this point: SDK `README.md`, `PROVENANCE.md` (linked from the README), SDK rustdoc
(`cargo doc -p concord-sdk-frankfurter --no-deps`), `docs/CALLER_GUIDE.md`, the Concord facade
rustdoc for `Error`, `ErrorKind` and `Operation`, the public frankfurter.dev v2 and v1 pages and
the OpenAPI document linked from them (documentation only, no rate endpoint was called).

- **Call:** `Frankfurter::new(reqwest::Client::new()).with_base_url(server.url())?` then
  `api.latest_pair("EUR", "USD").fetch().await`. `with_base_url` returns `concord::Error`, not the
  SDK `Error` (rustdoc shows both as plain `Error`; the caller guide disambiguates).
- **Request:** exactly one `GET /v2/rate/EUR/USD`, no query, empty body (`body_len == 0`, not
  chunked, no `content-type`), one `accept: application/json` (the `Json` format's suggestion,
  which displaces reqwest's `*/*`), no `authorization`/`cookie`/API-key header. Currency text is
  sent as supplied: `"eur"` stays lower-case, `"A/B"` becomes the single segment `A%2FB`.
  `set_providers("ECB,TCMB")` adds one query pair `providers` whose decoded value is `ECB,TCMB`
  (raw spelling probably `ECB%2CTCMB`).
- **Return value:** `RateResponse { rate: Rate { date: 2026-03-25, base: "EUR", quote: "USD",
  rate: Decimal("1.1568") }, meta }` for the OpenAPI 200 example, `meta.status == 200`,
  `meta.headers` holding the fake's headers, `meta.operation` named `Frankfurter.latest_pair`
  with method `GET`. The decimal compares equal to `"1.1568".parse::<Decimal>()`, and a
  30-significant-digit token survives exactly (no `f64`). An integer `1` (the documented identity
  rate for a same-currency pair) decodes. Unknown fields, including the schema's optional
  `providers` array, are ignored.
- **Completion:** success only after the whole JSON object is read. A body cut off after a
  prefix gives `Error::Concord` with `source.kind() == Transport` (possibly `Decode`) and
  `meta` = the real 200 head; no partial rate exists. A 200 with a non-JSON `content-type`
  is `Error::Concord`, kind `Metadata`, meta present. Missing `rate`, a quoted rate, zero,
  a negative number or trailing bytes after the object are kind `Decode`, meta present.
- **Failure signalling:** every non-2xx is `Error::Provider(Box<ProviderError>)`:
  - documented 422 `{"status":422,"message":"invalid currency: ABC"}` gives
    `meta.status == 422` and `document == Ok(Problem { status: Some(422), message })`;
  - OpenAPI-style 404/503 bodies with only `message` give `Problem { status: None, .. }`;
    a 503 `Retry-After` header is readable from `meta.headers`;
  - an HTML error page gives `document == Err(ErrorBodyError::Metadata(_))`, invalid JSON
    gives `Err(ErrorBodyError::Decode(_))`, a body over 16 KiB or a cut-off error body gives
    `Err(ErrorBodyError::Read(_))`; the status is kept in every case.
  - The connection closing before any head gives `Error::Concord`, kind `Transport`,
    `meta == None`, `status() == None`, `operation()` still `Some`.
  - An empty currency is rejected before sending: `Error::Concord`, kind `Build`, no request
    on the wire.
  - `Display`/`Debug` of the SDK error do not contain the provider's `message` text.

**A checked against the prediction:** the first build of the outcome-A tests compiled without
a diagnostic and all 14 passed on the first run. Every prediction above held, including the
raw query spelling `providers=ECB%2CTCMB` and kind `Transport` (not `Decode`) for a success
body cut off after a prefix. No SDK source or test was opened.

### Outcome B: historical pair rate (written before the outcome-B test code)

- **Call:** `let date: Date = "2024-01-15".parse()?;` then
  `api.historical_pair("EUR", "USD", date).fetch().await`. `date` is required and positional;
  `set_date` replaces it on an existing request.
- **Request:** one `GET /v2/rate/EUR/USD?date=2024-01-15` (query pair `date`, value
  `YYYY-MM-DD`), same headers and empty body as A. With the filter, the rustdoc's order
  ("Query (when present): date, providers") gives `?date=2024-01-15&providers=ECB`.
  `prepare()` shows the same URL without any I/O.
- **Return value:** `RateResponse` whose `rate.date` is the provider's observation date, not
  the requested one; `meta.operation` is `Frankfurter.historical_pair` / `GET`. For a Saturday
  request (`2024-01-13`) the fake answers with a `2024-01-12` row (synthetic; the pages I read do
  not document weekend behaviour) and the SDK hands back `2024-01-12` untouched.
- **Invalid dates:** `"2024-02-30".parse::<Date>()` fails in the application; no request can
  carry an impossible calendar date.
- **Completion and failure:** as in A. A date the fake has no row for is a 404 with a
  message-only `Problem` (`status: None`); an unknown currency is the documented 422; a
  cut-off body is `Error::Concord` with kind `Transport` and the 200 head kept.
- **Reuse:** a request value is `Clone`; fetching a clone sends a second identical request.
  A and B can run concurrently on one cloned `Frankfurter` handle.

**B checked against the prediction:** one compiler diagnostic in my own test code (E0369,
below); after the fix all 8 outcome-B tests passed on the first run and every prediction held,
including the query order `date=2024-01-15&providers=ECB`.

### Two extra checks (predicted before writing them, after A and B passed)

- **Generated `send()` beside `fetch()`:** on the documented 422, `send()` returns
  `concord::Error` with kind `Status`, `status() == Some(422)`, and the unread body from
  `into_response()`; it never yields `Error::Provider`. On 200 it returns the same `Rate` as
  `fetch()`.
- **Base URL as configuration:** an application helper returning the SDK `Error` can use `?` on
  `with_base_url` (the rustdoc lists `From<concord::Error>` for the SDK error). A base URL with a
  query is refused as `Error::Concord`, kind `Build`, no metadata, no operation identity.

**Extra checks against the prediction:** both held. The only failure on the way was my own
test's expectation: I compared the fake's formatted 422 body with the fixture file, which has a
final LF. I fixed the expected bytes; the SDK's behaviour was as predicted.

## Trace (in order)

1. Opened the capsule, then `Cargo.toml` and a file listing of `examples/frankfurter-sdk/`. The
   listing shows `FRICTION.md`, `src/` and `tests/`; none of them was opened.
2. **First entry point:** `examples/frankfurter-sdk/README.md`, then `PROVENANCE.md`, which it
   links. The README points only to `PROVENANCE.md`, not to fixture files, so I did not reuse
   the SDK's fixtures. I wrote my own under `fixtures/first-use/` (provenance below).
3. SDK rustdoc (`cargo doc --locked -p concord-sdk-frankfurter --no-deps`), read as text: the
   crate page, `Frankfurter`, `FrankfurterLatestPair`, `FrankfurterHistoricalPair`, `Rate`,
   `RateResponse`, `ResponseMeta`, `Error`, `ProviderError`, `Problem`, `ErrorBodyError`,
   `API_VERSION`, `ERROR_BODY_LIMIT`.
4. Provider documentation, read through a fetch tool that summarizes pages: frankfurter.dev (v2
   Rate, Errors, "no API key"), frankfurter.dev/v1 (deprecated v1 routes, which I used to make
   the v1-shaped negative case), and the OpenAPI document linked from them
   (`api.frankfurter.dev/v2/openapi.json`, `info.version` 2.1.1: the `getRate` 200 example, the
   `Rate` schema, the `date`/`providers` parameters, the `NotFound`/`UnprocessableEntity`/
   `ServiceUnavailable` responses and `Retry-After`). That was a documentation download; no rate
   endpoint was called.
5. Concord documentation: `docs/CALLER_GUIDE.md` sections 1 to 6 (it settled that `with_base_url`
   returns `concord::Error`; it also has the `ErrorKind` table), and the facade rustdoc
   (`cargo doc --locked -p concord --no-deps --features csv,jsonl`) for `Operation`, `Error` and
   `ErrorKind`.
6. Public API of `examples/testserver/src/lib.rs` (`Recorded`, `Reply`, `Body`, `Script`,
   `Cleanup`). This is the dev-dependency fake-server helper, not SDK code.
7. Wrote the outcome-A prediction, seven fixtures and 14 tests. They compiled with **no
   diagnostic** and 14 of 14 passed. A later raw-query assertion (`providers=ECB%2CTCMB`) also
   passed.
8. Wrote the outcome-B prediction, two fixtures and 8 tests. **Diagnostic E0369**: `==` cannot
   be applied to `Result<Option<u16>, &ErrorBodyError>`, because `ErrorBodyError` does not
   implement `PartialEq`. Rustc's note printed the definition line of the SDK's `src/error.rs`
   (`pub enum ErrorBodyError {`). I did not open that file. I fixed the test by comparing
   `.ok()`, and 22 of 22 passed.
9. Wrote the two extra checks. One assertion failed because of my own fixture/LF mix-up
   (described above); after the fix, 24 of 24 passed.
10. Ran `rustfmt --edition 2024` on `tests/first_use.rs`. `cargo clippy --locked -p
    concord-sdk-frankfurter --test first_use` gave no warnings. Three more full runs gave 24 of
    24 each time.

**Interventions:** none. I did not read the SDK's `src/` or existing tests, and did not open
`FRICTION.md`. The only SDK source text I saw was the single line quoted by rustc in step 8. I
used no private API and no native escape. `send()`, `prepare()?.request()` and
`concord::Error::into_response()` are documented public calls, and I used them on purpose to
check the README's claims.

**Working path:** `Frankfurter::new(reqwest_client).with_base_url(origin)?`, then
`api.latest_pair(base, quote)` or `api.historical_pair(base, quote, date)`, optionally
`.set_providers(..)`, then `.fetch().await` gives `RateResponse { rate, meta }`. Failures are
handled with a `match`: `Error::Provider(p)` (read `p.meta.status`, `p.meta.headers`,
`p.document`) or `Error::Concord { source, meta }` (read `source.kind()`, plus `meta` when a
head arrived).

## Friction (concrete)

Each item is labelled observed (it happened in this run) or inferred.

1. **Two different types are both shown as plain `Error` in the SDK rustdoc** (observed).
   `with_base_url`, `prepare`, `send`, `Error::Concord { source }` and `ErrorBodyError::Read`
   show an unlinked `Error`, which means `concord::Error`. `fetch` and the request types show
   the linked SDK `Error`. With `--no-deps` the only way to tell them apart is the missing
   link. I resolved `with_base_url` from `docs/CALLER_GUIDE.md`. Reproduce:
   `cargo doc -p concord-sdk-frankfurter --no-deps`, then read `Frankfurter::with_base_url` and
   `ErrorBodyError::Read` in the generated pages.
2. **Classifying an `Error::Concord` needs the `concord` crate itself** (observed in code). To
   tell a cut-off or stalled body (`Transport`) from a refused media type (`Metadata`), a bad
   document (`Decode`) or an unbuildable request (`Build`), I had to destructure
   `Error::Concord { source, .. }` and compare `source.kind()` with `concord::ErrorKind`. The
   SDK error offers `status()`, `meta()` and `operation()`, but has no kind accessor and does not
   re-export `ErrorKind`. Inferred: an external application has to add a direct,
   version-matched `concord` dependency for this and for `concord::reqwest` (used by the README
   example). Here `concord` was already available as the SDK's own dependency, so I did not see
   that `Cargo.toml` step. Reproduce: the `concord_error` helper and
   `a_success_body_cut_off_yields_no_rate`.
3. **Two terminal methods with different error contracts** (observed). Each request type has
   both the generated `send()` (`concord::Error`, where a non-2xx is kind `Status` and the body
   stays unread) and the SDK's `fetch()` (typed `Error::Provider` with `Problem`). Both compile
   in the same position. Picking `send()` silently drops the typed error document. The README
   says so, and `a_generated_send_skips_the_typed_error_adapter` confirms it.
4. **The generated rustdoc wording for the required `date` says "when present"** (observed).
   `historical_pair` shows "Query (when present): date, providers", although `date` is a
   required argument and is always sent. The block looks like Concord-generated text (inferred).
   It contradicts the argument list a few lines below ("date: required value").
5. **Gaps in the provider documentation that the fake had to fill** (observed). The website has
   no 200 example and no `date` example for the pair route, so I used the OpenAPI document.
   Weekend and holiday behaviour is not documented in the pages I read. The fake's
   Saturday-gives-Friday row is synthetic. The SDK's "inspect the returned date" guidance is
   the only statement covering it.
6. Minor (observed): `ResponseMeta::operation` is `Option<Operation>` even after a successful
   `fetch`, so asserting the name needs `.expect`. `ErrorBodyError` has no `PartialEq` (E0369
   above), which costs a `.ok()` in assertions.
7. Test helper, not the SDK (observed): `concord_example_testserver::Body::{FailAfter, Chunks}`
   take `bytes::Bytes`, which is neither re-exported nor a dev-dependency of this SDK. I built
   the chunks from a `Vec<u8>` with `.into()` and let the expected type infer `Bytes`.

## Fixture provenance (`fixtures/first-use/`, authored 2026-11-06, UTF-8 + final LF)

All files are hand-authored. None is captured HTTP response bytes. `fixtures/** -text` in the
existing `.gitattributes` covers this directory (`git check-attr` reports `text: unset`).

| File | Source | Status |
| --- | --- | --- |
| `latest-eur-usd.json` | OpenAPI 2.1.1 `getRate` 200 example (`2026-03-25`, EUR/USD, `1.1568`) | Values transcribed and minified. A documented sample, not a live rate. |
| `identity-eur-eur.json` | OpenAPI `getRate` description: a same-currency pair returns the identity rate 1 | Date reused from the example; the rest is shaped to the `Rate` schema. Synthetic row. |
| `precise-eur-jpy.json` | `Rate.rate` is a JSON number with `exclusiveMinimum: 0` | Synthetic 30-significant-digit value for the exact-decimal check. |
| `expanded-eur-usd.json` | `Rate.providers` (optional, `expand=providers`), items `key`/`date`/`rate`/`excluded` | Synthetic values shaped to the schema. Used to check that unknown and optional fields are ignored. |
| `invalid-currency-422.json` | frankfurter.dev v2 "Errors" example | Transcribed and minified. |
| `not-found-404.json` | OpenAPI `responses.NotFound` (body with only `message`) | Message text is synthetic and marked `first-use synthetic`. |
| `unavailable-503.json` | OpenAPI `responses.ServiceUnavailable` (body with only `message`, `Retry-After` header) | Message text and the `Retry-After: 7` the test sends are synthetic. |
| `historical-eur-usd-2024-01-15.json` | `Rate` schema; `date` example `2024-01-15` from `components.parameters.date` | Rate value `1.0942` is invented. |
| `historical-eur-usd-2024-01-12.json` | `Rate` schema | Invented row (`1.0950`) that the fake returns for weekend requests. The scenario is synthetic. |

The rest of the test data is inline in `tests/first_use.rs` and synthetic: the invalid success
documents, the v1-shaped envelope (inspired by v1's `rates[to]` access, not a documented v1
example), the HTML 502 body, the broken and oversized error documents, the cut-off and stalled
bodies, and the dropped connection.

## Result

`cargo test --locked -p concord-sdk-frankfurter --test first_use`: 24 passed, 0 failed
(14 for outcome A, 8 for outcome B, 2 extra checks).

## Between waves 2-3 (coordinator integration)

The client now declares `send SendPolicy;`: the generated `send()` returns
`Result<RateResponse, Error>` and the `fetch()` methods described above were removed. The
tests in `tests/first_use.rs` call `.send()`; the Concord-only check now uses
`prepare()?.send()`. Details and each test change: [INTEGRATION.md](INTEGRATION.md).
