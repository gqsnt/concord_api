//! Secrets: Credentials API key + decoded HMAC key (src/auth.rs:57); distinct account_id
//! queries for balances/orders/trades (src/lib.rs:53,57,62); OTP per option record; withheld
//! string options cl_ord_id, trade start/end bounds and pair (src/models.rs:76-222).
//! Every input gets its own canary. Only the signing-key canary is wire-exempt: it is supplied
//! Base64-encoded and used to sign, never sent. Verify API-Sign independently with its decoded
//! bytes and prove neither raw nor Base64 key is on the wire. No trading writes exist; all
//! seven scenarios cover signed balance reads, with orders/history sent for the controls.
//! Excludes native reqwest, request_unredacted/source_unredacted/body_unredacted and returned
//! clear model/TradeBound fields. Response wrapper values and provider error echoes are checked.

use concord_check_canary::Canaries;
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::time::Duration;

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_testserver::refused_url()
}

fn reply(case: usize, media: &str, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::text(400, media, problem),
        2 => Reply::text(503, media, problem),
        3 => Reply::text(200, media, format!("<{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::text(200, media, "").with_body(Body::FailAfter(vec![
            format!("<{{incomplete {echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error<D: 'static>(c: &mut Canaries, e: &concord::Error<D>) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

async fn receive<T: std::fmt::Debug, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) {
    c.assert_debug_absent("Prepared", &prepared);
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode().await;
            c.assert_debug_absent("native decoded Result", &decoded);
            if let Err(e) = decoded {
                concord_error(c, &e);
            }
        }
        Err(e) => concord_error(c, &e),
    }
}

use base64::{Engine as _, engine::general_purpose::STANDARD};
use concord_sdk_kraken_spot_rest::{
    BalanceOptions, Credentials, Error, Kraken, KrakenConfig, Nonces, OpenOrdersOptions, Policy,
    Resume, TradeBound, TradesHistoryOptions,
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256, Sha512};

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    c.assert_debug_absent("error status", &e.status());
    match e {
        Error::Concord(e) => concord_error(c, e),
        Error::Http(h) => {
            c.assert_debug_absent("HTTP failure", h);
            c.assert_debug_absent("HTTP metadata", &h.meta);
            c.assert_debug_absent("provider document", &h.document);
            if let Err(e) = &h.document {
                concord_error(c, e);
            }
        }
        Error::Api(a) => {
            c.assert_debug_absent("API failure", a);
        }
        _ => {}
    }
}

