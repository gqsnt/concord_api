# FIRST_USE — fresh consumer of SDK47 Kraken Spot REST

Role: fresh consumer (did not write this SDK). Date: 2026-11-07.
Live provider calls: **NOT_RUN**. Every call below runs against `examples/testserver` on loopback.

## Sources read before the first working call

- `examples/kraken-spot-rest-sdk/README.md` (whole file).
- `examples/kraken-spot-rest-sdk/Cargo.toml` (dependency list, to know which dev-dependencies exist).
- Generated rustdoc: `cargo doc --locked --offline -p concord-sdk-kraken-spot-rest --no-deps` — every page under
  `target/doc/concord_sdk_kraken_spot_rest/` (crate index, client/scope/endpoint structs, `Policy`, `Nonces`,
  `Credentials`, `Signed`, `Response`, models, options, errors, constants), converted to text with a local helper.
- Concord public rustdoc (`cargo doc -p concord -p concord-example-testserver --no-deps`): `concord` crate index,
  `ResponseMeta`, `Error`, `ErrorKind`, `Operation`, `Prepared`, `Sends`, `IntoValue`; the test server crate index,
  `TestServer`, `Recorded`, `Reply`, `Gate`, `Script`.
- Official Kraken documentation, read 2026-11-07 (read-only, via a page fetch):
  - https://docs.kraken.com/exchange/guides/rest/authentication — headers `API-Key`/`API-Sign`; `nonce` is an
    increasing unsigned 64-bit integer per key and cannot be reset lower; out-of-order arrival can cause
    `EAPI:Invalid nonce`; signature = Base64(HMAC-SHA512(Base64-decoded secret, URI path || SHA256(nonce || POST
    data))); worked example with secret `kQH5HW/8…XuXg==`, nonce `1616492376594`, payload
    `nonce=1616492376594&ordertype=limit&pair=XBTUSD&price=37500&type=buy&volume=1.25`, path `/0/private/AddOrder`,
    expected `API-Sign` `4/dpxb3iT4tp/…ndwLUQ==`.
  - https://docs.kraken.com/api/docs/rest-api/get-account-balance/ — `POST /0/private/Balance`, optional query
    `account_id` (example `WX6V-JUKW-KKPB-QE36`), body `nonce` + optional `rebase_multiplier`
    (`rebased`|`base`), page shows `application/json` body; result is a map asset → decimal string; example
    response values (`XXBT: 1011.1908877900`, `ETH2.S`, `USD.M`, …).
  - https://docs.kraken.com/api/docs/rest-api/get-open-orders/ — `POST /0/private/OpenOrders`, optional query
    `account_id`, body `nonce`, `trades`, `userref`, `cl_ord_id`, `consolidate_taker`, `with_cursor`, `limit`,
    `cursor`, `rebase_multiplier`; result `open` keyed by txid plus `cursor` only in cursor mode; example orders
    `OQCLML-BW3P3-BUCMWZ` and `OB5VMB-B4U2U-DK2WRW`.

Not read: the SDK's `src/`, `tests/`, `fixtures/`, `FRICTION.md`, `PROVENANCE.md`. Public-market fixture data in my
tests (AssetPairs, Ticker) is authored from general knowledge of the public Kraken API shape, not from any authorized
page; the authorized pages do not document those two endpoints.

## Predictions

Written before any test code. Setup I expect to need for every workflow:

```rust
let credentials = Credentials::new(KEY, SECRET_B64)?;              // LocalError on bad key/secret
let policy = Policy::private(credentials, Nonces::new(last_used));  // or Policy::public()
let api = Kraken::new(transport()?, KrakenConfig { policy }).with_base_url(server.url())?;
```

### Workflow 1 — public market data, select a trading pair

Calls (ordinary path is the helper `api.select_market("XBT/USD")`; generated endpoints for options / partial results):

