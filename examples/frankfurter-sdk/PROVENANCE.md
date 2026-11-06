# Provider and fixture provenance

Access date: **2026-11-06**. Route family: **Frankfurter v2**. The official linked
[OpenAPI document](https://api.frankfurter.dev/v2/openapi.json) reported OpenAPI `3.1.2`
and `info.version = "2.1.1"`. SHA-256 of the downloaded document bytes:
`2f3e6a4012b8461401e462237ef4f5122fd6cf71b766db948fe86fd696745fe4`.
The documentation download was not a rate API call. No provider rate/resource endpoint
was called, and no credentials were obtained or used.

Official pages rechecked:

- [Frankfurter v2](https://frankfurter.dev/), **Rate**, **Filtering by Provider**, **Errors**
  and the no-API-key introduction: single-pair path, optional date/providers, JSON error
  example and HTTP error descriptions. Page has no separate release identifier; paired
  with the linked OpenAPI version above.
- [Historical v1 documentation](https://frankfurter.dev/v1/), **API Endpoints** and
  **Currency Conversion**: reviewed to distinguish v1 from v2. No v1 route is implemented.
- [OpenAPI 2.1.1](https://api.frankfurter.dev/v2/openapi.json),
  `paths./rate/{base}/{quote}.get`, `components.schemas.Rate`, `components.parameters.date`,
  `components.parameters.providers`, and `components.responses`: response fields, positive
  numeric rate, date spelling, optional filter and error shapes. The pair endpoint has
  a single object, without a page/cursor contract.

All fixture files below were authored locally as UTF-8 with a final LF. Documented
examples were transcribed/minified; these are **not captured HTTP response bytes**.
Every row uses the access date above. The local `.gitattributes` marks `fixtures/** -text`.

| Fixture under `fixtures/` | Official source / version / section | Transformation and status |
| --- | --- | --- |
| `latest.json` | OpenAPI 2.1.1, `/rate/{base}/{quote}` → 200 → JSON example | Copied field values from the documented `2026-03-25` EUR/USD `1.1568` object; minified plus LF. A documented sample, not a current live rate. |
| `historical.json` | OpenAPI 2.1.1, `/providers/{provider}/rates` → 200 → JSON example | Extracted the `2024-01-15` EUR/USD `1.089` object from the documented array and minified it. Serving this object from the pair endpoint is a synthetic response scenario. |
| `extended.json` | v2 Rate; OpenAPI 2.1.1 `Rate` schema | Based on `latest.json`, with invented `future_metadata` and `future_flag` fields. Synthetic compatibility perturbation; no claim those fields exist. |
| `provider-attribution.json` | OpenAPI 2.1.1 `components.schemas.Rate.properties.providers`, rechecked 2026-11-06 | Hand-authored from the documented nested `key`/`date`/`rate`/`excluded` shape, retaining the documented latest example's top-level values. All attribution values are invented, including the long decimal and dates that differ from the top-level date. Serving attribution from the pair endpoint is a synthetic forward-compatibility scenario: its operation does not document `expand`. Not captured response bytes. |
| `invalid-currency.json` | v2 website, Errors | Transcribed/minified the documented 422 example, retaining its status and message. |
| `not-found.json` | OpenAPI 2.1.1, `responses.NotFound` | Hand-authored message-only error matching the documented schema; message text is synthetic. |
| `bad-request.json` | v2 website, Errors (400) | Hand-authored date-error message/status following the website's envelope; synthetic text. |
| `unavailable.json` | OpenAPI 2.1.1, `responses.ServiceUnavailable` | Hand-authored message-only error. Synthetic text and Retry-After value used by tests. |
| `status-mismatch.json` | v2 website, Errors envelope | Deliberately inconsistent body status for an HTTP 422. Entire mismatch scenario is synthetic. |
| `malformed.json` | OpenAPI 2.1.1 `Rate` schema | Removed the number and closing brace from an otherwise shaped observation. Synthetic malformed JSON, also reused as an invalid error document. |
| `v1-shape.json` | v1 Currency Conversion accesses `data.rates[to]`; OpenAPI 2.1.1 `Rate` defines the accepted v2 object | Invented v1-like legacy envelope for rejecting the wrong shape. Neither `amount` nor the complete bytes are claimed as a documented v1 example. |
| `decimal-cases.json` | OpenAPI 2.1.1 `Rate.rate`, positive JSON number | Synthetic table of literal wire tokens and exact expected values: decimal fractions, long mantissa, integer beyond 2^53, trailing zeroes, exponents and values outside f64 range. `tests/protocol.rs::rate_document` inserts `wire` verbatim into the rate field; it does not serialize through a float. |
| `invalid-rates.json` | OpenAPI 2.1.1 `Rate.rate`, positive JSON number | Synthetic invalid tokens, wrong JSON types, nonpositive numbers and exponent overflow; same verbatim insertion. Array strings hold test fragments, not accepted wire strings. |

Other synthetic transformations in `tests/protocol.rs`: empty/object/array/null responses,
invalid calendar date, duplicate rate field, trailing JSON, missing/wrong/repeated media
types, HTML error, exact 16 KiB and over-limit error messages, broken chunked transfers,
connection closed before headers, and synthetic header/message redaction markers.
Headers `x-fixture-id`, `etag`, `set-cookie`, and test User-Agent values are invented.
HTTP 429/502 tests exercise generic failure handling; they do not claim a documented pair
endpoint example. `Retry-After` exists in the 503 schema; all tested values are invented.

The historical workflow's request for 2024-01-16 returning a 2024-01-15 observation is a
synthetic check that the returned date is preserved. It does not verify provider fallback
or calendar availability. Filtered and unfiltered test responses are intentionally equal;
they test request compatibility, not provider blending or accuracy.

Maintenance tests also derive two synthetic invalid responses from
`provider-attribution.json`: quoting the top-level numeric rate, and truncating the
unknown attribution's boolean literal from `true` to `tru`. Neither is a provider example.

## Implementation sources and dependencies

Concord baseline: `e04236ca`, left unchanged. Read `README.md`, the author/caller guides,
cheat sheet and recipes under `docs/`, relevant crate READMEs, and generated the facade
rustdoc with `cargo doc -p concord --no-deps --features csv,jsonl` using the required
build environment. `Received`, `Endpoint`, `Json`, `Decode`, `Operation` and `bytes_limited`
are public facade APIs. No private reach-through or vendor SDK is used.

Existing example influence is explicit: `examples/github-sdk/src/error.rs` and
`context.rs` supplied response/error-redaction reference patterns; the finite
`examples/nager-holidays-sdk/src/lib.rs` and `error.rs`, plus `docs/RECIPES.md`'s response
metadata recipe, supplied the receive/capture/decode pattern. Frankfurter's declaration,
decimal decoding, typed bounded error-document adapter and fixtures were authored here.
No implementation was copied from a Frankfurter vendor client.

- `bigdecimal` 0.4.11: mature exact decimal parser/representation, avoiding handwritten
  numeric parsing and binary floats. Checked its [crate documentation](https://docs.rs/bigdecimal/0.4.11/bigdecimal/)
  on the access date. New lockfile packages: `bigdecimal`, `num-bigint`, `libm`.
- `serde` with derive and `serde_json` with `raw_value`: typed JSON plus preserved numeric
  tokens. [RawValue documentation](https://docs.rs/serde_json/1.0.154/serde_json/value/struct.RawValue.html)
  checked on the access date. Existing workspace dependencies; only `raw_value` is newly
  required by this SDK. No global arbitrary-precision JSON feature is needed.
- `chrono` 0.4.45, `std`/`serde`, no clock: existing workspace lock entry, reused for
  calendar-date parsing/formatting instead of a handwritten date parser.
- `concord`: facade only, existing workspace library; owns request construction/execution.
- Dev dependencies: existing `examples/testserver` by path and `tokio` macros/runtime/time
  for deterministic loopback tests. No pagination/streaming crate is necessary for one JSON object.
