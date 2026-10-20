#![cfg(feature = "sse")]

use std::future::{Future, pending, poll_fn};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use concord::http::SafeProxy;
use concord::observation::{OperationEvent, OperationObserver, StreamOutcome};
use concord::records::{EventSource, EventStream, ServerEvent};
use concord::{ApiError, ItemStream, api};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};

api! {
    client LifecycleFeed {
        base "http://events.example.test/";
        GET live() -> ItemStream<ServerEvent> via EventStream;
    }
}

fn client(server: &ScriptedServer) -> LifecycleFeed {
    let proxy = SafeProxy::parse(server.url("")).unwrap();
    LifecycleFeed::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .unwrap()
}

fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

// Keep the executor runnable while real loopback I/O progresses. A paused
// Tokio clock must never auto-advance to a request or stream deadline here.
async fn drive_without_advancing_time<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    poll_fn(|context| match future.as_mut().poll(context) {
        Poll::Ready(value) => Poll::Ready(value),
        Poll::Pending => {
            context.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await
}

#[tokio::test(start_paused = true)]
async fn reconnect_opens_at_the_configured_deadline_and_never_before_it() {
    let server = ScriptedServer::start([
        ResponseScript::ok(Vec::new()).with_header("Content-Type", "text/event-stream"),
        ResponseScript::status(204, Vec::new()),
    ])
    .unwrap();
    let feed = client(&server);
    // Start with an already exhausted managed stream so registering the
    // reconnect timer needs no race with the server's EOF notification.
    let mut exhausted = drive_without_advancing_time(async { feed.live().await })
        .await
        .unwrap();
    assert!(
        drive_without_advancing_time(exhausted.next())
            .await
            .is_none()
    );
    let first = std::sync::Mutex::new(Some(exhausted));
    let opens = Arc::new(AtomicUsize::new(0));
    let connection_opens = Arc::clone(&opens);
    let mut source = EventSource::new(move |_| {
        connection_opens.fetch_add(1, Ordering::SeqCst);
        let first = first.lock().unwrap().take();
        let feed = feed.clone();
        async move {
            match first {
                Some(stream) => Ok(stream),
                None => feed.live().await,
            }
        }
    })
    .with_reconnect_time(Duration::from_secs(7));
    let mut next = Box::pin(source.next());
    assert!(poll_once(next.as_mut()).is_pending());
    assert_eq!(opens.load(Ordering::SeqCst), 1);

    tokio::time::advance(Duration::from_millis(6_999)).await;
    assert!(poll_once(next.as_mut()).is_pending());
    assert_eq!(opens.load(Ordering::SeqCst), 1);
    assert_eq!(server.request_count(), 1);

    tokio::time::advance(Duration::from_millis(1)).await;
    assert!(drive_without_advancing_time(next).await.is_none());
    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(source.connections(), 2);
    assert_eq!(server.request_count(), 2);
}

struct CancelProbe(Arc<AtomicUsize>);

impl Drop for CancelProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn dropping_pending_next_drops_the_in_progress_connector_immediately() {
    let cancelled = Arc::new(AtomicUsize::new(0));
    let connector_cancelled = Arc::clone(&cancelled);
    let mut source = EventSource::new(move |_| {
        let probe = CancelProbe(Arc::clone(&connector_cancelled));
        async move {
            let _probe = probe;
            pending::<Result<ItemStream<ServerEvent>, ApiError>>().await
        }
    });
    let mut next = Box::pin(source.next());
    assert!(poll_once(next.as_mut()).is_pending());
    assert_eq!(cancelled.load(Ordering::SeqCst), 0);
    drop(next);
    assert_eq!(cancelled.load(Ordering::SeqCst), 1);
    drop(source);
    assert_eq!(cancelled.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failures_before_response_exhaust_the_exact_reconnect_allowance() {
    for max_reconnects in [0, 1, 3] {
        let server = ScriptedServer::start(
            (0..(max_reconnects + 1) * 16).map(|_| ResponseScript::CloseWithoutResponse),
        )
        .unwrap();
        let feed = client(&server);
        let opens = Arc::new(AtomicUsize::new(0));
        let connection_opens = Arc::clone(&opens);
        let mut source = EventSource::new(move |_| {
            connection_opens.fetch_add(1, Ordering::SeqCst);
            let feed = feed.clone();
            async move { feed.live().await }
        })
        .with_reconnect_time(Duration::ZERO)
        .with_max_reconnects(max_reconnects);
        let error = source.next().await.unwrap().unwrap_err();
        assert!(matches!(
            error.category(),
            concord::error::ErrorCategory::Connect | concord::error::ErrorCategory::Transport
        ));
        let expected = max_reconnects + 1;
        assert_eq!(opens.load(Ordering::SeqCst), expected as usize);
        assert_eq!(source.connections(), expected);
        assert!(server.request_count() >= expected as usize);
        assert!(source.next().await.is_none());
        assert_eq!(opens.load(Ordering::SeqCst), expected as usize);
    }
}

struct StreamDrops(Arc<AtomicUsize>);

impl OperationObserver for StreamDrops {
    fn observe(&self, event: OperationEvent<'_>) {
        if matches!(event, OperationEvent::StreamFinished(finished) if finished.outcome == StreamOutcome::Dropped)
        {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[tokio::test]
async fn dropping_pending_read_closes_the_open_stream_and_keeps_resume_state() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedChunkedWithHeaders {
            status: 200,
            headers: vec![("Content-Type".into(), "text/event-stream".into())],
            first_chunk: b"id: saved\nretry: 0\n\ndata: first\n\n".to_vec(),
            remaining_chunk: b"data: should-not-be-read\n\n".to_vec(),
            gate: gate.clone(),
        },
        ResponseScript::ok(b"data: resumed\n\n".to_vec())
            .with_header("Content-Type", "text/event-stream"),
    ])
    .unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let observer = Arc::new(StreamDrops(Arc::clone(&drops)));
    let proxy = SafeProxy::parse(server.url("")).unwrap();
    let feed = LifecycleFeed::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.operation_observer(observer))
        .build()
        .unwrap();
    let mut source = EventSource::new(move |_| {
        let feed = feed.clone();
        async move { feed.live().await }
    });
    assert_eq!(source.next().await.unwrap().unwrap().data(), "first");
    let mut next = Box::pin(source.next());
    assert!(poll_once(next.as_mut()).is_pending());
    drop(next);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(source.last_event_id(), "saved");
    assert_eq!(source.reconnect_time(), Duration::ZERO);
    gate.release();
    let resumed = source.next().await.unwrap().unwrap();
    assert_eq!(
        (resumed.data(), resumed.last_event_id()),
        ("resumed", "saved")
    );
    assert_eq!(resumed.retry(), Some(Duration::ZERO));
    assert_eq!(source.connections(), 2);
    drop(source);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
}
