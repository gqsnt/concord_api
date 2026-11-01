//! Request bodies, response representations, status handling and the native response path.
//! Scenario IDs refer to `v4/TEST_MATRIX.md`.

mod support;

use concord::{Binary, ErrorKind, Json, Text, reqwest};
use serde::{Deserialize, Serialize};
use support::{Reply, Server};

/// A created item.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Item {
    id: u64,
    name: String,
}

/// Input for `create`.
#[derive(Serialize)]
pub struct NewItem {
    name: String,
    note: Option<String>,
}

concord::api! {
    /// Body/response fixture.
    pub client Shop {
        base "http://shop.invalid";

        POST create(body item: Json<NewItem>) -> Json<Item> { path "items"; }
        PUT nullable(body value: Json<Option<NewItem>>) { path "nullable"; }
        PUT text(body value: Text) { path "text"; }
        PUT binary(body value: Binary) { path "binary"; }
        PATCH maybe(body value?: Json<NewItem>) { path "maybe"; }

        GET item(id: u64) -> Json<Item> { path "items/{id}"; }
        GET find(id: u64) -> Json<Option<Item>> { path "find/{id}"; }
        GET text_out() -> Text { path "text"; }
        GET bytes_out() -> Binary { path "bytes"; }
        DELETE remove(id: u64) { path "items/{id}"; }
        HEAD exists(id: u64) { path "items/{id}"; }
    }
}

async fn shop(reply: Reply) -> (Server, Shop) {
    let server = Server::reply(reply).await;
    let api = Shop::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    (server, api)
}

/// R01: a JSON body is serialized with `Content-Type: application/json`; the response is decoded.
#[tokio::test]
async fn r01_post_json() {
    let (server, api) = shop(Reply::json(201, r#"{"id":1,"name":"pen"}"#)).await;
    let item = api
        .create(NewItem {
            name: "pen".into(),
            note: None,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(
        item,
        Item {
            id: 1,
            name: "pen".into()
        }
    );
    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.body, br#"{"name":"pen","note":null}"#);
    assert_eq!(
        request.header("accept"),
        Some("*/*"),
        "Concord sets no Accept; this is reqwest's default"
    );
}

/// R02: no body, a zero-length body and JSON `null` are three different requests. Observed: hyper
/// sends neither `Content-Length` nor a body for a zero-length body, so on the wire "no body" and
/// "empty text body" differ only by the representation's `Content-Type`.
#[tokio::test]
async fn r02_no_body_vs_empty_vs_null() {
    let (server, api) = shop(Reply::new(204)).await;
    api.clone().maybe().send().await.unwrap();
    api.clone().text(String::new()).send().await.unwrap();
    api.nullable(None).send().await.unwrap();
    let r = server.requests();

    assert_eq!(r[0].header("content-type"), None);
    assert!(r[0].body.is_empty());
    assert!(matches!(r[0].header("content-length"), None | Some("0")));

    assert_eq!(
        r[1].header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert!(matches!(r[1].header("content-length"), None | Some("0")));
    assert!(r[1].body.is_empty());

    assert_eq!(r[2].header("content-type"), Some("application/json"));
    assert_eq!(r[2].body, b"null");
}

/// An optional body: `set_*` sends it, `maybe_*(None)` sends none.
#[tokio::test]
async fn optional_body_controls() {
    let (server, api) = shop(Reply::new(204)).await;
    api.clone()
        .maybe()
        .set_value(NewItem {
            name: "x".into(),
            note: Some("n".into()),
        })
        .send()
        .await
        .unwrap();
    api.maybe().maybe_value(None).send().await.unwrap();
    let r = server.requests();
    assert_eq!(r[0].body, br#"{"name":"x","note":"n"}"#);
    assert!(r[1].body.is_empty());
}

/// R03: malformed JSON is a decode error naming the position, not the content.
#[tokio::test]
async fn r03_malformed_json_is_a_decode_error() {
    let (_server, api) = shop(Reply::json(200, r#"{"id":"SECRET-BODY","name":"x"}"#)).await;
    let error = api.item(1).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    let shown = format!("{error} {error:?}");
    assert!(
        shown.contains("JSON decode error (data) at line 1, column"),
        "{shown}"
    );
    assert!(!shown.contains("SECRET-BODY"), "{shown}");
    assert!(std::error::Error::source(&error).is_none());
    // The decoder's own message is available explicitly, and may quote the body.
    assert!(
        error
            .body_source()
            .unwrap()
            .to_string()
            .contains("SECRET-BODY")
    );
}

/// R03: the response `Content-Type` is not consulted: JSON is decoded by declaration.
#[tokio::test]
async fn r03_content_type_is_not_checked() {
    let (_server, api) =
        shop(Reply::text(200, r#"{"id":2,"name":"y"}"#).header("content-type", "text/html")).await;
    assert_eq!(api.item(2).send().await.unwrap().id, 2);
}

/// R04: `()` accepts 204 and any 2xx body; JSON on 204 fails as end of input.
#[tokio::test]
async fn r04_empty_responses() {
    let (_server, api) = shop(Reply::new(204)).await;
    api.clone().remove(1).send().await.unwrap();
    let error = api.item(1).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(
        error.to_string().contains("unexpected end of input"),
        "{error}"
    );

    let (_server, api) = shop(Reply::json(200, r#"{"deleted":true}"#)).await;
    api.remove(1).send().await.unwrap();
}

/// R05: a non-2xx status is an error that keeps the unread response for the caller.
#[tokio::test]
async fn r05_status_error_keeps_the_response() {
    let (_server, api) = shop(Reply::json(422, r#"{"message":"SECRET-DETAIL"}"#)).await;
    let error = api.item(1).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(
        error.status(),
        Some(reqwest::StatusCode::UNPROCESSABLE_ENTITY)
    );
    assert_eq!(
        error.to_string(),
        "GET Shop.item: HTTP status 422 Unprocessable Entity"
    );
    assert!(!format!("{error:?}").contains("SECRET-DETAIL"));
    assert!(!format!("{error:?}").contains("127.0.0.1"));
    let body = error.into_response().unwrap().text().await.unwrap();
    assert_eq!(body, r#"{"message":"SECRET-DETAIL"}"#);
}

/// R06: an `Option` output does not turn 404 into `None`; JSON `null` does.
#[tokio::test]
async fn r06_option_output_has_no_status_magic() {
    let (_server, api) = shop(Reply::json(404, "null")).await;
    let error = api.find(1).send().await.unwrap_err();
    assert_eq!(error.status(), Some(reqwest::StatusCode::NOT_FOUND));

    let (_server, api) = shop(Reply::json(200, "null")).await;
    assert_eq!(api.find(1).send().await.unwrap(), None);
}

/// R08: the raw response can be consumed natively (streamed chunk by chunk).
#[tokio::test]
async fn r08_raw_response_streaming() {
    let (_server, api) = shop(Reply::new(200).body(vec![7u8; 100_000])).await;
    let mut response = api.bytes_out().prepare().unwrap().send_raw().await.unwrap();
    let mut total = 0;
    while let Some(chunk) = response.chunk().await.unwrap() {
        total += chunk.len();
    }
    assert_eq!(total, 100_000);
}
