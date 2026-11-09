//! `mut connection` permits coordinated realm, credential-source and environment replacement.
//! It remains required in QuickBooksConfig. There are no independent realm or token setters.
//! This example only prepares requests; authentication and official-host selection still run
//! in Connection's existing Rust send policy, covered by tests/environment.rs.

use concord_sdk_quickbooks_online::{
    Connection, EntityId, Environment, Error, OAuthApp, QuickBooks, RealmId, RefreshToken,
    http_client,
};

fn stored(environment: Environment, realm: &str, refresh: &str) -> Result<Connection, Error> {
    let app = OAuthApp::new(
        environment,
        "synthetic-client",
        "synthetic-secret",
        "https://app.example.test/callback",
    )?;
    Ok(app.connection(RealmId::new(realm)?, RefreshToken::new(refresh)?))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = QuickBooks::connect(
        http_client()?,
        stored(Environment::Sandbox, "111", "synthetic-refresh-old")?,
    );
    let earlier_scope = client.customer(EntityId::new("29")?);
    let earlier_request = earlier_scope.read();

    // One setter installs one complete connection. Old scopes and requests keep the old one.
    let client = client.set_connection(stored(
        Environment::Production,
        "222",
        "synthetic-refresh-fresh",
    )?);
    let old = earlier_request.prepare()?;
    let old_scope = earlier_scope.read().prepare()?;
    let fresh = client.customer(EntityId::new("29")?).read().prepare()?;
    assert_eq!(
        old.request().url().as_str(),
        "https://sandbox-quickbooks.api.intuit.com/v3/company/111/customer/29?minorversion=75"
    );
    assert_eq!(old.request().url(), old_scope.request().url());
    assert_eq!(
        fresh.request().url().as_str(),
        "https://quickbooks.api.intuit.com/v3/company/222/customer/29?minorversion=75"
    );
    assert!(!fresh.request().headers().contains_key("authorization"));

    // The replacement connection supplies the base during preparation and the token on send.

    // The customer ID is a fixed scope value. Re-enter the scope to select another customer;
    // `client.customer(id).set_id(other)` is now E0599. Endpoint setters remain unchanged.
    let other = client.customer(EntityId::new("30")?).read().prepare()?;
    assert_eq!(other.request().url().path(), "/v3/company/222/customer/30");

    // Fixture construction selects the destination on the replacement connection itself.
    let fixture = client.set_connection(
        stored(Environment::Sandbox, "333", "synthetic-refresh-fixture")?
            .for_fixture("http://127.0.0.1:1234")?,
    );
    let fixture_request = fixture.customer(EntityId::new("29")?).read().prepare()?;
    assert_eq!(
        fixture_request.request().url().as_str(),
        "http://127.0.0.1:1234/v3/company/333/customer/29?minorversion=75"
    );
    assert!(!format!("{fixture:?} {fixture_request:?}").contains("synthetic-refresh-fixture"));
    Ok(())
}
