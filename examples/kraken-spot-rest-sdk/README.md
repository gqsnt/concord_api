# Kraken Spot REST SDK47

The declaration owns the provider error document: `error crate::ErrorDocument in concord::Json;`. Native prepared/decoded
paths carry that document as `D` in `Prepared<T, D>` and `concord::Error<D>`; SDK status routing
and validation remain in the policy. Concord withholds document contents in formatting.

`concord-sdk-kraken-spot-rest` is a bounded, read-only SDK at Concord baseline
`c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`. **Live provider calls: NOT_RUN.**
All execution and verification use `examples/testserver` on loopback. This result is
**example-assisted** through Concord's guide snippets and the shared test server;
the exact consultation record is in `FRICTION.md` and `PROVENANCE.md`.

The generated hierarchy is client → scope → endpoint:

| Generated call | Wire operation | Successful `send()` value |
|---|---|---|
| `api.public().pairs()` | GET `/0/public/AssetPairs`, optional `set_pair` | `Response<Pairs>` |
| `api.public().ticker(pair)` | GET `/0/public/Ticker?pair=…` | `Response<Tickers>` |
| `api.account().balances(options)` | POST `/0/private/Balance` | `Response<Balances>` |
| `api.account().open_orders(options)` | POST `/0/private/OpenOrders` | `Response<OpenOrders>` |
| `api.account().trades_history(options)` | POST `/0/private/TradesHistory`, first page | `Response<TradesHistory>` |
| `api.trade_history(options, account_id, from)` | the same POST per page, `ofs` set by the SDK | `TradePages` (a `concord_pagination::Pages`) |

Every `send()` returns `Result<…, Error>` through the ordinary generated send
policy. Results keep response metadata and endpoint identity. Balance and order
quantities stay decimal strings; timestamps are fractional seconds. Provider
models retain identifiers and unknown status strings rather than inventing enums.

`select_market(selector)` fetches the pair listing, requires a unique online match
by canonical identifier, alternate name or display name, then fetches that pair's
ticker. `account_snapshot()` fetches default-wallet balances followed by all open
orders with two independently signed requests. These are connected workflows;
the account result is not an atomic snapshot. The helper fails on either read.

This complete example runs entirely against fixtures (also executed as a doctest):

```rust
use concord_sdk_kraken_spot_rest::{Credentials, Kraken, KrakenConfig, Nonces, Policy, transport};
use concord_testserver::{Recorded, Reply, TestServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/0/public/AssetPairs" => Reply::json(200,
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/pairs.json"))),
        "/0/public/Ticker" => Reply::json(200,
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/ticker.json"))),
        "/0/private/Balance" => Reply::json(200,
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/balances.json"))),
        "/0/private/OpenOrders" => Reply::json(200,
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/open-orders.json"))),
        _ => Reply::new(404),
    }).await;
    // Synthetic credentials, never real account credentials.
    let credentials = Credentials::new("fixture-key", "Zml4dHVyZS1zaWduaW5nLXNlY3JldA==")?;
    let policy = Policy::private(credentials, Nonces::new(5000));
    let api = Kraken::new(transport()?, KrakenConfig { policy })
        .with_base_url(server.url())?;
    let market = api.select_market("XBT/USD").await?;
    let account = api.account_snapshot().await?;
    assert_eq!(market.id, "XXBTZUSD");
    assert_eq!(account.balances.value["XXBT"], "1011.1908877900");
    assert_eq!(account.orders.value.open.len(), 1);
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
    Ok(())
}
```

Trade history

`trade_history(options, account_id, from)` walks `TradesHistory` by `ofs` until
the provider's `count` is reached. It is a `concord_pagination::Pages` of
`(trade id, Trade)` items, keyed by the next offset; nothing is sent until it is
read. Every page sends the same filters and wallet query, and each page is
signed by the shared `Policy` with a fresh nonce after the previous page
arrived. Pages of other walks or calls using the key can interleave with any
arrival order; serialize them where a strict provider requires it (see below).
A page must report `count`, hold at most 50 trades (the documented
default page size; no `limit` is sent), end within `count` and advance;
otherwise the walk stops with `Error::Page`. The cap defaults to
`TRADES_PAGE_CAP` (20 signed requests) and `max_pages` replaces it.

`collect_complete()` returns every page, or an `Incomplete` that keeps the
completed pages: `PagesError::Bounded` when the cap was reached (resume from the
first unread offset) or `PagesError::Provider` with the SDK `Error` and the
failed page (resume there, deliberately; nothing is retried). A resume point
belongs to the filters it came from. Kraken returns newest trades first, so
without a fixed `end` a trade arriving during the walk shifts later offsets and
an older trade can appear twice: merge by trade id, as below.

