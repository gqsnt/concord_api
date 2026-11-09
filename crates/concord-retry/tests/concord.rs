//! Loopback acceptance through generated endpoints.
#![cfg(feature = "concord")]
#![allow(missing_docs)]

use concord_core::{Error, Received};
use concord_retry::{
    Decision, Policy, Replay, Retry, StopReason, concord::ReplayMode, http::retry_after,
};
use concord_testserver::{Reply, TestServer};
use std::{
    cell::Cell,
    convert::Infallible,
    future::ready,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, UNIX_EPOCH},
};

struct NativeBody;
impl concord_core::Encode<reqwest::Body> for NativeBody {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(body: reqwest::Body) -> Result<reqwest::Body, concord_core::CodecError> {
        Ok(body)
    }
}

concord::api! {
    pub client Api(header("idempotency-key") key: String) {
        base "http://unused.invalid";
        GET get() { path "items"; }
        POST upload(body data: reqwest::Body) { request NativeBody; path "items"; }
        GET events() -> stream { path "events"; }
    }
}

fn api(server: &TestServer) -> Api {
    let client = reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .unwrap();
    Api::new(
        client,
        ApiConfig {
            key: "stable-key".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}
fn retry(attempts: usize) -> Retry {
    Retry::new(
        Policy::new(NonZeroUsize::new(attempts).unwrap()),
        Replay::Permitted,
    )
}
fn classify<T>(outcome: &Result<Received<T>, Error>) -> Decision {
    match outcome {
        Ok(r) if r.status().as_u16() == 503 || r.status().as_u16() == 429 => {
            let hint = retry_after(
                r.head()
                    .headers()
                    .get_all("retry-after")
                    .iter()
                    .map(|v| v.as_bytes()),
                UNIX_EPOCH,
            )
            .unwrap();
            Decision::RetryAfter(hint)
        }
        _ => Decision::Done,
    }
}

#[tokio::test]
async fn generated_endpoint_503_then_200_keeps_identity_key_and_fresh_hook_after_wait() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = TestServer::start(move |_| {
        if seen.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::new(503)
        } else {
            Reply::new(200)
        }
    })
    .await;
    let endpoint = api(&server).upload(reqwest::Body::from("buffered"));
    let prepared = endpoint.prepare().unwrap();
    let operation = prepared.operation().unwrap().name();
    let clock = Cell::new(Duration::ZERO);
    let mut p = Policy::new(NonZeroUsize::new(3).unwrap());
    p.initial_delay = Duration::from_secs(1);
    p.max_delay = Duration::from_secs(1);
    let result = Retry::new(p, Replay::Permitted)
        .receive(
            prepared,
            ReplayMode::Required,
            |request, n| {
                assert_eq!(clock.get(), Duration::from_secs((n - 1) as u64));
                assert_eq!(
                    request.headers().get("authorization"),
                    None,
                    "each hook gets a fresh clone"
                );
                request
                    .headers_mut()
                    .insert("authorization", format!("fresh-{n}").parse().unwrap());
            },
            classify,
            || clock.get(),
            |d| {
                clock.set(clock.get() + d);
                ready(())
            },
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(result.report.attempts, 2);
    assert_eq!(result.report.stop, StopReason::Done);
    assert_eq!(result.report.total_waited, Duration::from_secs(1));
    let received = result.outcome.unwrap().unwrap();
    assert_eq!(received.operation().unwrap().name(), operation);
    assert_eq!(received.status().as_u16(), 200);
    received.decode().await.unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|r| r.method == "POST" && r.body_text() == "buffered")
    );
    assert_eq!(
        requests
            .iter()
            .map(|r| r.header("idempotency-key").unwrap())
            .collect::<Vec<_>>(),
        ["stable-key", "stable-key"]
    );
    assert_eq!(
        requests
            .iter()
            .map(|r| r.header("authorization").unwrap())
            .collect::<Vec<_>>(),
        ["fresh-1", "fresh-2"]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn generated_endpoint_429_honors_one_second_through_injected_sleep() {
    let calls = AtomicUsize::new(0);
    let server = TestServer::start(move |_| {
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::new(429).header("retry-after", "1")
        } else {
            Reply::new(200)
        }
    })
    .await;
    let clock = Cell::new(Duration::ZERO);
    let result = retry(2)
        .receive(
            api(&server).get().prepare().unwrap(),
            ReplayMode::Required,
            |_, _| {},
            classify,
            || clock.get(),
            |d| {
                assert_eq!(d, Duration::from_secs(1));
                clock.set(clock.get() + d);
                ready(())
            },
            |_, _| Duration::ZERO,
        )
        .await;
    assert_eq!(result.report.waits, [Duration::from_secs(1)]);
    assert_eq!(result.report.total_waited, Duration::from_secs(1));
    assert_eq!(result.report.attempts, 2);
    assert_eq!(result.outcome.unwrap().unwrap().status().as_u16(), 200);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

fn one_shot(polls: Arc<AtomicUsize>) -> reqwest::Body {
    reqwest::Body::wrap_stream(futures_util::stream::poll_fn(move |_| {
        if polls.fetch_add(1, Ordering::SeqCst) == 0 {
            std::task::Poll::Ready(Some(Ok::<_, Infallible>("streamed")))
        } else {
            std::task::Poll::Ready(None)
        }
    }))
}

#[tokio::test]
async fn noncloneable_stream_is_refused_before_hook_poll_connect_or_io() {
    let server = TestServer::start(|_| Reply::new(200)).await;
    let polls = Arc::new(AtomicUsize::new(0));
    let prepared = api(&server)
        .upload(one_shot(polls.clone()))
        .prepare()
        .unwrap();
    assert!(prepared.request().try_clone().is_none());
    let result = retry(2)
        .receive(
            prepared,
            ReplayMode::Required,
            |_, _| panic!("no hook"),
            |_| panic!("no outcome"),
            || Duration::ZERO,
            |_| async { panic!("no sleep") },
            |_, _| panic!("no jitter"),
        )
        .await;
    assert_eq!(result.report.stop, StopReason::NotReplayable);
    assert_eq!(result.report.attempts, 0);
    assert!(!result.report.replay_available);
    assert!(result.outcome.is_none());
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert_eq!(server.connections(), 0);
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn best_effort_stream_sends_once_and_reports_unavailable_replay() {
    for status in [200, 503] {
        let server = TestServer::start(move |_| Reply::new(status)).await;
        let prepared = api(&server)
            .upload(one_shot(Arc::new(AtomicUsize::new(0))))
            .prepare()
            .unwrap();
        let result = retry(2)
            .receive(
                prepared,
                ReplayMode::BestEffort,
                |_, _| {},
                classify,
                || Duration::ZERO,
                |_| async { panic!("no wait") },
                |_, _| panic!("no jitter"),
            )
            .await;
        assert_eq!(
            result.report.stop,
            if status == 200 {
                StopReason::Done
            } else {
                StopReason::NotReplayable
            }
        );
        assert_eq!(result.report.attempts, 1);
        assert!(!result.report.replay_available);
        assert_eq!(result.outcome.unwrap().unwrap().status().as_u16(), status);
        assert_eq!(server.single().body_text(), "streamed");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn strict_one_attempt_and_semantically_forbidden_streams_can_send_once() {
    for (attempts, replay) in [(1, Replay::Permitted), (3, Replay::Never)] {
        let server = TestServer::start(|_| Reply::new(503)).await;
        let prepared = api(&server)
            .upload(one_shot(Arc::new(AtomicUsize::new(0))))
            .prepare()
            .unwrap();
        let result = Retry::new(Policy::new(NonZeroUsize::new(attempts).unwrap()), replay)
            .receive(
                prepared,
                ReplayMode::Required,
                |_, _| {},
                classify,
                || Duration::ZERO,
                |_| ready(()),
                |_, ceiling| ceiling,
            )
            .await;
        assert_eq!(result.report.attempts, 1);
        assert_eq!(result.report.stop, StopReason::NotReplayable);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn byte_cloneability_and_429_do_not_grant_semantic_permission() {
    let server = TestServer::start(|_| Reply::new(429).header("retry-after", "1")).await;
    let prepared = api(&server).get().prepare().unwrap();
    assert!(prepared.request().try_clone().is_some());
    let result = Retry::new(Policy::new(NonZeroUsize::new(3).unwrap()), Replay::Never)
        .receive(
            prepared,
            ReplayMode::Required,
            |_, _| {},
            classify,
            || Duration::ZERO,
            |_| async { panic!("no wait") },
            |_, _| panic!("no jitter"),
        )
        .await;
    assert_eq!(result.report.stop, StopReason::NotReplayable);
    assert_eq!(result.report.attempts, 1);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn stream_is_returned_unread_and_body_failure_never_restarts_it() {
    let server = TestServer::start(|_| {
        Reply::new(200).with_body(concord_testserver::Body::FailAfter(vec!["item\n".into()]))
    })
    .await;
    let result = retry(3)
        .receive(
            api(&server).events().prepare().unwrap(),
            ReplayMode::Required,
            |_, _| {},
            classify,
            || Duration::ZERO,
            |_| ready(()),
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(result.report.attempts, 1);
    assert_eq!(result.report.stop, StopReason::Done);
    let mut stream = result
        .outcome
        .unwrap()
        .unwrap()
        .decode()
        .await
        .unwrap()
        .into_response();
    assert_eq!(stream.chunk().await.unwrap().unwrap(), "item\n");
    assert!(stream.chunk().await.is_err());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

struct Problem(Vec<u8>);
struct ProblemBytes;
impl concord_core::Decode<Problem> for ProblemBytes {
    type Context = ();
    fn context(_: &concord_core::ResponseHead<'_>) -> Result<(), concord_core::CodecError> {
        Ok(())
    }
    fn decode_body(
        (): (),
        body: concord_core::bytes::Bytes,
    ) -> Result<Problem, concord_core::CodecError> {
        Ok(Problem(body.to_vec()))
    }
}
concord::api! {
    client Typed {
        base "http://unused.invalid";
        error Problem in ProblemBytes;
        GET item();
    }
}

#[tokio::test]
async fn both_receive_adapters_keep_the_declared_document_decoder_and_bound() {
    let server = TestServer::start(|_| Reply::new(409).body("provider-document")).await;
    let api = Typed::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    let first = retry(1)
        .receive(
            api.item().prepare().unwrap().error_body_limit(64),
            ReplayMode::BestEffort,
            |_, _| {},
            |_| Decision::Done,
            || Duration::ZERO,
            |_| ready(()),
            |_, ceiling| ceiling,
        )
        .await
        .outcome
        .unwrap()
        .unwrap();
    let error: concord_core::Error<Problem> = first.decode().await.unwrap_err();
    assert_eq!(error.document().unwrap().0, b"provider-document");
    assert_eq!(error.operation().unwrap().name(), "Typed.item");
    let second = retry(1)
        .receive_with(
            api.item().prepare().unwrap().error_body_limit(4),
            ReplayMode::BestEffort,
            |request, _| ready(Ok::<_, Infallible>((request, ()))),
            |_, _| Decision::Done,
            || Duration::ZERO,
            |_| ready(()),
            |_, ceiling| ceiling,
        )
        .await
        .outcome
        .unwrap()
        .unwrap();
    let error: concord_core::Error<Problem> = second.decode().await.unwrap_err();
    assert!(matches!(
        error.provider().unwrap().state(),
        concord_core::Document::TooLong { limit: 4 }
    ));
    assert_eq!(error.provider().unwrap().body().as_ref(), b"prov");
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn receive_adapter_keeps_an_exceeded_custom_error_body_bound() {
    let server = TestServer::start(|_| Reply::new(409).body("provider-document")).await;
    let api = Typed::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    let received = retry(1)
        .receive(
            api.item().prepare().unwrap().error_body_limit(4),
            ReplayMode::BestEffort,
            |_, _| {},
            |_| Decision::Done,
            || Duration::ZERO,
            |_| ready(()),
            |_, ceiling| ceiling,
        )
        .await
        .outcome
        .unwrap()
        .unwrap();
    let error: concord_core::Error<Problem> = received.decode().await.unwrap_err();
    assert!(matches!(
        error.provider().unwrap().state(),
        concord_core::Document::TooLong { limit: 4 }
    ));
    assert_eq!(error.provider().unwrap().body().as_ref(), b"prov");
    assert_eq!(error.operation().unwrap().name(), "Typed.item");
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}
