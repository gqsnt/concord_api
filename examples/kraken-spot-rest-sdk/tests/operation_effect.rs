//! Signed POST-read effects survive dispatch.
use concord_sdk_kraken_spot_rest::{
    BalanceOptions, Credentials, Error, Kraken, KrakenConfig, Nonces, Policy, concord::ErrorKind,
    transport,
};
use concord_testserver::{Recorded, Reply, TestServer};

#[tokio::test]
async fn a_post_read_stays_signed_and_is_dispatched_once_even_without_a_response() {
    for disconnect in [false, true] {
        let server = TestServer::start(move |request: &Recorded| {
            assert_eq!(request.method, "POST");
            assert_eq!(request.path(), "/0/private/Balance");
            assert_eq!(request.header("api-key"), Some("fixture-key"));
            assert!(request.header("api-sign").is_some());
            assert_eq!(request.body_text(), "nonce=9001");
            if disconnect {
                Reply::hang_up()
            } else {
                Reply::json(200, include_str!("../fixtures/balances.json"))
            }
        })
        .await;
        let policy = Policy::private(
            Credentials::new("fixture-key", "Zml4dHVyZS1zaWduaW5nLXNlY3JldA==").unwrap(),
            Nonces::new(9000),
        );
        let api = Kraken::new(transport().unwrap(), KrakenConfig { policy })
            .with_base_url(server.url())
            .unwrap();
        let endpoint = api.account().balances(BalanceOptions::default());
        let unsigned = endpoint.clone().prepare().unwrap();
        assert!(unsigned.operation().unwrap().is_read());
        assert_eq!(unsigned.request().method().as_str(), "POST");
        assert!(unsigned.request().headers().get("api-key").is_none());
        assert!(unsigned.request().headers().get("api-sign").is_none());

        match endpoint.send().await {
            Ok(response) if !disconnect => {
                assert!(response.meta.operation().unwrap().is_read());
            }
            Err(Error::Concord(error)) if disconnect => {
                assert_eq!(error.kind(), ErrorKind::Transport);
                assert!(error.operation().unwrap().is_read());
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
