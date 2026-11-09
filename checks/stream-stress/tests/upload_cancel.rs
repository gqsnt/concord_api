//! Transport coverage for cancelling an upload: a JSON Lines upload sent through reqwest and hyper
//! to a loopback receiver that keeps up, dropped mid-upload. The guard's cancellation boundary
//! is tested with explicit synchronization on the guard itself
//! (`concord-core`, `src/cancel.rs`). This test covers the real transport without forcing an
//! interleaving or claiming an exact cancellation boundary.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::jsonl::StreamRecords;
use concord::stream::from_iter;
use concord_check_stream_stress::{GenError, Rec, Workload, rec};
use concord_example_testserver::{Receive, Reply, TestServer};
use tokio::sync::oneshot;

/// The record at which the test drops the send: far past what hyper buffers before writing (16
/// frames of 128 records), so the upload is on the wire.
const DROP_AT: u64 = 10_000;
/// Hang guards, not timings: each wait ends as soon as its condition holds.
const HANG_GUARD: Duration = Duration::from_secs(10);

/// Signals when dropped: the upload source is dropped only when the transport drops the body.
struct OnDrop(Option<oneshot::Sender<()>>);

impl Drop for OnDrop {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_dropped_send_stops_the_upload_through_the_transport() {
    let receive = Receive {
        record_limit: 0,
        read_size: 64 * 1024,
        read_delay: None,
    };
    let server = TestServer::start_with(receive, |_| Reply::new(204)).await;
    let http = concord::reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let api = Workload::new(http).with_base_url(server.url()).unwrap();
    let polls = Arc::new(AtomicU64::new(0));
    let (reached_tx, reached) = oneshot::channel::<()>();
    let reached_tx = Mutex::new(Some(reached_tx));
    let (destroyed_tx, destroyed) = oneshot::channel::<()>();
    let on_drop = OnDrop(Some(destroyed_tx));
    let p = Arc::clone(&polls);
    let iter: Box<dyn Iterator<Item = Result<Rec, GenError>> + Send> =
        Box::new((0..10_000_000u64).map(move |i| {
            let _source_lives = &on_drop;
            p.fetch_add(1, Ordering::SeqCst);
            if i == DROP_AT
                && let Some(tx) = reached_tx.lock().unwrap().take()
            {
                let _ = tx.send(());
            }
            Ok(rec(i))
        }));
    let mut send = Box::pin(
        api.upload_jsonl(StreamRecords::new(from_iter(iter)))
            .prepare()
            .unwrap()
            .send(),
    );
    tokio::select! {
        result = &mut send => panic!("the send ended before the drop: {result:?}"),
        _ = reached => {}
    }
    drop(send);
    let at_drop = polls.load(Ordering::SeqCst);
    // The transport drops the body (and with it the source) once it has given up the request.
    tokio::time::timeout(HANG_GUARD, destroyed)
        .await
        .expect("the transport still held the upload body 10 s after the drop")
        .unwrap();
    assert_eq!(
        polls.load(Ordering::SeqCst),
        at_drop,
        "the source was polled after the send's drop returned"
    );
    // The receiver never got the end of the body: once it has accepted the connection, read from
    // it and seen it end, it has recorded no complete request.
    assert!(
        server
            .wait_until(HANG_GUARD, |s| s.request_body_bytes() > 0)
            .await
    );
    assert!(
        server
            .wait_until(HANG_GUARD, |s| s.live_connections() == 0)
            .await,
        "the connection was still open 10 s after the drop"
    );
    assert!(
        server.requests().is_empty(),
        "the receiver saw a complete body: a cancelled upload must not end cleanly"
    );
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}
