//! Offline caller: a read can be a signed POST without becoming replayable.
use concord_sdk_kraken_spot_rest::{
    BalanceOptions, Credentials, Kraken, KrakenAccountBalances, KrakenConfig, Nonces, Policy,
    transport,
};
use concord_testserver::{Recorded, Reply, TestServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = TestServer::start(|request: &Recorded| {
        assert_eq!(request.method, "POST");
        assert_eq!(request.path(), "/0/private/Balance");
        assert!(request.header("api-key").is_some());
        assert!(request.header("api-sign").is_some());
        assert_eq!(request.body_text(), "nonce=9001");
        Reply::json(200, include_str!("../fixtures/balances.json"))
    })
    .await;
    let policy = Policy::private(
        Credentials::new("fixture-key", "Zml4dHVyZS1zaWduaW5nLXNlY3JldA==")?,
        Nonces::new(9000),
    );
    let api = Kraken::new(transport()?, KrakenConfig { policy }).with_base_url(server.url())?;

    let operation = KrakenAccountBalances::operation();
    assert_eq!(operation.method(), Some("POST"));
    assert!(operation.is_read());
    let balances = api
        .account()
        .balances(BalanceOptions::default())
        .send()
        .await?;
    assert!(balances.meta.operation().unwrap().is_read());
    println!("{} balance entries", balances.value.len());

    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
    Ok(())
}
