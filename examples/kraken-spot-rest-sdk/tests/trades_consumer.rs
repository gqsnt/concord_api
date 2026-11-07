//! Consumer use of the trade-history traversal, written as an application would: select a
//! market, read the account, then build a fill report for that market over several budgeted
//! runs that persist a resume point and survive a failed page. Loopback only; NOT_RUN live.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use concord_sdk_kraken_spot_rest::{
    ApiClass, Credentials, Error, Kraken, KrakenConfig, Nonces, PagesError, Policy, Resume, Trade,
    TradeBound, TradesHistoryOptions, transport,
};
use concord_testserver::{Recorded, Reply, TestServer};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::BTreeMap,
    num::NonZeroUsize,
    sync::atomic::{AtomicBool, Ordering},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const KEY: &str = "consumer-trades-key";
/// Synthetic secret (`consumer trade report secret`), never a real account key.
const SECRET: &str = "Y29uc3VtZXIgdHJhZGUgcmVwb3J0IHNlY3JldA==";
const PAIRS: &str = include_str!("../fixtures/pairs.json");
const TICKER: &str = include_str!("../fixtures/ticker.json");
const BALANCES: &str = include_str!("../fixtures/balances.json");
const ORDERS: &str = include_str!("../fixtures/open-orders.json");
const TOTAL: usize = 120;

/// Independent API-Sign recomputation from the authentication guide (read 2026-11-07).
fn api_sign(path: &str, nonce: &str, body: &[u8]) -> String {
    let mut inner = Sha256::new();
    inner.update(nonce.as_bytes());
    inner.update(body);
    let key = STANDARD.decode(SECRET).unwrap();
    let mut mac = Hmac::<Sha512>::new_from_slice(&key).unwrap();
    mac.update(path.as_bytes());
    mac.update(&inner.finalize());
    STANDARD.encode(mac.finalize().into_bytes())
}
fn field(r: &Recorded, name: &str) -> Option<String> {
    r.form_pairs()
        .into_iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
}

/// Synthetic account fills for XXBTZUSD; even trades buy 0.01, odd trades sell 0.02.
fn fill(i: usize) -> (String, Value) {
    let buy = i.is_multiple_of(2);
    let value = json!({
        "ordertxid": "OQCLML-BW3P3-BUCMWZ", "pair": "XXBTZUSD",
        "time": 1_688_667_796.0 - i as f64, "type": if buy { "buy" } else { "sell" },
        "ordertype": "limit", "price": "30010.0", "cost": "300.10000",
        "fee": if buy { "0.00000" } else { "0.78026" },
        "vol": if buy { "0.01000000" } else { "0.02000000" }, "misc": ""
    });
    (format!("TC{i:04}-FILLS-XBTUSD"), value)
}

