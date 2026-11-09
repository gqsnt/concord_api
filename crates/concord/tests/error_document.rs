//! A1: `error D in F;` — a non-2xx answer read once, bounded, as the declared error document,
//! the status authoritative; nearest owner wins; no request byte changes; the native exits stay.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use concord::bytes::Bytes;
use concord::{CodecError, Decode, Document, ErrorKind, Json, ResponseHead, reqwest};
use serde::Deserialize;
use support::{Reply, Server};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

/// A document whose version comes from the response head (the format's context).
#[derive(Debug)]
pub struct Versioned {
    /// From `x-doc-version`.
    pub version: u64,
    /// The body, as text.
    pub text: String,
}

/// An error format with a context: `x-doc-version` must be a number.
pub struct HeaderVersion;

impl Decode<Versioned> for HeaderVersion {
    type Context = u64;
    fn context(head: &ResponseHead<'_>) -> Result<u64, CodecError> {
        head.headers()
            .get("x-doc-version")
            .and_then(|v| v.to_str().ok()?.parse().ok())
            .ok_or(CodecError::new("no document version"))
    }
    fn decode_body(version: u64, body: Bytes) -> Result<Versioned, CodecError> {
        let text = String::from_utf8_lossy(&body).into_owned();
        Ok(Versioned { version, text })
    }
}

/// How often [`Counted`] decoded a body.
static DECODED: AtomicUsize = AtomicUsize::new(0);

/// An error format that counts its body decodings (used by one test only).
pub struct Counted;

impl Decode<Bytes> for Counted {
    type Context = ();
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<Bytes, CodecError> {
        DECODED.fetch_add(1, Ordering::SeqCst);
        Ok(body)
    }
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

        /// An error format with a context.
        scope versioned {
            error Versioned in HeaderVersion;

            GET doc() -> Problem { path "doc"; }
        }

        /// An instrumented error format.
        scope counted {
            error Bytes in Counted;

            GET big() -> Problem { path "big"; }
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

#[tokio::test]
async fn an_oversized_body_is_never_decoded() {
    let (_server, api) = api(Reply::new(500).body(vec![b'x'; 70 * 1024])).await;
    let error = api.counted().big().send().await.unwrap_err();
    let failure = error.provider().unwrap();
    assert!(matches!(
        failure.state(),
        Document::TooLong { limit: 65_536 }
    ));
    assert_eq!(failure.body().len(), 65_536);
    assert_eq!(DECODED.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn the_error_format_reads_its_context_from_the_head() {
    let (_server, first) = api(Reply::new(400)
        .header("x-doc-version", "3")
        .body("v3 problem"))
    .await;
    let error = first.versioned().doc().send().await.unwrap_err();
    let document = error.document().unwrap();
    assert_eq!(
        (document.version, document.text.as_str()),
        (3, "v3 problem")
    );
    // A refused context: no decoding, the bytes kept, the status authoritative.
    let (_unversioned, client) = api(Reply::new(400).body("unversioned")).await;
    let error = client.versioned().doc().send().await.unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 400);
    let failure = error.provider().unwrap();
    assert!(
        matches!(failure.state(), Document::Unrecognized(e) if e.label() == "no document version")
    );
    assert_eq!(failure.body().as_ref(), b"unversioned");
}

/// A server that announces 100 body bytes, sends `prefix`, then closes the connection.
async fn broken(prefix: &'static [u8]) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buf).await.unwrap();
            if n == 0 {
                return;
            }
            request.extend_from_slice(&buf[..n]);
        }
        let head =
            "HTTP/1.1 502 X\r\ncontent-type: application/json\r\ncontent-length: 100\r\n\r\n";
        stream.write_all(head.as_bytes()).await.unwrap();
        stream.write_all(prefix).await.unwrap();
        stream.flush().await.unwrap();
    });
    url
}

