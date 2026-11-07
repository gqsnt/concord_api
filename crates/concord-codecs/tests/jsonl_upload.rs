//! Lazily produced JSON Lines uploads against a loopback receiver: zero source polls while the
//! request is prepared, one pass while it is sent, and a late source failure that fails the
//! transfer after the receiver got a prefix. Each test owns its receiver.
#![cfg(feature = "jsonl")]

mod support;

use std::error::Error as StdError;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use concord_codecs::jsonl::{JsonLines, JsonLinesOptions, JsonLinesWriter, StreamRecords};
use concord_codecs::stream::UploadError;
use concord_core::Encode;
use concord_core::reqwest::header::CONTENT_TYPE;
use futures_core::Stream;
use support::{Peer, client, read_chunked, read_head};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

const SECRET: &str = "SECRET-7f3a91";

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Rec {
    id: u32,
    name: String,
}

fn rec(id: u32) -> Rec {
    Rec {
        id,
        name: format!("item-{id}"),
    }
}

/// A one-shot source that counts its polls and refuses to be polled after its end.
struct Counted {
    next: u32,
    total: u32,
    ended: bool,
    polls: Arc<AtomicUsize>,
}

impl Stream for Counted {
    type Item = Result<Rec, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        assert!(!self.ended, "polled after its end (a replay)");
        if self.next == self.total {
            self.ended = true;
            return Poll::Ready(None);
        }
        self.next += 1;
        Poll::Ready(Some(Ok(rec(self.next))))
    }
}

/// A receiver that reads the request head and a chunked body, then answers 200 when the body
/// is complete.
async fn receiver() -> Peer<(String, Vec<u8>, bool)> {
    Peer::start(|mut s| async move {
        let (head, rest) = read_head(&mut s).await;
        let (body, complete) = read_chunked(&mut s, rest, |_| {}).await;
        if complete {
            s.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                .await
                .unwrap();
        }
        (head.to_ascii_lowercase(), body, complete)
    })
    .await
}

/// Preparing the upload (the writer's `Upload`, or `Encode<StreamRecords<S>>` as the `api!`
/// macro would call it) polls the source zero times; sending polls it once per record plus
/// once for its end, in one pass, and the receiver gets exactly the finite encoding under the
/// matching `Content-Type`.
#[tokio::test]
async fn upload_polls_zero_times_at_preparation_and_once_at_send() {
    const N: u32 = 3000; // about 90 KB: several 16 KiB frames
    let expected = JsonLinesWriter::new(JsonLinesOptions::json_lines())
        .to_vec((1..=N).map(rec))
        .unwrap();

    // The runtime writer: body and Content-Type together, applied to a native request.
    let polls = Arc::new(AtomicUsize::new(0));
    let source = Counted {
        next: 0,
        total: N,
        ended: false,
        polls: Arc::clone(&polls),
    };
    let upload =
        JsonLinesWriter::new(JsonLinesOptions::json_lines()).upload(StreamRecords::new(source));
    assert_eq!(upload.content_type(), "application/jsonl");
    assert_eq!(polls.load(Ordering::SeqCst), 0, "no poll while preparing");
    let peer = receiver().await;
    let client = client();
    let mut request = client.post(peer.url()).build().unwrap();
    upload.apply(&mut request);
    assert_eq!(polls.load(Ordering::SeqCst), 0, "no poll before sending");
    let response = client.execute(request).await.unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(polls.load(Ordering::SeqCst), N as usize + 1, "one pass");
    let (head, body, complete) = peer.join().await;
    assert!(complete);
    assert!(head.contains("content-type: application/jsonl"), "{head}");
    assert!(head.contains("transfer-encoding: chunked"), "{head}");
    assert_eq!(body, expected);

    // The static route (`Encode<StreamRecords<S>>`), with its associated Content-Type.
    let polls = Arc::new(AtomicUsize::new(0));
    let source = Counted {
        next: 0,
        total: N,
        ended: false,
        polls: Arc::clone(&polls),
    };
    let body =
        <JsonLines as Encode<StreamRecords<Counted>>>::encode(StreamRecords::new(source)).unwrap();
    assert_eq!(polls.load(Ordering::SeqCst), 0, "no poll while encoding");
    let peer = receiver().await;
    let content_type = <JsonLines as Encode<StreamRecords<Counted>>>::CONTENT_TYPE.unwrap();
    let response = client
        .post(peer.url())
        .header(CONTENT_TYPE, content_type)
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(polls.load(Ordering::SeqCst), N as usize + 1, "one pass");
    drop(client);
    let (_, body, complete) = peer.join().await;
    assert!(complete);
    assert_eq!(body, expected);
}

/// A synthetic upstream failure whose message holds a secret.
#[derive(Debug)]
struct Upstream;

impl fmt::Display for Upstream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "upstream refused token={SECRET}")
    }
}

impl StdError for Upstream {}

/// Yields `k` records, then waits for `gate` (opened by the receiver once it holds those `k`
/// records), then fails.
struct FailAfter {
    sent: u32,
    k: u32,
    gate: Option<oneshot::Receiver<()>>,
}

