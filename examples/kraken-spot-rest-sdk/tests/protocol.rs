//! Wire contracts and hostile response cases; every connection is loopback.

use concord_sdk_kraken_spot_rest::{
    ApiClass, BalanceOptions, Credentials, ERROR_LIMIT, Error, Kraken, KrakenConfig, LocalError,
    Nonces, OpenOrdersOptions, Policy, RESPONSE_LIMIT, Rebase, concord, transport,
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use serde::Deserialize;
use std::{
    error::Error as _,
    time::{Duration, UNIX_EPOCH},
};

const BALANCES: &str = include_str!("../fixtures/balances.json");
const ORDERS: &str = include_str!("../fixtures/open-orders.json");

fn api(server: &TestServer, policy: Policy) -> Kraken {
    Kraken::new(transport().unwrap(), KrakenConfig { policy })
        .with_base_url(server.url())
        .unwrap()
}
fn credentials() -> Credentials {
    Credentials::new("fixture-key", "Zml4dHVyZS1zaWduaW5nLXNlY3JldA==").unwrap()
}
fn private() -> Policy {
    Policy::private(credentials(), Nonces::new(9000))
}

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

#[test]
fn official_signature_known_answer_and_exact_bytes() {
    #[derive(Deserialize)]
    struct Official {
        secret: String,
        #[serde(flatten)]
        vector: Vector,
    }
    let fixture: Official =
        serde_json::from_str(include_str!("../fixtures/official-signature.json")).unwrap();
    let key = Credentials::new("synthetic-public-key", &fixture.secret).unwrap();
    let v = fixture.vector;
    let signature = key.signature(&v.path, v.nonce, v.payload.as_bytes());
    assert_eq!(signature.to_str().unwrap(), v.signature);
    assert!(signature.is_sensitive());
    assert_ne!(
        key.signature("/0/private/Balance", v.nonce, v.payload.as_bytes()),
        signature
    );
    let altered = v.payload.replace("volume=1.25", "volume=1.250");
    assert_ne!(
        key.signature(&v.path, v.nonce, altered.as_bytes()),
        signature
    );
    assert_ne!(
        key.signature(&v.path, v.nonce + 1, v.payload.as_bytes()),
        signature
    );
}

#[tokio::test]
async fn signed_form_wire_matches_independent_python_vectors() {
    let fixture: Vectors =
        serde_json::from_str(include_str!("../fixtures/wire-vectors.json")).unwrap();
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/0/private/Balance" => Reply::json(200, BALANCES),
        "/0/private/OpenOrders" => Reply::json(200, ORDERS),
        _ => panic!("unexpected local route"),
    })
    .await;
    let policy = Policy::private(
        Credentials::new(&fixture.key, &fixture.secret).unwrap(),
        Nonces::new(9000),
    );
    let client = api(&server, policy.clone());
    let mut balance = BalanceOptions::default();
    balance.otp = Some("a+b &=/☃".into());
    balance.rebase_multiplier = Some(Rebase::Base);
    let operation = client
        .account()
        .balances(balance)
        .set_account_id("wallet +&/");
    let unsigned = operation.clone().prepare().unwrap();
    assert!(unsigned.request().headers().get("api-key").is_none());
    assert!(unsigned.request().headers().get("api-sign").is_none());
    assert!(
        !unsigned
            .request()
            .body()
            .unwrap()
            .as_bytes()
            .unwrap()
            .starts_with(b"nonce=")
    );
    let signed = policy.prepare_signed(operation).unwrap();
    assert!(signed.request_unredacted().headers()["api-key"].is_sensitive());
    assert!(signed.request_unredacted().headers()["api-sign"].is_sensitive());
    assert!(!format!("{signed:?}").contains("a+b"));
    signed.send().await.unwrap();
    let mut options = OpenOrdersOptions::default();
    options.trades = Some(true);
    options.userref = Some(-7);
    options.cl_ord_id = Some("client +&=/".into());
    options.consolidate_taker = Some(false);
    options.rebase_multiplier = Some(Rebase::Rebased);
    client.account().open_orders(options).send().await.unwrap();
    let requests = server.requests();
    for (r, expected) in requests.iter().zip(fixture.vectors) {
        assert_eq!(r.method, "POST");
        assert_eq!(r.path(), expected.path);
        assert_eq!(r.header("api-key"), Some("fixture-key"));
        assert_eq!(r.header("api-sign"), Some(expected.signature.as_str()));
        assert_eq!(
            r.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(r.header("accept"), Some("application/json"));
        assert_eq!(r.body_text(), expected.payload);
        assert!(!r.chunked);
        assert!(!r.body_text().contains(&fixture.secret));
    }
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].target,
        "/0/private/Balance?account_id=wallet+%2B%26%2F"
    );
    assert_eq!(requests[1].target, "/0/private/OpenOrders");
    server.shutdown().await;
}

