//! C7.1 and C7.5 against a real server: `concord::Http` follows no redirect. A `301`, `302`,
//! `307` or `308` is the response, with its `Location`, for a `GET` and for a `POST` with a
//! body, whether the redirect has a body of its own or not; the target is never asked.

use crate::server::{Server, Wire};
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use concord::error::Kind;
use concord::http::StatusCode;
use concord::Text;

concord::api! {
    client Moved {
        base "http://moved.test";
        read Text;
        GET page(place: String, code: u16) -> String { path "{place}/{code}"; }
        POST send(place: String, code: u16, note: String) -> String {
            path "{place}/{code}";
            body concord::Body::new(note.clone(), "text/plain");
        }
    }
}

/// `/empty/{code}` and `/with-body/{code}` answer `code` with `Location: /target`, the second
/// with a body; `/target` answers `200`.
fn moving() -> Router {
    async fn redirect(Path((place, code)): Path<(String, u16)>) -> Response {
        let status = StatusCode::from_u16(code).unwrap();
        let body = if place == "with-body" {
            "moved to /target"
        } else {
            ""
        };
        (status, [("location", "/target")], body).into_response()
    }
    Router::new()
        .route("/{place}/{code}", any(redirect))
        .route("/target", any(|| async { "the target" }))
}

async fn no_redirect_is_followed(wire: Wire) {
    let server = Server::start(moving()).await;
    let moved = Moved::new()
        .with_transport(wire.http())
        .with_base(server.url());
    let mut expected = 0;
    for code in [301u16, 302, 307, 308] {
        for place in ["empty", "with-body"] {
            let errors = [
                moved.page(place.to_owned(), code).await.unwrap_err(),
                moved
                    .send(place.to_owned(), code, "a note".to_owned())
                    .await
                    .unwrap_err(),
            ];
            expected += 2;
            for error in errors {
                // With no arm, a status that is not a 2xx is a Refused error (C5.2): the 3xx
                // is the response, with its `Location` (C7.1).
                assert_eq!(error.kind(), Kind::Refused, "{code} {place}");
                assert_eq!(error.status().map(|s| s.as_u16()), Some(code));
                assert_eq!(
                    error.headers().and_then(|headers| headers.get("location")),
                    Some("/target")
                );
                let body: &[u8] = if place == "with-body" {
                    b"moved to /target"
                } else {
                    b""
                };
                assert_eq!(error.body().unwrap_or_default(), body, "{code} {place}");
                assert_eq!(error.tries(), 1);
            }
            // `send_raw()` ends at the head: a head below 400 is final, like a value (C3.6).
            let raw = moved.page(place.to_owned(), code).send_raw().await.unwrap();
            expected += 1;
            assert_eq!(raw.status().as_u16(), code);
            assert_eq!(raw.headers()["location"], "/target");
        }
    }
    // Each call was one request, with its own method.
    let posts = server
        .seen
        .all()
        .iter()
        .filter(|arrival| arrival.method == "POST")
        .count();
    assert_eq!(posts, 8);
    let targets = server.seen.targets();
    assert_eq!(targets.len(), expected);
    assert!(
        targets.iter().all(|target| target != "/target"),
        "the target of a redirect was asked: {targets:?}"
    );
}

#[tokio::test]
async fn c7_1_no_301_302_307_or_308_is_followed_and_the_3xx_is_the_response_over_http1() {
    no_redirect_is_followed(Wire::One).await;
}

#[tokio::test]
async fn c7_1_no_301_302_307_or_308_is_followed_and_the_3xx_is_the_response_over_http2() {
    no_redirect_is_followed(Wire::Two).await;
}

/// A caller's own `reqwest::Client` follows them, below the middleware: the contrast that
/// shows the server above answers what a client would follow.
#[tokio::test]
async fn c7_5_a_client_of_the_callers_own_follows_the_same_redirect_below_the_engine() {
    let server = Server::start(moving()).await;
    let own = concord::reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let moved = Moved::new().with_transport(own).with_base(server.url());
    assert_eq!(
        moved.page("empty".to_owned(), 307).await.unwrap(),
        "the target"
    );
    assert_eq!(server.seen.targets(), ["/empty/307", "/target"]);
}
