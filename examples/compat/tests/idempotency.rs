//! Loopback compatibility evidence for idempotency.
mod support;
use concord_example_compat::{Fresh, Shared};
use support::{http, Reply, Server};

#[tokio::test]
async fn fresh_per_handle_clone_for_retry_and_root_pitfall() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Fresh::new(http()).with_base_url(server.url()).unwrap();
    let first = api.charge();
    let second = api.charge();
    first.clone().send().await.unwrap();
    first.send().await.unwrap();
    second.send().await.unwrap();
    let shared = Shared::new(http()).with_base_url(server.url()).unwrap();
    shared.charge().send().await.unwrap();
    shared.charge().send().await.unwrap();
    let requests = server.requests();
    let keys: Vec<_> = requests
        .iter()
        .map(|r| r.header("idempotency-key").unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "fixture-1",
            "fixture-1",
            "fixture-2",
            "fixture-3",
            "fixture-3"
        ]
    );
}