#[tokio::test]
async fn public_requests_use_query_encoding_and_never_private_headers() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.method, "GET");
        assert!(r.header("api-key").is_none());
        assert!(r.header("api-sign").is_none());
        assert!(r.header("content-type").is_none());
        assert!(r.body.is_empty());
        assert_eq!(r.header("accept"), Some("application/json"));
        match r.target.as_str() {
            "/0/public/AssetPairs?pair=XBT%2FUSD+%2B%26%3D" => {
                Reply::json(200, include_str!("../fixtures/pairs.json"))
            }
            "/0/public/Ticker?pair=XXBTZUSD" => {
                Reply::json(200, include_str!("../fixtures/ticker.json"))
            }
            _ => panic!("unexpected local target"),
        }
    })
    .await;
    let client = api(&server, private());
    client
        .public()
        .pairs()
        .set_pair("XBT/USD +&=")
        .send()
        .await
        .unwrap();
    client.public().ticker("XXBTZUSD").send().await.unwrap();
    assert_eq!(server.requests().len(), 2);
    Kraken::validate_header_names().unwrap();
    server.shutdown().await;
}

#[test]
fn credential_validation_and_nonce_exhaustion_are_value_free() {
    for key in ["", "bad key", "key\r\nsecret-echo", "é"] {
        let error = Credentials::new(key, "Zg==").unwrap_err();
        assert_eq!(error, LocalError::InvalidKey);
        assert!(!format!("{error:?} {error}").contains("secret-echo"));
    }
    for secret in ["", "%%%secret-echo", "Zg", "Zg==\n"] {
        assert_eq!(
            Credentials::new("key", secret).unwrap_err(),
            LocalError::InvalidSecret
        );
    }
    let nonces = Nonces::new(u64::MAX - 1);
    assert_eq!(nonces.next().unwrap(), u64::MAX);
    assert_eq!(nonces.clone().next(), Err(LocalError::NonceExhausted));
    assert_eq!(nonces.next(), Err(LocalError::NonceExhausted));
}

