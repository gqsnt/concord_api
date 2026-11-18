//! Contracts for calls in flight for each key of a bucket, using scripted HTTP and virtual time.
#![cfg(feature = "limit")]
// Test fixtures intentionally fail on invalid URLs, headers or error types.
#![allow(clippy::unwrap_used)]

use concord::http::HeaderValue;
use concord::limit::{remaining, Bucket, Limit, LimitError, Window};
use concord::reqwest::{Method, Request, Response};
use concord::transport::{TransportError, TryInfo};
use concord::{Middleware, Next};
use concord_testkit::Script;
use futures_util::FutureExt;
use std::error::Error as _;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::time::Instant;

concord::api! {
    client Data {
        base "https://one.test";
        read concord::Text;
        GET get() -> String { path "data"; }
        GET stream() -> concord::ByteStream { path "stream"; read concord::read::Stream; }
    }
}

fn sec(n: u64) -> Duration {
    Duration::from_secs(n)
}
fn tenant() -> Bucket {
    Bucket::new(|call| {
        Some(
            call.request()
                .headers()
                .get("x-tenant")?
                .to_str()
                .ok()?
                .into(),
        )
    })
}
async fn call(limit: &Limit, script: &Script, tenant: &str) -> Result<Response, TransportError> {
    let mut request = Request::new(Method::GET, "https://one.test/data".parse().unwrap());
    request
        .headers_mut()
        .insert("x-tenant", HeaderValue::from_str(tenant).unwrap());
    limit
        .handle(request, TryInfo::new("data", 1), Next::to(script))
        .await
}
fn cause(error: &TransportError) -> &LimitError {
    error.source().unwrap().downcast_ref().unwrap()
}
fn times(script: &Script, start: Instant, expected: &[u64]) {
    assert_eq!(
        script
            .requests()
            .iter()
            .map(|r| r.at() - start)
            .collect::<Vec<_>>(),
        expected.iter().map(|n| sec(*n)).collect::<Vec<_>>()
    );
    assert!(script.requests().iter().all(|r| r.info().number() == 1));
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_full_key_waits_and_holds_nothing_in_another_bucket() {
    let script = Script::new();
    script.answer(200).body("held").stall();
    for _ in 0..2 {
        script.answer(200);
    }
    let application =
        Bucket::new(|_| Some("application".into())).window(Window::rolling(2, sec(10)));
    let limit = Limit::new()
        .bucket(tenant().max_in_flight(1))
        .bucket(application);
    let start = Instant::now();
    let held = call(&limit, &script, "red").await.unwrap();
    // Red's place is taken: this call waits, and reserves nothing of the application's window.
    let mut red = Box::pin(call(&limit, &script, "red"));
    assert!(red.as_mut().now_or_never().is_none());
    // Blue spends the application's second unit at once.
    call(&limit, &script, "blue").await.unwrap();
    drop(held);
    // Red has its place again, then waits for the application's window.
    red.await.unwrap();
    times(&script, start, &[0, 0, 10]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_place_is_released_when_the_body_ends_fails_or_is_dropped() {
    // Read to the end.
    let script = Script::new();
    script.answer(200).body("first");
    script.answer(200).body("next");
    let api = Data::new()
        .with_transport(script.clone())
        .with_middleware(Limit::new().bucket(Bucket::new(|_| Some("k".into())).max_in_flight(1)));
    let start = Instant::now();
    assert_eq!(api.get().await.unwrap(), "first");
    assert_eq!(api.get().await.unwrap(), "next");
    times(&script, start, &[0, 0]);

    // A body that breaks off.
    let script = Script::new();
    script.answer(200).body("first").pause(sec(2)).cut();
    script.answer(200).body("next");
    let api = Data::new()
        .with_transport(script.clone())
        .with_middleware(Limit::new().bucket(Bucket::new(|_| Some("k".into())).max_in_flight(1)));
    let start = Instant::now();
    assert_eq!(
        api.get().await.unwrap_err().class(),
        Some(concord::error::Class::Body)
    );
    assert_eq!(api.get().await.unwrap(), "next");
    times(&script, start, &[0, 2]);

    // A stream that is dropped before its end.
    let script = Script::new();
    script.answer(200).body("first").stall();
    script.answer(200).body("next");
    let api = Data::new()
        .with_transport(script.clone())
        .with_middleware(Limit::new().bucket(Bucket::new(|_| Some("k".into())).max_in_flight(1)));
    let start = Instant::now();
    let mut body = api.stream().await.unwrap();
    assert_eq!(body.try_next().await.unwrap().unwrap(), "first");
    let mut next = Box::pin(api.get().send());
    assert!(next.as_mut().now_or_never().is_none());
    tokio::time::sleep(sec(3)).await;
    assert!(next.as_mut().now_or_never().is_none());
    drop(body);
    assert_eq!(next.await.unwrap(), "next");
    times(&script, start, &[0, 3]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_place_is_released_by_a_failed_send_and_a_dropped_call() {
    let script = Script::new();
    script.fail(TransportError::connect()).after(sec(1));
    script.never();
    script.answer(200);
    // A known window: nothing waits for a first head, only for the cap.
    let limit = Limit::new().bucket(
        tenant()
            .window(Window::rolling(100, sec(60)))
            .max_in_flight(1),
    );
    let start = Instant::now();
    let mut failing = Box::pin(call(&limit, &script, "red"));
    assert!(failing.as_mut().now_or_never().is_none());
    let mut next = Box::pin(call(&limit, &script, "red"));
    assert!(next.as_mut().now_or_never().is_none());
    assert_eq!(script.requests().len(), 1);
    assert!(failing.await.is_err());
    // The failed send freed the place: the next call is sent as it fails.
    let mut sending = next;
    // That call is dropped while it is sent.
    assert!(sending.as_mut().now_or_never().is_none());
    let mut waiting = Box::pin(call(&limit, &script, "red"));
    assert!(waiting.as_mut().now_or_never().is_none());
    assert_eq!(script.requests().len(), 2);
    drop(sending);
    waiting.await.unwrap();
    times(&script, start, &[0, 1, 1]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_full_key_fails_at_max_hold_with_the_existing_error() {
    let script = Script::new();
    script.answer(200).body("held").stall();
    script.answer(200);
    let limit = Limit::new()
        .max_hold(sec(3))
        .bucket(Bucket::new(|_| Some("k".into())).max_in_flight(1));
    let api = Data::new()
        .with_transport(script.clone())
        .with_middleware(limit.clone());
    let start = Instant::now();
    let held = call(&limit, &script, "red").await.unwrap();
    let error = api.get().await.unwrap_err();
    assert!(matches!(
        LimitError::find(&error),
        Some(LimitError::MaxHold { required: None, retry_after: None, waited, .. }) if *waited == sec(3)
    ));
    assert_eq!(
        error.to_string(),
        "transport error: the request was blocked before it was sent, in data.get"
    );
    assert_eq!(
        LimitError::find(&error).unwrap().to_string(),
        "limit wait exceeds max_hold"
    );
    assert_eq!(error.tries(), 1);
    assert_eq!(start.elapsed(), sec(3));
    assert_eq!(script.requests().len(), 1);
    drop(held);
    api.get().await.unwrap();
    times(&script, start, &[0, 3]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_bucket_with_only_a_cap_admits_up_to_its_cap_at_once() {
    let script = Script::new();
    for _ in 0..4 {
        script.answer(200).after(sec(5));
    }
    let limit = Limit::new().bucket(tenant().max_in_flight(3));
    let start = Instant::now();
    let calls: Vec<_> = (0..4)
        .map(|_| {
            let (limit, script) = (limit.clone(), script.clone());
            tokio::spawn(async move { call(&limit, &script, "red").await.map(drop) })
        })
        .collect();
    for call in calls {
        call.await.unwrap().unwrap();
    }
    times(&script, start, &[0, 0, 0, 5]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_bucket_that_learns_still_waits_for_its_first_head_under_a_cap() {
    let script = Script::new();
    for _ in 0..3 {
        script.answer(200).after(sec(5));
    }
    let limit = Limit::new().bucket(
        tenant()
            .max_in_flight(3)
            .learn(remaining("x-limit", "x-left", "x-reset")),
    );
    let start = Instant::now();
    let calls: Vec<_> = (0..3)
        .map(|_| {
            let (limit, script) = (limit.clone(), script.clone());
            tokio::spawn(async move { call(&limit, &script, "red").await.map(drop) })
        })
        .collect();
    for call in calls {
        call.await.unwrap().unwrap();
    }
    // The responses teach nothing: one call at a time, for good.
    times(&script, start, &[0, 5, 10]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_zero_cap_is_refused_for_the_whole_policy_before_any_callback() {
    let script = Script::new();
    script.answer(200);
    let keys = Arc::new(AtomicUsize::new(0));
    let (first, second) = (keys.clone(), keys.clone());
    let limit = Limit::new()
        .bucket(Bucket::new(move |_| {
            first.fetch_add(1, Ordering::SeqCst);
            None
        }))
        .bucket(
            Bucket::new(move |_| {
                second.fetch_add(1, Ordering::SeqCst);
                None
            })
            .max_in_flight(0),
        );
    let error = call(&limit, &script, "red").await.unwrap_err();
    assert!(matches!(cause(&error), LimitError::InvalidPolicy));
    assert_eq!(cause(&error).to_string(), "the limit policy is invalid");
    assert_eq!(keys.load(Ordering::SeqCst), 0);
    assert!(script.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn c11_6_keys_with_calls_in_flight_refuse_the_next_new_key_at_once() {
    let script = Script::new();
    for _ in 0..1024 {
        script.answer(200).body("held").stall();
    }
    script.answer(200);
    let limit = Limit::new().bucket(tenant().max_in_flight(1));
    let start = Instant::now();
    let mut held = Vec::new();
    for n in 0..1024 {
        held.push(call(&limit, &script, &n.to_string()).await.unwrap());
    }
    let error = call(&limit, &script, "one-more").await.unwrap_err();
    assert!(matches!(cause(&error), LimitError::TooManyKeys));
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert_eq!(script.requests().len(), 1024);
    // One body ends: its key holds nothing, and is forgotten for the new one.
    held.pop();
    call(&limit, &script, "one-more").await.unwrap();
    assert_eq!(script.requests().len(), 1025);
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_retried_attempt_takes_a_new_place_and_nothing_is_sent_twice() {
    let script = Script::new();
    script.answer(503).body("busy");
    script.answer(200).body("ok");
    let api = Data::new()
        .with_transport(script.clone())
        .with_middleware(Limit::new().bucket(Bucket::new(|_| Some("k".into())).max_in_flight(1)))
        .with_retry(concord::Retry::none().tries(2).when(|_| Some(sec(1))));
    assert_eq!(api.get().await.unwrap(), "ok");
    let numbers: Vec<_> = script
        .requests()
        .iter()
        .map(|r| r.info().number())
        .collect();
    assert_eq!(numbers, [1, 2]);
}
