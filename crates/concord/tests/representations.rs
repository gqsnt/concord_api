//! The `text` and `bytes` formats, custom formats, endpoints without output, status handling and
//! transport errors — without the `json` feature, so these run in every feature combination.
//! Scenario IDs refer to `v4/TEST_MATRIX.md`.

use concord::{Binary, Text};
mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use concord::{ErrorKind, bytes::Bytes, reqwest};
use support::{Reply, Server};
use tokio::io::AsyncWriteExt;

static DECODED_LEN: AtomicUsize = AtomicUsize::new(usize::MAX);

/// A user format that measures the complete body it receives.
pub struct Measured;

impl concord::Decode<usize> for Measured {
    type Context = ();
    fn context(_: &concord::ResponseHead<'_>) -> Result<(), concord::CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<usize, concord::CodecError> {
        DECODED_LEN.store(body.len(), Ordering::SeqCst);
        Ok(body.len())
    }
}

/// A user format with an invalid content type: usable only where a declared media type replaces
/// it (otherwise the declaration does not compile, `tests/ui/format_invalid_content_type.rs`).
pub struct BadType;

impl concord::Encode<String> for BadType {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x\u{2011}custom");
    fn encode(value: String) -> Result<reqwest::Body, concord::CodecError> {
        Ok(value.into())
    }
}

concord::api! {
    /// Representation fixture.
    pub client Reps {
        base "http://reps.invalid";

        PUT text(body value: String) {
            request Text;
            path "text";
        }
        PUT binary(body value: concord::bytes::Bytes) {
            request Binary;
            path "binary";
        }
        GET text_out() -> String {
            response Text;
            path "text";
        }
        GET bytes_out() -> concord::bytes::Bytes {
            response Binary;
            path "bytes";
        }
        DELETE remove(id: u32) { path "items/{id}"; }
        HEAD exists(id: u32) { path "items/{id}"; }
        GET measured() -> usize {
            response Measured;
            path "big";
        }
        POST bad(body value: String) {
            request BadType "application/x-custom";
            path "bad";
        }
    }
}

async fn reps(reply: Reply) -> (Server, Reps) {
    let server = Server::reply(reply).await;
    let api = Reps::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    (server, api)
}

/// R07: text and binary bodies are sent unchanged, with their content types.
#[tokio::test]
async fn r07_text_and_binary_bodies() {
    let (server, api) = reps(Reply::new(204)).await;
    api.clone().text("héllo ✓".to_owned()).send().await.unwrap();
    api.binary(Bytes::from_static(&[0, 159, 146, 150, 255]))
        .send()
        .await
        .unwrap();
    let r = server.requests();
    assert_eq!(
        r[0].header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(r[0].body, "héllo ✓".as_bytes());
    assert_eq!(
        r[1].header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(r[1].body, [0, 159, 146, 150, 255]);
}

/// R07: text and binary responses are returned unchanged; invalid UTF-8 text is a decode error.
#[tokio::test]
async fn r07_text_and_binary_responses() {
    let (_server, api) = reps(Reply::text(200, "héllo ✓")).await;
    assert_eq!(api.text_out().send().await.unwrap(), "héllo ✓");

    let (_server, api) = reps(Reply::new(200).body([0u8, 255, 1])).await;
    assert_eq!(
        api.clone().bytes_out().send().await.unwrap().as_ref(),
        [0, 255, 1]
    );
    let error = api.text_out().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(
        error
            .to_string()
            .ends_with("the body is not valid UTF-8 (byte 1)"),
        "{error}"
    );
}

/// R04: `()` accepts 204 and a 2xx with a body; HEAD works.
#[tokio::test]
async fn r04_unit_accepts_any_2xx_body() {
    let (_server, api) = reps(Reply::new(204)).await;
    api.remove(1).send().await.unwrap();
    let (_server, api) = reps(Reply::text(200, "deleted")).await;
    api.remove(1).send().await.unwrap();
    let (server, api) = reps(Reply::new(200)).await;
    api.exists(3).send().await.unwrap();
    assert_eq!(server.single().method, "HEAD");
}

/// C4-R-010: a custom decoder receives the complete body; an endpoint without `->` succeeds on any
/// 2xx and drains the body (that it is not buffered is checked by `checks/heap`).
#[tokio::test]
async fn custom_decoder_gets_the_body_and_no_output_drains() {
    let (_server, api) = reps(Reply::new(200).body(vec![9u8; 1_000_000])).await;
    assert_eq!(api.clone().measured().send().await.unwrap(), 1_000_000);
    assert_eq!(DECODED_LEN.load(Ordering::SeqCst), 1_000_000);
    api.remove(1).send().await.unwrap();
}

/// A raw server that announces `content-length: 1000` but closes after 10 bytes.
async fn truncating_server() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 1000\r\n\r\n0123456789")
                .await;
            let _ = stream.shutdown().await;
        }
    });
    addr
}

