//! U28 (R08): raw streaming against a REAL chunked fixture, early stop and an application
//! deadline.

mod support;

use std::sync::atomic::Ordering;
use std::time::Duration;

use concord_example_native::streaming::{StreamError, first_event_chunks};
use support::{chunked, inventory_at};

#[tokio::test]
async fn early_stop_closes_the_connection_before_the_body_ends() {
    let (addr, observed) = chunked(2_000, Duration::from_millis(5), None).await;
    let api = inventory_at(addr, "Bearer t");
    let chunks = first_event_chunks(&api, 3, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(chunks.len(), 3);
    assert!(chunks.iter().all(|c| !c.is_empty()));
    assert!(
        observed.wait_for_close(Duration::from_secs(5)).await,
        "server should see the client go away"
    );
    assert!(!observed.finished.load(Ordering::SeqCst));
    let written = observed.chunks_written.load(Ordering::SeqCst);
    assert!(written < 2_000, "server wrote {written} chunks");
}

#[tokio::test]
async fn a_stalled_stream_hits_the_application_deadline() {
    let (addr, _observed) = chunked(10, Duration::from_millis(1), Some(2)).await;
    let api = inventory_at(addr, "Bearer t");
    let error = first_event_chunks(&api, 10, Duration::from_millis(300))
        .await
        .unwrap_err();
    let StreamError::Stalled { received } = error else {
        panic!("expected a stall, got {error:?}");
    };
    assert!((1..=2).contains(&received), "received {received}");
}

#[tokio::test]
async fn a_short_body_yields_fewer_chunks() {
    let (addr, _observed) = chunked(2, Duration::from_millis(1), None).await;
    let api = inventory_at(addr, "Bearer t");
    let chunks = first_event_chunks(&api, 10, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), 2 * 1024);
}

#[tokio::test]
async fn non_2xx_is_reported_without_reading_the_body() {
    let server = support::Server::reply(support::Reply::text(503, "busy")).await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = first_event_chunks(&api, 3, Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(
        matches!(error, StreamError::Status(s) if s.as_u16() == 503),
        "{error:?}"
    );
}
