//! These tests check retry budgets and body idle deadlines.

use concord::error::{Class, Kept, Kind};
use concord::transport::{TransportError, TryInfo};
use concord::{reqwest, Middleware, Next, Retry};
use concord_testkit::Script;
use futures_core::Stream;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tokio::time::Instant;

// advance moves the paused clock before yielding. One poll models time spent
// in synchronous author code without yielding the surrounding call.
fn elapsed_in_sync_code(seconds: u64) {
    let before = Instant::now();
    let mut cx = Context::from_waker(Waker::noop());
    assert!(
        std::pin::pin!(tokio::time::advance(Duration::from_secs(seconds)))
            .as_mut()
            .poll(&mut cx)
            .is_pending()
    );
    assert_eq!(before.elapsed(), Duration::from_secs(seconds));
}

#[tokio::test(start_paused = true)]
async fn c4_3_slow_retry_rules_keep_the_last_refusal_body_and_requested_wait() {
    for slow_unless in [false, true] {
        let script = Script::new();
        script
            .answer(503)
            .header("retry-after", "2")
            .body("evidence");
        script.answer(200).body("unused");
        let rules = Arc::new(AtomicUsize::new(0));
        let count = rules.clone();
        let start = Instant::now();
        let error = tour::Files::new("key")
            .with_transport(script.clone())
            .readme("r")
            .with_timeout(Duration::from_secs(5))
            .with_retry(
                Retry::standard()
                    .unless(move |_| {
                        if slow_unless {
                            elapsed_in_sync_code(4);
                        }
                        false
                    })
                    .when(move |_| {
                        count.fetch_add(1, Ordering::SeqCst);
                        if !slow_unless {
                            elapsed_in_sync_code(4);
                        }
                        Some(Duration::from_secs(2))
                    }),
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(
            error.status(),
            Some(concord::http::StatusCode::SERVICE_UNAVAILABLE)
        );
        assert_eq!(error.tries(), 1);
        assert_eq!(error.body(), Some(b"evidence".as_slice()));
        assert_eq!(error.kept(), Some(Kept::Complete));
        assert_eq!(error.retry_after(), Some(Duration::from_secs(2)));
        assert_eq!(start.elapsed(), Duration::from_secs(4));
        assert_eq!(rules.load(Ordering::SeqCst), 1);
        assert_eq!(script.requests().len(), 1);
        assert_eq!(script.unused(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c4_3_slow_retry_rules_keep_the_last_raw_head_without_reading_its_body() {
    for slow_unless in [false, true] {
        let script = Script::new();
        script.answer(503).header("retry-after", "2").stall();
        let start = Instant::now();
        let response = tour::Files::new("key")
            .with_transport(script.clone())
            .readme("r")
            .with_timeout(Duration::from_secs(5))
            .with_retry(
                Retry::standard()
                    .unless(move |_| {
                        if slow_unless {
                            elapsed_in_sync_code(4);
                        }
                        false
                    })
                    .when(move |_| {
                        if !slow_unless {
                            elapsed_in_sync_code(4);
                        }
                        Some(Duration::from_secs(2))
                    }),
            )
            .send_raw()
            .await
            .unwrap();
        assert_eq!(response.status(), 503);
        assert_eq!(response.headers()["retry-after"], "2");
        assert_eq!(start.elapsed(), Duration::from_secs(4));
        assert_eq!(script.requests().len(), 1);
    }
}

struct SlowFirstPoll<S> {
    source: Pin<Box<S>>,
    first: bool,
}

impl<S: Stream> Stream for SlowFirstPoll<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.first {
            self.first = false;
            elapsed_in_sync_code(4);
        }
        self.source.as_mut().poll_next(cx)
    }
}

struct SlowFirstBodyPoll;

impl Middleware for SlowFirstBodyPoll {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let response = next.send(request, info).await?;
        let body = SlowFirstPoll {
            source: Box::pin(response.bytes_stream()),
            first: true,
        };
        Ok(concord::reply(200, reqwest::Body::wrap_stream(body)))
    }
}

#[tokio::test(start_paused = true)]
async fn c7_6_body_idle_deadline_starts_before_the_first_poll_and_ready_work_wins() {
    for stalled in [true, false] {
        let script = Script::new();
        if stalled {
            script.answer(200).stall();
        } else {
            script.answer(200).body("ready");
        }
        let start = Instant::now();
        let result = tour::Files::new("key")
            .with_transport(script.clone())
            .with_middleware(SlowFirstBodyPoll)
            .readme("r")
            .with_idle_timeout(Duration::from_secs(3))
            .await;
        if stalled {
            let error = result.unwrap_err();
            assert_eq!(error.kind(), Kind::Transport);
            assert_eq!(error.class(), Some(Class::Timeout));
            assert_eq!(error.tries(), 1);
            // "After a head, while a body is read: `transport error: the body was silent for 30 s after status 200 (<hint>), in <endpoint>`."
            assert_eq!(error.to_string(), "transport error: the body was silent for 3 s after status 200 (with_idle_timeout changes it), in files.readme");
        } else {
            assert_eq!(result.unwrap(), "ready");
        }
        assert_eq!(start.elapsed(), Duration::from_secs(4));
        assert_eq!(script.requests().len(), 1);
    }
}
