//! Trade-history maintenance: single page, `ofs` traversal, cap/resume, failures and nonces.
//! Loopback only; every provider answer is a fixture. Live provider calls: NOT_RUN.

use concord_sdk_kraken_spot_rest::{
    ApiClass, AssetClass, BalanceOptions, Credentials, Error, Incomplete, Kraken, KrakenConfig,
    Nonces, Page, PageProblem, PagesError, Policy, Resume, TRADES_PAGE_CAP, Trade, TradeBound,
    TradePages, TradeType, TradesHistoryOptions, TradesPageMeta, concord,
    concord_pagination::OffsetError, transport,
};
use concord_testserver::{Gate, Recorded, Reply, TestServer};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinSet;

/// Verbatim response example of the Get Trades History reference (read 2026-11-07); no `count`.
const OFFICIAL: &str = include_str!("../fixtures/trades-history.json");
const BALANCES: &str = include_str!("../fixtures/balances.json");
const PATH: &str = "/0/private/TradesHistory";

#[derive(Deserialize)]
struct Vector {
    path: String,
    nonce: u64,
    payload: String,
    signature: String,
}
#[derive(Deserialize)]
struct Vectors {
    key: String,
    secret: String,
    vectors: Vec<Vector>,
}
fn vectors() -> Vectors {
    serde_json::from_str(include_str!("../fixtures/trades-signatures.json")).unwrap()
}

fn private(nonces: Nonces) -> Policy {
    Policy::private(
        Credentials::new("fixture-key", "Zml4dHVyZS1zaWduaW5nLXNlY3JldA==").unwrap(),
        nonces,
    )
}
fn api(server: &TestServer, policy: Policy) -> Kraken {
    Kraken::new(transport().unwrap(), KrakenConfig { policy })
        .with_base_url(server.url())
        .unwrap()
}
fn form(r: &Recorded, name: &str) -> Option<String> {
    let mut values = r.form_pairs().into_iter().filter(|(k, _)| k == name);
    let value = values.next().map(|(_, v)| v);
    assert!(values.next().is_none(), "{name} repeated");
    value
}
fn nonce(r: &Recorded) -> u64 {
    form(r, "nonce").unwrap().parse().unwrap()
}
fn ofs(r: &Recorded) -> Option<usize> {
    form(r, "ofs").map(|v| v.parse().unwrap())
}
/// The form without the per-page fields: what must be identical on every page.
fn filters(r: &Recorded) -> Vec<(String, String)> {
    r.form_pairs()
        .into_iter()
        .filter(|(k, _)| k != "nonce" && k != "ofs")
        .collect()
}

/// Filters of the signed walk vectors (independent Python oracle in `fixtures/signatures.py`).
fn walk_options() -> TradesHistoryOptions {
    let mut options = TradesHistoryOptions::default();
    options.otp = Some("a+b &=/☃".into());
    options.trade_type = Some(TradeType::NoPosition);
    options.start = Some(TradeBound::Time(1_688_600_000));
    options.end = Some(TradeBound::Trade("THVRQM-33VKH-UCI7BS".into()));
    options.consolidate_taker = Some(false);
    options.ledgers = Some(true);
    options.aclass = Some(AssetClass::Forex);
    options.pair = Some("XXBTZUSD".into());
    options
}

/// A synthetic trade shaped like the reference example; id and trade_id identify it.
fn trade(i: usize) -> (String, Value) {
    let value = json!({
        "ordertxid": "OQCLML-BW3P3-BUCMWZ", "postxid": "TKH2SE-M7IF5-CFI7LT",
        "pair": "XXBTZUSD", "time": 1_688_667_796.880_2 - i as f64,
        "type": if i.is_multiple_of(2) { "buy" } else { "sell" }, "ordertype": "limit",
        "price": "30010.00000", "cost": "300.10000", "fee": "0.00000",
        "vol": "0.01000000", "margin": "0.00000", "misc": "",
        "trade_id": 39_000_000 + i, "maker": true
    });
    (format!("TS{i:04}-SYNTH-TRADE"), value)
}
fn page(range: std::ops::Range<usize>, count: Option<usize>) -> Vec<u8> {
    let trades: serde_json::Map<String, Value> = range.map(trade).collect();
    let mut result = json!({ "trades": trades });
    if let Some(count) = count {
        result["count"] = count.into();
    }
    serde_json::to_vec(&json!({ "error": [], "result": result })).unwrap()
}
/// Synthetic provider holding `total` matching trades, sliced 50 per page by `ofs`.
fn ledger(total: usize) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    move |r: &Recorded| {
        assert_eq!(r.path(), PATH);
        let at = ofs(r).unwrap_or(0);
        Reply::json(200, page(at..total.min(at + 50), Some(total)))
    }
}
fn ids(pages: &[Page<(String, Trade), u64, TradesPageMeta>]) -> BTreeSet<String> {
    pages
        .iter()
        .flat_map(|p| p.items.iter().map(|(id, _)| id.clone()))
        .collect()
}
fn expected_ids(range: std::ops::Range<usize>) -> BTreeSet<String> {
    range.map(|i| trade(i).0).collect()
}