#[tokio::test]
async fn local_refusals_make_no_requests() {
    let server = TestServer::start(|_: &Recorded| panic!("local refusal must not send")).await;
    let client = api(&server, Policy::public());
    assert!(matches!(
        client
            .account()
            .balances(BalanceOptions::default())
            .send()
            .await,
        Err(Error::Local(LocalError::MissingCredentials))
    ));
    let client = api(
        &server,
        Policy::private(credentials(), Nonces::new(u64::MAX)),
    );
    assert!(matches!(
        client
            .account()
            .open_orders(OpenOrdersOptions::default())
            .send()
            .await,
        Err(Error::Local(LocalError::NonceExhausted))
    ));
    let client = api(&server, private())
        .with_base_url(format!("{}/prefix", server.url()))
        .unwrap();
    assert!(matches!(
        client
            .account()
            .balances(BalanceOptions::default())
            .send()
            .await,
        Err(Error::Local(LocalError::SigningInput))
    ));
    let build_error = client
        .clone()
        .with_base_url("not a URL secret-echo")
        .unwrap_err();
    assert_eq!(build_error.kind(), concord::ErrorKind::Build);
    assert!(!format!("{build_error:?} {build_error}").contains("secret-echo"));
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn every_sdk_api_error_class_is_preserved_without_replay() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../fixtures/api-errors.json")).unwrap();
    for case in cases {
        let message = case["message"].as_str().unwrap().to_owned();
        let body = serde_json::to_vec(&serde_json::json!({"error":[message]})).unwrap();
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, &body)).await;
        let error = api(&server, private())
            .account()
            .balances(BalanceOptions::default())
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), 200);
        let Error::Api(ref failure) = error else {
            panic!("expected API error: {error}")
        };
        assert_eq!(failure.errors_unredacted(), &[message]);
        assert_eq!(
            format!("{:?}", failure.classes()[0]),
            case["class"].as_str().unwrap()
        );
        assert!(!format!("{error:?} {error}").contains("secret-echo"));
        assert!(error.source().is_none());
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn api_error_order_and_unknown_strings_survive() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(
            200,
            r#"{"error":["EAPI:Invalid nonce","EUnknown:secret-echo","EAPI:Invalid nonce"]}"#,
        )
    })
    .await;
    let error = api(&server, private())
        .account()
        .open_orders(OpenOrdersOptions::default())
        .send()
        .await
        .unwrap_err();
    let Error::Api(e) = error else {
        panic!("expected API failure")
    };
    assert_eq!(
        e.classes(),
        [
            ApiClass::InvalidNonce,
            ApiClass::Other,
            ApiClass::InvalidNonce
        ]
    );
    assert_eq!(e.errors_unredacted().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn http_status_is_authoritative_and_retry_after_is_advisory() {
    for status in [400, 401, 403, 404, 429, 500, 503] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, r#"{"error":["EUnknown:secret-echo"]}"#)
                .header("retry-after", "3")
                .header("x-echo", "secret-echo")
        })
        .await;
        let error = api(&server, private())
            .account()
            .balances(BalanceOptions::default())
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), status);
        assert!(!format!("{error:?} {error}").contains("secret-echo"));
        let Error::Http(e) = error else {
            panic!("expected HTTP failure")
        };
        assert_eq!(
            e.retry_after(UNIX_EPOCH).unwrap(),
            Some(Duration::from_secs(3))
        );
        assert_eq!(
            e.document.unwrap().errors_unredacted(),
            &["EUnknown:secret-echo"]
        );
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unreadable_http_error_bodies_retain_status_and_document_failure() {
    use concord::ErrorKind::{Decode, Limit, Metadata, Transport};
    for (case, expected) in [
        (0, Decode),
        (1, Metadata),
        (2, Limit),
        (3, Transport),
        (4, Decode),
    ] {
        let server = TestServer::start(move |_: &Recorded| match case {
            0 => Reply::json(503, include_str!("../fixtures/malformed.json")),
            1 => Reply::text(503, "text/html", "secret-echo"),
            2 => Reply::json(503, vec![b' '; ERROR_LIMIT + 1]),
            3 => Reply::json(503, "").with_body(Body::FailAfter(vec![
                concord::bytes::Bytes::from_static(b"{\"error\":["),
            ])),
            _ => Reply::json(503, ""),
        })
        .await;
        let error = api(&server, Policy::public())
            .public()
            .pairs()
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), 503);
        let Error::Http(failure) = error else {
            panic!("expected HTTP failure")
        };
        let cause = failure.document.unwrap_err();
        assert_eq!(cause.kind(), expected);
        assert_eq!(cause.head().unwrap().status().as_u16(), 503);
        assert!(cause.operation().is_some());
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn successful_http_bodies_fail_at_the_correct_stage() {
    use concord::ErrorKind::{Decode, Limit, Metadata, Transport};
    for (case, expected) in [
        (0, Decode),
        (1, Decode),
        (2, Metadata),
        (3, Metadata),
        (4, Metadata),
        (5, Limit),
        (6, Transport),
        (7, Decode),
        (8, Metadata),
        (9, Decode),
    ] {
        let server = TestServer::start(move |_: &Recorded| match case {
            0 => Reply::json(200, include_str!("../fixtures/malformed.json")),
            1 => Reply::json(200, include_str!("../fixtures/wrong-shape.json")),
            2 => Reply::text(200, "text/html", BALANCES),
            3 => Reply::new(200).body(BALANCES),
            4 => Reply::json(200, BALANCES).header("content-type", "application/json"),
            5 => Reply::json(200, vec![b' '; RESPONSE_LIMIT + 1]),
            6 => Reply::json(200, "").with_body(Body::FailAfter(vec![
                concord::bytes::Bytes::from_static(b"{\"error\":[]"),
            ])),
            7 => Reply::json(200, ""),
            8 => Reply::text(200, "application/json; charset=latin1", BALANCES),
            _ => Reply::json(200, b"\xffsecret-echo"),
        })
        .await;
        let error = api(&server, private())
            .account()
            .balances(BalanceOptions::default())
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), 200);
        assert!(!format!("{error:?} {error}").contains("secret-echo"));
        let Error::Concord(e) = error else {
            panic!("expected Concord failure")
        };
        assert_eq!(e.kind(), expected);
        assert_eq!(e.head().unwrap().status().as_u16(), 200);
        assert!(e.operation().is_some());
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn missing_or_null_result_is_a_protocol_failure() {
    for body in [
        include_str!("../fixtures/missing-result.json"),
        r#"{"error":[],"result":null}"#,
    ] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, body)).await;
        let error = api(&server, Policy::public())
            .public()
            .pairs()
            .send()
            .await
            .unwrap_err();
        assert!(matches!(error, Error::MissingResult(_)));
        assert_eq!(error.status().unwrap().as_u16(), 200);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn headless_transport_failure_is_not_retried() {
    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let error = api(&server, private())
        .account()
        .balances(BalanceOptions::default())
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status(), None);
    let Error::Concord(e) = error else {
        panic!("expected transport failure")
    };
    assert_eq!(e.kind(), concord::ErrorKind::Transport);
    assert!(e.head().is_none());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn custom_credentials_are_never_forwarded_on_redirects() {
    let destination = TestServer::start(|_: &Recorded| panic!("must not follow redirect")).await;
    let url = destination.url();
    let server = TestServer::start(move |_: &Recorded| {
        Reply::new(307).header("location", format!("{url}/0/private/Balance"))
    })
    .await;
    let error = api(&server, private())
        .account()
        .balances(BalanceOptions::default())
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 307);
    assert!(destination.requests().is_empty());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
    destination.shutdown().await;
}

