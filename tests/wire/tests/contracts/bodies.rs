//! Bodies against a real server: a body of a declared `Content-Length` that ends short
//! (C5.4), chunks that come slowly under the idle timeout (C7.6), a large download read as a
//! stream and within the bound of a whole read (C5.4, C5.11), a multipart upload (C2.16), and
//! repeatable bodies sent again after a `503`, byte for byte (C3.2, C12.12).

use crate::server::{
    after_head, on_paused_clock, settle, Ending, Feed, Feeder, Frozen, Gate, Peer, Probe, Server,
    Wire,
};
use axum::body::Body as Wired;
use axum::extract::{ConnectInfo, Multipart, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::Router;
use concord::bytes::Bytes;
use concord::error::{Class, Kind};
use concord::read::Stream as Chunks;
use concord::{Binary, ByteStream, Text};
use futures_core::Stream;
use std::collections::VecDeque;
use std::future::IntoFuture;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

/// The pieces of a repeatable upload, opened again for each attempt.
struct Pieces(VecDeque<Bytes>);

impl Stream for Pieces {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.pop_front().map(Ok))
    }
}

/// Three pieces, the same at each opening.
fn pieces() -> Pieces {
    Pieces(
        [&b"first piece, "[..], b"second piece, ", b"last piece"]
            .into_iter()
            .map(Bytes::from_static)
            .collect(),
    )
}

const UPLOAD: &[u8] = b"first piece, second piece, last piece";

concord::api! {
    client Store {
        base "http://store.test";
        read Binary;
        GET short() -> Bytes { path "short"; }
        GET held() -> Bytes { path "held"; }
        GET slow() -> ByteStream { path "slow"; read Chunks; }
        GET large() -> ByteStream { path "large"; read Chunks; }
        GET large_whole() -> Bytes { path "large"; }
        GET large_bounded() -> Bytes { path "large"; read Binary.limit(LARGE); }
        POST upload(title: String, photo: Bytes) -> String {
            path "upload";
            body concord::multipart()
                .text("title", title)
                .file("photo", "lamp.bin", "application/octet-stream", photo.clone())
                .finish();
            read Text;
        }
        PUT replace() -> String {
            path "repeat";
            body concord::Body::repeatable(|| async { Ok::<_, io::Error>(pieces()) });
            read Text;
        }
        POST upload_again(title: String) -> String {
            path "repeat";
            idempotent;
            body concord::multipart()
                .text("title", title)
                .file(
                    "pieces",
                    "pieces.txt",
                    "text/plain",
                    concord::Body::repeatable(|| async { Ok::<_, io::Error>(pieces()) }),
                )
                .finish();
            read Text;
        }
    }
}

// A declared length that the body does not reach ----------------------------------------

async fn short_body(wire: Wire) {
    let router = Router::new().route(
        "/short",
        get(|ConnectInfo(peer): ConnectInfo<Peer>| async move {
            let first = Bytes::from_static(b"only forty of the hundred bytes promised");
            Response::builder()
                .header("content-length", "100")
                .body(after_head(peer.link, first, Ending::Stop))
                .unwrap()
        }),
    );
    let server = Server::start(router).await;
    let error = Store::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .short()
        .await
        .unwrap_err();

    // A body that ends before its announced size is a Transport error that keeps the status
    // and the headers; it is never a shorter value.
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert_eq!(error.status().map(|status| status.as_u16()), Some(200));
    assert_eq!(
        error
            .headers()
            .and_then(|headers| headers.get("content-length")),
        Some("100")
    );
}

#[tokio::test]
async fn c5_4_a_body_that_ends_before_its_content_length_is_a_body_error_over_http1() {
    short_body(Wire::One).await;
}

#[tokio::test]
async fn c5_4_a_body_that_ends_before_its_content_length_is_a_body_error_over_http2() {
    short_body(Wire::Two).await;
}

// Slow chunks and the idle timeout ------------------------------------------------------

