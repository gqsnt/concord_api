//! Connected workflows, concurrent nonce uniqueness and deterministic arrival reordering.

use concord_sdk_kraken_spot_rest::{
    ApiClass, BalanceOptions, Credentials, Error, Kraken, KrakenConfig, LocalError, Nonces,
    OpenOrdersOptions, Policy, transport,
};
use concord_testserver::{Gate, Recorded, Reply, TestServer};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::Barrier, task::JoinSet};

const PAIRS: &str = include_str!("../fixtures/pairs.json");
const TICKER: &str = include_str!("../fixtures/ticker.json");
const BALANCES: &str = include_str!("../fixtures/balances.json");
const ORDERS: &str = include_str!("../fixtures/open-orders.json");

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
fn nonce(request: &Recorded) -> u64 {
    let values: Vec<_> = request
        .form_pairs()
        .into_iter()
        .filter(|(k, _)| k == "nonce")
        .collect();
    assert_eq!(values.len(), 1);
    values[0].1.parse().unwrap()
}

#[tokio::test]
async fn discover_select_quote_then_authenticate_and_read_account() {
    let server = TestServer::start(|r: &Recorded| match r.target.as_str() {
        "/0/public/AssetPairs" => Reply::json(200, PAIRS),
        "/0/public/Ticker?pair=XXBTZUSD" => Reply::json(200, TICKER),
        "/0/private/Balance" => Reply::json(200, BALANCES),
        "/0/private/OpenOrders" => Reply::json(200, ORDERS),
        _ => panic!("unexpected fixture request"),
    })
    .await;
    let client = api(&server, private(Nonces::new(5000)));
    let market = client.select_market("XBT/USD").await.unwrap();
    assert_eq!(market.id, "XXBTZUSD");
    assert_eq!(market.pair.base, "XXBT");
    assert_eq!(market.pair.quote, "ZUSD");
    assert_eq!(market.ticker.c[0], "37500.5");
    assert_eq!(market.ticker.b[0], "37500.0");
    assert_eq!(
        market.pairs_meta.operation().unwrap(),
        concord_sdk_kraken_spot_rest::KrakenPublicPairs::operation()
    );
    let snapshot = client.account_snapshot().await.unwrap();
    assert_eq!(snapshot.balances.value["XXBT"], "1011.1908877900");
    assert_eq!(snapshot.balances.value["ETH2.S"], "198.3970800000");
    let order = &snapshot.orders.value.open["OQCLML-BW3P3-BUCMWZ"];
    assert_eq!(order.descr.pair, market.pair.altname);
    assert_eq!(order.status, "open");
    assert_eq!(order.vol, "1.25000000");
    assert_eq!(order.vol_exec, "0.37500000");
    assert_eq!(order.trades.as_ref().unwrap(), &["TCCCTY-WE2O6-P3NB37"]);
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    for public in &requests[..2] {
        assert!(public.header("api-key").is_none());
        assert!(public.header("api-sign").is_none());
    }
    let signatures: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/account-signatures.json")).unwrap();
    for (r, expected) in requests[2..].iter().zip(signatures.as_array().unwrap()) {
        assert_eq!(r.method, "POST");
        assert_eq!(r.header("api-key"), Some("fixture-key"));
        assert_eq!(r.header("api-sign"), expected["signature"].as_str());
        assert_eq!(r.body_text(), expected["payload"].as_str().unwrap());
    }
    assert_eq!(nonce(&requests[2]), 5001);
    assert_eq!(nonce(&requests[3]), 5002);
    assert_eq!(snapshot.orders.meta.status().as_u16(), 200);
    server.shutdown().await;
}