#[tokio::test]
async fn all_credential_bearing_debug_and_display_outputs_are_redacted() {
    let secret = "Zml4dHVyZS1zaWduaW5nLXNlY3JldA==";
    let key = credentials();
    let policy = private();
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(200, BALANCES).header("set-cookie", "secret-echo")
    })
    .await;
    let config = KrakenConfig {
        policy: policy.clone(),
    };
    let config_debug = format!("{config:?}");
    let client = api(&server, policy.clone());
    let scope = client.account();
    let mut options = BalanceOptions::default();
    options.otp = Some("otp-secret-echo".into());
    let options_debug = format!("{options:?}");
    let op = scope
        .balances(options)
        .set_account_id("account-secret-echo");
    let prepared = op.clone().prepare().unwrap();
    let signed = policy.prepare_signed(op.clone()).unwrap();
    let signature = signed.request_unredacted().headers()["api-sign"]
        .to_str()
        .unwrap()
        .to_owned();
    let response = signed.send().await.unwrap();
    let outputs = [
        format!("{key:?} {key}"),
        format!("{policy:?}"),
        config_debug,
        options_debug,
        format!("{client:?}"),
        format!("{scope:?}"),
        format!("{op:?}"),
        format!("{prepared:?}"),
        format!("{response:?}"),
    ];
    for shown in outputs {
        for value in [
            "fixture-key",
            secret,
            "fixture-signing-secret",
            "secret-echo",
            &signature,
        ] {
            assert!(!shown.contains(value), "secret appeared in diagnostics");
        }
    }
    server.shutdown().await;
}

#[tokio::test]
async fn retry_after_rejects_repeated_and_invalid_fields_and_accepts_a_date() {
    use concord_sdk_kraken_spot_rest::RetryAfterError;
    for case in 0..4 {
        let server = TestServer::start(move |_: &Recorded| {
            let reply = Reply::json(429, r#"{"error":["EAPI:Rate limit exceeded"]}"#);
            match case {
                0 => reply.header("retry-after", "1").header("retry-after", "2"),
                1 => reply.header("retry-after", "+3-secret-echo"),
                2 => reply.header("retry-after", "Thu, 01 Jan 1970 00:00:05 GMT"),
                _ => reply,
            }
        })
        .await;
        let error = api(&server, Policy::public())
            .public()
            .pairs()
            .send()
            .await
            .unwrap_err();
        let Error::Http(failure) = error else {
            panic!("expected HTTP failure")
        };
        let parsed = failure.retry_after(UNIX_EPOCH);
        match case {
            0 => assert_eq!(parsed, Err(RetryAfterError::Repeated)),
            1 => assert_eq!(parsed, Err(RetryAfterError::Invalid)),
            2 => assert_eq!(parsed.unwrap(), Some(Duration::from_secs(5))),
            _ => assert_eq!(parsed.unwrap(), None),
        }
        assert!(!format!("{failure:?}").contains("secret-echo"));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn empty_and_optional_account_data_remain_readable() {
    let mut data: serde_json::Value = serde_json::from_str(ORDERS).unwrap();
    let order = data["result"]["open"]["OQCLML-BW3P3-BUCMWZ"]
        .as_object_mut()
        .unwrap();
    order.remove("trades");
    order.remove("userref");
    order.insert("refid".into(), serde_json::Value::Null);
    let body = serde_json::to_vec(&data).unwrap();
    let server = TestServer::start(move |r: &Recorded| {
        if r.path().ends_with("Balance") {
            Reply::json(200, r#"{"error":[],"result":{}}"#)
        } else {
            Reply::json(200, &body)
        }
    })
    .await;
    let snapshot = api(&server, private()).account_snapshot().await.unwrap();
    assert!(snapshot.balances.value.is_empty());
    let order = &snapshot.orders.value.open["OQCLML-BW3P3-BUCMWZ"];
    assert!(order.trades.is_none());
    assert!(order.userref.is_none());
    assert!(order.refid.is_none());
    server.shutdown().await;
}