/// A route that answers with the body of the next feed, chunked.
fn feeding(path: &str, content_type: &'static str) -> (Router, Arc<Mutex<Vec<Feed<Bytes>>>>) {
    let feeds: Arc<Mutex<Vec<Feed<Bytes>>>> = Arc::default();
    let router = Router::new()
        .route(
            path,
            get(
                move |State(feeds): State<Arc<Mutex<Vec<Feed<Bytes>>>>>| async move {
                    let feed = feeds.lock().unwrap().remove(0);
                    ([("content-type", content_type)], Wired::from_stream(feed))
                },
            ),
        )
        .with_state(feeds.clone());
    (router, feeds)
}

async fn slow_chunks(wire: Wire) {
    let _clock = Frozen::hold();
    let (router, feeds) = feeding("/slow", "application/octet-stream");
    let (feeder, feed): (Feeder<Bytes>, _) = Feed::new();
    feeds.lock().unwrap().push(feed);
    let server = Server::start(router).await;
    let idle = Duration::from_secs(1);
    let mut stream = Store::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .slow()
        .with_idle_timeout(idle)
        .await
        .unwrap();

    // The reader waits for each chunk in a task of its own, so that its wait is in progress
    // while the clock moves.
    let (got, mut chunks) = tokio::sync::mpsc::unbounded_channel();
    let reader = tokio::spawn(async move {
        loop {
            let next = stream.try_next().await;
            let end = !matches!(next, Ok(Some(_)));
            got.send(next).unwrap();
            if end {
                return stream;
            }
        }
    });
    // Five chunks, each after nine tenths of the idle timeout: 4.5 seconds in all, and the
    // body is never cut off while it keeps arriving (G7).
    for n in 0..5u8 {
        tokio::time::advance(idle * 9 / 10).await;
        feeder.send(Bytes::from(vec![n; 3]));
        let chunk = chunks.recv().await.unwrap().unwrap().unwrap();
        assert_eq!(chunk.as_ref(), [n; 3]);
    }
    // Then a silence longer than the idle timeout.
    tokio::time::advance(idle + Duration::from_millis(100)).await;
    let error = chunks.recv().await.unwrap().unwrap_err();
    assert!(error.is_timeout());
    assert_eq!(error.class(), Some(Class::Timeout));
    assert_eq!(error.status().map(|status| status.as_u16()), Some(200));
    assert!(
        error.to_string()
            == "transport error: no body data for 1 s after status 200 (with_idle_timeout changes it), in store.slow",
        "{error}"
    );
    drop(reader.await.unwrap());

    // The call timeout bounds a stream that was handed over, until its end, also while its
    // chunks keep arriving (C7.6).
    let (feeder, feed): (Feeder<Bytes>, _) = Feed::new();
    feeds.lock().unwrap().push(feed);
    let mut stream = Store::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .slow()
        .with_idle_timeout(idle)
        .with_timeout(Duration::from_secs(2))
        .await
        .unwrap();
    let (got, mut chunks) = tokio::sync::mpsc::unbounded_channel();
    let reader = tokio::spawn(async move {
        loop {
            let next = stream.try_next().await;
            let end = !matches!(next, Ok(Some(_)));
            got.send(next).unwrap();
            if end {
                return stream;
            }
        }
    });
    for n in 0..2u8 {
        tokio::time::advance(idle * 9 / 10).await;
        feeder.send(Bytes::from(vec![n; 3]));
        assert_eq!(
            chunks.recv().await.unwrap().unwrap().unwrap().as_ref(),
            [n; 3]
        );
    }
    tokio::time::advance(idle * 9 / 10).await;
    let error = chunks.recv().await.unwrap().unwrap_err();
    assert!(error.is_timeout());
    assert_eq!(
        error.to_string(),
        "transport error: the call took longer than 2 s (with_timeout), in store.slow"
    );
    drop(reader.await.unwrap());
    drop(feeder);
}