impl Stream for FailAfter {
    type Item = Result<Rec, Upstream>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.sent < self.k {
            self.sent += 1;
            return Poll::Ready(Some(Ok(rec(self.sent))));
        }
        match self.gate.as_mut() {
            Some(gate) => match Pin::new(gate).poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => {
                    self.gate = None;
                    Poll::Ready(Some(Err(Upstream)))
                }
            },
            None => panic!("polled after its failure (a replay)"),
        }
    }
}

/// A source failure after the receiver already got records fails the transfer: `send` returns a
/// reqwest error wrapping an `UploadError` (counting the records serialized), the receiver holds
/// exactly the complete records sent before the failure and never sees a complete body, and no
/// diagnostic prints the source's secret.
#[tokio::test]
async fn late_source_error_fails_the_transfer_after_a_received_prefix() {
    const K: u32 = 5;
    let (open, gate) = oneshot::channel::<()>();
    let peer = Peer::start(move |mut s| async move {
        let (_, rest) = read_head(&mut s).await;
        let mut open = Some(open);
        let (body, complete) = read_chunked(&mut s, rest, |body| {
            if body.iter().filter(|&&b| b == b'\n').count() >= K as usize
                && let Some(open) = open.take()
            {
                let _ = open.send(());
            }
        })
        .await;
        (body, complete)
    })
    .await;
    let source = FailAfter {
        sent: 0,
        k: K,
        gate: Some(gate),
    };
    let upload =
        JsonLinesWriter::new(JsonLinesOptions::ndjson()).upload(StreamRecords::new(source));
    let (body, content_type) = upload.into_parts();
    assert_eq!(content_type, "application/x-ndjson");
    let client = client();
    let error = client
        .post(peer.url())
        .header(CONTENT_TYPE, content_type)
        .body(body)
        .send()
        .await
        .expect_err("the transfer fails");
    assert!(error.is_request() || error.is_body(), "{error:?}");

    // The UploadError is in the chain, with the record count; the secret only unredacted.
    let mut upload_error = None;
    let mut shown = format!("{error} | {error:?}");
    let mut cause: Option<&(dyn StdError + 'static)> = error.source();
    while let Some(e) = cause {
        shown.push_str(&format!(" | {e} | {e:?}"));
        if let Some(u) = e.downcast_ref::<UploadError>() {
            upload_error = Some(u);
        }
        cause = e.source();
    }
    let upload_error = upload_error.expect("an UploadError in the chain");
    assert_eq!(upload_error.label(), "the upload source failed");
    assert_eq!(upload_error.records(), u64::from(K));
    assert!(upload_error.codec().is_none());
    assert!(
        upload_error
            .source_unredacted()
            .unwrap()
            .to_string()
            .contains(SECRET)
    );
    assert!(!shown.contains(SECRET), "{shown}");

    drop(client);
    let (received, complete) = peer.join().await;
    assert!(!complete, "the receiver never saw the end of the body");
    let expected = JsonLinesWriter::new(JsonLinesOptions::ndjson())
        .to_vec((1..=K).map(rec))
        .unwrap();
    assert_eq!(
        received, expected,
        "exactly the records sent before the failure"
    );
}

/// A record that cannot be serialized fails the transfer the same way, labelled as an encoding
/// failure with its record number. Here the failure happens in the body's first poll (both
/// records are ready at once and the frame holding record 1 is discarded with the error); hyper
/// may then not even have written the request head (observed with hyper 1.x: it writes the head
/// together with the first body frame), so the receiver accepts anything up to the close and
/// only requires that no complete body arrived.
#[tokio::test]
async fn unencodable_record_fails_the_transfer() {
    use std::collections::BTreeMap;
    use tokio::io::AsyncReadExt;
    let mut bad = BTreeMap::new();
    bad.insert((1, 2), 3);
    let records = vec![Ok::<_, std::io::Error>(BTreeMap::new()), Ok(bad)];
    let peer = Peer::start(|mut s| async move {
        let mut received = Vec::new();
        let _ = s.read_to_end(&mut received).await;
        received
    })
    .await;
    let upload = JsonLinesWriter::new(JsonLinesOptions::json_lines()).upload(StreamRecords::new(
        concord_codecs::stream::from_iter(records),
    ));
    let client = client();
    let mut request = client.post(peer.url()).build().unwrap();
    upload.apply(&mut request);
    let error = client
        .execute(request)
        .await
        .expect_err("the transfer fails");
    let mut cause: Option<&(dyn StdError + 'static)> = error.source();
    let mut found = None;
    while let Some(e) = cause {
        if let Some(u) = e.downcast_ref::<UploadError>() {
            found = Some((
                u.label(),
                u.records(),
                u.codec().map(|c| (c.label(), c.get("record"))),
            ));
        }
        cause = e.source();
    }
    assert_eq!(
        found,
        Some((
            "an upload record could not be encoded",
            1,
            Some(("a record could not be encoded as JSON", Some(2)))
        ))
    );
    drop(client);
    let received = peer.join().await;
    assert!(
        support::find(&received, b"\r\n0\r\n\r\n").is_none(),
        "no complete body: {:?}",
        String::from_utf8_lossy(&received)
    );
}
