//! Inspecting errors without consuming them (`Error::response`, `BodyError::body_source`), the
//! static hints of the JSON decoder, the base-URL message, and the auto traits of the runtime types.

mod support;

use concord::{BodyError, Decode, Error, ErrorKind, reqwest};
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
    fn decode(body: concord::bytes::Bytes) -> Result<String, BodyError> {
        Err(BodyError::new("strict format refused the body")
            .with_source(Refused(String::from_utf8_lossy(&body).into_owned())))
    }
}

concord::api! {
    /// Error-inspection fixture.
    pub client Api {
        base "http://inspect.invalid";
        response json;

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
    let problem = <concord::Json as Decode<Problem>>::decode(bytes).unwrap();
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
async fn body_source_of_a_standalone_body_error() {
    // Through a Concord call.
    let (_server, api) = api(Reply::text(200, "CONTENT")).await;
    let error = api.strict().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(!format!("{error} {error:?}").contains("CONTENT"));
    let source = error.body_source().unwrap();
    assert_eq!(source.to_string(), "refused: CONTENT");
    // Directly, as when decoding an error document yourself.
    let error = <Strict as Decode<String>>::decode("CONTENT".into()).unwrap_err();
    assert_eq!(error.to_string(), "strict format refused the body");
    assert!(!format!("{error:?}").contains("CONTENT"));
    assert_eq!(error.body_source().unwrap().to_string(), "refused: CONTENT");
    assert!(std::error::Error::source(&error).is_none());
}

const EMPTY_HINT: &str = "the response body is empty or only whitespace, which is not JSON; omit `->` if this endpoint never returns a body, or use a format that accepts an empty one";
const TEXT_HINT: &str =
    "if the response is not JSON, the endpoint may need `response text;` or `response bytes;`";

/// Empty, whitespace and `null` stay distinct; no output accepts anything.
#[tokio::test]
async fn empty_responses_are_not_null() {
    for reply in [Reply::new(204), Reply::text(200, " \r\n\t")] {
        let (_server, api) = api(reply).await;
        for error in [
            api.unit().send().await.unwrap_err(),
            api.maybe().send().await.map(drop).unwrap_err(),
            api.value().send().await.map(drop).unwrap_err(),
        ] {
            assert_eq!(error.kind(), ErrorKind::Decode);
            assert!(error.to_string().ends_with(EMPTY_HINT), "{error}");
        }
        api.nothing().send().await.unwrap();
    }
    let (_server, api) = api(Reply::json(200, "null")).await;
    api.unit().send().await.unwrap();
    assert!(api.maybe().send().await.unwrap().is_none());
    assert_eq!(api.value().send().await.unwrap(), serde_json::Value::Null);
}

/// The not-JSON hint appears for a syntax error at the first byte only, and never quotes the
/// body.
#[tokio::test]
async fn not_json_hint_is_static_and_only_at_the_first_byte() {
    let png: &[u8] = b"\x89PNG\r\n\x1a\n";
    let cases: [(&[u8], bool); 5] = [
        (b"hello", true),
        (b"<html><body>SECRET</body></html>", true),
        (png, true),
        (b"2.4.1", false),
        (b"{\"title\": 1}", false),
    ];
    for (body, hinted) in cases {
        let (_server, api) = api(Reply::new(200).body(body)).await;
        for error in [
            api.text().send().await.map(drop).unwrap_err(),
            api.raw().send().await.map(drop).unwrap_err(),
            api.problem().send().await.map(drop).unwrap_err(),
        ] {
            let shown = error.to_string();
            assert_eq!(error.kind(), ErrorKind::Decode, "{shown}");
            assert_eq!(shown.ends_with(TEXT_HINT), hinted, "{body:?}: {shown}");
            assert!(
                !shown.contains("SECRET") && !shown.contains("hello"),
                "{shown}"
            );
        }
    }
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
    send_sync::<BodyError>();
    send_sync::<concord::Prepared<Problem>>();
    send_sync::<concord::Decoder<Problem>>();
    send_sync::<Api>();
    let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(BodyError::new("x"));
    drop(boxed);
    let api = Api::new(reqwest::Client::new());
    send(&api.problem().send());
    send(&api.problem().prepare().unwrap().send());
}