Use `concord_sdk_kraken_spot_rest::Resume` for the position. `Resume::Start` repeats trade
history from offset 0 with `ofs` omitted, replacing `options.ofs`; `Resume::At(key)` takes the
`u64` trade offset in a returned page's `next`.

```rust
use concord_sdk_kraken_spot_rest::{
    Credentials, Kraken, KrakenConfig, Nonces, Policy, Resume, TradeBound, TradesHistoryOptions,
    transport,
};
use concord_testserver::{Recorded, Reply, TestServer};
use std::collections::BTreeMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Synthetic provider: 60 matching trades, sliced 50 per page by the signed `ofs`.
    let server = TestServer::start(|request: &Recorded| {
        let pairs = request.form_pairs();
        let ofs = pairs.iter().find(|(k, _)| k == "ofs").map_or(0, |(_, v)| v.parse().unwrap());
        let trades: serde_json::Map<_, _> = (ofs..60.min(ofs + 50))
            .map(|i: usize| (format!("T{i:04}-SYNTH"), serde_json::json!({
                "ordertxid": "OQCLML-BW3P3-BUCMWZ", "pair": "XXBTZUSD", "time": 1688667796.0,
                "type": "buy", "ordertype": "limit", "price": "30010.00000",
                "cost": "300.10000", "fee": "0.00000", "vol": "0.01000000",
            })))
            .collect();
        let page = serde_json::json!({"error": [], "result": {"count": 60, "trades": trades}});
        Reply::json(200, page.to_string())
    }).await;
    let credentials = Credentials::new("fixture-key", "Zml4dHVyZS1zaWduaW5nLXNlY3JldA==")?;
    let policy = Policy::private(credentials, Nonces::new(5000));
    let api = Kraken::new(transport()?, KrakenConfig { policy }).with_base_url(server.url())?;
    let mut options = TradesHistoryOptions::default();
    options.pair = Some("XXBTZUSD".into());
    options.end = Some(TradeBound::Time(1_688_667_796)); // a fixed window keeps offsets stable
    let pages = api.trade_history(options, None, Resume::Start);
    let trades: BTreeMap<_, _> = match pages.collect_complete().await {
        Ok(pages) => pages.into_iter().flat_map(|page| page.items).collect(),
        // Cap or failed page: keep `incomplete.collected`, resume at `incomplete.error.resume()`.
        Err(incomplete) => return Err(incomplete.into()),
    };
    assert_eq!(trades.len(), 60);
    assert_eq!(server.requests().len(), 2); // two signed pages, two nonces
    server.shutdown().await;
    Ok(())
}
```

`TradesHistoryOptions` carries `otp`, `trade_type`, `start`/`end`
(`TradeBound::Time` seconds or `TradeBound::Trade` transaction identifier),
`consolidate_taker`, `ledgers`, `rebase_multiplier`, `aclass` and `pair`. The
offset is SDK-owned; the generated `trades_history` call reads the first page.
`count` is optional on that single page because the reference's own response
example omits it; the walk requires it.

Use `Policy::public()` for public-only access. Private methods then refuse locally.
For OTP, trade IDs, user reference, client order ID, consolidation or xStocks
representation, construct the options using `Default` and assign their public
fields before calling the endpoint. Both private endpoints offer
`set_account_id` for an optional wallet query. The convenience account helper uses
default options; call the generated endpoints for other options.

Signing and nonces

Concord's `Form` serializes the typed options once. `Policy` prepends its allocated
decimal nonce to those exact bytes, hashes `decimal nonce || encoded body`, then
HMAC-SHA512 signs `URI path || hash` with the decoded secret and Base64 encodes the
result. It sets sensitive `API-Key` and `API-Sign` headers. The query and origin
are not included in the signature. The exact path comes from the prepared URL,
a base URL's path prefix included. Whether a request is signed is decided per
declared operation (an exhaustive `match` over the crate-private generated `KrakenOperation`),
not by its URL, so a base URL with a path prefix is followed by public and signed
calls alike. Signing the exact outgoing path is right only when the verifier sees
that same path (Kraken itself, or a gateway that forwards the path unchanged). A
proxy that strips its mount (`/kraken`) before forwarding invalidates the
signature: Kraken verifies over `/0/private/...`. This SDK has no setting for that
deployment; send signed calls to a base URL without such a prefix. The form choice
follows the authentication guide's encoded-payload example; the endpoint pages
currently demonstrate JSON instead.

Share one `Nonces` source across every caller using a key. Its seed is the last
used nonce; the first allocation is seed + 1. Clones share one atomic counter;
endpoint cloning and unsigned preparation allocate nothing. Allocation at signed
preparation is unique and strictly increasing, and overflow refuses without
wrapping. Abandoned or cancelled work consumes its allocation. The application
must persist a suitable high-water mark and coordinate any other processes or
clients using that key. The SDK does not discover the provider's previous nonce.