/// One provider for the whole application session. The first request for `ofs=100` is
/// throttled; a later run succeeds.
fn provider(throttled_once: &'static AtomicBool) -> impl Fn(&Recorded) -> Reply + Send + Sync {
    move |r: &Recorded| match r.path() {
        "/0/public/AssetPairs" => Reply::json(200, PAIRS),
        "/0/public/Ticker" => Reply::json(200, TICKER),
        "/0/private/Balance" => Reply::json(200, BALANCES),
        "/0/private/OpenOrders" => Reply::json(200, ORDERS),
        "/0/private/TradesHistory" => {
            let at: usize = field(r, "ofs").map_or(0, |v| v.parse().unwrap());
            if at == 100 && !throttled_once.swap(true, Ordering::SeqCst) {
                return Reply::json(200, r#"{"error":["EAPI:Rate limit exceeded"]}"#);
            }
            let trades: serde_json::Map<_, _> = (at..TOTAL.min(at + 50)).map(fill).collect();
            Reply::json(
                200,
                json!({"error": [], "result": {"count": TOTAL, "trades": trades}}).to_string(),
            )
        }
        _ => Reply::new(404),
    }
}

/// Application state persisted between runs: merged fills and where to continue.
#[derive(Default)]
struct FillReport {
    fills: BTreeMap<String, Trade>,
    resume: Option<Resume<u64>>,
    complete: bool,
}

/// Exact decimal string to integer units at the pair's lot precision (no float rounding).
fn units(decimal: &str, places: u8) -> u64 {
    let (whole, frac) = decimal.split_once('.').unwrap_or((decimal, ""));
    let frac = format!("{frac:0<width$}", width = usize::from(places));
    format!("{whole}{}", &frac[..usize::from(places)])
        .parse()
        .unwrap()
}

/// One budgeted run: continue from the persisted point, keep whatever completed.
async fn run(
    api: &Kraken,
    filters: &TradesHistoryOptions,
    report: &mut FillReport,
) -> Option<Error> {
    let from = report.resume.take().unwrap_or(Resume::Start);
    let pages = api
        .trade_history(filters.clone(), None, from)
        .max_pages(NonZeroUsize::new(2).unwrap());
    let (pages, failure) = match pages.collect_complete().await {
        Ok(pages) => (pages, None),
        Err(incomplete) => {
            report.resume = incomplete.error.resume().cloned();
            let failure = match incomplete.error {
                PagesError::Provider { error, .. } => Some(error),
                _ => None, // the run budget was reached
            };
            (incomplete.collected, failure)
        }
    };
    report.complete = report.resume.is_none() && failure.is_none();
    for page in pages {
        report.fills.extend(page.items);
    }
    failure
}

#[tokio::test]
async fn market_selection_account_read_and_a_resumable_fill_report() -> TestResult {
    static THROTTLED: AtomicBool = AtomicBool::new(false);
    let server = TestServer::start(provider(&THROTTLED)).await;
    let policy = Policy::private(
        Credentials::new(KEY, SECRET)?,
        Nonces::new(1_794_043_800_000),
    );
    let api = Kraken::new(transport()?, KrakenConfig { policy }).with_base_url(server.url())?;

    // Existing workflows, unchanged: the report is scoped to the selected market.
    let market = api.select_market("XBT/USD").await?;
    let account = api.account_snapshot().await?;
    assert_eq!(market.id, "XXBTZUSD");
    let order = &account.orders.value.open["OQCLML-BW3P3-BUCMWZ"];

    let mut filters = TradesHistoryOptions::default();
    filters.pair = Some(market.id.clone());
    filters.start = Some(TradeBound::Time(1_688_600_000));
    filters.end = Some(TradeBound::Time(1_688_667_796)); // fixed window: offsets stay stable

    let mut report = FillReport::default();
    // Run 1: budget of two pages, 100 fills kept, resume point persisted.
    assert!(run(&api, &filters, &mut report).await.is_none());
    assert_eq!(
        (report.fills.len(), report.resume.clone(), report.complete),
        (100, Some(Resume::At(100)), false)
    );
    // Run 2: the provider throttles; nothing completed is lost, nothing is retried.
    let Some(Error::Api(throttled)) = run(&api, &filters, &mut report).await else {
        panic!("the throttled page must surface as an API failure");
    };
    assert_eq!(throttled.classes(), [ApiClass::RateLimited]);
    assert_eq!(
        (report.fills.len(), report.resume.clone(), report.complete),
        (100, Some(Resume::At(100)), false)
    );
    // Run 3: retry the failed page later, deliberately, with the same filters.
    assert!(run(&api, &filters, &mut report).await.is_none());
    assert_eq!(
        (report.fills.len(), report.resume.clone(), report.complete),
        (TOTAL, None, true)
    );

    // Report: exact volumes in lot units from decimal strings, joined to the open order.
    let lot = market.pair.lot_decimals;
    let volume = |side: &str| -> u64 {
        let fills = report.fills.values().filter(|t| t.side == side);
        fills.map(|t| units(&t.vol, lot)).sum()
    };
    let (bought, sold) = (volume("buy"), volume("sell"));
    assert_eq!((bought, sold), (60 * 1_000_000, 60 * 2_000_000));
    assert!(report.fills.values().all(|t| t.pair == market.id));
    assert!(
        report
            .fills
            .values()
            .all(|t| t.ordertxid == "OQCLML-BW3P3-BUCMWZ")
    );
    assert_eq!(order.descr.pair, market.pair.altname);

    // Wire: every private call verifies independently; one sequential caller sees rising nonces.
    let requests = server.requests();
    let private: Vec<_> = requests
        .iter()
        .filter(|r| r.path().starts_with("/0/private/"))
        .collect();
    let mut previous = 0;
    for r in &private {
        let nonce = field(r, "nonce").unwrap();
        assert_eq!(r.header("api-key"), Some(KEY));
        assert_eq!(
            r.header("api-sign").unwrap(),
            api_sign(r.path(), &nonce, &r.body)
        );
        let nonce: u64 = nonce.parse()?;
        assert!(nonce > previous);
        previous = nonce;
    }
    let trade_pages: Vec<_> = private
        .iter()
        .filter(|r| r.path() == "/0/private/TradesHistory")
        .collect();
    assert_eq!(
        trade_pages
            .iter()
            .map(|r| field(r, "ofs"))
            .collect::<Vec<_>>(),
        [
            None,
            Some("50".into()),
            Some("100".into()),
            Some("100".into())
        ]
    );
    for r in &trade_pages {
        assert_eq!(field(r, "pair").as_deref(), Some("XXBTZUSD"));
        assert_eq!(field(r, "start").as_deref(), Some("1688600000"));
        assert_eq!(field(r, "end").as_deref(), Some("1688667796"));
    }
    assert_eq!(requests.len(), 2 + 2 + 4);
    server.shutdown().await;
    Ok(())
}