| Call | Wire | Returns |
|---|---|---|
| `api.public().pairs().send()` | `GET /0/public/AssetPairs`, no query, no body; `accept: application/json`; no `content-type`, no `api-key`, no `api-sign` | `Response<Pairs>`: `value` is `BTreeMap<canonical id, Pair>`, `meta.status() == 200`, `meta.operation()` named `Kraken.public.pairs` |
| `api.public().pairs().set_pair("XBTUSD,ETHUSD")` | same path, query `pair=XBTUSD%2CETHUSD` (I expect the comma to be percent-encoded; decoded value `XBTUSD,ETHUSD`) | as above, restricted by the provider |
| `api.public().ticker("XXBTZUSD").send()` | `GET /0/public/Ticker?pair=XXBTZUSD`, `accept: application/json`, no auth headers | `Response<Tickers>`: map with exactly one key |
| `api.select_market("XBT/USD")` | the two GETs above in order; the ticker query uses the **canonical** id (`XXBTZUSD`) even when the selector was the display name or altname | `MarketSelection { id: "XXBTZUSD", pair, ticker, pairs_meta, ticker_meta }` |

Public requests carry no credential headers even when the client holds a private policy, and allocate no nonce
(the next private call still uses `last_used + 1`).

Failures:
- Selector with zero matches, two matches (e.g. a selector that is one pair's altname and another's display name),
  or a single match whose `status` is not `online` (e.g. `cancel_only`) → `Error::Local(LocalError::PairUnavailable)`;
  exactly one request (AssetPairs) on the wire, the ticker is never fetched.
- Ticker response keyed by a different pair or holding two pairs → `Error::Local(LocalError::TickerMismatch)` after
  two requests.
- 200 with `{"error":["EQuery:Unknown asset pair"]}` → `Error::Api`; `classes()` — I predict `[ApiClass::Other]`
  (I guess "invalid arguments" is `EGeneral:Invalid arguments`); `errors_unredacted()` returns the raw string;
  `Display`/`Debug` do not contain it.
- 503 with JSON `{"error":["EService:Unavailable"]}` and `Retry-After: 2` → `Error::Http`, `status() == 503`,
  `document` is `Ok` with that string, `retry_after(now) == Ok(Some(2s))`; one request, no retry.
- 200 with `content-type: text/html` → `Error::Concord` with `ErrorKind::Metadata`; `status()` is `Some(200)`.
- 200 JSON but malformed (`{"error":[],"result":`) → `Error::Concord` with `ErrorKind::Decode`.
- 200 `{"error":[],"result":null}` or no `result` → `Error::MissingResult`.
- Pagination end: none — both endpoints are one finite document; nothing to page.
- Partial results: `select_market` returns only `Err` when the ticker step fails; the already-fetched pair listing
  is lost. A caller who wants it must call `pairs()` and `ticker()` separately.

### Workflow 2 — authenticate; balances and open orders with signed private requests

| Call | Wire | Returns |
|---|---|---|
| `api.account().balances(BalanceOptions::default()).send()` | `POST /0/private/Balance`, no query; headers `api-key: <key>`, `api-sign: <88-char Base64>`, `content-type: application/x-www-form-urlencoded`, `accept: application/json`; body exactly `nonce=<last_used+1>` | `Response<Balances>` (`BTreeMap<String, String>`, e.g. `"XXBT" → "1011.1908877900"`, `"ETH2.S"`, `"USD.M"` kept verbatim) |
| `api.account().open_orders(opts).send()` with `opts.trades = Some(true)`, `opts.cl_ord_id = Some(..)` | `POST /0/private/OpenOrders`; body `nonce=<n>&trades=true&cl_ord_id=<id>` (nonce first, then the set options in struct field order, absent options omitted) | `Response<OpenOrders>`; `open` keyed by txid; wire `descr.type` appears as `descr.side`; unknown provider fields (`time_in_force`, `margin`, `descr.aclass`) are ignored |
| `.set_account_id("WX6V-JUKW-KKPB-QE36")` on either | query `?account_id=WX6V-JUKW-KKPB-QE36`; signature still computed over the bare path `/0/private/Balance` | same |
| `api.account_snapshot()` | Balance then OpenOrders, default options, nonces `last_used+1` then `last_used+2` | `AccountSnapshot { balances, orders }` |

`API-Sign` = Base64(HMAC-SHA512(Base64-decode(secret), path || SHA256(decimal nonce || body))). I will recompute it in
the test from the recorded body with my own HMAC code and compare. `Credentials::signature` must reproduce the
official authentication-guide vector.

Failures:
- `Credentials::new("", …)` → `LocalError::InvalidKey`; secret that is not standard padded Base64 →
  `LocalError::InvalidSecret`.