Allocation order does **not** promise network arrival order. Concurrent requests
can be delivered out of order, and a strict provider can reject a lower arrival
with `EAPI:Invalid nonce`. Tests prove both uniqueness under 64 concurrent cloned
sends and this failure with a controlled reversed arrival. Serialize calls for
a key when arrival reordering is unacceptable; this SDK does not configure a
provider nonce window and never retries an invalid nonce.

Generated `prepare()` returns an **unsigned** `Prepared<Envelope<T>>`, with no
nonce and no policy execution. Executing it directly bypasses signing and SDK
envelope handling. Prefer generated `send()`. For inspection or controlled
scheduling, keep a clone of `Policy` and call `policy.prepare_signed(operation)`;
this allocates once and returns a noncloneable `Signed<T>` with the same bounded
SDK `send()` behavior. `request_unredacted()` is an explicit disclosure boundary.

Failure and transport semantics

| Failure | SDK result and behavior |
|---|---|
| Invalid credentials, missing auth, exhausted nonce, an endpoint whose declared name and method are not Kraken's given to `prepare_signed` | `Error::Local`, no dispatch |
| Ambiguous, absent or offline pair; missing/mismatched ticker | `Error::Local`, workflow stops |
| Invalid base URL / request preparation | Concord `Build`, no dispatch |
| Connection loss, timeout, truncated body | `Error::Concord` / `Transport`; no replay |
| Missing, repeated or wrong JSON media type or unsupported charset | Concord `Metadata` before body decoding |
| Malformed, empty, invalid UTF-8 or wrong-shaped JSON | Concord `Decode` |
| Successful body over 1 MiB | Concord `Limit`; no partial success |
| Non-2xx | `Error::Http`, authoritative status plus bounded error-document outcome |
| Valid envelope with nonempty `error` | `Error::Api`, ordered raw strings available explicitly |
| Empty errors with missing/null result | `Error::MissingResult` |
| Trade-history page without `count`, over 50 trades, beyond `count` or not advancing | `Error::Page` with the page head and offset; the walk stops, completed pages kept |

Envelope schema validation precedes SDK interpretation. A wrong-shaped result is
a decode error even if a provider also supplies an error list. Any nonempty valid
error list is treated as failure, including unknown strings and warnings. Known
classifications cover authentication, invalid nonce, permission, rate limit,
service and invalid arguments; everything else stays `Other`. Non-2xx error
documents require JSON and are bounded to 16 KiB. An unreadable, oversized,
interrupted or mislabeled error body never erases its HTTP status.

`HttpFailure::retry_after(now)` uses the shared strict parser and distinguishes
missing, invalid, repeated and overflowing fields; it accepts decimal seconds and
IMF-fixdate. It is advisory. There is no automatic retry, backoff or quota claim.
The provider quota and cross-process coordination remain caller responsibilities.

`transport()` disables proxies, redirects, referer forwarding and reqwest native
retries, and sets a ten-second request timeout. This package enables no TLS
feature because its verified transport is local HTTP. An application choosing
HTTPS must enable an appropriate reqwest TLS feature and validate that deployment
itself. A custom transport must preserve the redirect and retry restrictions:
custom API headers otherwise can leave their intended destination, and native
retries can repeat an already signed nonce. The generated `new` accepts caller
transport configuration unchanged.

Credentials, policy state, option records, generated handle Debug, signed request
Debug, response wrappers and SDK error Debug/Display withhold secrets and raw
provider error text. Raw native inspection, error accessors, provider model
fields, application logs and the runtime base URL are explicit disclosure
boundaries. Memory is not encrypted or zeroized. No real credentials are used.

Scope and evidence limits

Order placement, modification/cancellation, withdrawals, futures, WebSocket,
real-money trading and all other endpoints are excluded. OpenOrders cursor mode
and server-side `limit` are excluded: the request uses the documented default of
returning all open orders, or fails the byte bound. TradesHistory uses the `ofs`
offset walk, which the reference now marks deprecated in favour of
`with_cursor`/`cursor`; cursor mode, `limit` and `without_count` (which makes
`count` zero) are not offered. Position-closure trade fields are not modelled. Pair listing and ticker
fixtures are authored synthetic contract cases; the three authorized research
pages do not independently verify those public schemas. Fixtures establish local
request/response behavior, not live provider compatibility or provider quotas.

```text
CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-kraken-spot-rest
CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-kraken-spot-rest --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-kraken-spot-rest --no-deps
```

See `PROVENANCE.md` for exact sources, fixture derivation and validation, and
`FRICTION.md` for Concord costs and capability decisions.

Run the offline operation-effect example with `cargo run --locked --offline -p concord-sdk-kraken-spot-rest --example kraken_spot_rest_operation_effect`.
