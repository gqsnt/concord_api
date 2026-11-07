//! Fresh-consumer application tests (see FIRST_USE.md). Loopback only; live provider calls NOT_RUN.
//!
//! Workflow 1: fetch public market data and select a trading pair.
//! Workflow 2: authenticate and fetch balances and open orders with signed private requests,
//! including concurrent requests that share one nonce source.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256, Sha512};

use concord_sdk_kraken_spot_rest::concord::ErrorKind;
use concord_sdk_kraken_spot_rest::{
    ApiClass, BalanceOptions, Credentials, Error, Kraken, KrakenConfig, LocalError, Nonces,
    OpenOrdersOptions, Policy, Rebase, transport,
};
use concord_testserver::{Recorded, Reply, TestServer};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Synthetic credentials. The secret decodes to 64 ASCII bytes; neither value is a real account key.
const KEY: &str = "fresh-consumer-loopback-key-0001";
const SECRET: &str =
    "ZnJlc2gtY29uc3VtZXIgc3ludGhldGljIHNlY3JldDsgbG9vcGJhY2sgZml4dHVyZXMgb25seTsgdjAwMDEhIQ==";
/// Last nonce this key used: a millisecond timestamp (2026-11-07T09:30:00Z) persisted by the app.
const LAST_USED: u64 = 1_794_043_800_000;

/// Authored from the general shape of Kraken's public AssetPairs result (not an authorized page).
/// Extra provider fields are kept to prove the model ignores them.
const PAIRS: &str = r#"{"error":[],"result":{
 "XXBTZUSD":{"altname":"XBTUSD","wsname":"XBT/USD","aclass_base":"currency","base":"XXBT","aclass_quote":"currency","quote":"ZUSD","lot":"unit","cost_decimals":5,"pair_decimals":1,"lot_decimals":8,"lot_multiplier":1,"leverage_buy":[2,3,4,5],"leverage_sell":[2,3,4,5],"fees":[[0,0.4],[10000,0.35]],"fees_maker":[[0,0.25],[10000,0.2]],"fee_volume_currency":"ZUSD","margin_call":80,"margin_stop":40,"ordermin":"0.00005","costmin":"0.5","tick_size":"0.1","status":"online"},
 "XETHZUSD":{"altname":"ETHUSD","wsname":"ETH/USD","aclass_base":"currency","base":"XETH","aclass_quote":"currency","quote":"ZUSD","lot":"unit","cost_decimals":5,"pair_decimals":2,"lot_decimals":8,"lot_multiplier":1,"leverage_buy":[2,3],"leverage_sell":[2,3],"fees":[[0,0.4]],"fees_maker":[[0,0.25]],"fee_volume_currency":"ZUSD","margin_call":80,"margin_stop":40,"ordermin":"0.002","costmin":"0.5","tick_size":"0.01","status":"online"},
 "XXLMZUSD":{"altname":"XLMUSD","wsname":"XLM/USD","aclass_base":"currency","base":"XXLM","aclass_quote":"currency","quote":"ZUSD","lot":"unit","cost_decimals":5,"pair_decimals":6,"lot_decimals":8,"lot_multiplier":1,"leverage_buy":[],"leverage_sell":[],"fees":[[0,0.4]],"fees_maker":[[0,0.25]],"fee_volume_currency":"ZUSD","margin_call":80,"margin_stop":40,"ordermin":"30","costmin":"0.5","tick_size":"0.000001","status":"cancel_only"},
 "AMBAZUSD":{"altname":"AMBUSD","wsname":"AMB/USD","base":"AMBA","quote":"ZUSD","pair_decimals":4,"lot_decimals":8,"status":"online"},
 "AMBBZUSD":{"altname":"AMB/USD","base":"AMBB","quote":"ZUSD","pair_decimals":4,"lot_decimals":8,"status":"online"}
}}"#;

const TICKER_XBT: &str = r#"{"error":[],"result":{"XXBTZUSD":{"a":["101250.10000","1","1.000"],"b":["101249.90000","3","3.000"],"c":["101250.10000","0.00052100"],"v":["1520.14593112","2675.86011702"],"p":["100912.41871","100744.06650"],"t":[41255,73390],"l":["99880.00000","99510.20000"],"h":["101540.00000","101540.00000"],"o":"100120.30000"}}}"#;
const TICKER_ETH: &str = r#"{"error":[],"result":{"XETHZUSD":{"a":["3630.51000","12","12.000"],"b":["3630.50000","4","4.000"],"c":["3630.51000","0.25000000"],"v":["20115.62014711","39811.15806027"],"p":["3611.77215","3602.45567"],"t":[18842,33020],"l":["3577.00000","3561.12000"],"h":["3644.80000","3644.80000"],"o":"3590.02000"}}}"#;

/// Verbatim example result of the Get Account Balance page (read 2026-11-07).
const BALANCES: &str = r#"{"error":[],"result":{"ZUSD":"171288.6158","ZEUR":"504861.8946","XXBT":"1011.1908877900","XETH":"818.5500000000","USDT":"500000.00000000","DAI":"9999.9999999999","DOT":"2.5000000000","ETH2.S":"198.3970800000","ETH2":"2.5885574330","USD.M":"1213029.2780"}}"#;