- `Policy::public()` and a private call → `Error::Local(LocalError::MissingCredentials)`, zero requests sent.
- `Nonces::new(u64::MAX)` → `Error::Local(LocalError::NonceExhausted)`, zero requests sent.
- 200 with `{"error":["EAPI:Invalid key"]}` → `Error::Api`, `[ApiClass::Authentication]`; `EAPI:Invalid nonce` →
  `[InvalidNonce]`; `EGeneral:Permission denied` → `[Permission]`; `EAPI:Rate limit exceeded` → `[RateLimited]`.
  Exactly one request each (never retried).
- Concurrency: N concurrent sends through clones sharing one `Nonces` → N distinct nonces covering
  `last_used+1 ..= last_used+N`; the server may see them in any order. A strict provider that rejects a nonce
  not greater than the highest it has seen will reject some late-arriving lower nonces with `EAPI:Invalid nonce`;
  those surface as `Error::Api` classified `InvalidNonce`, with no retry. I will not assert arrival order.
- Controlled reversal (`policy.prepare_signed(...)` twice, send the second first) → the first-allocated request
  fails with `InvalidNonce`.
- Partial results: `account_snapshot` returns `Err` if open orders fail; the successful balances are dropped. To
  keep them, call `balances()` and `open_orders()` separately.
- Pagination end: no cursor or limit is sent; all orders arrive in one document. A `cursor` key in the result is
  ignored (not an error). An empty `open: {}` is a successful empty map.
- Secrets: `Debug`/`Display` of `Credentials`, `Policy`, `Kraken`, `KrakenConfig`, options with `otp`, `Signed` and
  `Error` never contain the key, the secret, the OTP, the `API-Sign` value or provider error text.

## First working call

Written right after the predictions, from the README example and the rustdoc of `Kraken`, `KrakenConfig`,
`Policy` and `transport`; it compiled on the first attempt and passed (0.5 s build, cached dependencies):

```rust
use concord_sdk_kraken_spot_rest::{Kraken, KrakenConfig, Policy, transport};
use concord_testserver::{Recorded, Reply, TestServer};

let server = TestServer::start(|_r: &Recorded| Reply::json(200, PAIRS)).await;
let api = Kraken::new(transport()?, KrakenConfig { policy: Policy::public() })
    .with_base_url(server.url())?;
let pairs = api.public().pairs().send().await?;
assert_eq!(pairs.value["XXBTZUSD"].altname, "XBTUSD");
```

Observed wire: `GET /0/public/AssetPairs`, headers `accept: application/json` and `host` only, empty body.
What it took: four SDK imports, an explicit `Policy::public()` inside `KrakenConfig`, and knowing that `send()`
yields `Response<T>` with `.value` (README table; the endpoint rustdoc names `Envelope<Pairs>`, see F2). No source
lookup. That smoke test was then replaced by the full application below.

## The application (`tests/first_use.rs`, 11 tests)

| Test | Workflow | What it proves |
|---|---|---|
| `workflow1_select_market_then_quote` | 1 | `select_market` by display name, altname and canonical id; wire of both GETs; canonical ticker query; app computes the spread; `set_pair` encoding; public calls send no credentials and spend no nonce |
| `workflow1_selection_failures_stop_before_the_ticker` | 1 | absent, offline (`cancel_only`), ambiguous, wrong-case and empty selectors → `PairUnavailable` after exactly one request |
| `workflow1_ticker_failures_and_partial_results` | 1 | `TickerMismatch`; the app keeps the pair listing by calling the generated endpoints; 503 + Retry-After, `EQuery` envelope error, HTML 200, truncated JSON, null result, HTML 502; recovery; one request per call |
| `workflow2_account_snapshot_with_signed_requests` | 2 | `account_snapshot` values and order; independent HMAC check of every `API-Sign`; exact form bodies; options + `account_id` query (signature over the bare path); diagnostics withhold key, secret, OTP, signature |
| `workflow2_local_refusals_send_nothing` | 2 | invalid key/secret; public policy; nonce exhaustion (`u64::MAX` is the last usable value); path-prefixed base URL; nothing dispatched |
| `workflow2_provider_errors_and_partial_snapshot` | 2 | helper fails on the second read; app keeps balances by calling endpoints; seven provider strings classified; empty `open`, stray `cursor`; 13 sequential nonces |
| `concurrent_sends_sharing_a_nonce_source_never_repeat_a_nonce` | 2 | 48 concurrent sends through two clients sharing one `Nonces`, strict provider: 48 distinct nonces, every one signed correctly, rejected ones are `InvalidNonce`, arrival order not asserted |
| `reversed_arrival_is_rejected_as_invalid_nonce_and_not_retried` | 2 | `policy.prepare_signed` twice, delivered in reverse → the earlier nonce fails `InvalidNonce`, no replay; unsigned `prepare()` allocates nothing |
| `application_serializes_one_key_so_a_strict_provider_accepts_every_call` | 2 | the README's "serialize per key" advice implemented by the app with a `tokio::sync::Mutex`: 24/24 accepted in allocation order |
| `one_order_missing_a_modelled_field_fails_the_whole_read` | 2 | one order without `limitprice` → `Concord` `Decode` for the whole snapshot |
| `official_signature_vector_matches_sdk_and_independent_computation` | 2 | the authentication guide's vector through `Credentials::signature` and through my own HMAC code |

