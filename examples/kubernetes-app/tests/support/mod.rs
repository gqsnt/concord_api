//! Shared test support: fixtures, the configured SDK context against the loopback server, and
//! request inspection.

#![allow(dead_code)]

use std::num::{NonZeroU32, NonZeroUsize};
use std::path::PathBuf;

use concord::reqwest;
use concord_example_kubernetes_app::collect::{ListPolicy, OnExpired};
use concord_example_kubernetes_sdk::{BearerToken, KubeContext, NamespaceName};
use concord_example_testserver::{Recorded, Reply, TestServer};

/// The synthetic bearer token every test context sends.
pub const TOKEN: &str = "synthetic-sa-token-5c1e";

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The bytes of a fixture file.
pub fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

/// A `200` JSON reply with a fixture body, labelled like the API server does.
pub fn json(name: &str) -> Reply {
    Reply::json(200, fixture(name))
}

/// A non-2xx `Status` reply with a fixture body.
pub fn status(code: u16, name: &str) -> Reply {
    Reply::json(code, fixture(name))
}

/// A `404` for an unexpected request (the test then fails on its assertions).
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

pub fn policy(chunk_size: u32, max_chunks: usize, on_expired: OnExpired) -> ListPolicy {
    ListPolicy {
        chunk_size: NonZeroU32::new(chunk_size).unwrap(),
        max_chunks: NonZeroUsize::new(max_chunks).unwrap(),
        on_expired,
    }
}

/// The single value of a query parameter, if present.
pub fn query(request: &Recorded, name: &str) -> Option<String> {
    let all = request.query_all(name);
    assert!(all.len() <= 1, "{name} repeated in {}", request.target);
    all.into_iter().next()
}

/// Requests whose path is exactly `path`.
pub fn to_path<'a>(requests: &'a [Recorded], path: &str) -> Vec<&'a Recorded> {
    requests.iter().filter(|r| r.path() == path).collect()
}

/// Every request carried the bearer token and nothing printed it.
pub fn assert_authenticated(requests: &[Recorded]) {
    for r in requests {
        assert_eq!(r.header("authorization"), Some(format!("Bearer {TOKEN}").as_str()), "{}", r.target);
    }
}

/// Shuts the server down and checks that every connection it held is closed.
pub async fn finish(server: TestServer) {
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0, "{cleanup:?}");
}