#[tokio::test]
async fn official_example_decodes_and_the_first_page_signs_like_the_other_private_reads() {
    let fixture = vectors();
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.path(), PATH);
        Reply::json(200, OFFICIAL)
    })
    .await;
    let policy = Policy::private(
        Credentials::new(&fixture.key, &fixture.secret).unwrap(),
        Nonces::new(6000),
    );
    let client = api(&server, policy.clone());
    let unsigned = client
        .account()
        .trades_history(TradesHistoryOptions::default())
        .prepare()
        .unwrap();
    assert!(unsigned.request().headers().get("api-sign").is_none());
    let response = client
        .account()
        .trades_history(TradesHistoryOptions::default())
        .send()
        .await
        .unwrap();
    assert_eq!(response.value.count, None);
    assert_eq!(response.value.trades.len(), 2);
    let t = &response.value.trades["THVRQM-33VKH-UCI7BS"];
    assert_eq!(t.ordertxid, "OQCLML-BW3P3-BUCMWZ");
    assert_eq!(t.postxid.as_deref(), Some("TKH2SE-M7IF5-CFI7LT"));
    assert_eq!((t.pair.as_str(), t.side.as_str()), ("XXBTZUSD", "buy"));
    assert_eq!(
        (t.price.as_str(), t.cost.as_str()),
        ("30010.00000", "600.20000")
    );
    assert_eq!((t.fee.as_str(), t.vol.as_str()), ("0.00000", "0.02000000"));
    assert_eq!(t.time, 1_688_667_796.880_2);
    assert_eq!((t.trade_id, t.maker), (Some(40_274_859), Some(true)));
    assert_eq!(
        response.meta.operation().unwrap(),
        concord_sdk_kraken_spot_rest::KrakenAccountTradesHistory::operation()
    );
    let r = server.single();
    let v = &fixture.vectors[0];
    assert_eq!(
        (r.method.as_str(), r.target.as_str()),
        ("POST", v.path.as_str())
    );
    assert_eq!(r.header("api-key"), Some("fixture-key"));
    assert_eq!(r.header("api-sign"), Some(v.signature.as_str()));
    assert_eq!(r.body_text(), v.payload);
    assert_eq!(nonce(&r), v.nonce);
    assert_eq!(
        r.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn walk_reads_until_count_with_identical_filters_and_a_fresh_signed_nonce_per_page() {
    let fixture = vectors();
    let server = TestServer::start(ledger(120)).await;
    let source = Nonces::new(7000);
    let client = api(&server, private(source.clone()));
    let pages: TradePages = client.trade_history(walk_options(), None, Resume::Start);
    assert!(
        server.requests().is_empty(),
        "constructing a traversal sends nothing"
    );
    let pages = pages.collect_complete().await.unwrap();
    assert_eq!(
        pages.iter().map(|p| p.items.len()).collect::<Vec<_>>(),
        [50, 50, 20]
    );
    assert_eq!(
        pages
            .iter()
            .map(|p| (p.meta.offset, p.meta.count))
            .collect::<Vec<_>>(),
        [(0, 120), (50, 120), (100, 120)]
    );
    assert!(pages.iter().all(|p| p.meta.meta.status().as_u16() == 200));
    assert_eq!(ids(&pages), expected_ids(0..120));
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for (r, v) in requests.iter().zip(&fixture.vectors[1..]) {
        assert_eq!(r.target, v.path); // default wallet: no query
        assert_eq!(r.header("api-key"), Some("fixture-key"));
        assert_eq!(r.header("api-sign"), Some(v.signature.as_str()));
        assert_eq!(r.body_text(), v.payload);
        assert_eq!(filters(r), filters(&requests[0]));
    }
    assert_eq!(
        requests.iter().map(ofs).collect::<Vec<_>>(),
        [None, Some(50), Some(100)]
    );
    assert_eq!(
        requests.iter().map(nonce).collect::<Vec<_>>(),
        [7001, 7002, 7003]
    );
    assert_eq!(source.next().unwrap(), 7004);
    server.shutdown().await;
}

#[tokio::test]
async fn wallet_query_is_kept_on_every_page_and_an_empty_history_is_complete() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(
            r.target,
            "/0/private/TradesHistory?account_id=wallet+%2B%26%2F"
        );
        Reply::json(200, page(0..0, Some(0)))
    })
    .await;
    let client = api(&server, private(Nonces::new(0)));
    let pages = client
        .trade_history(
            TradesHistoryOptions::default(),
            Some("wallet +&/".into()),
            Resume::Start,
        )
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(pages.len(), 1);
    assert!(pages[0].items.is_empty() && pages[0].next.is_none());
    assert_eq!(server.single().body_text(), "nonce=1");

    let server = TestServer::start(ledger(60)).await;
    let mut pages = api(&server, private(Nonces::new(0))).trade_history(
        TradesHistoryOptions::default(),
        Some("w-1".into()),
        Resume::Start,
    );
    while pages.next_page().await.unwrap().is_some() {}
    let targets: Vec<_> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(targets, ["/0/private/TradesHistory?account_id=w-1"; 2]);
    server.shutdown().await;
}