#[tokio::test(start_paused = true)]
async fn c7_6_slow_chunks_restart_the_idle_timeout_and_the_call_timeout_still_ends_the_stream_over_http1(
) {
    slow_chunks(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c7_6_slow_chunks_restart_the_idle_timeout_and_the_call_timeout_still_ends_the_stream_over_http2(
) {
    slow_chunks(Wire::Two).await;
}

async fn a_head_held_back(wire: Wire) {
    let _clock = Frozen::hold();
    let gate = Gate::default();
    let router = Router::new()
        .route(
            "/held",
            get(|State(gate): State<Gate>| async move {
                gate.wait().await;
                "too late"
            }),
        )
        .with_state(gate.clone());
    let server = Server::start(router).await;
    let idle = Duration::from_secs(1);
    let call = Store::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .held()
        .with_idle_timeout(idle);
    let call = tokio::spawn(call.into_future());
    // The server has the request, and holds its head back.
    server.seen.until(1).await;
    settle().await;
    tokio::time::advance(idle + Duration::from_millis(100)).await;
    let error = call.await.unwrap().unwrap_err();

    // The idle timeout watches the transport until the head (C7.6).
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Timeout));
    assert!(error.is_timeout());
    assert_eq!(error.status(), None);
    assert!(!error.request_not_sent());
    assert_eq!(error.tries(), 1);
    gate.open();
}