/// C4-R-010: draining reads the body to its end: a truncated body is a transport error, not a
/// silent success.
#[tokio::test]
async fn unit_drain_detects_truncated_body() {
    let addr = truncating_server().await;
    let api = Reps::new(reqwest::Client::new())
        .with_base_url(format!("http://{addr}"))
        .unwrap();
    let error = api.remove(1).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert_eq!(
        error.to_string(),
        "DELETE Reps.remove: transport error (reading the response body)"
    );
}

/// R05 + `Decoder`: the status check applies to natively executed requests too; the unread
/// response is kept.
#[tokio::test]
async fn decoder_applies_status_handling() {
    let (_server, api) = reps(Reply::text(500, "server says no")).await;
    let (client, request, decoder) = api.text_out().prepare().unwrap().into_parts();
    let response = client.execute(request).await.unwrap();
    let error = decoder.decode(response).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(
        error.status(),
        Some(reqwest::StatusCode::INTERNAL_SERVER_ERROR)
    );
    assert_eq!(
        error.to_string(),
        "GET Reps.text_out: HTTP status 500 Internal Server Error"
    );
    assert_eq!(
        error.into_response().unwrap().text().await.unwrap(),
        "server says no"
    );
}

/// M05: a transport failure is one attempt — the server sees exactly one connection.
#[tokio::test]
async fn m05_transport_failure_is_one_attempt() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = std::sync::Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            count.fetch_add(1, Ordering::SeqCst);
            drop(stream); // close without answering
        }
    });
    let api = Reps::new(reqwest::Client::new())
        .with_base_url(format!("http://{addr}"))
        .unwrap();
    let error = api.text_out().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
}

/// A refused connection is a transport error with a static message and no URL.
#[tokio::test]
async fn transport_error_message() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let api = Reps::new(reqwest::Client::new())
        .with_base_url(format!("http://{addr}"))
        .unwrap();
    let error = api.text_out().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.transport().unwrap().is_connect());
    assert_eq!(
        error.to_string(),
        "GET Reps.text_out: transport error (connect)"
    );
}

/// ADV-7 / 8g: a format's invalid `CONTENT_TYPE` is a compile error wherever it may be sent
/// (`tests/ui/format_invalid_content_type.rs`), and is not read at all where a declared media type
/// replaces it: this body is sent with the declared one.
#[tokio::test]
async fn declared_media_type_replaces_an_invalid_constant() {
    let (server, api) = reps(Reply::new(204)).await;
    api.bad("x".to_owned()).send().await.unwrap();
    let r = server.single();
    assert_eq!(r.header("content-type"), Some("application/x-custom"));
    assert_eq!(r.body, b"x");
}

/// D13: `into_response` hands back the unread response of a status error and returns any other
/// error unchanged, so `?` propagates it (it used to drop non-status errors).
#[tokio::test]
async fn into_response_keeps_other_errors() {
    let (_server, api) = reps(Reply::text(503, "busy")).await;
    let e = api.text_out().send().await.unwrap_err();
    let response = e.into_response().unwrap();
    assert_eq!(response.status(), 503);
    assert_eq!(response.text().await.unwrap(), "busy");

    let unreachable = Reps::new(reqwest::Client::new())
        .with_base_url("http://127.0.0.1:1")
        .unwrap();
    let e = unreachable.text_out().send().await.unwrap_err();
    let e = e.into_response().unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Transport);
    assert_eq!(e.endpoint(), Some("Reps.text_out"));
}