#[tokio::test]
async fn a_broken_body_keeps_its_exact_prefix() {
    let api = Api::new(reqwest::Client::new())
        .with_base_url(broken(br#"{"title":"pre"#).await)
        .unwrap();
    let error = api.item().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status().unwrap().as_u16(), 502);
    let failure = error.provider().unwrap();
    let Document::Broken(cause) = failure.state() else {
        panic!("expected a broken body, got {:?}", failure.state());
    };
    assert_eq!(cause.kind(), ErrorKind::Transport);
    assert_eq!(cause.delivery(), concord::Delivery::Answered);
    assert_eq!(failure.body().as_ref(), br#"{"title":"pre"#);
}

#[tokio::test]
async fn the_error_bound_survives_follow_into_parts_and_finish() {
    let body = format!(r#"{{"title":"{}"}}"#, "x".repeat(64));
    let (server, api) = api(Reply::json(400, &body)).await;
    let bounded = || api.item().prepare().unwrap().error_body_limit(16);
    let too_long = |error: concord::Error<Problem>| {
        let failure = error.provider().unwrap();
        assert!(matches!(failure.state(), Document::TooLong { limit: 16 }));
        assert_eq!(failure.body().len(), 16);
    };
    // `follow` keeps the decoder and its bound.
    let followed = bounded()
        .follow(&format!("{}/item?page=2", server.url()))
        .unwrap();
    too_long(followed.send().await.unwrap_err());
    // `into_parts` then `Decoder::finish`.
    let (client, request, decoder) = bounded().into_parts();
    too_long(
        decoder
            .finish(client.execute(request).await)
            .await
            .unwrap_err(),
    );
    // `into_parts` then `Decoder::execute` and `Received::decode`.
    let (client, request, decoder) = bounded().into_parts();
    let received = decoder.execute(&client, request).await.unwrap();
    too_long(received.decode().await.unwrap_err());
    assert_eq!(server.requests().len(), 3);
    assert_eq!(server.requests()[0].target, "/item?page=2");
}

/// An SDK's provider answer, built from the failure of the declared document.
pub struct Refusal {
    status: u16,
    title: Option<String>,
}

impl std::fmt::Debug for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Refusal")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl From<concord::Failure<Problem>> for Refusal {
    fn from(failure: concord::Failure<Problem>) -> Self {
        Self {
            status: failure.status().as_u16(),
            title: failure.document().map(|problem| problem.title.clone()),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "refused with {}", self.status)
    }
}

/// An SDK's error enum: `?` on a `concord::Error<Problem>` splits it.
#[derive(Debug, concord::derive::Error)]
pub enum SdkError {
    /// Every other failure.
    #[display("{0}")]
    Concord(#[source] concord::Error),
    /// The provider's answer.
    #[display("{0}")]
    #[from(concord::Error<Problem>)]
    Api(Box<Refusal>),
}

async fn sdk_item(api: &Api) -> Result<Problem, SdkError> {
    Ok(api.item().send().await?)
}

#[tokio::test]
async fn the_derive_splits_the_provider_answer_from_other_failures() {
    use std::error::Error as _;
    let (_server, api) = api(Reply::json(409, r#"{"title":"SECRET-TITLE"}"#)).await;
    let error = sdk_item(&api).await.unwrap_err();
    let SdkError::Api(refusal) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(refusal.status, 409);
    assert_eq!(refusal.title.as_deref(), Some("SECRET-TITLE"));
    assert_eq!(error.to_string(), "refused with 409");
    // SDK payloads that extract document fields must withhold them in their own formatting.
    for rendered in [
        format!("{error}"),
        format!("{error:#}"),
        format!("{error:?}"),
        format!("{error:#?}"),
    ] {
        assert!(!rendered.contains("SECRET-TITLE"));
    }
    assert!(error.source().is_none());

    // A 2xx body that does not decode is not the provider's answer: it stays Concord's error,
    // and is the source.
    let (_server, plain) = self::api(Reply::json(200, "not json")).await;
    let error = sdk_item(&plain).await.unwrap_err();
    let SdkError::Concord(concord) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(concord.kind(), ErrorKind::Decode);
    assert_eq!(error.source().unwrap().to_string(), concord.to_string());
}

#[tokio::test]
async fn a_response_outside_the_declaration_reads_a_named_document() {
    let server = Server::reply(Reply::json(409, r#"{"title":"SECRET-TITLE"}"#)).await;
    let plain = Plain::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    let received = plain.item().prepare().unwrap().receive().await.unwrap();
    let failure = received.failure_as::<Json, Problem>(64).await;
    assert_eq!(failure.status().as_u16(), 409);
    assert_eq!(failure.operation().map(|op| op.name()), Some("Plain.item"));
    assert_eq!(failure.document().unwrap().title, "SECRET-TITLE");
    assert!(!format!("{failure:?}").contains("SECRET-TITLE"));
    // The same bound and states as a declared document: too long is never decoded.
    let server = Server::reply(Reply::json(500, r#"{"title":"SECRET-TITLE"}"#)).await;
    let received = concord::Decoder::drain().receive(reqwest::get(server.url()).await.unwrap());
    let failure = received.failure_as::<Json, Problem>(8).await;
    assert!(matches!(failure.state(), Document::TooLong { limit: 8 }));
    assert_eq!(failure.body().len(), 8);
}
