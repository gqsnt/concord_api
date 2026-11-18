//! Contracts for the retry ceiling that can be set, with and without a limit, on virtual time.
#![cfg(feature = "limit")]
// Test fixtures intentionally fail on invalid URLs, headers or error types.
#![allow(clippy::unwrap_used)]

use concord::limit::{Bucket, Limit, LimitError, Window};
use concord::Retry;
use concord_testkit::Script;
use std::time::Duration;
use tokio::time::Instant;

concord::api! {
    client Data {
        base "https://one.test";
        read concord::Text;
        GET get() -> String { path "data"; }
    }
}

fn sec(n: u64) -> Duration {
    Duration::from_secs(n)
}
fn sent(script: &Script, start: Instant) -> Vec<Duration> {
    script.requests().iter().map(|r| r.at() - start).collect()
}
/// A 429 that asks for two minutes, then a success.
fn two_minutes() -> Script {
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", "120")
        .body("slow down");
    script.answer(200).body("ok");
    script
}
fn ceiling() -> Retry {
    Retry::standard().max_retry_after(sec(600))
}

#[tokio::test(start_paused = true)]
async fn c4_3_by_default_a_server_that_asks_120_seconds_gets_its_429() {
    let script = two_minutes();
    let api = Data::new()
        .with_transport(script.clone())
        .with_retry(Retry::standard());
    let start = Instant::now();
    let error = api.get().await.unwrap_err();
    assert!(error.is_status(429));
    assert_eq!(error.tries(), 1);
    assert_eq!(error.retry_after(), Some(sec(120)));
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_raised_ceiling_waits_and_retries() {
    let script = two_minutes();
    let api = Data::new()
        .with_transport(script.clone())
        .with_retry(ceiling());
    let start = Instant::now();
    assert_eq!(api.get().await.unwrap(), "ok");
    // The random part of the first wait is at most one second.
    let at = sent(&script, start);
    assert_eq!(at.len(), 2);
    assert!(at[1] >= sec(120) && at[1] <= sec(121), "{at:?}");
    let numbers: Vec<_> = script
        .requests()
        .iter()
        .map(|r| r.info().number())
        .collect();
    assert_eq!(numbers, [1, 2]);
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_raised_ceiling_ends_the_retries_before_the_call_timeout() {
    let script = two_minutes();
    let api = Data::new()
        .with_transport(script.clone())
        .with_retry(ceiling());
    let start = Instant::now();
    let error = api.get().with_timeout(sec(100)).await.unwrap_err();
    // The wait would end after the timeout: it is not started, and the 429 is the result.
    assert!(error.is_status(429));
    assert!(!error.is_timeout());
    assert_eq!(error.tries(), 1);
    assert_eq!(error.retry_after(), Some(sec(120)));
    assert_eq!(start.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_raised_ceiling_meets_the_limit_and_its_max_hold_on_each_try() {
    // The 429 holds the limit for 120 s; the retry's wait covers it, so the second try
    // passes the limit as soon as it is made.
    let script = two_minutes();
    let api = Data::new()
        .with_transport(script.clone())
        .with_middleware(Limit::new().max_hold(sec(10)))
        .with_retry(ceiling());
    let start = Instant::now();
    assert_eq!(api.get().await.unwrap(), "ok");
    let at = sent(&script, start);
    assert!(at[1] >= sec(120) && at[1] <= sec(121), "{at:?}");

    // A window that the first try spent keeps the second one waiting 180 s more than
    // max_hold allows: the second try fails in the limit, and that ends the call.
    let script = two_minutes();
    let limit = Limit::new()
        .max_hold(sec(60))
        .bucket(Bucket::new(|_| Some("k".into())).window(Window::rolling(1, sec(300))));
    let api = Data::new()
        .with_transport(script.clone())
        .with_middleware(limit)
        .with_retry(ceiling());
    let start = Instant::now();
    let error = api.get().await.unwrap_err();
    assert!(matches!(
        LimitError::find(&error),
        Some(LimitError::MaxHold { required: Some(wait), .. }) if *wait >= sec(179)
    ));
    assert_eq!(error.tries(), 2);
    assert_eq!(script.requests().len(), 1);
    let waited = start.elapsed();
    assert!(waited >= sec(120) && waited <= sec(121), "{waited:?}");
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_caller_that_cancels_during_the_long_wait_sends_nothing_more() {
    let script = two_minutes();
    let api = Data::new()
        .with_transport(script.clone())
        .with_retry(ceiling());
    let mut call = Box::pin(api.get().send());
    // Thirty seconds into the wait, the caller gives up.
    assert!(tokio::time::timeout(sec(30), &mut call).await.is_err());
    drop(call);
    tokio::time::sleep(sec(600)).await;
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.unused(), 1);
}
