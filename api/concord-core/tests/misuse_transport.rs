//! C6.2/A75: typed HTTP failures, source retention, and standard retry decisions.
#![allow(clippy::unwrap_used)]

use concord_core::__private::{Core, Draft, Idempotent, Options, Piece, Run};
use concord_core::error::{Class, Kind};
use concord_core::transport::TransportError;
use concord_core::{http, reqwest, Answer, Retry};
use concord_testkit::Script;
use std::error::Error as _;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

// A memory-backed HTTP peer: no socket, resolver, unsafe code, or timer.
struct Wire {
    answer: Option<Vec<u8>>,
    at: usize,
    written: bool,
}
impl hyper::rt::Read for Wire {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        mut buf: hyper::rt::ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let Some(answer) = self.answer.as_ref().filter(|_| self.written) else {
            return Poll::Pending;
        };
        let end = self.at.saturating_add(buf.remaining()).min(answer.len());
        buf.put_slice(answer.get(self.at..end).unwrap());
        self.at = end;
        Poll::Ready(Ok(()))
    }
}
impl hyper::rt::Write for Wire {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.written = true;
        cx.waker().wake_by_ref();
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

async fn hyper_failure(answer: Option<Vec<u8>>, body: reqwest::Body) -> hyper::Error {
    let wire = Wire {
        answer,
        at: 0,
        written: false,
    };
    let (mut sender, connection) = hyper::client::conn::http1::handshake(wire).await.unwrap();
    let driver = tokio::spawn(connection);
    let request = http::Request::builder()
        .method(http::Method::POST)
        .uri("/")
        .body(body)
        .unwrap();
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        sender.send_request(request),
    )
    .await
    .unwrap()
    .unwrap_err();
    driver.abort();
    let _ = driver.await;
    error
}

struct Resolver(Mutex<Option<hyper::Error>>);
impl reqwest::dns::Resolve for Resolver {
    fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let error = self.0.lock().unwrap().take().unwrap();
        Box::pin(async move { Err(Box::new(error) as Box<dyn std::error::Error + Send + Sync>) })
    }
}

// Reqwest's constructors are private. Its injected resolver safely puts the real
// Hyper error into a real Request error, without attempting any socket or DNS.
async fn reqwest_failure(error: hyper::Error) -> reqwest::Error {
    reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .dns_resolver(Arc::new(Resolver(Mutex::new(Some(error)))))
        .build()
        .unwrap()
        .get("http://fixture.invalid/PRIVATE_URL?key=PRIVATE_URL")
        .send()
        .await
        .unwrap_err()
}

fn malformed_heads() -> Vec<Vec<u8>> {
    vec![
        b"not HTTP at all\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\ncontent-length: 1\r\ncontent-length: 2\r\n\r\nxx".to_vec(),
        format!("HTTP/1.1 200 OK\r\n{}\r\n", "x-h: a\r\n".repeat(128)).into_bytes(),
        format!(
            "HTTP/1.1 200 OK\r\nx-big: {}\r\n\r\n",
            "a".repeat(1024 * 1024)
        )
        .into_bytes(),
    ]
}

async fn assert_failure(error: reqwest::Error, message: &str, stream: Option<io::ErrorKind>) {
    assert!(error.is_request());
    let failure = TransportError::from(error);
    assert_eq!(failure.class(), Class::Other);
    let script = Script::new();
    script.fail(failure);
    script.answer(200); // A replayable GET must still stop, even with a spare success.
    let core = Core::<()>::default()
        .with_transport(script.clone())
        .with_retry(Retry::standard());
    let options = Options::default();
    let run = Run::start(&core, &options, "misuse.get");
    let mut draft = Draft::new(http::Method::GET);
    draft.base(&[Piece::Lit("http://fixture.invalid/")]);
    let error = draft
        .prepare(
            run,
            core,
            options,
            Idempotent::ByMethod,
            |r, _| async move { Ok(Answer::new(r.status(), r.headers().clone().into(), ())) },
        )
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Other));
    assert_eq!(error.tries(), 1);
    assert_eq!(
        error.to_string(),
        format!("transport error: {message}, in misuse.get")
    );
    assert!(!format!("{error} {error:?} {error:#?}").contains("PRIVATE_"));
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.unused(), 1);
    let reqwest = error
        .source()
        .unwrap()
        .downcast_ref::<reqwest::Error>()
        .unwrap();
    assert_eq!(reqwest.url(), None);
    let mut cause = error.source();
    let mut typed = false;
    let mut original = stream.is_none();
    while let Some(error) = cause {
        if let Some(error) = error.downcast_ref::<hyper::Error>() {
            typed |= if stream.is_some() {
                error.is_user()
            } else {
                error.is_parse()
            };
        }
        if let Some(error) = error.downcast_ref::<io::Error>() {
            if Some(error.kind()) == stream {
                original = error.to_string().contains("PRIVATE_STREAM");
            }
        }
        cause = error.source();
    }
    assert!(typed, "retain the original typed HTTP failure");
    assert!(original, "retain the caller's original stream failure");
}

#[tokio::test(start_paused = true)]
async fn c6_2_c4_2_a75_parse_errors_are_other_and_never_retried_without_sockets() {
    for head in malformed_heads() {
        let error = hyper_failure(Some(head), reqwest::Body::from("")).await;
        assert!(error.is_parse());
        assert_failure(
            reqwest_failure(error).await,
            "the head of the answer could not be read",
            None,
        )
        .await;
    }
}

#[tokio::test(start_paused = true)]
async fn c6_2_c4_2_a75_user_body_errors_are_other_and_keep_the_source_without_sockets() {
    for kind in [
        io::ErrorKind::Other,
        io::ErrorKind::ConnectionReset,
        io::ErrorKind::TimedOut,
    ] {
        let body = reqwest::Body::wrap_stream(futures_util::stream::iter([
            Ok(bytes::Bytes::from_static(b"first chunk")),
            Err(io::Error::new(kind, "PRIVATE_STREAM")),
        ]));
        let error = hyper_failure(None, body).await;
        assert_failure(
            reqwest_failure(error).await,
            "the body of the request failed",
            Some(kind),
        )
        .await;
    }
}

// The architect runs this wire-level complement: the sandbox denies TCP bind.
#[tokio::test(start_paused = true)]
async fn c6_2_a75_real_reqwest_malformed_heads_and_request_body_failures() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut cases: Vec<_> = malformed_heads().into_iter().map(Some).collect();
    cases.push(None);
    for head in cases {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let upload = head.is_none();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut received = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let n = socket.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                received.extend_from_slice(buf.get(..n).unwrap());
                if !upload && received.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            if let Some(head) = head {
                let _ = socket.write_all(&head).await;
            }
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .build()
            .unwrap();
        let mut request = client.request(
            if upload {
                http::Method::POST
            } else {
                http::Method::GET
            },
            format!("http://{address}/PRIVATE_URL?key=PRIVATE_URL"),
        );
        if upload {
            request = request.body(reqwest::Body::wrap_stream(futures_util::stream::iter([
                Ok(bytes::Bytes::from_static(b"first chunk")),
                Err(io::Error::other("PRIVATE_STREAM")),
            ])));
        }
        let error = request.send().await.unwrap_err();
        server.await.unwrap();
        assert_failure(
            error,
            if upload {
                "the body of the request failed"
            } else {
                "the head of the answer could not be read"
            },
            upload.then_some(io::ErrorKind::Other),
        )
        .await;
    }
}