fn verify_signature(r: &Recorded, secret: &str, encoded_secret: &str) {
    let nonce = r
        .form_pairs()
        .into_iter()
        .find(|(k, _)| k == "nonce")
        .unwrap()
        .1;
    let mut hash = Sha256::new();
    hash.update(nonce.as_bytes());
    hash.update(&r.body);
    let mut mac = Hmac::<Sha512>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(r.path().as_bytes());
    mac.update(&hash.finalize());
    let expected = STANDARD.encode(mac.finalize().into_bytes());
    assert_eq!(r.header("api-sign"), Some(expected.as_str()));
    let wire = concord_check_canary::wire_text(r);
    assert!(!wire.contains(secret));
    assert!(!wire.contains(encoded_secret));
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("kraken-spot-rest");
    let key = c.issue();
    let signing_secret = c.issue();
    let encoded_secret = STANDARD.encode(&signing_secret);
    let balance_account = c.issue();
    let order_account = c.issue();
    let trade_account = c.issue();
    let mut balance = BalanceOptions::default();
    balance.otp = Some(c.issue());
    let mut orders = OpenOrdersOptions::default();
    orders.otp = Some(c.issue());
    orders.cl_ord_id = Some(c.issue());
    let mut trades = TradesHistoryOptions::default();
    trades.otp = Some(c.issue());
    trades.start = Some(TradeBound::Trade(c.issue()));
    trades.end = Some(TradeBound::Trade(c.issue()));
    trades.pair = Some(c.issue());
    let credentials = Credentials::new(&key, &encoded_secret).unwrap();
    c.assert_debug_absent("credentials", &credentials);
    c.assert_display_absent("credentials", &credentials);
    c.assert_debug_absent("balance options", &balance);
    c.assert_debug_absent("order options", &orders);
    c.assert_debug_absent("trade options", &trades);
    for case in 0..7 {
        let echo = key.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let success = match r.path() {
                "/0/private/OpenOrders" => r#"{"error":[],"result":{"open":{}}}"#.to_owned(),
                "/0/private/TradesHistory" => {
                    r#"{"error":[],"result":{"trades":{},"count":0}}"#.to_owned()
                }
                _ => format!(r#"{{"error":[],"result":{{"ZUSD":"{echo}"}}}}"#),
            };
            let problem = format!(r#"{{"error":["EGeneral:Invalid arguments:{echo}"]}}"#);
            reply(case, "application/json", &success, &problem, &echo)
        })
        .await;
        let nonces = Nonces::new(5000);
        c.assert_debug_absent("nonce source", &nonces);
        let policy = Policy::private(credentials.clone(), nonces);
        c.assert_debug_absent("policy", &policy);
        let config = KrakenConfig {
            policy: policy.clone(),
        };
        c.assert_debug_absent("config", &config);
        let api = Kraken::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let account = api.account();
        let read = account
            .balances(balance.clone())
            .set_account_id(&balance_account);
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("public scope", &api.public());
        c.assert_debug_absent("account scope", &account);
        c.assert_debug_absent("balance endpoint", &read);
        // The unsigned Prepared still tests Concord's redaction/response/error path.
        receive(&mut c, read.clone().prepare().unwrap()).await;
        let signed = policy.prepare_signed(read).unwrap();
        assert_eq!(
            format!("{signed:?}"),
            "Kraken.account.balances: POST <withheld>"
        );
        c.assert_debug_absent("signed request", &signed);
        let result = signed.send().await;
        c.assert_debug_absent("signed balance Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("balance response wrapper", &response);
                c.assert_debug_absent("balance metadata", &response.meta);
                assert_eq!(response.value["ZUSD"], key);
            }
            Err(e) => {
                error(&mut c, &e);
                if case == 1 {
                    assert_eq!(e.status().unwrap().as_u16(), 400);
                    assert!(e.to_string().contains("400"));
                }
            }
        }
        let op = account
            .open_orders(orders.clone())
            .set_account_id(&order_account);
        c.assert_debug_absent("orders endpoint", &op);
        c.assert_debug_absent("orders Prepared", &op.clone().prepare().unwrap());
        let result = op.send().await;
        c.assert_debug_absent("orders Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = result {
            error(&mut c, &e);
        }
        let op = account
            .trades_history(trades.clone())
            .set_account_id(&trade_account);
        c.assert_debug_absent("trade endpoint", &op);
        c.assert_debug_absent("trades Prepared", &op.clone().prepare().unwrap());
        let result = op.send().await;
        c.assert_debug_absent("trades Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = result {
            error(&mut c, &e);
        }
        let mut walk =
            api.trade_history(trades.clone(), Some(trade_account.clone()), Resume::Start);
        c.assert_debug_absent("trade traversal", &walk);
        let page = walk.next_page().await;
        c.assert_debug_absent("trade page Result", &page);
        if let Err(e) = page {
            c.assert_error_absent("PagesError", &e);
        }
        c.assert_debug_absent("traversal checkpoint", &walk.checkpoint());
        c.assert_debug_absent("traversal progress", &walk.progress());
        c.assert_debug_absent("traversal report", &walk.finish());
        if case == 0 {
            let requests = server.requests();
            c.assert_on_wire_except(&requests, &[&signing_secret]);
            // receive() above intentionally uses an unsigned native Prepared.
            for r in requests.iter().skip(1) {
                assert_eq!(r.header("api-key"), Some(key.as_str()));
                verify_signature(r, &signing_secret, &encoded_secret);
            }
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    let echo = key.clone();
    let server = TestServer::start(move |_| {
        Reply::json(
            200,
            format!(r#"{{"error":["EGeneral:Invalid arguments:{echo}"]}}"#),
        )
    })
    .await;
    let api = Kraken::new(
        http(),
        KrakenConfig {
            policy: Policy::private(credentials, Nonces::new(1)),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    let e = api.account().balances(balance).send().await.unwrap_err();
    error(&mut c, &e);
    assert!(matches!(e, Error::Api(_)));
    server.shutdown().await;
    let e = Credentials::new(&format!("{key}\n"), &encoded_secret).unwrap_err();
    c.assert_debug_absent("local credential error", &e);
    c.assert_error_absent("local credential error", &e);
    assert!(c.checked() >= 300);
    println!(
        "kraken-spot-rest: {} renderings; positive control and independent signatures passed (HMAC key exempt)",
        c.checked()
    );
}
