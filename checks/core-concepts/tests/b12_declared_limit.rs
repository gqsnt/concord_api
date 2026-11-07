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

// Review b12 (gpt-6.1-sol), findings 3 and 5: a forwarded expression keeps its grouping, and the
// bound resolves names only in the caller's context.
macro_rules! forwarded_bound {
    ($divisor:expr) => {
        concord::api! {
            /// A bound written by a wrapper macro.
            pub client Forwarded() {
                base "http://forwarded.invalid";
                format Json;
                limit 16 / $divisor;
                GET get() -> Doc { path "d"; }
            }
        }
    };
}
forwarded_bound!(1 + 3);

/// A caller constant with the name the generated code once used.
const __LIMIT: usize = 32;
/// Caller constants named like the generated statics of `prepare()` (review b12-review2, 1).
const __DECODE: usize = 48;
/// See `__DECODE`.
const __META: usize = 64;

concord::api! {
    /// Bounds named like generated statics.
    pub client Statics() {
        base "http://statics.invalid";
        format Json;
        GET decode() -> Doc { path "d"; limit __DECODE; }
        GET meta() -> Doc { path "m"; limit __META; }
    }
}

concord::api! {
    /// A bound named like a former generated constant.
    pub client Named() {
        base "http://named.invalid";
        format Json;
        limit __LIMIT;
        GET get() -> Doc { path "d"; }
    }
}

mod aliased {
    use super::{Doc, Json};
    /// A caller alias that shadows the primitive name.
    #[allow(non_camel_case_types, dead_code)]
    type usize = u8;

    concord::api! {
        /// A bound declared where `usize` names another type.
        pub client Aliased() {
            base "http://aliased.invalid";
            format Json;
            limit 7;
            GET get() -> Doc { path "d"; }
        }
    }
}

#[test]
fn a_declared_bound_is_evaluated_as_written_in_the_callers_context() {
    let http = || concord::reqwest::Client::new();
    let limit = |p: concord::Prepared<Doc>| p.into_parts().2.limit();
    assert_eq!(
        limit(Forwarded::new(http()).get().prepare().unwrap()),
        Some(4)
    );
    assert_eq!(limit(Named::new(http()).get().prepare().unwrap()), Some(32));
    assert_eq!(
        limit(aliased::Aliased::new(http()).get().prepare().unwrap()),
        Some(7)
    );
    let statics = Statics::new(http());
    assert_eq!(limit(statics.decode().prepare().unwrap()), Some(48));
    assert_eq!(limit(statics.meta().prepare().unwrap()), Some(64));
}
