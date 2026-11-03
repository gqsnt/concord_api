//! Inspecting errors without consuming them (`Error::response`, `Error::codec_error`,
//! `CodecError::source_unredacted`), the static labels of the JSON format (its strict response
//! head check, and empty bodies), the base-URL message, and the auto traits of the runtime types.

mod support;

use concord::{CodecError, Decode, Error, ErrorKind, Json, reqwest};
use serde::Deserialize;
use support::{Reply, Server};

/// An API error document.
#[derive(Debug, Deserialize)]
pub struct Problem {
    /// Its title.
    pub title: String,
}

/// A response format that refuses everything, with an underlying error of its own.
pub struct Strict;

/// Why `Strict` refused.
#[derive(Debug)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "refused: {}", self.0)
    }
}

impl std::error::Error for Refused {}

impl Decode<String> for Strict {
    type Context = ();
    fn context(_: &concord::ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: concord::bytes::Bytes) -> Result<String, CodecError> {
        Err(CodecError::new("strict format refused the body")
            .with_source(Refused(String::from_utf8_lossy(&body).into_owned())))
    }
}

concord::api! {
    /// Error-inspection fixture.
    pub client Api {
        base "http://inspect.invalid";
        response Json;

        GET problem() -> Problem { path "problem"; }
        GET unit() -> () { path "unit"; }
        GET maybe() -> Option<Problem> { path "maybe"; }
        GET value() -> serde_json::Value { path "value"; }
        GET text() -> String { path "text"; }
        GET raw() -> Vec<u8> { path "raw"; }
        GET nothing() { path "nothing"; }
        GET strict() -> String {
            path "strict";
            response Strict;
        }
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
async fn response_is_borrowed_and_the_body_stays_unread() {
    let (_server, api) = api(Reply::json(429, r#"{"title":"slow down"}"#)
        .header("retry-after", "7")
        .header("x-request-id", "r-1"))
    .await;
    let error = api.problem().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    // Read while the error is kept, as a predicate `Fn(&Error) -> bool` would.
    let retry_after = |e: &Error| -> Option<u64> {
        e.response()?
            .headers()
            .get(reqwest::header::RETRY_AFTER)?
            .to_str()
            .ok()?
            .parse()
            .ok()
    };
    assert_eq!(retry_after(&error), Some(7));
    assert_eq!(error.response().unwrap().headers()["x-request-id"], "r-1");
    assert_eq!(
        error.response().unwrap().status(),
        reqwest::StatusCode::TOO_MANY_REQUESTS
    );
    // The body is still there for the error document.
    let response = error.into_response().unwrap();
    let bytes = response.bytes().await.unwrap();
    let problem = <concord::Json as Decode<Problem>>::decode_body((), bytes).unwrap();
    assert_eq!(problem.title, "slow down");
}

#[tokio::test]
async fn response_is_none_for_other_errors() {
    let (_server, api) = api(Reply::json(200, "{}")).await;
    let error = api.problem().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(error.response().is_none());
}

#[tokio::test]
async fn source_of_a_codec_error() {
    // Through a Concord call.
    let (_server, api) = api(Reply::text(200, "CONTENT")).await;
    let error = api.strict().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(!format!("{error} {error:?}").contains("CONTENT"));
    let source = error
        .codec_error()
        .and_then(concord::CodecError::source_unredacted)
        .unwrap();
    assert_eq!(source.to_string(), "refused: CONTENT");
    // Directly, as when decoding an error document yourself.
    let error = <Strict as Decode<String>>::decode_body((), "CONTENT".into()).unwrap_err();
    assert_eq!(error.to_string(), "strict format refused the body");
    assert!(!format!("{error:?}").contains("CONTENT"));
    assert_eq!(
        error.source_unredacted().unwrap().to_string(),
        "refused: CONTENT"
    );
    assert!(std::error::Error::source(&error).is_none());
}

const EMPTY: &str = "response not decoded: the body is empty or only whitespace, which is not JSON; omit `->` if this endpoint returns no body, or use a format that accepts an empty one";

/// Empty, whitespace and `null` stay distinct under a JSON media type; no output accepts
/// anything. Without a JSON `Content-Type`, the same bodies are refused by the head check first.
#[tokio::test]
async fn empty_responses_are_not_null() {
    let json_204 = Reply::new(204).header("content-type", "application/json");
    for reply in [json_204, Reply::json(200, " \r\n\t")] {
        let (_server, api) = api(reply).await;
        for error in [
            api.unit().send().await.unwrap_err(),
            api.maybe().send().await.map(drop).unwrap_err(),
            api.value().send().await.map(drop).unwrap_err(),
        ] {
            assert_eq!(error.kind(), ErrorKind::Decode);
            assert!(error.to_string().ends_with(EMPTY), "{error}");
        }
        api.nothing().send().await.unwrap();
    }
    // A bodyless 204 without `Content-Type`: refused before any body is read (C10: a format of
    // your own decides what a 204 means; `Json` is not lenient).
    let (_bare, bare) = api(Reply::new(204)).await;
    let error = bare.maybe().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(
        error.codec_error().map(CodecError::label),
        Some("the response has no Content-Type")
    );
    let (_server, api) = api(Reply::json(200, "null")).await;
    api.unit().send().await.unwrap();
    assert!(api.maybe().send().await.unwrap().is_none());
    assert_eq!(api.value().send().await.unwrap(), serde_json::Value::Null);
}

/// A response that is not labelled JSON is refused by its head, before its body is read, whatever
/// the body looks like; the error keeps the response unread and never quotes it. A JSON-labelled
/// body that is not JSON is a decode error with a static label and positions.
#[tokio::test]
async fn non_json_responses_are_refused_by_the_head_and_never_quoted() {
    let png: &[u8] = b"\x89PNG\r\n\x1a\n";
    let bodies: [&[u8]; 4] = [
        b"hello",
        b"<html><body>SECRET</body></html>",
        png,
        b"{\"title\": \"looks like JSON\"}",
    ];
    for body in bodies {
        for content_type in [None, Some("text/html"), Some("text/plain; charset=utf-8")] {
            let reply = match content_type {
                Some(ct) => Reply::new(200).header("content-type", ct).body(body),
                None => Reply::new(200).body(body),
            };
            let (_server, api) = api(reply).await;
            let error = api.problem().send().await.unwrap_err();
            let shown = format!("{error} {error:?}");
            assert_eq!(error.kind(), ErrorKind::Metadata, "{shown}");
            assert!(
                !shown.contains("SECRET") && !shown.contains("hello"),
                "{shown}"
            );
            let response = error.into_response().unwrap();
            assert_eq!(response.bytes().await.unwrap().as_ref(), body);
        }
    }
    let (_server, api) = api(Reply::json(200, "<html>SECRET</html>")).await;
    let error = api.problem().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    let codec = error.codec_error().unwrap();
    assert_eq!(codec.label(), "the body is not valid JSON");
    assert_eq!((codec.get("line"), codec.get("column")), (Some(1), Some(1)));
    assert!(!format!("{error} {error:?}").contains("SECRET"));
    assert!(codec.source_unredacted().is_some());
}

#[test]
fn invalid_base_url_is_not_a_request_error() {
    let error = Api::new(reqwest::Client::new())
        .with_base_url("ftp://inspect.invalid")
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert_eq!(error.endpoint(), None);
    assert_eq!(
        error.to_string(),
        "invalid base URL: it must use http or https"
    );
}

/// The runtime types can cross threads; the generated `send()` future is `Send` for `Send`
/// values. No size is promised.
#[test]
fn runtime_types_are_send_and_sync() {
    fn send_sync<T: Send + Sync>() {}
    fn send<T: Send>(_: &T) {}
    send_sync::<Error>();
    send_sync::<CodecError>();
    send_sync::<concord::Prepared<Problem>>();
    send_sync::<concord::Decoder<Problem>>();
    send_sync::<Api>();
    let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(CodecError::new("x"));
    drop(boxed);
    let api = Api::new(reqwest::Client::new());
    send(&api.problem().send());
    send(&api.problem().prepare().unwrap().send());
}
