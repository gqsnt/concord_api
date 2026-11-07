//! Between waves 1 and 2, candidate A: a declared finite-body bound (`limit N;`). Baseline: every
//! SDK that bounds bodies writes a send policy only to call `decode_limited(n)`, and the native
//! `prepare()?.send()` bypasses that bound (SDK21, SDK22, SDK25 F4, SDK23; ten SDK20 SDKs).

use concord::{ErrorKind, Json};
use concord_example_testserver::{Reply, TestServer};
use serde::Deserialize;

/// A document.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Doc {
    /// Its text.
    pub text: String,
}

const SMALL: usize = 32;

concord::api! {
    /// A store whose finite bodies are bounded where declared.
    pub client Store() {
        base "http://store.invalid";
        format Json;
        limit SMALL;

        /// Inherits the client's bound.
        GET small(id: u32) -> Doc { path "docs/{id}"; }
        scope reports {
            limit 4 * 1024;
            /// Inherits the scope's larger bound.
            GET report(id: u32) -> Doc { path "reports/{id}"; }
        }
        /// No typed output: drained, never buffered, not bounded by `limit`.
        DELETE remove(id: u32) { path "docs/{id}"; }
    }
}

fn body(len: usize) -> String {
    format!(r#"{{"text":"{}"}}"#, "x".repeat(len))
}

#[tokio::test]
async fn every_finite_path_honours_the_declared_bound() {
    let server = TestServer::start(|r| {
        Reply::json(200, body(if r.path().contains("/big") { 100 } else { 3 }))
    })
    .await;
    let store = Store::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    // Within the bound.
    assert_eq!(store.small(1).send().await.unwrap().text, "xxx");
    // Generated send(), the native prepare()?.send(), and Received::decode() all refuse a body
    // over the bound with ErrorKind::Limit — the native path no longer bypasses it.
    let big = || {
        let s = Store::new(concord::reqwest::Client::new())
            .with_base_url(format!("{}/big", server.url()))
            .unwrap();
        s.small(2)
    };
    assert_eq!(big().send().await.unwrap_err().kind(), ErrorKind::Limit);
    assert_eq!(
        big().prepare().unwrap().send().await.unwrap_err().kind(),
        ErrorKind::Limit
    );
    let received = big().prepare().unwrap().receive().await.unwrap();
    assert_eq!(
        received.decode().await.unwrap_err().kind(),
        ErrorKind::Limit
    );
    // An explicit bound still replaces the declared one for one call.
    let received = big().prepare().unwrap().receive().await.unwrap();
    assert_eq!(received.decode_limited(1024).await.unwrap().text.len(), 100);
    // The decoder reports its bound.
    let (_, _, decoder) = big().prepare().unwrap().into_parts();
    assert_eq!(decoder.limit(), Some(SMALL));
}

#[tokio::test]
async fn the_nearest_owner_wins_and_drains_are_not_bounded() {
    let server = TestServer::start(|r| {
        Reply::json(
            200,
            body(if r.path().starts_with("/reports") {
                100
            } else {
                500
            }),
        )
    })
    .await;
    let store = Store::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    assert_eq!(
        store.reports().report(1).send().await.unwrap().text.len(),
        100
    );
    store
        .remove(1)
        .send()
        .await
        .expect("a drain reads any length");
}