#[tokio::test]
async fn page_cap_is_a_bounded_error_with_a_resume_point_that_completes_the_walk() {
    let server = TestServer::start(ledger(120)).await;
    let source = Nonces::new(7000);
    let client = api(&server, private(source));
    let incomplete: Incomplete<_, Error, u64> = client
        .trade_history(walk_options(), None, Resume::Start)
        .max_pages(NonZeroUsize::new(2).unwrap())
        .collect_complete()
        .await
        .unwrap_err();
    let PagesError::Bounded { ref resume, .. } = incomplete.error else {
        panic!("a cap with trades left is not completion")
    };
    assert_eq!(resume, &Resume::At(100));
    assert_eq!(ids(&incomplete.collected), expected_ids(0..100));
    assert_eq!(
        server.requests().len(),
        2,
        "the cap counts signed page requests"
    );

    let rest = client
        .trade_history(
            walk_options(),
            None,
            incomplete.error.resume().unwrap().clone(),
        )
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(ids(&rest), expected_ids(100..120));
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(ofs(&requests[2]), Some(100));
    assert_eq!(
        nonce(&requests[2]),
        7003,
        "resuming allocates a fresh nonce"
    );
    assert_eq!(filters(&requests[2]), filters(&requests[0]));
    // Diagnostics print counts, not keys or trades.
    let shown = format!("{incomplete:?} {incomplete} {:?}", incomplete.collected[0]);
    for value in ["TS0001-SYNTH-TRADE", "100", "a+b", "XXBTZUSD"] {
        assert!(!shown.contains(value), "value appeared in diagnostics");
    }
    server.shutdown().await;
}

