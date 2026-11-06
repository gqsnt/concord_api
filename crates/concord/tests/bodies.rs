//! Request bodies, response formats, status handling and the native response path. The fixture
//! is a mostly-JSON client (`request Json; response Json;`) with exceptions written as endpoint
//! statements.
//! Scenario IDs refer to `v4/TEST_MATRIX.md`.

use concord::{Binary, IgnoreContentType, Json, Text};
mod support;

use concord::{ErrorKind, reqwest};
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
        request Json;
        response Json;

        POST create(body item: NewItem) -> Item { path "items"; }
        PUT nullable(body value: Option<NewItem>) { path "nullable"; }
        PUT text(body value: String) {
            request Text;
            path "text";
        }
        PUT binary(body value: concord::bytes::Bytes) {
            request Binary;
            path "binary";
        }
        PATCH maybe(body value?: NewItem) { path "maybe"; }

        GET item(id: u64) -> Item { path "items/{id}"; }
        GET find(id: u64) -> Option<Item> { path "find/{id}"; }
        GET text_out() -> String {
            response Text;
            path "text";
        }
        GET bytes_out() -> concord::bytes::Bytes {
            response Binary;
            path "bytes";
        }
        DELETE remove(id: u64) { path "items/{id}"; }
        HEAD exists(id: u64) { path "items/{id}"; }
        /// A server that mislabels its JSON: the check is skipped deliberately, here only.
        GET legacy(id: u64) -> Item {
            response IgnoreContentType<Json>;
            path "legacy/{id}";
        }
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
        Some("application/json"),
        "no Accept is declared: `Json` suggests one"
    );
}

/// R02: no body, a zero-length body and JSON `null` are three different requests. Observed: hyper
/// sends neither `Content-Length` nor a body for a zero-length body, so on the wire "no body" and
/// "empty text body" differ only by the format's `Content-Type`.
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

/// A declared body that encodes to zero bytes is content of length zero: `Content-Length: 0` is
/// sent (reqwest alone omits it); no body sends no length at all.
#[tokio::test]
async fn an_empty_declared_body_is_sent_with_content_length_zero() {
    let (server, api) = shop(Reply::new(204)).await;
    api.clone().maybe().send().await.unwrap();
    api.clone().text(String::new()).send().await.unwrap();
    api.clone().text("x".to_owned()).send().await.unwrap();
    let r = server.requests();
    assert_eq!(r[0].header("content-length"), None);
    assert_eq!(r[1].header_all("content-length"), ["0"]);
    assert!(r[1].body.is_empty());
    assert_eq!(r[1].header("transfer-encoding"), None);
    assert_eq!(r[2].header_all("content-length"), ["1"]);
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

/// R03: malformed JSON under a JSON media type is a decode error naming the position, not the
/// content.
#[tokio::test]
async fn r03_malformed_json_is_a_decode_error() {
    let (_server, api) = shop(Reply::json(200, r#"{"id":"SECRET-BODY","name":"x"}"#)).await;
    let error = api.item(1).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    let shown = format!("{error} {error:?}");
    assert!(
        shown.contains(
            "response not decoded: the JSON does not match the expected type (line 1, column"
        ),
        "{shown}"
    );
    assert!(!shown.contains("SECRET-BODY"), "{shown}");
    assert!(std::error::Error::source(&error).is_none());
    // The decoder's own message is available explicitly, and may quote the body.
    assert!(
        error
            .codec_error()
            .and_then(concord::CodecError::source_unredacted)
            .unwrap()
            .to_string()
            .contains("SECRET-BODY")
    );
}

/// R03 (C03): JSON bytes labelled `text/html` are refused by `Json`'s head check, before the body
/// is read; the body stays available. `IgnoreContentType<Json>`, selected on one endpoint, accepts
/// them and still decodes them as JSON.
#[tokio::test]
async fn r03_content_type_is_checked_unless_deliberately_ignored() {
    let html = || {
        Reply::new(200)
            .header("content-type", "text/html")
            .body(r#"{"id":2,"name":"y"}"#)
    };
    let (_server, api) = shop(html()).await;
    let error = api.item(2).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(error.status(), None);
    assert_eq!(
        error.to_string(),
        "GET Shop.item: response metadata not accepted: the response Content-Type is not application/json or an application/*+json type"
    );
    assert_eq!(
        error.response().unwrap().headers()["content-type"],
        "text/html"
    );
    let body = error.into_response().unwrap().bytes().await.unwrap();
    assert_eq!(body.as_ref(), br#"{"id":2,"name":"y"}"#);

    let (_server, api) = shop(html()).await;
    assert_eq!(api.legacy(2).send().await.unwrap().id, 2);
    // Only the media check is skipped: malformed JSON is still a decode error.
    let (_server, api) = shop(
        Reply::new(200)
            .header("content-type", "text/html")
            .body("<html>"),
    )
    .await;
    assert_eq!(
        api.legacy(2).send().await.unwrap_err().kind(),
        ErrorKind::Decode
    );
}

/// R04: no output accepts 204 and any 2xx body. A bodyless 204 to a JSON output has no
/// `Content-Type`, so `Json` refuses its head; labelled JSON, it fails as a decode error (an empty
/// body is not JSON), with a static label and the serde error as the unredacted source.
#[tokio::test]
async fn r04_empty_responses() {
    let (_server, api) = shop(Reply::new(204)).await;
    api.clone().remove(1).send().await.unwrap();
    let error = api.item(1).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    let (_server, api) = shop(Reply::new(204).header("content-type", "application/json")).await;
    let error = api.item(1).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(
        error
            .to_string()
            .ends_with("the body is empty or only whitespace, which is not JSON; omit `->` if this endpoint returns no body, or use a format that accepts an empty one"),
        "{error}"
    );
    assert!(
        error
            .codec_error()
            .and_then(concord::CodecError::source_unredacted)
            .unwrap()
            .to_string()
            .contains("EOF while parsing")
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