/// The two example orders of the Get Open Orders page (read 2026-11-07), plus documented fields the
/// example omits (`time_in_force`, `margin`, `descr.aclass`) to prove they are tolerated.
const OPEN_ORDERS: &str = r#"{"error":[],"result":{"open":{
 "OQCLML-BW3P3-BUCMWZ":{"refid":null,"userref":0,"status":"open","opentm":1688666559.8974,"starttm":0,"expiretm":0,"descr":{"pair":"XBTUSD","type":"buy","ordertype":"limit","price":"30010.0","price2":"0","leverage":"none","order":"buy 1.25000000 XBTUSD @ limit 30010.0","close":"","aclass":"forex"},"time_in_force":"gtc","margin":false,"vol":"1.25000000","vol_exec":"0.37500000","cost":"11253.7","fee":"0.00000","price":"30010.0","stopprice":"0.00000","limitprice":"0.00000","misc":"","oflags":"fciq","trades":["TCCCTY-WE2O6-P3NB37"]},
 "OB5VMB-B4U2U-DK2WRW":{"refid":null,"userref":45326,"status":"open","opentm":1688665899.5699,"starttm":0,"expiretm":0,"descr":{"pair":"XBTUSD","type":"buy","ordertype":"limit","price":"14500.0","price2":"0","leverage":"5:1","order":"buy 0.27500000 XBTUSD @ limit 14500.0 with 5:1 leverage","close":""},"vol":"0.27500000","vol_exec":"0.00000000","cost":"0.00000","fee":"0.00000","price":"0.00000","stopprice":"0.00000","limitprice":"0.00000","misc":"","oflags":"fciq"}
}}}"#;

const FORM: &str = "application/x-www-form-urlencoded";

// ---------------------------------------------------------------- application helpers

fn private_api(server: &TestServer, nonces: Nonces) -> Result<Kraken, Box<dyn std::error::Error>> {
    let credentials = Credentials::new(KEY, SECRET)?;
    let policy = Policy::private(credentials, nonces);
    Ok(Kraken::new(transport()?, KrakenConfig { policy }).with_base_url(server.url())?)
}

/// Independent API-Sign recomputation (authentication guide, read 2026-11-07).
fn api_sign(secret_b64: &str, path: &str, nonce: &str, body: &[u8]) -> String {
    let mut inner = Sha256::new();
    inner.update(nonce.as_bytes());
    inner.update(body);
    let digest = inner.finalize();
    let key = STANDARD.decode(secret_b64).expect("test secret is Base64");
    let mut mac = Hmac::<Sha512>::new_from_slice(&key).expect("HMAC takes any key length");
    mac.update(path.as_bytes());
    mac.update(&digest);
    STANDARD.encode(mac.finalize().into_bytes())
}

fn nonce_of(request: &Recorded) -> u64 {
    let pairs = request.form_pairs();
    let values: Vec<&str> = pairs
        .iter()
        .filter(|(k, _)| k == "nonce")
        .map(|(_, v)| v.as_str())
        .collect();
    assert_eq!(values.len(), 1, "exactly one nonce field");
    values[0].parse().expect("decimal nonce")
}

/// Asserts the wire shape of one signed private request and that its signature verifies.
fn assert_signed(request: &Recorded, path: &str, query: Option<&str>) -> u64 {
    assert_eq!(request.method, "POST");
    assert_eq!(request.path(), path);
    assert_eq!(request.query(), query);
    assert_eq!(request.header("api-key"), Some(KEY));
    assert_eq!(request.header("content-type"), Some(FORM));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert!(!request.chunked);
    let nonce = nonce_of(request);
    let sign = request.header("api-sign").expect("API-Sign header");
    assert_eq!(sign.len(), 88, "Base64 of a 64-byte HMAC-SHA512");
    assert_eq!(
        sign,
        api_sign(SECRET, path, &nonce.to_string(), &request.body)
    );
    nonce
}

fn assert_public(request: &Recorded, path: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), path);
    assert_eq!(request.header("accept"), Some("application/json"));
    assert!(request.header_all("api-key").is_empty());
    assert!(request.header_all("api-sign").is_empty());
    assert!(request.header_all("content-type").is_empty());
    assert_eq!(request.body_len, 0);
}

fn market_server() -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    |request: &Recorded| match request.path() {
        "/0/public/AssetPairs" => Reply::json(200, PAIRS),
        "/0/public/Ticker" => match request.query_all("pair").as_slice() {
            [p] if p == "XXBTZUSD" => Reply::json(200, TICKER_XBT),
            [p] if p == "XETHZUSD" => Reply::json(200, TICKER_ETH),
            _ => Reply::json(200, r#"{"error":["EQuery:Unknown asset pair"]}"#),
        },
        "/0/private/Balance" => Reply::json(200, BALANCES),
        "/0/private/OpenOrders" => Reply::json(200, OPEN_ORDERS),
        _ => Reply::new(404),
    }
}

/// Application logic: mid price and relative spread (display only; strings stay authoritative).
fn spread_bps(ask: &str, bid: &str) -> f64 {
    let (a, b): (f64, f64) = (ask.parse().unwrap(), bid.parse().unwrap());
    (a - b) / ((a + b) / 2.0) * 10_000.0
}

// ---------------------------------------------------------------- workflow 1

