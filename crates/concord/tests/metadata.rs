//! Response metadata: a format checks the response head before the body is read (C01, C03, C06,
//! C07, C13, C16, C17, C20, E04), against loopback servers.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::bytes::Bytes;
use concord::reqwest::header::HeaderName;
use concord::{
    CodecError, Decode, Decoder, ErrorKind, IgnoreContentType, Json, ResponseHead, reqwest,
};
use serde::Deserialize;
use support::{Reply, Server};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// An item.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Item {
    /// Identifier.
    pub id: u64,
}

/// The response header carrying the schema version of [`Versioned`].
pub const API_VERSION: HeaderName = HeaderName::from_static("x-api-version");

/// A downstream format that needs response metadata (C13): `application/json` checked by
/// `Json`'s own rule, a required `x-api-version: 2` checked in `context`, and the version handed to
/// the body decoder. Written against the public traits only.
pub struct Versioned;

/// How many times the body decoder of [`Versioned`] ran. The tests that read it hold [`SERIAL`].
pub static DECODED: AtomicUsize = AtomicUsize::new(0);
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

impl Decode<Item> for Versioned {
    type Context = u32;
    const ACCEPT: Option<&'static str> = Some("application/json");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Json as Decode<Item>>::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<u32, CodecError> {
        let mut values = head.headers().get_all(&API_VERSION).iter();
        let (Some(value), None) = (values.next(), values.next()) else {
            return Err(CodecError::new("the response has no single x-api-version"));
        };
        match value.to_str().ok().and_then(|v| v.parse().ok()) {
            Some(2) => Ok(2),
            Some(v) => Err(CodecError::new("unsupported x-api-version").at("version", v)),
            None => Err(CodecError::new("x-api-version is not a number")),
        }
    }

    fn decode_body(version: u32, body: Bytes) -> Result<Item, CodecError> {
        DECODED.fetch_add(1, Ordering::SeqCst);
        let item: Item = <Json as Decode<Item>>::decode_body((), body)?;
        // The version known from the head shapes decoding: v2 ids are offset.
        Ok(Item {
            id: item.id + u64::from(version) * 1000,
        })
    }
}

concord::api! {
    /// Metadata fixture.
    pub client Meta {
        base "http://meta.invalid";
        response Json;
        GET item() -> Item { path "item"; }
        GET versioned() -> Item { path "item"; response Versioned; }
        GET versioned_any_type() -> Item {
            path "item";
            response IgnoreContentType<Versioned>;
        }
        GET nothing() { path "item"; }
    }
}

fn meta(url: String) -> Meta {
    Meta::new(reqwest::Client::new())
        .with_base_url(url)
        .unwrap()
}

/// A server that sends a response head announcing a body of `body.len()` bytes, then withholds the
/// body until `release` fires. Returns its URL and a counter of body bytes written.
async fn withholding(
    head: &'static str,
    body: &'static [u8],
) -> (String, oneshot::Sender<()>, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (release, released) = oneshot::channel::<()>();
    let written = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&written);
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buf).await.unwrap();
            request.extend_from_slice(&buf[..n]);
        }
        let head = format!("{head}\r\ncontent-length: {}\r\n\r\n", body.len());
        stream.write_all(head.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
        let _ = released.await;
        stream.write_all(body).await.unwrap();
        counter.fetch_add(body.len(), Ordering::SeqCst);
        stream.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    });
    (url, release, written)
}

