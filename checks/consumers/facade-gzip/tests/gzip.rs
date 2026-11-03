//! Compressed responses through reqwest's `gzip` support, against a loopback server that sends a
//! scripted head and can withhold the body. `Content-Encoding` says how the bytes are packed for
//! transfer; `Content-Type` says what they are once unpacked. Concord checks only the latter.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::{ErrorKind, reqwest};
use consumer_facade_gzip::{Catalog, Seen, Shop};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::timeout;

const PLAIN: &[u8] = include_bytes!("fixtures/catalog.json");
const PACKED: &[u8] = include_bytes!("fixtures/catalog.json.gz");
const LIMIT: Duration = Duration::from_secs(5);

/// A server for one request: it sends `head` (plus `content-length`), then the body once
/// `release` fires (or at once when `withhold` is false). It records the request head and counts
/// the body bytes it has written.
struct Fixture {
    url: String,
    release: Option<oneshot::Sender<()>>,
    written: Arc<AtomicUsize>,
    request: Arc<Mutex<String>>,
}

impl Fixture {
    async fn start(head: &'static str, body: &'static [u8], withhold: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (release, released) = oneshot::channel::<()>();
        let written = Arc::new(AtomicUsize::new(0));
        let request = Arc::new(Mutex::new(String::new()));
        let (counter, recorded) = (Arc::clone(&written), Arc::clone(&request));
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 1024];
            while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buf).await.unwrap();
                assert!(n > 0, "the client closed before sending a request");
                seen.extend_from_slice(&buf[..n]);
            }
            *recorded.lock().unwrap() = String::from_utf8_lossy(&seen).to_ascii_lowercase();
            let head = format!("{head}\r\ncontent-length: {}\r\n\r\n", body.len());
            stream.write_all(head.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            if withhold {
                let _ = released.await;
            }
            stream.write_all(body).await.unwrap();
            counter.fetch_add(body.len(), Ordering::SeqCst);
            stream.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        Self {
            url,
            release: withhold.then_some(release),
            written,
            request,
        }
    }

    fn shop(&self, client: reqwest::Client) -> Shop {
        Shop::new(client).with_base_url(self.url.clone()).unwrap()
    }

    fn request_has(&self, line: &str) -> bool {
        self.request.lock().unwrap().lines().any(|l| l.trim() == line)
    }
}

const GZIP_JSON: &str = "HTTP/1.1 200 OK\r\ncontent-type: application/json; charset=utf-8\r\n\
                         content-encoding: gzip\r\nvary: accept-encoding";

fn expected() -> Catalog {
    let plain = concord::bytes::Bytes::from_static(PLAIN);
    <concord::Json as concord::Decode<Catalog>>::decode_body((), plain).unwrap()
}

/// Decompression enabled (reqwest's default once its `gzip` feature is on): the request offers
/// gzip, reqwest unpacks the body, and `Json` decodes the unpacked bytes. The format sees the head
/// reqwest exposes: the `Content-Type` as sent, without the transfer packing (`Content-Encoding`)
/// and the packed length, which no longer describe the bytes the format receives.
#[tokio::test]
async fn gzip_json_is_decoded_from_the_unpacked_body() {
    let server = Fixture::start(GZIP_JSON, PACKED, false).await;
    let catalog = timeout(LIMIT, server.shop(reqwest::Client::new()).catalog().send())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(catalog, expected());
    assert!(server.request_has("accept-encoding: gzip"));

    let server = Fixture::start(GZIP_JSON, PACKED, false).await;
    let (catalog, seen) = timeout(LIMIT, server.shop(reqwest::Client::new()).observed().send())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(catalog, expected());
    assert_eq!(
        seen,
        Seen {
            content_type: Some("application/json; charset=utf-8".into()),
            content_encoding: None,
            content_length: None,
        }
    );

    // The fixture is correctly compressed: reqwest unpacks it to exactly the plain JSON.
    let server = Fixture::start(GZIP_JSON, PACKED, false).await;
    let raw = server.shop(reqwest::Client::new()).raw().send().await.unwrap();
    assert_eq!(raw.as_ref(), PLAIN);
}

/// An incompatible `Content-Type` on a compressed response is refused from the head, while the
/// server still withholds the body: nothing of it has been sent yet, so Concord cannot have read
/// it. The retained response then yields the whole body, unpacked by reqwest. The positive control
/// uses the same server with a compatible head: `send()` waits for the body.
#[tokio::test]
async fn an_incompatible_type_is_refused_before_the_body() {
    let mut server = Fixture::start(
        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-encoding: gzip",
        PACKED,
        true,
    )
    .await;
    let error = timeout(LIMIT, server.shop(reqwest::Client::new()).catalog().send())
        .await
        .expect("refused without waiting for the body")
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(server.written.load(Ordering::SeqCst), 0, "the body was not sent yet");
    let response = error.into_response().expect("the unread response is retained");
    assert_eq!(response.headers()["content-type"], "text/html");
    server.release.take().unwrap().send(()).unwrap();
    let body = timeout(LIMIT, response.bytes()).await.unwrap().unwrap();
    assert_eq!(body.as_ref(), PLAIN);

    let mut server = Fixture::start(GZIP_JSON, PACKED, true).await;
    let shop = server.shop(reqwest::Client::new());
    let pending = tokio::spawn(async move { shop.catalog().send().await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!pending.is_finished(), "a compatible head waits for the body");
    server.release.take().unwrap().send(()).unwrap();
    let catalog = timeout(LIMIT, pending).await.unwrap().unwrap().unwrap();
    assert_eq!(catalog, expected());
}

/// Control, decompression disabled on the client: the request offers no gzip, the packed bytes
/// reach the format, and the head still says `application/json`. `Json` therefore accepts the
/// metadata and fails to decode (a `Decode` error, not `Metadata`); `Binary` returns the packed
/// bytes unchanged.
#[tokio::test]
async fn decompression_disabled_control() {
    let client = || reqwest::Client::builder().no_gzip().build().unwrap();
    let server = Fixture::start(GZIP_JSON, PACKED, false).await;
    let error = timeout(LIMIT, server.shop(client()).catalog().send())
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(!server.request_has("accept-encoding: gzip"));

    let server = Fixture::start(GZIP_JSON, PACKED, false).await;
    let raw = server.shop(client()).raw().send().await.unwrap();
    assert_eq!(raw.as_ref(), PACKED);
}

/// `Content-Type: application/gzip` without `Content-Encoding` is a gzip file, not packed JSON:
/// reqwest leaves it alone even with decompression enabled, `Json` refuses it from the head, and
/// `Binary` returns the file as sent.
#[tokio::test]
async fn a_gzip_media_type_is_not_a_content_encoding() {
    let head = "HTTP/1.1 200 OK\r\ncontent-type: application/gzip";
    let server = Fixture::start(head, PACKED, false).await;
    let error = timeout(LIMIT, server.shop(reqwest::Client::new()).catalog().send())
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    let response = error.into_response().unwrap();
    assert_eq!(response.bytes().await.unwrap().as_ref(), PACKED);

    let server = Fixture::start(head, PACKED, false).await;
    let raw = server.shop(reqwest::Client::new()).raw().send().await.unwrap();
    assert_eq!(raw.as_ref(), PACKED);
}
