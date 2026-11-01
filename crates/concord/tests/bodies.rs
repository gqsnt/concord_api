//! Request bodies, response representations, status handling and the native response path.
//! Scenario IDs refer to `v4/TEST_MATRIX.md`.

mod support;

use concord::{Binary, ErrorKind, Json, Text, bytes::Bytes, reqwest};
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

/// R07: text and binary bodies are sent unchanged.
#[tokio::test]
async fn r07_text_and_binary_bodies() {
    let (server, api) = shop(Reply::new(204)).await;
    api.clone().text("héllo ✓".to_owned()).send().await.unwrap();
    api.binary(Bytes::from_static(&[0, 159, 146, 150, 255]))
        .send()
        .await
        .unwrap();
    let r = server.requests();
    assert_eq!(r[0].body, "héllo ✓".as_bytes());
    assert_eq!(
        r[1].header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(r[1].body, [0, 159, 146, 150, 255]);
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

/// HEAD with `()` works (no body is read).
#[tokio::test]
async fn head_request() {
    let (server, api) = shop(Reply::new(200)).await;
    api.exists(3).send().await.unwrap();
    assert_eq!(server.single().method, "HEAD");
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

/// R07: text and binary responses are returned unchanged; invalid UTF-8 text is a decode error.
#[tokio::test]
async fn r07_text_and_binary_responses() {
    let (_server, api) = shop(Reply::text(200, "héllo ✓")).await;
    assert_eq!(api.text_out().send().await.unwrap(), "héllo ✓");

    let (_server, api) = shop(Reply::new(200).body([0u8, 255, 1])).await;
    assert_eq!(
        api.clone().bytes_out().send().await.unwrap().as_ref(),
        [0, 255, 1]
    );
    let error = api.text_out().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(
        error
            .to_string()
            .contains("not valid UTF-8 (valid up to byte 1)"),
        "{error}"
    );
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

/// A transport failure is reported once, with no retry, as a transport error.
#[tokio::test]
async fn transport_error() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let api = Shop::new(reqwest::Client::new())
        .with_base_url(format!("http://{addr}"))
        .unwrap();
    let error = api.item(1).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.transport().unwrap().is_connect());
    assert_eq!(
        error.to_string(),
        "GET Shop.item: transport error (connect)"
    );
}

/// C4-R-005: `()` drains a large body chunk by chunk (no buffering) and succeeds.
#[tokio::test]
async fn c4_r_005_unit_drains_large_body() {
    let (server, api) = shop(Reply::new(200).body(vec![1u8; 8 * 1024 * 1024])).await;
    api.remove(1).send().await.unwrap();
    assert_eq!(server.requests().len(), 1);
}

static DECODED_LEN: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);

/// A user representation that asks for the body to be drained.
pub struct Drained;

impl concord::Decode for Drained {
    type Output = ();
    const DISCARD: bool = true;
    fn decode(body: Bytes) -> Result<(), concord::BodyError> {
        DECODED_LEN.store(body.len(), std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

/// A user representation with an invalid content type.
pub struct BadType;

impl concord::Encode for BadType {
    type Value = String;
    const CONTENT_TYPE: Option<&'static str> = Some("application/x\u{2011}custom");
    fn encode(value: String) -> Result<reqwest::Body, concord::BodyError> {
        Ok(value.into())
    }
}

concord::api! {
    /// ADV-6 / ADV-7 fixture.
    pub client Reprs {
        base "http://reprs.invalid";
        GET drained() -> Drained { path "big"; }
        POST bad(body value: BadType) { path "bad"; }
    }
}

/// ADV-6: a `DISCARD` representation never receives body bytes (the body is drained, not buffered).
#[tokio::test]
async fn adv_6_discard_decoders_receive_no_bytes() {
    let server = Server::reply(Reply::new(200).body(vec![9u8; 1_000_000])).await;
    let api = Reprs::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    api.drained().send().await.unwrap();
    assert_eq!(DECODED_LEN.load(std::sync::atomic::Ordering::SeqCst), 0);
}

/// ADV-7: an invalid content type from a representation is a build error, not a panic.
#[test]
fn adv_7_invalid_content_type_is_a_build_error() {
    let error = Reprs::new(reqwest::Client::new())
        .bad("x".to_owned())
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert!(error.to_string().contains("content-type"), "{error}");
}