#[tokio::test(start_paused = true)]
async fn c7_6_a_head_held_back_past_the_idle_timeout_is_a_timeout_over_http1() {
    a_head_held_back(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c7_6_a_head_held_back_past_the_idle_timeout_is_a_timeout_over_http2() {
    a_head_held_back(Wire::Two).await;
}

// A large download ----------------------------------------------------------------------

const LARGE: usize = 24 * 1024 * 1024;
const PIECE: usize = 64 * 1024;

/// `LARGE` bytes, in pieces of `PIECE`: byte `i` is `i % 251`.
struct Large(usize);

impl Stream for Large {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.0 >= LARGE {
            return Poll::Ready(None);
        }
        let start = self.0;
        let end = (start + PIECE).min(LARGE);
        self.0 = end;
        Poll::Ready(Some(Ok((start..end).map(|i| (i % 251) as u8).collect())))
    }
}

async fn large_download(wire: Wire) {
    let router = Router::new().route(
        "/large",
        get(|| async {
            Response::builder()
                .header("content-length", LARGE.to_string())
                .body(Wired::from_stream(Large(0)))
                .unwrap()
        }),
    );
    let server = Server::start(router).await;
    let store = Store::new()
        .with_transport(wire.http())
        .with_base(server.url());

    // A body read as a stream has no limit: every byte arrives, in order, a chunk at a time.
    let mut stream = store.large().await.unwrap();
    let (mut at, mut largest) = (0usize, 0usize);
    while let Some(chunk) = stream.try_next().await.unwrap() {
        assert!(
            chunk
                .iter()
                .enumerate()
                .all(|(i, byte)| *byte == ((at + i) % 251) as u8),
            "a byte out of place after {at} bytes"
        );
        at += chunk.len();
        largest = largest.max(chunk.len());
    }
    assert_eq!(at, LARGE);
    assert!(largest < LARGE / 16, "a chunk of {largest} bytes");

    // Read whole, the same body is over the 10 MiB of `Binary`: a Mismatch before anything
    // is read, by its announced size, with no byte kept.
    let error = store.large_whole().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert!(error.kept().is_none());
    // Within a bound that holds it, it is read whole.
    assert_eq!(store.large_bounded().await.unwrap().len(), LARGE);
}

#[tokio::test]
async fn c5_4_a_large_download_streams_whole_and_a_whole_read_holds_its_bound_over_http1() {
    large_download(Wire::One).await;
}

#[tokio::test]
async fn c5_4_a_large_download_streams_whole_and_a_whole_read_holds_its_bound_over_http2() {
    large_download(Wire::Two).await;
}

// A multipart upload --------------------------------------------------------------------

async fn multipart_upload(wire: Wire) {
    async fn fields(mut form: Multipart) -> String {
        let mut seen = Vec::new();
        while let Some(field) = form.next_field().await.unwrap() {
            let name = field.name().unwrap_or_default().to_owned();
            let file = field.file_name().map(str::to_owned);
            let kind = field.content_type().map(str::to_owned);
            let bytes = field.bytes().await.unwrap();
            seen.push(format!("{name} {file:?} {kind:?} {}", bytes.len()));
        }
        seen.join("; ")
    }
    let server = Server::start(Router::new().route("/upload", any(fields))).await;
    let photo = Bytes::from((0..=255u8).cycle().take(10_000).collect::<Vec<_>>());
    let answer = Store::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .upload("a lamp".to_owned(), photo)
        .await
        .unwrap();
    assert_eq!(
        answer,
        "title None None 6; photo Some(\"lamp.bin\") Some(\"application/octet-stream\") 10000"
    );
    let content_type = server.seen.all()[0].headers["content-type"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        content_type.starts_with("multipart/form-data; boundary="),
        "{content_type}"
    );
}

#[tokio::test]
async fn c2_16_a_multipart_upload_reaches_a_multipart_parser_field_by_field_over_http1() {
    multipart_upload(Wire::One).await;
}

#[tokio::test]
async fn c2_16_a_multipart_upload_reaches_a_multipart_parser_field_by_field_over_http2() {
    multipart_upload(Wire::Two).await;
}

// A repeatable body sent again ----------------------------------------------------------

/// `/repeat` keeps each body that it receives, and answers `503`, then `200` with `kept`.
fn repeat() -> (Router, Arc<Mutex<Vec<Bytes>>>) {
    let bodies: Arc<Mutex<Vec<Bytes>>> = Arc::default();
    let router = Router::new()
        .route(
            "/repeat",
            any(
                |State(bodies): State<Arc<Mutex<Vec<Bytes>>>>, body: Bytes| async move {
                    let mut bodies = bodies.lock().unwrap();
                    bodies.push(body);
                    if bodies.len() == 1 {
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        "kept".into_response()
                    }
                },
            ),
        )
        .with_state(bodies.clone());
    (router, bodies)
}

async fn repeatable_again(wire: Wire) {
    let _clock = Frozen::hold();
    let (router, bodies) = repeat();
    let server = Server::start(router).await;
    let probe = Probe::default();
    let store = Store::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .with_middleware(probe.clone())
        .with_retry(store::Retry::standard());

    let value = on_paused_clock(
        &probe,
        Duration::from_millis(50),
        Duration::from_secs(10),
        store.replace().into_future(),
    )
    .await;
    assert_eq!(value.unwrap(), "kept");
    let sent: Vec<Bytes> = std::mem::take(&mut *bodies.lock().unwrap());
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].as_ref(), UPLOAD);
    assert_eq!(sent[1].as_ref(), UPLOAD);

    // A multipart body with a repeatable part is repeatable: its random boundary is chosen
    // once, so the server sees the same bytes twice (C2.16, C12.12).
    let value = on_paused_clock(
        &probe,
        Duration::from_millis(50),
        Duration::from_secs(10),
        store.upload_again("pieces".to_owned()).into_future(),
    )
    .await;
    assert_eq!(value.unwrap(), "kept");
    let sent: Vec<Bytes> = std::mem::take(&mut *bodies.lock().unwrap());
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0], sent[1]);
    assert!(sent[0].windows(UPLOAD.len()).any(|window| window == UPLOAD));
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_repeatable_body_is_sent_again_after_a_503_byte_for_byte_over_http1() {
    repeatable_again(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_repeatable_body_is_sent_again_after_a_503_byte_for_byte_over_http2() {
    repeatable_again(Wire::Two).await;
}