Results (2026-11-07, this worktree, `CARGO_TARGET_DIR=<worktree>/target`, `CARGO_BUILD_JOBS=4`, offline):
- `cargo test --locked --offline -p concord-sdk-kraken-spot-rest --test first_use` → 11 passed; the binary rerun 8 more
  times → 11 passed each time. Concurrent strict-provider runs: 10–23 of 48 accepted, 25–38 `EAPI:Invalid nonce`,
  11–21 arrival inversions; uniqueness and signature checks held in every run.
- `cargo test --locked --offline -p concord-sdk-kraken-spot-rest` (whole package) → unit 0, first_use 11, protocol 16,
  workflows 7, doctest 1: all passed. I did not read `protocol.rs` or `workflows.rs`.
- `cargo clippy --locked --offline -p concord-sdk-kraken-spot-rest --all-targets -- -D warnings` → clean (after
  fixing one `unnecessary_literal_unwrap` in my test). `rustfmt --check` on `first_use.rs` → clean.
- `Cargo.lock` / `Cargo.toml` unchanged (the package was already a workspace member). No new crate: the independent
  HMAC check uses the SDK's own `hmac`, `sha2`, `base64` dependencies, which integration tests can already use.

## Friction incidents, in order

- **F1 (README example).** The complete example loads `include_str!(…/fixtures/*.json)` from the SDK package. An
  outside application cannot reuse those paths, and as a fresh consumer I did not open them; I authored my own
  fixtures (balances and orders from the official pages, public pairs/ticker from general knowledge).
- **F2 (surprising generated signature/docs).** Each endpoint's rustdoc says "Output: `Envelope<Pairs>`, decoded from a
  2xx response…", and `send()` is `impl Future<Output = <Policy as SendWith<Envelope<Pairs>>>::Output>`. What the caller
  actually receives, `Result<Response<Pairs>, Error>` with the envelope removed, is stated only on `Policy`'s
  `SendWith` impl (`type Output = Result<Response<T>, Error>`) and in the README table. The generated text describes
  the wire type, not the caller's value. (Concord-generated docs for a client with a send policy.)
