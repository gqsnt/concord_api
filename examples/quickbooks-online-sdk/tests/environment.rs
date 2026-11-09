//! Environment replacement and wire controls. Every receiver is loopback; the HTTPS proxy
//! refuses CONNECT rather than opening a tunnel to Intuit.

mod support;

use std::time::Duration;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_quickbooks_online::concord::{ErrorKind, reqwest};
use concord_sdk_quickbooks_online::{
    Connection, Decimal, EntityId, Environment, Error, NewInvoice, OAuthApp, QuickBooks,
    RefreshToken, RequestId, SalesItemLine, UncertainCause,
};
use support::*;

fn stored(
    environment: Environment,
    tokens: &TestServer,
    realm_id: &str,
    refresh: &str,
) -> Connection {
    OAuthApp::new(environment, CLIENT_ID, CLIENT_SECRET, REDIRECT)
        .unwrap()
        .with_token_base_url(&tokens.url())
        .unwrap()
        .connection(realm(realm_id), RefreshToken::new(refresh).unwrap())
}

fn invoice() -> NewInvoice {
    NewInvoice::new(
        EntityId::new("29").unwrap(),
        SalesItemLine::new(Decimal::from_cents(10000), EntityId::new("1").unwrap()),
    )
}

async fn official_targets(initial: Environment, next: Environment, replace: bool) -> Vec<Recorded> {
    let tokens = TestServer::start(|r| {
        assert_eq!(r.path(), TOKEN_PATH);
        json_fixture(200, "token-issued.json")
    })
    .await;
    let proxy = TestServer::start(|_| Reply::text(502, "text/plain", "no upstream tunnel")).await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .proxy(reqwest::Proxy::https(proxy.url()).unwrap())
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let next_connection = stored(next, &tokens, REALM_B, "synthetic-refresh-next");
    let api = if replace {
        QuickBooks::connect(
            http,
            stored(initial, &tokens, REALM_A, "synthetic-refresh-old"),
        )
        .set_connection(next_connection)
    } else {
        QuickBooks::connect(http, next_connection)
    };

    let read = api.customer(EntityId::new("29").unwrap()).read();
    // Preparation stays token-free and reads the replacement connection's base.
    let prepared = read.clone().prepare().unwrap();
    let snapshot_environment = next;
    assert_eq!(
        prepared.request().url().as_str(),
        format!(
            "{}/v3/company/{REALM_B}/customer/29?minorversion=75",
            snapshot_environment.base_url()
        )
    );
    assert!(!prepared.request().headers().contains_key("authorization"));
    let error = read.send().await.unwrap_err();
    assert!(matches!(error, Error::Concord(e) if e.kind() == ErrorKind::Transport));
    let error = api
        .invoices()
        .create(invoice())
        .set_request_id(RequestId::new("environment-control").unwrap())
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::Uncertain(e) if matches!(e.cause(), UncertainCause::NoResponse(_)))
    );

    let issued = tokens.single();
    assert_eq!(issued.method, "POST");
    assert_eq!(issued.header("authorization"), Some(BASIC));
    assert_eq!(
        std::str::from_utf8(&issued.body).unwrap(),
        "grant_type=refresh_token&refresh_token=synthetic-refresh-next"
    );
    let observed = proxy.requests();
    assert_eq!(observed.len(), 2, "one CONNECT per read/write, no retries");
    for request in &observed {
        assert_eq!(request.method, "CONNECT");
        assert!(request.header("authorization").is_none());
    }
    observed
}

async fn cross_environment(initial: Environment, next: Environment, expected: &str) {
    let observed = official_targets(initial, next, true).await;
    assert_eq!(
        observed
            .iter()
            .map(|r| r.target.as_str())
            .collect::<Vec<_>>(),
        [expected, expected],
        "the replacement connection selects the read and write destinations"
    );
}

#[tokio::test]
async fn sandbox_to_production_uses_the_replacement_environment() {
    cross_environment(
        Environment::Sandbox,
        Environment::Production,
        "quickbooks.api.intuit.com:443",
    )
    .await;
}

#[tokio::test]
async fn production_to_sandbox_uses_the_replacement_environment() {
    cross_environment(
        Environment::Production,
        Environment::Sandbox,
        "sandbox-quickbooks.api.intuit.com:443",
    )
    .await;
}

fn assert_same_wire(actual: &Recorded, expected: &Recorded) {
    assert_eq!(actual.method, expected.method);
    assert_eq!(actual.target, expected.target);
    assert_eq!(actual.headers, expected.headers);
    assert_eq!(actual.body, expected.body);
    assert_eq!(actual.body_len, expected.body_len);
    assert_eq!(actual.chunked, expected.chunked);
    assert_eq!(actual.host, expected.host);
}

#[tokio::test]
async fn same_environment_replacement_keeps_the_official_wire_target() {
    for environment in [Environment::Sandbox, Environment::Production] {
        let replaced = official_targets(environment, environment, true).await;
        let direct = official_targets(environment, environment, false).await;
        for (actual, expected) in replaced.iter().zip(&direct) {
            assert_same_wire(actual, expected);
        }
    }
}

#[tokio::test]
async fn fixture_replacement_keeps_the_read_and_write_wire() {
    for (initial, next) in [
        (Environment::Sandbox, Environment::Production),
        (Environment::Production, Environment::Sandbox),
        (Environment::Sandbox, Environment::Sandbox),
        (Environment::Production, Environment::Production),
    ] {
        let server = TestServer::start(|r| match r.path() {
            TOKEN_PATH => json_fixture(200, "token-issued.json"),
            path if path.ends_with("/invoice") => json_fixture(200, "invoice-created.json"),
            _ => json_fixture(200, "customer-read.json"),
        })
        .await;
        let next_connection = stored(next, &server, REALM_B, "synthetic-refresh-next")
            .for_fixture(&server.url())
            .unwrap();
        let http = http(Duration::from_secs(5));
        let api = QuickBooks::connect(
            http.clone(),
            stored(initial, &server, REALM_A, "synthetic-refresh-old")
                .for_fixture(&server.url())
                .unwrap(),
        )
        .set_connection(next_connection.clone());
        let direct = QuickBooks::connect(http, next_connection);
        for client in [&api, &direct] {
            client
                .customer(EntityId::new("29").unwrap())
                .read()
                .send()
                .await
                .unwrap();
            client
                .invoices()
                .create(invoice())
                .set_request_id(RequestId::new("environment-control").unwrap())
                .send()
                .await
                .unwrap();
        }
        let requests = api_requests(&server);
        assert_eq!(requests.len(), 4);
        assert_same_wire(&requests[0], &requests[2]);
        assert_same_wire(&requests[1], &requests[3]);
        for request in &requests {
            assert_eq!(request.host, server.addr().to_string());
            assert_eq!(path_realm(request).as_deref(), Some(REALM_B));
            assert_eq!(bearer(request).as_deref(), Some("synthetic-access-token-2"));
        }
        assert_eq!(token_requests(&server).len(), 1);
    }
}
