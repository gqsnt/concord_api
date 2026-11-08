//! Cancelling an upload while the connection is polling its source, deterministically: the source
//! stalls inside a poll (on the connection's task) and the send is dropped during the stall. The
//! smoke test `jsonl_upload_cancellation_stops_polling_the_source` checks the same property at a
//! fixed time, where whether a poll is in progress at the drop depends on scheduling.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::jsonl::StreamRecords;
use concord::stream::from_iter;
use concord_check_stream_stress::{GenError, RECORD_BYTES, Rec, Workload, rec};
use concord_example_testserver::{Receive, Reply, TestServer};
use tokio::sync::oneshot;

/// Records in one body frame: the upload body hands the transport about 16 KiB per poll.
const FRAME_RECORDS: u64 = 16 * 1024 / RECORD_BYTES as u64;
/// The record whose poll stalls: inside a frame, so the frame being assembled still needs
/// records after the stall.
const STALL_AT: u64 = 1_000;
const STALL: Duration = Duration::from_millis(200);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_send_dropped_during_a_source_poll_never_polls_the_source_again() {
    // A receiver that keeps up (no delay): the transport is never held back by a full socket.
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
    let p = Arc::clone(&polls);
    let iter: Box<dyn Iterator<Item = Result<Rec, GenError>> + Send> =
        Box::new((0..10_000_000u64).map(move |i| {
            p.fetch_add(1, Ordering::SeqCst);
            if i == STALL_AT {
                if let Some(tx) = reached_tx.lock().unwrap().take() {
                    let _ = tx.send(());
                }
                // Blocks the connection's task inside this poll; the send is dropped meanwhile.
                std::thread::sleep(STALL);
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
        result = &mut send => panic!("the send ended before the stall: {result:?}"),
        _ = reached => {}
    }
    drop(send);
    let at_drop = polls.load(Ordering::SeqCst);
    // Quiescence: once the server has seen the connection end, the transport has dropped the
    // body, so no later poll is possible.
    let closed = server
        .wait_until(Duration::from_secs(10), |s| s.live_connections() == 0)
        .await;
    let later = polls.load(Ordering::SeqCst);
    assert_eq!(
        at_drop, later,
        "the source was polled after the send was dropped ({at_drop} polls at the drop)"
    );
    assert!(closed, "the connection was still open 10 s after the drop");
    // The drop waited for the poll in progress (the rest of its frame), and for nothing more.
    assert!(
        at_drop > STALL_AT && at_drop <= STALL_AT + FRAME_RECORDS,
        "{at_drop} polls at the drop: the upload went on while the drop waited"
    );
    assert!(
        server.requests().is_empty(),
        "the receiver saw a complete body: the cancelled upload must not end cleanly"
    );
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}