#[tokio::test]
async fn count_reached_on_a_full_page_ends_without_an_extra_signed_request() {
    let server = TestServer::start(ledger(100)).await;
    let source = Nonces::new(0);
    let pages = api(&server, private(source.clone()))
        .trade_history(TradesHistoryOptions::default(), None, Resume::Start)
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[1].next, None);
    assert_eq!(
        server.requests().len(),
        2,
        "no probe for an empty third page"
    );
    assert_eq!(
        source.next().unwrap(),
        3,
        "no nonce spent beyond the two pages"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn the_default_cap_bounds_signed_requests_on_a_long_history() {
    let server = TestServer::start(ledger(10_000)).await;
    let incomplete = api(&server, private(Nonces::new(0)))
        .trade_history(TradesHistoryOptions::default(), None, Resume::Start)
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(TRADES_PAGE_CAP.get(), 20);
    assert_eq!(incomplete.collected.len(), TRADES_PAGE_CAP.get());
    assert_eq!(server.requests().len(), TRADES_PAGE_CAP.get());
    assert!(matches!(
        incomplete.error,
        PagesError::Bounded {
            resume: Resume::At(1000),
            ..
        }
    ));
    server.shutdown().await;
}

#[tokio::test]
async fn a_later_page_failure_keeps_completed_pages_and_is_never_retried() {
    for case in 0..4 {
        let server = TestServer::start(move |r: &Recorded| match ofs(r) {
            None => Reply::json(200, page(0..50, Some(120))),
            Some(50) => match case {
                0 => Reply::json(200, r#"{"error":["EAPI:Invalid nonce"]}"#),
                1 => Reply::json(200, r#"{"error":["EAPI:Rate limit exceeded"]}"#),
                2 => Reply::json(500, r#"{"error":["EService:Unavailable"]}"#),
                _ => Reply::hang_up(),
            },
            Some(_) => panic!("the walk must stop at the failed page"),
        })
        .await;
        let incomplete = api(&server, private(Nonces::new(0)))
            .trade_history(TradesHistoryOptions::default(), None, Resume::Start)
            .collect_complete()
            .await
            .unwrap_err();
        assert_eq!(incomplete.collected.len(), 1);
        assert_eq!(ids(&incomplete.collected), expected_ids(0..50));
        assert_eq!(incomplete.error.resume(), Some(&Resume::At(50)));
        let (error, failed, _) = incomplete.error.into_provider_error().unwrap();
        assert_eq!(failed, Resume::At(50));
        match (case, error) {
            (0, Error::Api(e)) => assert_eq!(e.classes(), [ApiClass::InvalidNonce]),
            (1, Error::Api(e)) => assert_eq!(e.classes(), [ApiClass::RateLimited]),
            (2, Error::Http(e)) => assert_eq!(e.meta.status().as_u16(), 500),
            (3, Error::Concord(e)) => assert_eq!(e.kind(), concord::ErrorKind::Transport),
            (_, other) => panic!("unexpected failure stage: {other}"),
        }
        let requests = server.requests();
        assert_eq!(requests.len(), 2, "no replay of a signed page");
        assert_eq!(requests.iter().map(nonce).collect::<Vec<_>>(), [1, 2]);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn inconsistent_pages_stop_the_walk_instead_of_looping_or_skipping() {
    use OffsetError::{BeyondCount, DidNotAdvance, TooManyItems};
    type Body = fn(Option<usize>) -> Vec<u8>;
    let cases: [(Body, usize, PageProblem); 4] = [
        // The reference's own example has no count: its first page is refused.
        (
            |_| OFFICIAL.as_bytes().to_vec(),
            0,
            PageProblem::MissingCount,
        ),
        (
            |_| page(0..51, Some(120)),
            0,
            PageProblem::Offset(TooManyItems),
        ),
        (
            |at| page(if at.is_none() { 0..50 } else { 0..0 }, Some(120)),
            50,
            PageProblem::Offset(DidNotAdvance),
        ),
        (
            |at| {
                page(
                    if at.is_none() { 0..50 } else { 50..100 },
                    Some(at.map_or(120, |_| 60)),
                )
            },
            50,
            PageProblem::Offset(BeyondCount),
        ),
    ];
    for (body, failed_at, problem) in cases {
        let server = TestServer::start(move |r: &Recorded| Reply::json(200, body(ofs(r)))).await;
        let incomplete = api(&server, private(Nonces::new(0)))
            .trade_history(TradesHistoryOptions::default(), None, Resume::Start)
            .collect_complete()
            .await
            .unwrap_err();
        let (error, failed, _) = incomplete.error.into_provider_error().unwrap();
        let Error::Page(failure) = error else {
            panic!("expected a page refusal")
        };
        assert_eq!(failure.problem, problem);
        assert_eq!(failure.offset, failed_at as u64);
        assert_eq!(failure.meta.status().as_u16(), 200);
        let expected_resume = if failed_at == 0 {
            Resume::Start
        } else {
            Resume::At(failed_at as u64)
        };
        assert_eq!(failed, expected_resume);
        assert_eq!(incomplete.collected.len(), usize::from(failed_at > 0));
        assert_eq!(server.requests().len(), 1 + usize::from(failed_at > 0));
        server.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_walks_and_reads_share_one_nonce_source_without_repeats() {
    const WALKS: usize = 8;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    // Uniqueness-only fixture: it never assumes arrival order across callers.
    let server = TestServer::start(move |r: &Recorded| {
        let pair = form(r, "pair");
        record.lock().unwrap().push((pair, ofs(r), nonce(r)));
        if r.path() == "/0/private/Balance" {
            return Reply::json(200, BALANCES);
        }
        let at = ofs(r).unwrap_or(0);
        Reply::json(200, page(at..120.min(at + 50), Some(120)))
    })
    .await;
    let source = Nonces::new(10_000);
    let client = api(&server, private(source.clone()));
    let mut jobs = JoinSet::new();
    for walk in 0..WALKS {
        let mut options = TradesHistoryOptions::default();
        options.pair = Some(format!("P{walk}"));
        let pages = client.trade_history(options, None, Resume::Start);
        jobs.spawn(async move { pages.collect_complete().await.map(|p| p.len()).ok() });
        let read = client.account().balances(BalanceOptions::default());
        jobs.spawn(async move { read.send().await.ok().map(|_| 0) });
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(done) = jobs.join_next().await {
            assert!(done.unwrap().is_some());
        }
    })
    .await
    .unwrap();
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), WALKS * 4);
    let nonces: BTreeSet<_> = seen.iter().map(|(_, _, n)| *n).collect();
    assert_eq!(nonces, (10_001..=10_000 + WALKS as u64 * 4).collect());
    // Within one walk a page is signed only after the previous page arrived, so its
    // nonces rise with its offsets; across walks and reads no order is asserted.
    let mut walks: BTreeMap<String, Vec<(Option<usize>, u64)>> = BTreeMap::new();
    for (pair, at, n) in seen {
        if let Some(pair) = pair {
            walks.entry(pair).or_default().push((at, n));
        }
    }
    assert_eq!(walks.len(), WALKS);
    for pages in walks.values_mut() {
        pages.sort_by_key(|(_, n)| *n);
        assert_eq!(
            pages.iter().map(|(at, _)| *at).collect::<Vec<_>>(),
            [None, Some(50), Some(100)]
        );
    }
    assert_eq!(source.next().unwrap(), 10_001 + WALKS as u64 * 4);
    server.shutdown().await;
}

#[tokio::test]
async fn an_earlier_allocation_sent_during_a_walk_is_refused_and_the_walk_continues() {
    let last = Arc::new(Mutex::new(0_u64));
    let state = last.clone();
    let second_page = Gate::new();
    let release = second_page.clone();
    // Synthetic strict provider: any arrival not above the last accepted nonce fails.
    let server = TestServer::start(move |r: &Recorded| {
        let mut last = state.lock().unwrap();
        let current = nonce(r);
        if current <= *last {
            return Reply::json(200, r#"{"error":["EAPI:Invalid nonce"]}"#);
        }
        *last = current;
        if r.path() == "/0/private/Balance" {
            return Reply::json(200, BALANCES);
        }
        match ofs(r) {
            None => Reply::json(200, page(0..50, Some(60))),
            // Hold the walk's final page until the delayed read has been refused.
            _ => Reply::json(200, page(50..60, Some(60))).after(release.clone()),
        }
    })
    .await;
    let policy = private(Nonces::new(3000));
    let client = api(&server, policy.clone());
    // Allocated first (3001), deliberately sent later.
    let held = policy
        .prepare_signed(client.account().balances(BalanceOptions::default()))
        .unwrap();
    let mut pages = client.trade_history(TradesHistoryOptions::default(), None, Resume::Start);
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 50);
    let (late, rest) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            async {
                let outcome = held.send().await;
                second_page.open();
                outcome
            },
            async {
                let page = pages.next_page().await;
                let end = pages.next_page().await;
                (page, end)
            }
        )
    })
    .await
    .unwrap();
    let Error::Api(failure) = late.unwrap_err() else {
        panic!("a lower nonce arriving later must fail")
    };
    assert_eq!(failure.classes(), [ApiClass::InvalidNonce]);
    let (page, end) = rest;
    assert_eq!(page.unwrap().unwrap().items.len(), 10);
    assert!(end.unwrap().is_none());
    let requests = server.requests();
    assert_eq!(requests.len(), 3, "nothing re-signed or replayed");
    let by_path: Vec<_> = requests
        .iter()
        .map(|r| (r.path().to_owned(), nonce(r)))
        .collect();
    assert!(by_path.contains(&("/0/private/Balance".into(), 3001)));
    assert!(by_path.contains(&(PATH.into(), 3002)));
    assert!(by_path.contains(&(PATH.into(), 3003)));
    server.shutdown().await;
}

#[test]
fn trade_history_options_debug_withholds_values() {
    let mut options = walk_options();
    options.otp = Some("otp-secret-echo".into());
    assert_eq!(format!("{options:?}"), "TradesHistoryOptions { .. }");
}
