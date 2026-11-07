use super::*;
use concord_testserver::{Reply, TestServer};
use std::{cell::Cell, convert::Infallible, future::ready, num::NonZeroUsize, rc::Rc, sync::Arc};

concord::api! {
    client Api(header("idempotency-key") key: String) {
        base "http://unused.invalid";
        POST send(body bytes: reqwest::Body) { request Native; path "items"; }
    }
}
struct Native;
impl concord_core::Encode<reqwest::Body> for Native {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(body: reqwest::Body) -> Result<reqwest::Body, concord_core::CodecError> {
        Ok(body)
    }
}
fn prepared(server: &TestServer, body: reqwest::Body) -> Prepared<()> {
    let http = reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .unwrap();
    Api::new(
        http,
        ApiConfig {
            key: "stable".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
    .send(body)
    .prepare()
    .unwrap()
}
fn policy() -> Policy {
    Policy {
        initial_delay: Duration::from_secs(1),
        max_delay: Duration::from_secs(1),
        ..Policy::new(NonZeroUsize::new(3).unwrap())
    }
}
use crate::Policy;

#[derive(Debug)]
struct Refusal(&'static str); // Deliberately no Error, Display or Clone implementation.
struct Lease(Rc<Cell<usize>>);
impl Lease {
    fn new(live: &Rc<Cell<usize>>) -> Self {
        live.set(live.get() + 1);
        Self(live.clone())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

#[tokio::test]
async fn async_hook_keeps_identity_and_context_but_releases_lease_before_backoff() {
    let server = TestServer::start(|_| Reply::new(200)).await;
    let clock = Cell::new(Duration::ZERO);
    let live = Rc::new(Cell::new(0));
    let operation = prepared(&server, "payload".into()).operation();
    let result = Retry::new(policy(), Replay::Permitted)
        .receive_with(
            prepared(&server, "payload".into()),
            ReplayMode::Required,
            |mut request, n| {
                let live = &live;
                let clock = &clock;
                async move {
                    tokio::task::yield_now().await;
                    assert_eq!(clock.get(), Duration::from_secs((n - 1) as u64));
                    assert_eq!(live.get(), 0);
                    assert!(!request.headers().contains_key("authorization"));
                    request
                        .headers_mut()
                        .insert("authorization", format!("token-{n}").parse().unwrap());
                    Ok::<_, Refusal>((request, (n, Lease::new(live))))
                }
            },
            |outcome, (n, _)| {
                assert_eq!(live.get(), 1);
                assert_eq!(outcome.as_ref().unwrap().operation(), operation);
                if *n == 1 {
                    Decision::RetryAfter(None)
                } else {
                    Decision::Done
                }
            },
            || clock.get(),
            |delay| {
                assert_eq!(
                    live.get(),
                    0,
                    "lease must be dropped before constructing sleep"
                );
                clock.set(clock.get() + delay);
                ready(())
            },
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(result.report.retry.attempts, 2);
    assert_eq!(result.report.completed, 2);
    assert_eq!(result.report.refused, 0);
    assert!(!result.report.no_response);
    assert_eq!(result.report.retry.waits, [Duration::from_secs(1)]);
    assert_eq!(result.report.retry.total_waited, Duration::from_secs(1));
    assert_eq!(result.report.retry.stop, StopReason::Done);
    result.outcome.unwrap().unwrap().decode().await.unwrap();
    assert_eq!(live.get(), 1, "final lease survives decoding");
    drop(result.context);
    assert_eq!(live.get(), 0);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.body_text(), "payload");
        assert_eq!(request.header("idempotency-key"), Some("stable"));
    }
    assert_eq!(requests[0].header("authorization"), Some("token-1"));
    assert_eq!(requests[1].header("authorization"), Some("token-2"));
    server.shutdown().await;
}

#[tokio::test]
async fn refusal_is_separate_from_the_last_head_or_transport_error() {
    for (first_refusal, headless) in [(true, false), (false, false), (false, true)] {
        let server = TestServer::start(move |_| {
            if headless {
                Reply::hang_up()
            } else {
                Reply::new(503)
            }
        })
        .await;
        let clock = Cell::new(Duration::ZERO);
        let classifications = Cell::new(0);
        let result = Retry::new(policy(), Replay::Permitted)
            .receive_with(
                prepared(&server, "payload".into()),
                ReplayMode::Required,
                |request, n| async move {
                    tokio::task::yield_now().await;
                    if first_refusal || n == 2 {
                        Err(Refusal("secret-refusal"))
                    } else {
                        Ok((request, "secret-context"))
                    }
                },
                |_, _| {
                    classifications.set(classifications.get() + 1);
                    Decision::RetryAfter(None)
                },
                || clock.get(),
                |delay| {
                    clock.set(clock.get() + delay);
                    ready(())
                },
                |_, ceiling| ceiling,
            )
            .await;
        let sent = usize::from(!first_refusal);
        assert_eq!(result.report.retry.attempts, sent);
        assert_eq!(result.report.completed, sent);
        assert_eq!(classifications.get(), sent);
        assert_eq!(result.report.refused, 1);
        assert_eq!(result.report.no_response, !first_refusal && headless);
        assert_eq!(result.report.retry.stop, StopReason::ClassifierStop);
        assert!(result.context.is_none());
        assert_eq!(result.refusal.as_ref().unwrap().0, "secret-refusal");
        assert!(!format!("{result:?}").contains("secret"));
        match result.outcome {
            None => assert!(first_refusal),
            Some(Err(_)) => assert!(headless),
            Some(Ok(head)) => assert_eq!(head.status().as_u16(), 503),
        }
        assert_eq!(server.requests().len(), sent);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn no_response_survives_a_later_successful_head() {
    let count = std::sync::atomic::AtomicUsize::new(0);
    let server = TestServer::start(move |_| {
        if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Reply::hang_up()
        } else {
            Reply::new(422)
        }
    })
    .await;
    let result = Retry::new(
        Policy::new(NonZeroUsize::new(2).unwrap()),
        Replay::Permitted,
    )
    .receive_with(
        prepared(&server, "payload".into()),
        ReplayMode::Required,
        |request, _| ready(Ok::<_, Infallible>((request, ()))),
        |outcome, _| {
            if outcome.is_err() {
                Decision::RetryAfter(None)
            } else {
                Decision::Done
            }
        },
        || Duration::ZERO,
        |_| ready(()),
        |_, ceiling| ceiling,
    )
    .await;
    assert_eq!(result.report.retry.attempts, 2);
    assert_eq!(result.report.completed, 2);
    assert!(result.report.no_response);
    assert_eq!(result.outcome.unwrap().unwrap().status().as_u16(), 422);
    server.shutdown().await;
}

#[tokio::test]
async fn deadline_distinguishes_pending_hook_from_pending_dispatch_and_releases_state() {
    for pending_hook in [true, false] {
        let server = TestServer::start(|_| Reply::new(200)).await;
        let clock = Cell::new(Duration::ZERO);
        let live = Rc::new(Cell::new(0));
        let mut bounds = policy();
        bounds.timeout = Some(Duration::from_secs(5));
        let result = Retry::new(bounds, Replay::Permitted)
            .receive_with(
                prepared(&server, "payload".into()),
                ReplayMode::Required,
                |request, _| {
                    let live = &live;
                    async move {
                        let lease = Lease::new(live);
                        if pending_hook {
                            std::future::pending::<()>().await;
                        }
                        Ok::<_, Refusal>((request, lease))
                    }
                },
                |_, _| panic!("deadline before a response head"),
                || clock.get(),
                |delay| {
                    clock.set(clock.get() + delay);
                    ready(())
                },
                |_, _| panic!("no backoff"),
            )
            .await;
        assert_eq!(result.report.retry.stop, StopReason::Deadline);
        assert_eq!(result.report.retry.attempts, usize::from(!pending_hook));
        assert_eq!(result.report.completed, 0);
        assert_eq!(result.report.refused, 0);
        assert_eq!(result.report.no_response, !pending_hook);
        assert!(result.outcome.is_none());
        assert!(result.context.is_none());
        assert!(result.refusal.is_none());
        assert_eq!(live.get(), 0);
        assert!(result.report.retry.waits.is_empty());
        server.shutdown().await;
    }
}

#[tokio::test]
async fn exact_expiry_after_admission_prevents_dispatch_even_when_hook_is_ready() {
    for refuse in [false, true] {
        let server = TestServer::start(|_| Reply::new(200)).await;
        let clock = Cell::new(Duration::ZERO);
        let mut bounds = policy();
        bounds.timeout = Some(Duration::from_secs(1));
        let result = Retry::new(bounds, Replay::Permitted)
            .receive_with(
                prepared(&server, "payload".into()),
                ReplayMode::Required,
                |request, _| {
                    clock.set(Duration::from_secs(1));
                    ready(if refuse {
                        Err(Refusal("expired"))
                    } else {
                        Ok((request, ()))
                    })
                },
                |_, _| panic!("not dispatched"),
                || clock.get(),
                |_| async { panic!("no timer for ready hook") },
                |_, ceiling| ceiling,
            )
            .await;
        assert_eq!(result.report.retry.stop, StopReason::Deadline);
        assert_eq!(result.report.retry.attempts, 0);
        assert_eq!(result.report.refused, usize::from(refuse));
        assert!(!result.report.no_response);
        assert_eq!(result.refusal.is_some(), refuse);
        assert!(server.requests().is_empty());
        server.shutdown().await;
    }
}

#[tokio::test]
async fn replay_preflight_and_single_send_modes_do_not_poll_streams_to_clone() {
    for (mode, replay, attempts, preflight) in [
        (ReplayMode::Required, Replay::Permitted, 2, true),
        (ReplayMode::BestEffort, Replay::Permitted, 2, false),
        (ReplayMode::Required, Replay::Never, 2, false),
        (ReplayMode::Required, Replay::Permitted, 1, false),
    ] {
        let server = TestServer::start(|_| Reply::new(503)).await;
        let polled = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = polled.clone();
        let body = reqwest::Body::wrap_stream(futures_util::stream::once(async move {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok::<_, Infallible>("payload")
        }));
        let hooks = Cell::new(0);
        let result = Retry::new(Policy::new(NonZeroUsize::new(attempts).unwrap()), replay)
            .receive_with(
                prepared(&server, body),
                mode,
                |request, _| {
                    hooks.set(hooks.get() + 1);
                    ready(Ok::<_, Infallible>((request, ())))
                },
                |_, _| Decision::RetryAfter(None),
                || Duration::ZERO,
                |_| async { panic!("no wait") },
                |_, _| panic!("no jitter"),
            )
            .await;
        assert_eq!(result.report.retry.stop, StopReason::NotReplayable);
        assert!(!result.report.retry.replay_available);
        assert_eq!(result.report.retry.attempts, usize::from(!preflight));
        assert_eq!(hooks.get(), usize::from(!preflight));
        assert_eq!(
            polled.load(std::sync::atomic::Ordering::SeqCst),
            usize::from(!preflight)
        );
        assert_eq!(server.requests().len(), usize::from(!preflight));
        server.shutdown().await;
    }
}

#[tokio::test]
async fn a_later_hook_or_dispatch_deadline_retains_the_previous_outcome_without_its_lease() {
    for pending_hook in [true, false] {
        let server = TestServer::start(|_| Reply::new(503)).await;
        let clock = Cell::new(Duration::ZERO);
        let classified = Cell::new(0);
        let live = Rc::new(Cell::new(0));
        let mut bounds = Policy::new(NonZeroUsize::new(3).unwrap());
        bounds.timeout = Some(Duration::from_secs(5));
        let result = Retry::new(bounds, Replay::Permitted)
            .receive_with(
                prepared(&server, "payload".into()),
                ReplayMode::Required,
                |request, n| {
                    let live = &live;
                    async move {
                        assert_eq!(live.get(), 0);
                        let lease = Lease::new(live);
                        if pending_hook && n == 2 {
                            std::future::pending::<()>().await;
                        }
                        Ok::<_, Refusal>((request, lease))
                    }
                },
                |_, _| {
                    classified.set(classified.get() + 1);
                    Decision::RetryAfter(None)
                },
                || clock.get(),
                |delay| {
                    let clock = &clock;
                    let classified = &classified;
                    async move {
                        if classified.get() == 0 {
                            std::future::pending::<()>().await;
                        }
                        clock.set(clock.get() + delay);
                    }
                },
                |_, ceiling| ceiling,
            )
            .await;
        assert_eq!(result.report.retry.stop, StopReason::Deadline);
        assert_eq!(
            result.report.retry.attempts,
            if pending_hook { 1 } else { 2 }
        );
        assert_eq!(result.report.completed, 1);
        assert_eq!(result.report.refused, 0);
        assert_eq!(result.report.no_response, !pending_hook);
        assert_eq!(result.outcome.unwrap().unwrap().status().as_u16(), 503);
        assert!(result.context.is_none());
        assert_eq!(live.get(), 0);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn body_failure_after_a_head_is_outside_retry_and_headless_accounting() {
    let server = TestServer::start(|_| {
        Reply::new(200).with_body(concord_testserver::Body::FailAfter(vec!["partial".into()]))
    })
    .await;
    let result = Retry::new(policy(), Replay::Permitted)
        .receive_with(
            prepared(&server, "payload".into()),
            ReplayMode::Required,
            |request, _| ready(Ok::<_, Infallible>((request, "secret-context"))),
            |_, _| Decision::Done,
            || Duration::ZERO,
            |_| async { panic!("no wait") },
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(result.report.retry.attempts, 1);
    assert!(!result.report.no_response);
    assert!(!format!("{result:?}").contains("secret-context"));
    assert!(result.outcome.unwrap().unwrap().decode().await.is_err());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn bounds_keep_final_context_and_never_shorten_a_server_minimum() {
    for (decision, replay, attempts, timeout, stop) in [
        (Decision::Done, Replay::Never, 1, None, StopReason::Done),
        (
            Decision::Stop,
            Replay::Never,
            1,
            None,
            StopReason::ClassifierStop,
        ),
        (
            Decision::RetryAfter(None),
            Replay::Never,
            1,
            None,
            StopReason::NotReplayable,
        ),
        (
            Decision::RetryAfter(None),
            Replay::Permitted,
            1,
            None,
            StopReason::Attempts,
        ),
        (
            Decision::RetryAfter(None),
            Replay::Permitted,
            2,
            Some(Duration::from_secs(1)),
            StopReason::Deadline,
        ),
        (
            Decision::RetryAfter(Some(Duration::from_secs(5))),
            Replay::Permitted,
            2,
            Some(Duration::from_secs(5)),
            StopReason::Deadline,
        ),
    ] {
        let server = TestServer::start(|_| Reply::new(503)).await;
        let live = Rc::new(Cell::new(0));
        let bounds = Policy {
            max_attempts: NonZeroUsize::new(attempts).unwrap(),
            timeout,
            ..policy()
        };
        let result = Retry::new(bounds, replay)
            .receive_with(
                prepared(&server, "payload".into()),
                ReplayMode::Required,
                |request, _| ready(Ok::<_, Infallible>((request, Lease::new(&live)))),
                |_, _| decision,
                || Duration::ZERO,
                |_| std::future::pending(),
                |_, _| Duration::MAX,
            )
            .await;
        assert_eq!(result.report.retry.stop, stop);
        assert_eq!(result.report.retry.attempts, 1);
        assert_eq!(result.report.completed, 1);
        assert_eq!(result.report.refused, 0);
        assert!(!result.report.no_response);
        assert!(result.report.retry.waits.is_empty());
        assert_eq!(live.get(), 1);
        drop(result.context);
        assert_eq!(live.get(), 0);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
