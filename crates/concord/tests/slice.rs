//! Gate 2 vertical slice: declaration → generated client → native request → loopback HTTP →
//! typed response. Scenario IDs refer to `v4/TEST_MATRIX.md`.

mod support;

use concord::reqwest;
use serde::Deserialize;
use support::{Reply, Server};

/// Health document.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Health {
    status: String,
}

/// An item.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Item {
    id: u64,
    name: String,
}

concord::api! {
    /// A tiny API.
    pub client Tiny {
        base "http://tiny.invalid/v1";

        GET health() -> Health via concord::Json {
            path "health";
        }

        GET item(id: u64) -> Item via concord::Json {
            path "items/{id}";
        }
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

/// M01 + M05: zero-input client, one GET, typed JSON, exactly one request on the wire.
#[tokio::test]
async fn m01_health_round_trip() {
    let server = Server::reply(Reply::json(200, r#"{"status":"ok"}"#)).await;
    let api = Tiny::new(http())
        .with_base_url(server.url() + "/v1")
        .unwrap();

    let health = api.health().send().await.unwrap();

    assert_eq!(
        health,
        Health {
            status: "ok".into()
        }
    );
    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, "/v1/health");
}

/// Path value placement and typed decoding.
#[tokio::test]
async fn item_path_value() {
    let server = Server::reply(Reply::json(200, r#"{"id":7,"name":"seven"}"#)).await;
    let api = Tiny::new(http())
        .with_base_url(server.url() + "/v1")
        .unwrap();

    let item = api.item(7).send().await.unwrap();

    assert_eq!(
        item,
        Item {
            id: 7,
            name: "seven".into()
        }
    );
    assert_eq!(server.single().target, "/v1/items/7");
}

/// M02: an application-configured reqwest client is used as is (its default headers reach the
/// wire; Concord adds no transport configuration of its own).
#[tokio::test]
async fn m02_application_client_is_used() {
    let server = Server::reply(Reply::json(200, r#"{"status":"ok"}"#)).await;
    let mut defaults = reqwest::header::HeaderMap::new();
    defaults.insert("x-app", "configured".parse().unwrap());
    let http = reqwest::Client::builder()
        .default_headers(defaults)
        .user_agent("app/1.0")
        .build()
        .unwrap();
    let api = Tiny::new(http).with_base_url(server.url() + "/v1").unwrap();

    api.health().send().await.unwrap();

    let request = server.single();
    assert_eq!(request.header("x-app"), Some("configured"));
    assert_eq!(request.header("user-agent"), Some("app/1.0"));
}

/// M03: the native request is available before sending, without contacting a server.
#[tokio::test]
async fn m03_prepare_does_not_contact_the_server() {
    let server = Server::reply(Reply::json(200, "{}")).await;
    let api = Tiny::new(http())
        .with_base_url(server.url() + "/v1")
        .unwrap();

    let prepared = api.item(42).prepare().unwrap();

    assert_eq!(prepared.request().method(), reqwest::Method::GET);
    assert_eq!(
        prepared.request().url().as_str(),
        format!("{}/v1/items/42", server.url())
    );
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(server.requests().is_empty());
}

/// M04: the native response is available, with no status check and no decoding.
#[tokio::test]
async fn m04_send_raw_returns_the_native_response() {
    let server = Server::reply(Reply::text(503, "busy")).await;
    let api = Tiny::new(http())
        .with_base_url(server.url() + "/v1")
        .unwrap();

    let response = api.health().prepare().unwrap().send_raw().await.unwrap();

    assert_eq!(response.status(), 503);
    assert_eq!(response.text().await.unwrap(), "busy");
    assert_eq!(server.requests().len(), 1);
}

/// M05: a failing status is returned once; nothing is retried.
#[tokio::test]
async fn m05_failure_is_not_retried() {
    let server = Server::reply(Reply::text(500, "boom")).await;
    let api = Tiny::new(http())
        .with_base_url(server.url() + "/v1")
        .unwrap();

    let error = api.health().send().await.unwrap_err();

    assert_eq!(error.kind(), concord::ErrorKind::Status);
    assert_eq!(
        error.status(),
        Some(reqwest::StatusCode::INTERNAL_SERVER_ERROR)
    );
    assert_eq!(server.requests().len(), 1);
}

/// The native parts can be modified, executed by the application and decoded as declared.
#[tokio::test]
async fn into_parts_then_decode() {
    let server = Server::reply(Reply::json(200, r#"{"id":1,"name":"one"}"#)).await;
    let api = Tiny::new(http())
        .with_base_url(server.url() + "/v1")
        .unwrap();

    let (client, mut request, decoder) = api.item(1).prepare().unwrap().into_parts();
    request
        .headers_mut()
        .insert("x-trace", "abc".parse().unwrap());
    let response = client.execute(request).await.unwrap();
    let item = decoder.decode(response).await.unwrap();

    assert_eq!(item.id, 1);
    assert_eq!(server.single().header("x-trace"), Some("abc"));
}