#[tokio::test]
async fn canonical_and_alternate_pair_selectors_both_resolve() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/0/public/AssetPairs" => Reply::json(200, PAIRS),
        "/0/public/Ticker" => Reply::json(200, TICKER),
        _ => panic!("public fixture route expected"),
    })
    .await;
    let client = api(&server, Policy::public());
    for selector in ["XXBTZUSD", "XBTUSD"] {
        assert_eq!(client.select_market(selector).await.unwrap().id, "XXBTZUSD");
    }
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn absent_offline_and_ambiguous_pairs_stop_before_ticker() {
    for (selector, ambiguous) in [("unknown", false), ("ETHUSD", false), ("XBTUSD", true)] {
        let mut data: serde_json::Value = serde_json::from_str(PAIRS).unwrap();
        if ambiguous {
            data["result"]["XETHZUSD"]["altname"] = "XBTUSD".into();
        }
        let body = serde_json::to_vec(&data).unwrap();
        let server = TestServer::start(move |r: &Recorded| {
            assert_eq!(r.path(), "/0/public/AssetPairs");
            Reply::json(200, &body)
        })
        .await;
        assert!(matches!(
            api(&server, Policy::public()).select_market(selector).await,
            Err(Error::Local(LocalError::PairUnavailable))
        ));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn mismatched_or_empty_ticker_is_not_a_selected_market() {
    for result in [
        r#"{}"#,
        r#"{"unexpected":{"a":["1","1","1"],"b":["1","1","1"],"c":["1","1"],"v":["1","1"],"p":["1","1"],"t":[1,1],"l":["1","1"],"h":["1","1"],"o":"1"}}"#,
    ] {
        let ticker = format!(r#"{{"error":[],"result":{result}}}"#);
        let server = TestServer::start(move |r: &Recorded| match r.path() {
            "/0/public/AssetPairs" => Reply::json(200, PAIRS),
            "/0/public/Ticker" => Reply::json(200, &ticker),
            _ => panic!("public fixture route expected"),
        })
        .await;
        assert!(matches!(
            api(&server, Policy::public()).select_market("XBTUSD").await,
            Err(Error::Local(LocalError::TickerMismatch))
        ));
        assert_eq!(server.requests().len(), 2);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn account_workflow_is_fail_fast_at_each_read() {
    for failed_path in ["/0/private/Balance", "/0/private/OpenOrders"] {
        let server = TestServer::start(move |r: &Recorded| {
            if r.path() == failed_path {
                return Reply::json(200, r#"{"error":["EGeneral:Permission denied"]}"#);
            }
            assert_eq!(r.path(), "/0/private/Balance");
            Reply::json(200, BALANCES)
        })
        .await;
        let error = api(&server, private(Nonces::new(0)))
            .account_snapshot()
            .await
            .unwrap_err();
        let Error::Api(e) = error else {
            panic!("expected API failure")
        };
        assert_eq!(e.classes(), [ApiClass::Permission]);
        assert_eq!(
            server.requests().len(),
            if failed_path.ends_with("Balance") {
                1
            } else {
                2
            }
        );
        server.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_cloned_endpoints_share_one_source_without_duplicate_nonces() {
    const COUNT: usize = 64;
    let observed = Arc::new(Mutex::new(BTreeSet::new()));
    let recorded = observed.clone();
    // Synthetic uniqueness-only fixture: it deliberately does not assume arrival order.
    let server = TestServer::start(move |r: &Recorded| {
        assert_eq!(r.path(), "/0/private/Balance");
        assert!(
            recorded.lock().unwrap().insert(nonce(r)),
            "nonce repeated on the wire"
        );
        assert_eq!(r.header("api-key"), Some("fixture-key"));
        assert!(r.header("api-sign").is_some());
        Reply::json(200, BALANCES)
    })
    .await;
    let source = Nonces::new(10000);
    let client = api(&server, private(source.clone()));
    let template = client.account().balances(BalanceOptions::default());
    // Unsigned preparation does not allocate; cloning the endpoint cannot clone a nonce.
    let _unsigned = template.clone().prepare().unwrap();
    let barrier = Arc::new(Barrier::new(COUNT + 1));
    let mut jobs = JoinSet::new();
    for _ in 0..COUNT {
        let op = template.clone();
        let ready = barrier.clone();
        jobs.spawn(async move {
            ready.wait().await;
            op.send().await
        });
    }
    barrier.wait().await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(result) = jobs.join_next().await {
            result.unwrap().unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(*observed.lock().unwrap(), (10001..=10064).collect());
    assert_eq!(server.requests().len(), COUNT);
    // Cancellation or intentional unused allocations cannot rewind this counter.
    assert_eq!(source.next().unwrap(), 10065);
    server.shutdown().await;
}

#[tokio::test]
async fn unique_allocation_does_not_guarantee_arrival_order_invalid_nonce_is_reported() {
    let last = Arc::new(Mutex::new(0_u64));
    let state = last.clone();
    // Synthetic strict provider: reject any arrival not greater than the last one.
    let server = TestServer::start(move |r: &Recorded| {
        let mut last = state.lock().unwrap();
        let current = nonce(r);
        if current <= *last {
            return Reply::json(200, r#"{"error":["EAPI:Invalid nonce"]}"#);
        }
        *last = current;
        match r.path() {
            "/0/private/Balance" => Reply::json(200, BALANCES),
            "/0/private/OpenOrders" => Reply::json(200, ORDERS),
            _ => panic!("private fixture route expected"),
        }
    })
    .await;
    let policy = private(Nonces::new(2000));
    let client = api(&server, policy.clone());
    let lower = policy
        .prepare_signed(client.account().balances(BalanceOptions::default()))
        .unwrap();
    let higher = policy
        .prepare_signed(client.account().open_orders(OpenOrdersOptions::default()))
        .unwrap();
    let release_lower = Gate::new();
    // Both futures are polled together. Controlled scheduling holds the first
    // allocation until the second response arrives; there are no timing sleeps.
    let (lower_result, higher_result) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            async {
                release_lower.wait().await;
                lower.send().await
            },
            async {
                let outcome = higher.send().await;
                release_lower.open();
                outcome
            },
        )
    })
    .await
    .unwrap();
    higher_result.unwrap();
    let Error::Api(failure) = lower_result.unwrap_err() else {
        panic!("old arrival must fail")
    };
    assert_eq!(failure.classes(), [ApiClass::InvalidNonce]);
    let requests = server.requests();
    assert_eq!(
        requests.iter().map(nonce).collect::<Vec<_>>(),
        [2001 + 1, 2001]
    );
    assert_eq!(requests.iter().map(nonce).collect::<BTreeSet<_>>().len(), 2);
    assert_eq!(requests.len(), 2); // no InvalidNonce retry or re-signing loop
    server.shutdown().await;
}
