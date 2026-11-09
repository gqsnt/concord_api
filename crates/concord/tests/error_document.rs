//! A1: `error D in F;` — a non-2xx answer read once, bounded, as the declared error document,
//! the status authoritative; nearest owner wins; no request byte changes; the native exits stay.

mod support;

use concord::{Document, ErrorKind, Json, reqwest};
use serde::Deserialize;
use support::{Reply, Server};

/// The client's error document.
#[derive(Debug, Deserialize)]
pub struct Problem {
    /// Its title.
    pub title: String,
}

/// A scope's own error document.
#[derive(Debug, Deserialize)]
pub struct Fault {
    /// Its code.
    pub code: u32,
}

concord::api! {
    /// With error documents.
    pub client Api {
        base "http://errors.invalid";
        response Json;
        error Problem in Json;

        GET item() -> Problem { path "item"; }
        GET nothing() { path "nothing"; }

        /// Overrides the client's document.
        scope admin {
            path "admin";
            error Fault in Json;

            GET stats() -> Problem { path "stats"; }
        }
    }
}

concord::api! {
    /// The same endpoints without an error document.
    pub client Plain {
        base "http://errors.invalid";
        response Json;

        GET item() -> Problem { path "item"; }
        GET nothing() { path "nothing"; }
    }
}

async fn api(reply: Reply) -> (Server, Api) {
    let server = Server::reply(reply).await;
    let api = Api::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    (server, api)
}

#[tokio::test]
async fn a_failed_status_carries_the_read_document() {
    let (server, api) = api(Reply::json(422, r#"{"title":"SECRET-TITLE"}"#)).await;
    let error = api.item().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status().unwrap().as_u16(), 422);
    assert_eq!(error.delivery(), concord::Delivery::Answered);
    assert_eq!(error.endpoint(), Some("Api.item"));
    assert_eq!(error.document().unwrap().title, "SECRET-TITLE");
    let failure = error.provider().unwrap();
    assert_eq!(failure.body().as_ref(), br#"{"title":"SECRET-TITLE"}"#);
    let shown = format!("{error} {error:?} {failure:?} {:?}", failure.state());
    assert!(!shown.contains("SECRET-TITLE"), "{shown}");
    assert_eq!(
        error.to_string(),
        "GET Api.item: HTTP status 422 Unprocessable Entity"
    );
    // The response was consumed: its head and bytes are in the failure.
    assert!(error.into_response().is_err());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_drained_endpoint_reads_its_document_too() {
    let (_server, api) = api(Reply::json(404, r#"{"title":"gone"}"#)).await;
    let error = api.nothing().send().await.unwrap_err();
    assert_eq!(error.document().unwrap().title, "gone");
}

#[tokio::test]
async fn the_nearest_owner_wins() {
    let (_server, api) = api(Reply::json(500, r#"{"code":7}"#)).await;
    let error: concord::Error<Fault> = api.admin().stats().send().await.unwrap_err();
    assert_eq!(error.document().unwrap().code, 7);
}

#[tokio::test]
async fn a_refused_head_or_other_bytes_keep_the_status() {
    let (_server, api) = api(Reply::text(503, "<html>proxy</html>")).await;
    let error = api.item().send().await.unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 503);
    let failure = error.provider().unwrap();
    assert!(matches!(failure.state(), Document::Unrecognized(_)));
    assert_eq!(failure.body().as_ref(), b"<html>proxy</html>");
}

#[tokio::test]
async fn the_bound_is_a_rust_setting_and_long_bodies_are_not_decoded() {
    let body = format!(r#"{{"title":"{}"}}"#, "x".repeat(64));
    let (_server, api) = api(Reply::json(400, &body)).await;
    let error = api
        .item()
        .prepare()
        .unwrap()
        .error_body_limit(16)
        .send()
        .await
        .unwrap_err();
    let failure = error.provider().unwrap();
    assert!(matches!(failure.state(), Document::TooLong { limit: 16 }));
    assert_eq!(failure.body().len(), 16);
}

#[tokio::test]
async fn receive_still_returns_any_status_unread() {
    let (_server, api) = api(Reply::json(409, r#"{"title":"conflict"}"#)).await;
    let received = api.item().prepare().unwrap().receive().await.unwrap();
    assert_eq!(received.status().as_u16(), 409);
    let response = received.into_response();
    assert_eq!(response.text().await.unwrap(), r#"{"title":"conflict"}"#);
}

#[tokio::test]
async fn without_error_the_status_error_keeps_its_response_unread() {
    let server = Server::reply(Reply::json(409, r#"{"title":"conflict"}"#)).await;
    let plain = Plain::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    let error = plain.item().send().await.unwrap_err();
    assert!(error.provider().is_none());
    let response = error.into_response().unwrap();
    assert_eq!(response.text().await.unwrap(), r#"{"title":"conflict"}"#);
}

#[test]
fn an_error_document_adds_no_request_byte() {
    let api = Api::new(reqwest::Client::new());
    let plain = Plain::new(reqwest::Client::new());
    let with = api.item().prepare().unwrap();
    let without = plain.item().prepare().unwrap();
    assert_eq!(with.request().headers(), without.request().headers());
    let with = api.nothing().prepare().unwrap();
    let without = plain.nothing().prepare().unwrap();
    assert_eq!(with.request().headers(), without.request().headers());
    assert!(with.request().headers().get("accept").is_none());
}