#[tokio::test]
async fn workflow1_select_market_then_quote() -> TestResult {
    let server = TestServer::start(market_server()).await;
    // A private-capable client: public calls must still carry no credentials and spend no nonce.
    let api = private_api(&server, Nonces::new(LAST_USED))?;

    let market = api.select_market("XBT/USD").await?;
    assert_eq!(market.id, "XXBTZUSD");
    assert_eq!(market.pair.altname, "XBTUSD");
    assert_eq!(market.pair.wsname.as_deref(), Some("XBT/USD"));
    assert_eq!(
        (market.pair.base.as_str(), market.pair.quote.as_str()),
        ("XXBT", "ZUSD")
    );
    assert_eq!(market.pair.status, "online");
    assert_eq!(
        (market.pair.pair_decimals, market.pair.lot_decimals),
        (1, 8)
    );
    assert_eq!(market.pair.ordermin.as_deref(), Some("0.00005"));
    assert_eq!(market.ticker.a[0], "101250.10000");
    assert_eq!(market.ticker.b[0], "101249.90000");
    assert_eq!(market.ticker.t, [41255, 73390]);
    assert_eq!(market.ticker.o, "100120.30000");
    let bps = spread_bps(&market.ticker.a[0], &market.ticker.b[0]);
    assert!(bps > 0.0 && bps < 1.0, "XBT/USD spread is {bps} bps");
    assert_eq!(market.pairs_meta.status().as_u16(), 200);
    assert_eq!(
        market.pairs_meta.operation().map(|o| o.name()),
        Some("Kraken.public.pairs")
    );
    assert_eq!(
        market.ticker_meta.operation().map(|o| o.name()),
        Some("Kraken.public.ticker")
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_public(&requests[0], "/0/public/AssetPairs");
    assert_eq!(requests[0].query(), None);
    assert_public(&requests[1], "/0/public/Ticker");
    assert_eq!(
        requests[1].query(),
        Some("pair=XXBTZUSD"),
        "canonical id, not the selector"
    );

    // Altname and canonical selectors reach the same canonical ticker query.
    assert_eq!(api.select_market("ETHUSD").await?.id, "XETHZUSD");
    assert_eq!(api.select_market("XETHZUSD").await?.id, "XETHZUSD");
    let requests = server.requests();
    assert_eq!(requests.len(), 6);
    assert_eq!(requests[3].query(), Some("pair=XETHZUSD"));
    assert_eq!(requests[5].query(), Some("pair=XETHZUSD"));

    // Generated endpoint with the optional provider-side restriction.
    let restricted = api
        .public()
        .pairs()
        .set_pair("XBTUSD,ETHUSD")
        .send()
        .await?;
    assert_eq!(
        restricted.value.len(),
        5,
        "the fixture ignores the restriction"
    );
    let request = &server.requests()[6];
    assert_public(request, "/0/public/AssetPairs");
    assert_eq!(request.query_all("pair"), vec!["XBTUSD,ETHUSD".to_string()]);
    assert_eq!(request.target, "/0/public/AssetPairs?pair=XBTUSD%2CETHUSD");

    // Public calls spent no nonce: the first private call uses LAST_USED + 1.
    api.account()
        .balances(BalanceOptions::default())
        .send()
        .await?;
    let nonce = assert_signed(&server.requests()[7], "/0/private/Balance", None);
    assert_eq!(nonce, LAST_USED + 1);

    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
    Ok(())
}

#[tokio::test]
async fn workflow1_selection_failures_stop_before_the_ticker() -> TestResult {
    let server = TestServer::start(market_server()).await;
    let api = Kraken::new(
        transport()?,
        KrakenConfig {
            policy: Policy::public(),
        },
    )
    .with_base_url(server.url())?;

    for selector in ["DOGE/EUR", "XLM/USD", "AMB/USD", "xbt/usd", ""] {
        let before = server.requests().len();
        let error = api.select_market(selector).await.expect_err(selector);
        assert!(
            matches!(error, Error::Local(LocalError::PairUnavailable)),
            "{selector:?}: {error:?}"
        );
        assert_eq!(error.status(), None);
        let after = server.requests();
        assert_eq!(
            after.len(),
            before + 1,
            "{selector:?}: only AssetPairs was sent"
        );
        assert_eq!(after[before].path(), "/0/public/AssetPairs");
    }
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn workflow1_ticker_failures_and_partial_results() -> TestResult {
    // The ticker answers for another market, then the provider fails, then succeeds.
    let ticker_calls = Arc::new(Mutex::new(0u32));
    let calls = ticker_calls.clone();
    let server = TestServer::start(move |request: &Recorded| match request.path() {
        "/0/public/AssetPairs" => Reply::json(200, PAIRS),
        "/0/public/Ticker" => {
            let mut n = calls.lock().unwrap();
            *n += 1;
            match *n {
                1 => Reply::json(200, TICKER_ETH), // wrong market for XXBTZUSD
                2 => Reply::json(503, r#"{"error":["EService:Unavailable"]}"#)
                    .header("retry-after", "2"),
                3 => Reply::json(200, r#"{"error":["EQuery:Unknown asset pair"]}"#),
                4 => Reply::text(200, "text/html", "<html><body>maintenance</body></html>"),
                5 => Reply::json(200, r#"{"error":[],"result":{"XXBTZUSD":"#),
                6 => Reply::json(200, r#"{"error":[],"result":null}"#),
                7 => Reply::text(502, "text/html", "<html>bad gateway</html>"),
                _ => Reply::json(200, TICKER_XBT),
            }
        }
        _ => Reply::new(404),
    })
    .await;
    let api = Kraken::new(
        transport()?,
        KrakenConfig {
            policy: Policy::public(),
        },
    )
    .with_base_url(server.url())?;

    // 1. Helper: the ticker does not match the selected market.
    let error = api.select_market("XBTUSD").await.unwrap_err();
    assert!(
        matches!(error, Error::Local(LocalError::TickerMismatch)),
        "{error:?}"
    );
    assert_eq!(server.requests().len(), 2);

    // The application keeps partial results by calling the generated endpoints itself.
    let pairs = api.public().pairs().send().await?;
    let (id, pair) = pairs
        .value
        .iter()
        .find(|(_, p)| p.wsname.as_deref() == Some("XBT/USD") && p.status == "online")
        .expect("listed");
    assert_eq!(id, "XXBTZUSD");
    assert_eq!(pair.altname, "XBTUSD");

    // 2. HTTP 503: status, provider document and advisory Retry-After survive; no retry.
    let error = api.public().ticker(id).send().await.unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(503));
    let Error::Http(failure) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(failure.meta.status().as_u16(), 503);
    let document = failure.document.as_ref().expect("JSON error document");
    assert_eq!(document.errors_unredacted(), ["EService:Unavailable"]);
    assert_eq!(
        failure.retry_after(SystemTime::now()),
        Ok(Some(Duration::from_secs(2)))
    );
    println!("Http Display: {error}");
    assert!(!format!("{error} {error:?}").contains("EService:Unavailable"));

    // 3. Provider error inside a 200 envelope.
    let error = api.public().ticker(id).send().await.unwrap_err();
    let Error::Api(failure) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(failure.meta.status().as_u16(), 200);
    assert_eq!(failure.errors_unredacted(), ["EQuery:Unknown asset pair"]);
    assert_eq!(failure.classes(), [ApiClass::Other]);
    println!("Api Display: {error}");
    assert!(!format!("{error} {error:?}").contains("Unknown asset pair"));

    // 4. Wrong media type: refused from the head, before the body.
    let error = api.public().ticker(id).send().await.unwrap_err();
    let Error::Concord(inner) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(inner.kind(), ErrorKind::Metadata);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(200));

    // 5. Truncated JSON.
    let error = api.public().ticker(id).send().await.unwrap_err();
    let Error::Concord(inner) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(inner.kind(), ErrorKind::Decode);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(200));

    // 6. Empty error list without a result.
    let error = api.public().ticker(id).send().await.unwrap_err();
    let Error::MissingResult(meta) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(meta.status().as_u16(), 200);

    // 7. Non-2xx with an unreadable (HTML) body keeps its status.
    let error = api.public().ticker(id).send().await.unwrap_err();
    let Error::Http(failure) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(failure.meta.status().as_u16(), 502);
    assert!(failure.document.is_err());
    assert_eq!(failure.retry_after(SystemTime::now()), Ok(None));

    // 8. Recovery: the same pair now quotes, and the earlier listing was never lost.
    let ticker = api.public().ticker(id).send().await?;
    assert_eq!(ticker.value.len(), 1);
    assert_eq!(ticker.value[id].c[0], "101250.10000");
    assert_eq!(pair.pair_decimals, 1);

    let requests = server.requests();
    // helper (2) + pairs (1) + ticker calls 2..=8 (7): one request per call, nothing retried.
    assert_eq!(requests.len(), 2 + 1 + 7);
    for request in &requests[3..] {
        assert_public(request, "/0/public/Ticker");
        assert_eq!(request.query(), Some("pair=XXBTZUSD"));
    }
    server.shutdown().await;
    Ok(())
}

// ---------------------------------------------------------------- workflow 2

#[tokio::test]
async fn workflow2_account_snapshot_with_signed_requests() -> TestResult {
    let server = TestServer::start(market_server()).await;
    let api = private_api(&server, Nonces::new(LAST_USED))?;

    let snapshot = api.account_snapshot().await?;
    let balances = &snapshot.balances.value;
    assert_eq!(balances.len(), 10);
    assert_eq!(balances["XXBT"], "1011.1908877900");
    assert_eq!(balances["ZUSD"], "171288.6158");
    assert_eq!(
        balances["ETH2.S"], "198.3970800000",
        "staked suffix kept verbatim"
    );
    assert_eq!(balances["USD.M"], "1213029.2780");
    assert_eq!(
        snapshot.balances.meta.operation().map(|o| o.name()),
        Some("Kraken.account.balances")
    );
    assert_eq!(
        snapshot.orders.meta.operation().map(|o| o.name()),
        Some("Kraken.account.open_orders")
    );

    let open = &snapshot.orders.value.open;
    assert_eq!(
        open.keys().collect::<Vec<_>>(),
        ["OB5VMB-B4U2U-DK2WRW", "OQCLML-BW3P3-BUCMWZ"]
    );
    let first = &open["OQCLML-BW3P3-BUCMWZ"];
    assert_eq!(first.refid, None);
    assert_eq!(first.userref, Some(0));
    assert_eq!(first.cl_ord_id, None);
    assert_eq!(first.status, "open");
    assert_eq!(first.opentm, 1688666559.8974);
    assert_eq!((first.starttm, first.expiretm), (0.0, 0.0));
    assert_eq!(first.descr.pair, "XBTUSD");
    assert_eq!(first.descr.side, "buy", "wire field `type`");
    assert_eq!(first.descr.ordertype, "limit");
    assert_eq!(first.descr.order, "buy 1.25000000 XBTUSD @ limit 30010.0");
    assert_eq!(
        (first.vol.as_str(), first.vol_exec.as_str()),
        ("1.25000000", "0.37500000")
    );
    assert_eq!(first.cost, "11253.7");
    assert_eq!(first.oflags, "fciq");
    assert_eq!(
        first.trades.as_deref(),
        Some(&["TCCCTY-WE2O6-P3NB37".to_string()][..])
    );
    let second = &open["OB5VMB-B4U2U-DK2WRW"];
    assert_eq!(second.userref, Some(45326));
    assert_eq!(second.descr.leverage, "5:1");
    assert_eq!(second.trades, None);

    // Application view: remaining open volume on XBTUSD, from the decimal strings.
    let remaining: f64 = open
        .values()
        .filter(|o| o.descr.pair == "XBTUSD")
        .map(|o| o.vol.parse::<f64>().unwrap() - o.vol_exec.parse::<f64>().unwrap())
        .sum();
    assert!((remaining - 1.15).abs() < 1e-9);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let n1 = assert_signed(&requests[0], "/0/private/Balance", None);
    let n2 = assert_signed(&requests[1], "/0/private/OpenOrders", None);
    assert_eq!((n1, n2), (LAST_USED + 1, LAST_USED + 2));
    assert_eq!(requests[0].body_text(), format!("nonce={n1}"));
    assert_eq!(requests[1].body_text(), format!("nonce={n2}"));
    assert_ne!(
        requests[0].header("api-sign"),
        requests[1].header("api-sign")
    );

    // Generated endpoints with explicit options and a wallet query.
    let mut options = OpenOrdersOptions::default();
    options.trades = Some(true);
    options.cl_ord_id = Some("9cc788d8-9c00-4b25-94d3-26d93603948d".to_string());
    options.otp = Some("246810".to_string());
    assert!(
        !format!("{options:?}").contains("246810"),
        "OTP withheld from Debug"
    );
    let orders = api
        .account()
        .open_orders(options)
        .set_account_id("WX6V-JUKW-KKPB-QE36")
        .send()
        .await?;
    assert_eq!(orders.value.open.len(), 2);
    let mut balance_options = BalanceOptions::default();
    balance_options.rebase_multiplier = Some(Rebase::Base);
    api.account()
        .balances(balance_options)
        .set_account_id("WX6V-JUKW-KKPB-QE36")
        .send()
        .await?;

    let requests = server.requests();
    let wallet = "account_id=WX6V-JUKW-KKPB-QE36";
    let query = Some(wallet);
    let n3 = assert_signed(&requests[2], "/0/private/OpenOrders", query);
    let n4 = assert_signed(&requests[3], "/0/private/Balance", query);
    assert_eq!((n3, n4), (LAST_USED + 3, LAST_USED + 4));
    // Nonce first, then the set options in declaration order; absent options are omitted.
    assert_eq!(
        requests[2].body_text(),
        format!("nonce={n3}&otp=246810&trades=true&cl_ord_id=9cc788d8-9c00-4b25-94d3-26d93603948d")
    );
    // The signature covers the bare path: the wallet query is not part of it.
    let with_query = format!("/0/private/OpenOrders?{wallet}");
    assert_ne!(
        requests[2].header("api-sign"),
        Some(api_sign(SECRET, &with_query, &n3.to_string(), &requests[2].body).as_str())
    );
    let mut fields = requests[2].form_pairs();
    fields.retain(|(k, _)| k != "nonce");
    fields.sort();
    assert_eq!(
        fields,
        [
            (
                "cl_ord_id".to_string(),
                "9cc788d8-9c00-4b25-94d3-26d93603948d".to_string()
            ),
            ("otp".to_string(), "246810".to_string()),
            ("trades".to_string(), "true".to_string()),
        ]
    );
    assert_eq!(
        requests[3].form_pairs(),
        [
            ("nonce".to_string(), n4.to_string()),
            ("rebase_multiplier".to_string(), "base".to_string())
        ]
    );

    // Secrets stay out of ordinary diagnostics.
    let credentials = Credentials::new(KEY, SECRET)?;
    let policy = Policy::private(credentials.clone(), Nonces::new(LAST_USED));
    let shown = format!(
        "{credentials:?} {credentials} {policy:?} {api:?} {:?} {:?} {snapshot:?}",
        KrakenConfig {
            policy: policy.clone()
        },
        api.account().balances(BalanceOptions::default()),
    );
    for secret in [KEY, SECRET, requests[0].header("api-sign").unwrap()] {
        assert!(!shown.contains(secret), "diagnostics leaked a secret");
    }
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn workflow2_local_refusals_send_nothing() -> TestResult {
    let server = TestServer::start(market_server()).await;

    assert_eq!(
        Credentials::new("", SECRET).unwrap_err(),
        LocalError::InvalidKey
    );
    assert_eq!(
        Credentials::new("key with space", SECRET).unwrap_err(),
        LocalError::InvalidKey
    );
    assert_eq!(
        Credentials::new(KEY, "").unwrap_err(),
        LocalError::InvalidSecret
    );
    assert_eq!(
        Credentials::new(KEY, "not base64!").unwrap_err(),
        LocalError::InvalidSecret
    );
    let unpadded = SECRET.trim_end_matches('=');
    assert_eq!(
        Credentials::new(KEY, unpadded).unwrap_err(),
        LocalError::InvalidSecret
    );

    // Public-only policy: private calls refuse locally.
    let public = Kraken::new(
        transport()?,
        KrakenConfig {
            policy: Policy::public(),
        },
    )
    .with_base_url(server.url())?;
    let error = public.account_snapshot().await.unwrap_err();
    assert!(
        matches!(error, Error::Local(LocalError::MissingCredentials)),
        "{error:?}"
    );
    let error = public
        .account()
        .open_orders(OpenOrdersOptions::default())
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::Local(LocalError::MissingCredentials)),
        "{error:?}"
    );

    // Exhausted nonce space: one last allocation, then a local refusal without wrapping.
    let nearly = private_api(&server, Nonces::new(u64::MAX - 1))?;
    nearly
        .account()
        .balances(BalanceOptions::default())
        .send()
        .await?;
    let error = nearly
        .account()
        .balances(BalanceOptions::default())
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::Local(LocalError::NonceExhausted)),
        "{error:?}"
    );
    let exhausted = private_api(&server, Nonces::new(u64::MAX))?;
    let error = exhausted.account_snapshot().await.unwrap_err();
    assert!(
        matches!(error, Error::Local(LocalError::NonceExhausted)),
        "{error:?}"
    );

    let sent = server.requests();
    assert_eq!(sent.len(), 1, "only the u64::MAX allocation was sent");
    assert_eq!(
        assert_signed(&sent[0], "/0/private/Balance", None),
        u64::MAX
    );

    // A base URL with a path prefix (a proxy mount). Predicted: public calls follow it and only
    // signing refuses. Observed: every call refuses locally as SigningInput, public ones and
    // public-only policies included (README: only the four declared paths are accepted).
    let mount = format!("{}/kraken", server.url());
    let mounted = private_api(&server, Nonces::new(LAST_USED))?.with_base_url(&mount)?;
    let error = mounted
        .account()
        .balances(BalanceOptions::default())
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::Local(LocalError::SigningInput)),
        "{error:?}"
    );
    let listed = mounted.public().pairs().send().await;
    assert!(
        matches!(listed, Err(Error::Local(LocalError::SigningInput))),
        "{listed:?}"
    );
    let public_mounted = Kraken::new(
        transport()?,
        KrakenConfig {
            policy: Policy::public(),
        },
    )
    .with_base_url(&mount)?;
    let listed = public_mounted.select_market("XBT/USD").await;
    assert!(
        matches!(listed, Err(Error::Local(LocalError::SigningInput))),
        "{listed:?}"
    );
    assert_eq!(server.requests().len(), 1, "the refused calls sent nothing");
    println!("Local Display: {error}");
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn workflow2_provider_errors_and_partial_snapshot() -> TestResult {
    let script = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let replies = script.clone();
    let server = TestServer::start(move |request: &Recorded| {
        let next = replies.lock().unwrap().remove(0);
        match (request.path(), next) {
            ("/0/private/Balance", "ok") => Reply::json(200, BALANCES),
            ("/0/private/OpenOrders", "ok") => Reply::json(200, OPEN_ORDERS),
            (_, "empty") => Reply::json(200, r#"{"error":[],"result":{"open":{}}}"#),
            (_, "cursor") => Reply::json(
                200,
                r#"{"error":[],"result":{"open":{},"cursor":{"next":null}}}"#,
            ),
            (_, error) => Reply::json(200, format!(r#"{{"error":["{error}"]}}"#)),
        }
    })
    .await;
    let api = private_api(&server, Nonces::new(LAST_USED))?;
    let set = |steps: &[&'static str]| script.lock().unwrap().extend_from_slice(steps);

    // Balances succeed, open orders are refused: the helper fails as a whole.
    set(&["ok", "EGeneral:Permission denied"]);
    let error = api.account_snapshot().await.unwrap_err();
    let Error::Api(failure) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(failure.classes(), [ApiClass::Permission]);
    assert_eq!(
        failure.meta.operation().map(|o| o.name()),
        Some("Kraken.account.open_orders")
    );
    assert!(!format!("{error} {error:?}").contains("Permission denied"));
    println!("snapshot Display: {error}");

    // The application keeps the partial result by sending the two reads itself.
    set(&["ok", "EGeneral:Permission denied"]);
    let balances = api
        .account()
        .balances(BalanceOptions::default())
        .send()
        .await?;
    let orders = api
        .account()
        .open_orders(OpenOrdersOptions::default())
        .send()
        .await;
    assert_eq!(balances.value["XXBT"], "1011.1908877900");
    assert!(matches!(orders, Err(Error::Api(_))));

    // Classified provider strings, in order and multiplicity; none is retried.
    for (provider, class) in [
        ("EAPI:Invalid key", ApiClass::Authentication),
        ("EAPI:Invalid signature", ApiClass::Authentication),
        ("EAPI:Invalid nonce", ApiClass::InvalidNonce),
        ("EAPI:Rate limit exceeded", ApiClass::RateLimited),
        ("EService:Unavailable", ApiClass::Service),
        ("EGeneral:Invalid arguments", ApiClass::InvalidArguments),
        ("WGeneral:Some future warning", ApiClass::Other),
    ] {
        set(&[provider]);
        let error = api
            .account()
            .balances(BalanceOptions::default())
            .send()
            .await
            .unwrap_err();
        let Error::Api(failure) = &error else {
            panic!("{provider}: {error:?}")
        };
        assert_eq!(failure.classes(), [class], "{provider}");
        assert_eq!(failure.errors_unredacted(), [provider]);
    }

    // Pagination end: one complete document; an empty map and a stray cursor are both success.
    set(&["empty", "cursor"]);
    let empty = api
        .account()
        .open_orders(OpenOrdersOptions::default())
        .send()
        .await?;
    assert!(empty.value.open.is_empty());
    let cursor = api
        .account()
        .open_orders(OpenOrdersOptions::default())
        .send()
        .await?;
    assert!(
        cursor.value.open.is_empty(),
        "an unexpected cursor key is ignored"
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 2 + 2 + 7 + 2, "one request per call");
    let nonces: Vec<u64> = requests
        .iter()
        .map(|r| assert_signed(r, r.path(), None))
        .collect();
    let expected: Vec<u64> = (LAST_USED + 1..=LAST_USED + 13).collect();
    assert_eq!(
        nonces, expected,
        "sequential calls: allocation order equals arrival order"
    );
    for request in &requests {
        assert!(
            !request
                .form_pairs()
                .iter()
                .any(|(k, _)| k == "cursor" || k == "limit")
        );
    }
    server.shutdown().await;
    Ok(())
}

// ---------------------------------------------------------------- shared nonce source

/// A strict provider: any nonce not above the highest accepted so far is refused.
fn strict_nonce_server(
    accepted: Arc<Mutex<Vec<u64>>>,
) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    move |request: &Recorded| {
        let nonce = nonce_of(request);
        let mut accepted = accepted.lock().unwrap();
        if accepted.last().is_some_and(|&high| nonce <= high) {
            Reply::json(200, r#"{"error":["EAPI:Invalid nonce"]}"#)
        } else {
            accepted.push(nonce);
            Reply::json(200, BALANCES)
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_sends_sharing_a_nonce_source_never_repeat_a_nonce() -> TestResult {
    const CALLS: u64 = 48;
    let accepted = Arc::new(Mutex::new(Vec::new()));
    let server = TestServer::start(strict_nonce_server(accepted.clone())).await;

    // Two independent clients for the same key share one nonce source through the policy.
    let credentials = Credentials::new(KEY, SECRET)?;
    let policy = Policy::private(credentials, Nonces::new(LAST_USED));
    let a = Kraken::new(
        transport()?,
        KrakenConfig {
            policy: policy.clone(),
        },
    )
    .with_base_url(server.url())?;
    let b = Kraken::new(transport()?, KrakenConfig { policy }).with_base_url(server.url())?;

    let mut tasks = Vec::new();
    for i in 0..CALLS {
        let api = if i % 2 == 0 { a.clone() } else { b.clone() };
        tasks.push(tokio::spawn(async move {
            api.account()
                .balances(BalanceOptions::default())
                .send()
                .await
        }));
    }
    let (mut ok, mut invalid_nonce) = (0u64, 0u64);
    for task in tasks {
        match task.await? {
            Ok(response) => {
                assert_eq!(response.value["XXBT"], "1011.1908877900");
                ok += 1;
            }
            Err(Error::Api(failure)) => {
                assert_eq!(failure.classes(), [ApiClass::InvalidNonce]);
                invalid_nonce += 1;
            }
            Err(other) => panic!("unexpected failure: {other:?}"),
        }
    }

    let requests = server.requests();
    assert_eq!(
        requests.len() as u64,
        CALLS,
        "each send dispatched exactly once"
    );
    let arrival: Vec<u64> = requests
        .iter()
        .map(|r| assert_signed(r, "/0/private/Balance", None))
        .collect();
    let unique: BTreeSet<u64> = arrival.iter().copied().collect();
    assert_eq!(unique.len() as u64, CALLS, "no nonce repeated");
    assert_eq!(
        unique,
        (LAST_USED + 1..=LAST_USED + CALLS).collect::<BTreeSet<_>>()
    );
    assert_eq!(ok + invalid_nonce, CALLS);
    assert_eq!(ok as usize, accepted.lock().unwrap().len());
    assert!(ok >= 1);
    // Arrival order is observed, not asserted: allocation order does not promise it.
    let reordered = arrival.windows(2).filter(|w| w[0] > w[1]).count();
    println!("concurrent: {ok} accepted, {invalid_nonce} invalid nonce, {reordered} inversions");

    // After the burst, the next allocation continues above every concurrent one.
    a.account()
        .balances(BalanceOptions::default())
        .send()
        .await?;
    let last = server.requests().pop().unwrap();
    assert_eq!(nonce_of(&last), LAST_USED + CALLS + 1);
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn reversed_arrival_is_rejected_as_invalid_nonce_and_not_retried() -> TestResult {
    let accepted = Arc::new(Mutex::new(Vec::new()));
    let server = TestServer::start(strict_nonce_server(accepted.clone())).await;
    let credentials = Credentials::new(KEY, SECRET)?;
    let policy = Policy::private(credentials, Nonces::new(LAST_USED));
    let api = Kraken::new(
        transport()?,
        KrakenConfig {
            policy: policy.clone(),
        },
    )
    .with_base_url(server.url())?;

    // Unsigned generated prepare(): no credentials, no nonce consumed.
    let unsigned = api
        .account()
        .balances(BalanceOptions::default())
        .prepare()?;
    assert!(unsigned.request().headers().get("api-sign").is_none());
    assert!(unsigned.request().headers().get("api-key").is_none());

    // Controlled scheduling: allocate two signed requests, deliver them in reverse.
    let earlier = policy.prepare_signed(api.account().balances(BalanceOptions::default()))?;
    let later = policy.prepare_signed(api.account().balances(BalanceOptions::default()))?;
    let shown = format!("{earlier:?} {later:?}");
    assert!(!shown.contains(KEY) && !shown.contains(SECRET));
    let native = later.request_unredacted();
    let sign = native.headers().get("api-sign").expect("signed");
    assert!(sign.is_sensitive());
    assert!(!shown.contains(sign.to_str()?));
    assert_eq!(native.url().path(), "/0/private/Balance");

    let later_result = later.send().await?;
    assert_eq!(later_result.value["ZUSD"], "171288.6158");
    let error = earlier.send().await.unwrap_err();
    let Error::Api(failure) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(failure.classes(), [ApiClass::InvalidNonce]);
    assert_eq!(failure.errors_unredacted(), ["EAPI:Invalid nonce"]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2, "the rejected nonce was not replayed");
    let arrival: Vec<u64> = requests
        .iter()
        .map(|r| assert_signed(r, "/0/private/Balance", None))
        .collect();
    assert_eq!(
        arrival,
        [LAST_USED + 2, LAST_USED + 1],
        "the unsigned prepare allocated nothing"
    );
    assert_eq!(*accepted.lock().unwrap(), [LAST_USED + 2]);

    // Recovery is a fresh allocation, never a replay of the rejected one.
    api.account()
        .balances(BalanceOptions::default())
        .send()
        .await?;
    assert_eq!(nonce_of(server.requests().last().unwrap()), LAST_USED + 3);
    server.shutdown().await;
    Ok(())
}

#[test]
fn official_signature_vector_matches_sdk_and_independent_computation() -> TestResult {
    // Authentication guide example, read 2026-11-07.
    let secret =
        "kQH5HW/8p1uGOVjbgWA7FunAmGO8lsSUXNsu3eow76sz84Q18fWxnyRzBHCd3pd5nE9qa99HAZtuZuj6F1huXg==";
    let payload =
        b"nonce=1616492376594&ordertype=limit&pair=XBTUSD&price=37500&type=buy&volume=1.25";
    let expected =
        "4/dpxb3iT4tp/ZCVEwSnEsLxx0bqyhLpdfOpc6fn7OR8+UClSV5n9E6aSS8MPtnRfp32bAb0nmbRn6H8ndwLUQ==";
    assert_eq!(
        api_sign(secret, "/0/private/AddOrder", "1616492376594", payload),
        expected
    );
    let credentials = Credentials::new("documentation-example-key", secret)?;
    let header = credentials.signature("/0/private/AddOrder", 1616492376594, payload);
    assert_eq!(header.to_str()?, expected);
    assert!(header.is_sensitive());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn application_serializes_one_key_so_a_strict_provider_accepts_every_call() -> TestResult {
    // The README's advice for strict providers, done by the application: hold one lock per key
    // across allocation, dispatch and answer, so arrival order equals allocation order.
    const CALLS: u64 = 24;
    let accepted = Arc::new(Mutex::new(Vec::new()));
    let server = TestServer::start(strict_nonce_server(accepted.clone())).await;
    let api = private_api(&server, Nonces::new(LAST_USED))?;
    let key_lock = Arc::new(tokio::sync::Mutex::new(()));

    let mut tasks = Vec::new();
    for _ in 0..CALLS {
        let (api, key_lock) = (api.clone(), key_lock.clone());
        tasks.push(tokio::spawn(async move {
            let _turn = key_lock.lock().await;
            api.account()
                .balances(BalanceOptions::default())
                .send()
                .await
        }));
    }
    for task in tasks {
        task.await??;
    }
    let arrival: Vec<u64> = server
        .requests()
        .iter()
        .map(|r| assert_signed(r, "/0/private/Balance", None))
        .collect();
    let expected: Vec<u64> = (LAST_USED + 1..=LAST_USED + CALLS).collect();
    assert_eq!(arrival, expected);
    assert_eq!(*accepted.lock().unwrap(), expected);
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn one_order_missing_a_modelled_field_fails_the_whole_read() -> TestResult {
    // The open-orders page's example omits documented fields, so absence is plausible. Here the
    // second example order lacks `limitprice`; the SDK models it as a required string.
    let without_limitprice = OPEN_ORDERS.replacen(
        r#""limitprice":"0.00000","misc":"","oflags":"fciq"}"#,
        r#""misc":"","oflags":"fciq"}"#,
        1,
    );
    assert_ne!(without_limitprice, OPEN_ORDERS);
    let server = TestServer::start(move |request: &Recorded| match request.path() {
        "/0/private/OpenOrders" => Reply::json(200, without_limitprice.as_str()),
        _ => Reply::json(200, BALANCES),
    })
    .await;
    let api = private_api(&server, Nonces::new(LAST_USED))?;
    let error = api.account_snapshot().await.unwrap_err();
    let Error::Concord(inner) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(inner.kind(), ErrorKind::Decode);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(200));
    println!("Decode Display: {error}");
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
    Ok(())
}