- **F3 (doc noise).** The client page leads with `validate_header_names()` (a long paragraph ending "returns
  `Ok(())`"), every scope repeats "Formats of the endpoints below…", and every endpoint lists `operation()`. None was
  needed for either workflow.
- **F4 (required config).** `Kraken::new` needs `KrakenConfig { policy }` even for public use; I had to write
  `Policy::public()` explicitly. Small, but every public-only caller pays it.
- **F5 (unneeded types for the common call).** The most common private calls take a positional, all-optional body:
  `balances(BalanceOptions::default())`, `open_orders(OpenOrdersOptions::default())`. Because the parameter is
  `impl IntoValue<…>`, `Default::default()` cannot be inferred (E0283, as Concord's `IntoValue` doc explains), so the
  option type must be imported and named. The options are `#[non_exhaustive]` with no setters: setting two fields
  takes `let mut o = OpenOrdersOptions::default(); o.trades = Some(true); o.cl_ord_id = Some(…)`.
- **F6 (split options).** `account_id` is an endpoint setter (`set_account_id`, sent as a query) while every other
  option is a body field. One call, two mechanisms. The README says so; it is still a second place to look.
- **F7 (extra import for error stages).** Matching Concord's stage needs
  `concord_sdk_kraken_spot_rest::concord::ErrorKind` through the re-export; rustdoc shows `Error::Concord(Error)` with
  two different types both named `Error`. Statuses are `reqwest::StatusCode`; I compared `.as_u16()`.
- **F8 (renamed field).** `OrderDescription.side` is the wire `descr.type`; the field doc says "Buy or sell" but not
  the wire name. I guessed right from the predictions.
- **F9 (prediction mismatch; misleading diagnostic).** With a path-prefixed base URL (`…/kraken`, a proxy mount),
  I expected public calls to follow the prefix and only signing to refuse. Observed: **every** call, public ones and
  a `Policy::public()` client included, fails locally as `Error::Local(LocalError::SigningInput)`, Display
  "Kraken local refusal: SigningInput", with nothing sent. The README does say "only the four declared paths are
  accepted", but the variant name and its doc ("A prepared request cannot be signed…") point at signing for a request
  that is never signed, and the generated `with_base_url` doc says "Declared paths are appended to its path", which
  the policy then refuses.
- **F10 (diagnostics without the call).** SDK Display for `Http`, `Api` and `Local` omits the endpoint:
  "Kraken API failure: [Permission]", "Kraken HTTP failure: 503". Concord's own errors name it
  ("POST Kraken.account.open_orders: response not decoded: …"). After `account_snapshot` fails, a log line cannot say
  which of the two reads failed; the caller must read `failure.meta.operation()` (I did, in the test).
- **F11 (one error for three causes).** `LocalError::PairUnavailable` covers no match, several matches and an offline
  match. Selection is case-sensitive (`"xbt/usd"` is refused), which the docs do not state. To tell a user
  "XLM/USD is cancel_only", the app has to fetch and search the pairs itself.
- **F12 (partial results dropped by helpers).** `select_market` and `account_snapshot` return only `Err` when their
  second step fails; the data already fetched is lost. Keeping it means calling the generated endpoints (one extra
  request in my tests). Documented ("The helper fails on either read"), but every partial-result caller pays it.
- **F13 (serialization left to the app).** Against a strict provider with no nonce window, 48 concurrent sends sharing
  a key were mostly rejected (25–38 `EAPI:Invalid nonce` per run). Uniqueness held, which is exactly what the README
  promises. It also says "serialize calls for a key" but offers nothing to do it with; I wrote a per-key
  `tokio::sync::Mutex` held across `send()` (24/24 accepted, in order).
- **F14 (strict order model).** `OpenOrder` requires `stopprice`, `limitprice`, `misc`, `oflags` (and others) as
  `String`. One order without `limitprice` fails the whole read as `Decode`, with no partial result. The official
  example omits several documented fields, so absence is plausible (not verified live). Documented fields outside the
  model (`time_in_force`, `margin`, `reduce_only`, `ext_ord_id`, `link_id`, `descr.aclass`) are dropped silently and
  cannot be read by a caller.
- **F15 (policy access).** `prepare_signed` lives on `Policy`, and `Kraken` has `set_policy` but no getter, so the
  advanced path only works if the caller kept a `Policy` clone before building the client (the README says so).
- **F16 (encoding vs endpoint pages).** The two endpoint pages (read 2026-11-07) show `application/json` bodies; the
  SDK sends form-encoded bodies following the authentication guide. The README discloses this; whether Kraken accepts
  both is NOT_RUN.
- Not SDK friction, recorded for honesty: my first full run had one failing assertion of my own (I miscounted the
  ticker calls: 10 requests, not 11). I fixed the count, not the SDK. One clippy finding in my test was fixed the same
  way.

## Predictions compared with observed behaviour

| Prediction | Observed | Verdict |
|---|---|---|
| AssetPairs: `GET /0/public/AssetPairs`, no query, `accept: application/json`, no content-type/auth, empty body | exactly that | match |
| `set_pair("XBTUSD,ETHUSD")` → `pair=XBTUSD%2CETHUSD` | target `/0/public/AssetPairs?pair=XBTUSD%2CETHUSD` | match |
| `select_market` sends AssetPairs then Ticker with the canonical id | `pair=XXBTZUSD` for `XBT/USD`, `XBTUSD`; `pair=XETHZUSD` for `ETHUSD`, `XETHZUSD` | match |
| `MarketSelection` metas named `Kraken.public.pairs` / `Kraken.public.ticker` | yes | match |
| Public calls spend no nonce | the first private call afterwards used `last_used + 1` | match |
| Absent / ambiguous / offline selector → `PairUnavailable`, one request | yes (also wrong case and empty selector) | match (case sensitivity not predicted) |
| Wrong ticker key → `TickerMismatch` after two requests | yes | match |
| `EQuery:Unknown asset pair` → `Api`, class `Other`, text hidden from Display | yes | match |
| 503 JSON + `Retry-After: 2` → `Http` 503, document kept, `Ok(Some(2s))`, no retry | yes | match |
| HTML 200 → `Concord` `Metadata`, status 200; truncated JSON → `Decode`; null result → `MissingResult` | yes | match |
| Non-2xx with unreadable body keeps status | 502 HTML → `Http` 502, `document` is `Err`, `retry_after` `Ok(None)` | match |
| Pagination: none for public endpoints | none | match |
| Path-prefixed base URL: public calls follow the prefix (prediction made while writing that test) | all calls `Local(SigningInput)`, nothing sent | **mismatch** (F9) |
| Balance: `POST /0/private/Balance`, `api-key`, 88-char `api-sign`, form content type, accept JSON, body `nonce=<last_used+1>` | exactly that; independent HMAC matches | match |
| OpenOrders body: nonce first, set options in field order, absent omitted | `nonce=…&otp=246810&trades=true&cl_ord_id=9cc788d8-…` | match |
| `account_id` sent as query; signature over the bare path | yes; the signature does not match path+query | match |
| `rebase_multiplier` value lowercase | `rebase_multiplier=base` | match |
| `descr.type` read as `side`; extra provider fields ignored | yes | match |
| `account_snapshot`: Balance then OpenOrders, nonces +1, +2 | yes | match |
| Empty key / bad secret → `InvalidKey` / `InvalidSecret` | yes; a key with a space → `InvalidKey`; unpadded Base64 → `InvalidSecret` | match |
| `Policy::public()` private call → `MissingCredentials`, nothing sent | yes | match |
| `Nonces::new(u64::MAX)` → `NonceExhausted`, nothing sent | yes; `Nonces::new(u64::MAX - 1)` first sends nonce `u64::MAX`, then refuses | match (edge detail observed) |
| Provider strings → Authentication / InvalidNonce / Permission / RateLimited, one request each | yes; also `EAPI:Invalid signature` → Authentication, `EService:Unavailable` → Service, `EGeneral:Invalid arguments` → InvalidArguments, `WGeneral:…` → Other | match |
| Concurrent sends: distinct nonces covering the range; a strict server rejects some late lower ones as `InvalidNonce`; no retry | yes; most were rejected (25–38 of 48) | match (more rejections than I expected) |
| Controlled reversal via `prepare_signed` → earlier nonce `InvalidNonce`; unsigned `prepare()` allocates nothing | yes | match |
| `account_snapshot` drops balances when open orders fail | yes | match |
| Empty `open: {}` is success; stray `cursor` ignored; no `cursor`/`limit` sent | yes | match |
| Debug/Display withhold key, secret, OTP, `API-Sign`, provider text | yes, for every value checked | match |
| (not predicted) one order missing a modelled field | whole read fails as `Decode` | new finding (F14) |

## Assistance label

`docs-only` — the SDK README, generated rustdoc (SDK, Concord, test server) and the three official Kraken pages.
I read none of the SDK's `src/`, `tests/`, `fixtures/`, `FRICTION.md` or `PROVENANCE.md`. Outside the SDK I read only
cargo registry sources of `hmac-0.13.0` (`src/lib.rs`, `README.md`), `digest-0.11.3` (`src/mac.rs`), `sha2-0.11.1`
(`src/lib.rs` exports) and `base64-0.23.1` (`src/lib.rs`, `src/engine/general_purpose/mod.rs`), to write my own
signature check. Live provider calls: NOT_RUN.
