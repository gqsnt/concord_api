//! A connection supplies its destination as well as its realm and credential source.
use concord_sdk_quickbooks_online::{
    Connection, EntityId, Environment, OAuthApp, QuickBooks, QuickBooksConfig, RealmId,
    RefreshToken,
};

fn connection(environment: Environment, realm: &str) -> Connection {
    OAuthApp::new(
        environment,
        "client",
        "secret",
        "https://app.example.test/callback",
    )
    .unwrap()
    .connection(
        RealmId::new(realm).unwrap(),
        RefreshToken::new("refresh").unwrap(),
    )
}

#[test]
fn destinations_are_official_or_explicit_loopback_fixtures() {
    for base in [
        "https://evil.example",
        "https://quickbooks.api.intuit.com.evil.example",
        "http://192.0.2.1",
    ] {
        assert!(
            connection(Environment::Sandbox, "111")
                .for_fixture(base)
                .is_err()
        );
    }
    let api = QuickBooks::new(
        concord::reqwest::Client::new(),
        QuickBooksConfig {
            connection: connection(Environment::Sandbox, "111"),
        },
    );
    let earlier = api.customer(EntityId::new("29").unwrap()).read();
    let api = api.set_connection(connection(Environment::Production, "222"));
    let next = api
        .customer(EntityId::new("29").unwrap())
        .read()
        .prepare()
        .unwrap();
    assert_eq!(
        earlier.prepare().unwrap().request().url().host_str(),
        Some("sandbox-quickbooks.api.intuit.com")
    );
    assert_eq!(
        next.request().url().as_str(),
        "https://quickbooks.api.intuit.com/v3/company/222/customer/29?minorversion=75"
    );
    let fixture = QuickBooks::connect(
        concord::reqwest::Client::new(),
        connection(Environment::Production, "222")
            .for_fixture("http://127.0.0.1:1234/prefix")
            .unwrap(),
    );
    let request = fixture
        .customer(EntityId::new("29").unwrap())
        .read()
        .prepare()
        .unwrap();
    assert_eq!(
        request.request().url().as_str(),
        "http://127.0.0.1:1234/prefix/v3/company/222/customer/29?minorversion=75"
    );
}
