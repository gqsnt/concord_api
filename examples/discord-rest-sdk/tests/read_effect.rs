//! The production limiter uses declared read effects without granting replay permission.

use concord::ErrorKind;
use concord_example_testserver::{Reply, TestServer};
use concord_sdk_discord_rest::{Error, RateLimiter, concord};

concord::api! {
    client ReadEffects(limiter: RateLimiter) {
        base "http://127.0.0.1";
        error concord_sdk_discord_rest::FailureDocument in concord_sdk_discord_rest::ErrorJson;
        format concord::Json;
        send limiter -> Result<_, Error>;

        #[read]
        POST search(query case: String) -> Vec<serde_json::Value> { path "probe"; }
        POST create(query case: String) -> Vec<serde_json::Value> { path "probe"; }
    }
}

#[tokio::test]
async fn declared_post_reads_avoid_write_uncertainty_and_are_never_replayed() {
    let server = TestServer::start(|request| match request.query_all("case")[0].as_str() {
        "decode" => Reply::json(200, "["),
        "transport" => Reply::hang_up(),
        _ => panic!("unexpected case"),
    })
    .await;
    let http = concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .build()
        .unwrap();
    let api = ReadEffects::new(
        http,
        ReadEffectsConfig {
            limiter: RateLimiter::new(),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    assert!(ReadEffectsSearch::operation().is_read());
    assert!(!ReadEffectsCreate::operation().is_read());
    assert_eq!(ReadEffectsSearch::operation().method(), Some("POST"));
    assert_eq!(ReadEffectsCreate::operation().method(), Some("POST"));

    for (case, kind, accepted) in [
        (
            "decode",
            ErrorKind::Decode,
            Some(concord::reqwest::StatusCode::OK),
        ),
        ("transport", ErrorKind::Transport, None),
    ] {
        let read_error = api.search(case.to_owned()).send().await.unwrap_err();
        let Error::Concord { error, resource } = read_error else {
            panic!("a declared read cannot be an uncertain write: {read_error:?}")
        };
        assert_eq!(error.kind(), kind);
        assert_eq!(error.operation(), Some(ReadEffectsSearch::operation()));
        assert_eq!(resource, None);

        let write_error = api.create(case.to_owned()).send().await.unwrap_err();
        let Error::AmbiguousWrite(write) = write_error else {
            panic!("a failed write must remain uncertain: {write_error:?}")
        };
        assert_eq!(write.error.kind(), kind);
        assert_eq!(write.accepted, accepted);
        assert_eq!(write.route.operation(), ReadEffectsCreate::operation());
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 4, "every operation is sent exactly once");
    for (request, case) in requests
        .iter()
        .zip(["decode", "decode", "transport", "transport"])
    {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, format!("/probe?case={case}"));
    }
    server.shutdown().await;
}