/// C20/C03: JSON bytes labelled `text/html` are refused while the server still withholds the
/// body: the decision needs only the head. The response, kept unread, then yields the whole body.
/// (A decoder that read the body first would wait for the release and time out here.)
#[tokio::test]
async fn the_head_is_refused_before_the_body_exists() {
    let (url, release, written) =
        withholding("HTTP/1.1 200 OK\r\ncontent-type: text/html", br#"{"id":7}"#).await;
    let error = tokio::time::timeout(Duration::from_secs(5), meta(url).item().send())
        .await
        .expect("refused without waiting for the body")
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(
        written.load(Ordering::SeqCst),
        0,
        "the body was not sent yet"
    );
    assert_eq!(
        error.codec_error().map(CodecError::label),
        Some("the response Content-Type is not application/json or an application/*+json type")
    );
    let response = error.into_response().unwrap();
    assert_eq!(response.headers()["content-type"], "text/html");
    release.send(()).unwrap();
    assert_eq!(response.bytes().await.unwrap().as_ref(), br#"{"id":7}"#);
}

/// C13: a downstream format reads a version header before the body and hands it to the body
/// decoder; a missing or unsupported version is refused before the body is read (E04), also
/// through `IgnoreContentType`, which skips only the `Content-Type` check.
#[tokio::test]
async fn a_downstream_format_uses_response_metadata() {
    let _serial = SERIAL.lock().await;
    let reply = |version: Option<&str>, ct: &str| {
        let r = Reply::new(200)
            .header("content-type", ct)
            .body(r#"{"id":7}"#);
        match version {
            Some(v) => r.header("x-api-version", v),
            None => r,
        }
    };
    let server = Server::reply(reply(Some("2"), "application/json")).await;
    assert_eq!(
        meta(server.url()).versioned().send().await.unwrap(),
        Item { id: 2007 }
    );
    assert_eq!(server.single().header("accept"), Some("application/json"));

    let before = DECODED.load(Ordering::SeqCst);
    for (version, label) in [
        (None, "the response has no single x-api-version"),
        (Some("3"), "unsupported x-api-version"),
        (Some("two"), "x-api-version is not a number"),
    ] {
        for adapted in [false, true] {
            let server = Server::reply(reply(version, "application/json")).await;
            let api = meta(server.url());
            let error = if adapted {
                api.versioned_any_type().send().await.unwrap_err()
            } else {
                api.versioned().send().await.unwrap_err()
            };
            assert_eq!(error.kind(), ErrorKind::Metadata);
            assert_eq!(error.codec_error().map(CodecError::label), Some(label));
        }
    }
    let server = Server::reply(reply(Some("3"), "application/json")).await;
    let error = meta(server.url()).versioned().send().await.unwrap_err();
    assert_eq!(error.codec_error().and_then(|e| e.get("version")), Some(3));
    assert_eq!(
        DECODED.load(Ordering::SeqCst),
        before,
        "no refused body was decoded"
    );

    // The adapter skips the media check only.
    let server = Server::reply(reply(Some("2"), "text/plain")).await;
    assert_eq!(
        meta(server.url())
            .versioned()
            .send()
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::Metadata
    );
    let server = Server::reply(reply(Some("2"), "text/plain")).await;
    assert_eq!(
        meta(server.url())
            .versioned_any_type()
            .send()
            .await
            .unwrap(),
        Item { id: 2007 }
    );
}

/// C07: a non-2xx status is a `Status` error before the format is consulted, whatever its
/// `Content-Type`; the error document stays unread and can be decoded body-only (no head check,
/// by name).
#[tokio::test]
async fn status_comes_before_the_format() {
    let _serial = SERIAL.lock().await;
    for reply in [
        Reply::new(502)
            .header("content-type", "text/html")
            .body("<html>proxy</html>"),
        Reply::new(404),
        Reply::json(409, r#"{"id":9}"#),
    ] {
        let server = Server::reply(reply).await;
        let before = DECODED.load(Ordering::SeqCst);
        let error = meta(server.url()).versioned().send().await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Status);
        assert!(error.codec_error().is_none());
        assert_eq!(DECODED.load(Ordering::SeqCst), before);
        let status = error.status().unwrap();
        let body = error.into_response().unwrap().bytes().await.unwrap();
        if status == reqwest::StatusCode::CONFLICT {
            let item = <Json as Decode<Item>>::decode_body((), body).unwrap();
            assert_eq!(item, Item { id: 9 });
        }
    }
}

/// C16/C17: the native exits. `send_raw` validates nothing; `Decoder::finish` after a native
/// execution applies the same status, head and body rules as `send`; a decoder built with
/// `Decoder::new` does too, outside any declaration.
#[tokio::test]
async fn native_exits_apply_the_same_rules() {
    let html = || {
        Reply::new(200)
            .header("content-type", "text/html")
            .body(r#"{"id":1}"#)
    };
    let server = Server::reply(html()).await;
    let raw = meta(server.url())
        .item()
        .prepare()
        .unwrap()
        .send_raw()
        .await
        .unwrap();
    assert_eq!(raw.status(), 200);

    let server = Server::reply(html()).await;
    let (client, request, decoder) = meta(server.url()).item().prepare().unwrap().into_parts();
    let error = decoder
        .finish(client.execute(request).await)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(error.endpoint(), Some("Meta.item"));

    let server = Server::reply(html()).await;
    let response = reqwest::Client::new().get(server.url()).send().await;
    let error = Decoder::<Item>::new::<Json>()
        .finish(response)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(error.endpoint(), None);
    let server = Server::reply(Reply::json(200, r#"{"id":3}"#)).await;
    let response = reqwest::Client::new().get(server.url()).send().await;
    let item = Decoder::<Item>::new::<Json>()
        .finish(response)
        .await
        .unwrap();
    assert_eq!(item, Item { id: 3 });
}

/// C09: an endpoint without `->` ignores the inherited format: no head check, any 2xx.
#[tokio::test]
async fn no_output_checks_nothing() {
    let server = Server::reply(
        Reply::new(200)
            .header("content-type", "text/html")
            .body("x"),
    )
    .await;
    meta(server.url()).nothing().send().await.unwrap();
    assert_eq!(server.single().header("accept"), Some("*/*"));
}

/// C06: a refused head's error prints no header value and no body.
#[tokio::test]
async fn metadata_errors_print_no_response_content() {
    let server = Server::reply(
        Reply::new(200)
            .header("content-type", "text/html; token=S3CR3T")
            .header("set-cookie", "session=S3CR3T")
            .body("S3CR3T-BODY"),
    )
    .await;
    let error = meta(server.url()).item().send().await.unwrap_err();
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains("S3CR3T"), "{shown}");
    assert!(std::error::Error::source(&error).is_none());
    // The unredacted response is reachable explicitly.
    assert!(
        error.response().unwrap().headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("S3CR3T")
    );
}

/// C15: the request future stays `Send` with a stateful context; the decoder handle is `Copy`.
#[test]
fn futures_stay_send() {
    fn send<T: Send>(_: &T) {}
    fn copy<T: Copy>() {}
    let api = Meta::new(reqwest::Client::new());
    send(&api.versioned().send());
    send(&api.versioned().prepare().unwrap().send());
    copy::<Decoder<Item>>();
}

/// A payload that can be sent once: not `Clone`, consumed by its encoder.
pub struct OneShot(Vec<u8>);

/// An encoder-only downstream format over a one-shot payload (C14).
pub struct Upload;

impl concord::Encode<OneShot> for Upload {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-upload");
    fn encode(value: OneShot) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value.0))
    }
}

concord::api! {
    /// Encoder-only and transport-metadata fixtures.
    pub client Transfer {
        base "http://transfer.invalid";
        PUT upload(body file: OneShot) { path "u"; request Upload; }
        GET packed() -> Vec<u8> { path "p"; response concord::Binary; }
        GET packed_json() -> Item { path "p"; response Json; }
    }
}

/// C14: the payload is moved into the body; no `Clone` is required of it (the request type is
/// then not `Clone` either), and the bytes arrive as encoded.
#[tokio::test]
async fn a_one_shot_payload_is_moved_into_the_body() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Transfer::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    api.upload(OneShot(b"once".to_vec())).send().await.unwrap();
    let request = server.single();
    assert_eq!(request.body, b"once");
    assert_eq!(request.header("content-type"), Some("application/x-upload"));
}

/// C19: `Content-Encoding` is not a media type, and Concord decompresses nothing (reqwest's own
/// decompression features are off in this build): `Binary` returns the encoded bytes unchanged,
/// and `Json` accepts the JSON media type but cannot decode gzip bytes (a decode error, not a
/// metadata one).
#[tokio::test]
async fn content_encoding_is_not_a_media_type() {
    let gzip: &[u8] = b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x00\x03";
    let reply = || {
        Reply::new(200)
            .header("content-type", "application/json")
            .header("content-encoding", "gzip")
            .body(gzip)
    };
    let server = Server::reply(reply()).await;
    let api = Transfer::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    assert_eq!(api.packed().send().await.unwrap(), gzip);
    let server = Server::reply(reply()).await;
    let api = Transfer::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    assert_eq!(
        api.packed_json().send().await.unwrap_err().kind(),
        ErrorKind::Decode
    );
}

/// Review F1 on the wire: a quoted UTF-8 `charset` followed by another parameter is accepted
/// (the pinned `mime` misreads it), and a repeated `charset` is refused before the body is read.
#[tokio::test]
async fn charset_parameters_are_read_exactly() {
    let reply = |ct: &str| {
        Reply::new(200)
            .header("content-type", ct)
            .body(r#"{"id":7}"#)
    };
    let server = Server::reply(reply("application/json; charset=\"utf-8\"; profile=x")).await;
    assert_eq!(
        meta(server.url()).item().send().await.unwrap(),
        Item { id: 7 }
    );
    let server = Server::reply(reply("application/json; charset=utf-8; charset=iso-8859-1")).await;
    let error = meta(server.url()).item().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(
        error.codec_error().map(CodecError::label),
        Some("the response Content-Type declares charset more than once")
    );
}
