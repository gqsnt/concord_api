//! Shared test support for the before/after consumers: fixtures, the SDK context against the
//! loopback server, request inspection and observable cleanup.

#![allow(dead_code)]

use std::num::{NonZeroU32, NonZeroUsize};
use std::path::PathBuf;
use std::time::Duration;

use concord::reqwest;
use concord_example_kubernetes_sdk::{BearerToken, KubeContext, NamespaceName};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};

/// The synthetic bearer token every test context sends.
pub const TOKEN: &str = "synthetic-maint-token-41d7";

/// The bytes of `tests/fixtures/<name>`.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

/// A `200` list/object reply with a fixture body.
pub fn json(name: &str) -> Reply {
    Reply::json(200, fixture(name))
}

/// A `200` watch reply: the fixture's events in one chunk each line, then the normal end of the
/// body.
pub fn watch_events(name: &str) -> Reply {
    let text = fixture(name);
    let chunks = text
        .split_inclusive(|b| *b == b'\n')
        .map(|line| (Duration::ZERO, concord::bytes::Bytes::copy_from_slice(line)))
        .collect();
    Reply::new(200)
        .header("content-type", "application/json")
        .with_body(Body::Chunks(chunks))
}

/// A `404` `Status` for a request no fixture answers (the test then fails on its assertions).
pub fn not_found() -> Reply {
    Reply::json(
        404,
        r#"{"kind":"Status","apiVersion":"v1","metadata":{},"status":"Failure","reason":"NotFound","code":404}"#,
    )
}

pub fn ns(name: &str) -> NamespaceName {
    NamespaceName::new(name).unwrap()
}

/// The SDK context pointed at `server`, with the synthetic bearer token.
pub fn kube(server: &TestServer) -> KubeContext {
    KubeContext::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap()
        .with_bearer(&BearerToken::new(TOKEN))
        .unwrap()
}

pub fn nz32(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

pub fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

/// The single decoded value of a query parameter, if present.
pub fn query(request: &Recorded, name: &str) -> Option<String> {
    let all = request.query_all(name);
    assert!(all.len() <= 1, "{name} repeated in {}", request.target);
    all.into_iter().next()
}

/// The raw request targets, in arrival order.
pub fn targets(requests: &[Recorded]) -> Vec<String> {
    requests.iter().map(|r| r.target.clone()).collect()
}

/// Every request was a `GET` with the bearer token and `Accept: application/json`.
pub fn assert_read_only_and_authenticated(requests: &[Recorded]) {
    for r in requests {
        assert_eq!(r.method, "GET", "{}", r.target);
        assert_eq!(
            r.header("authorization"),
            Some(format!("Bearer {TOKEN}").as_str()),
            "{}",
            r.target
        );
        assert_eq!(r.header("accept"), Some("application/json"), "{}", r.target);
    }
}

/// Shuts the server down and checks that every connection it held is closed.
pub async fn finish(server: TestServer) {
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0, "{cleanup:?}");
}
