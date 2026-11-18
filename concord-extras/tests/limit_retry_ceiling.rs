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

    // With a timeout that the wait fits in, the same 429 is retried: what refused it above
    // was the deadline, not the ceiling.
    let script = two_minutes();
    let api = Data::new()
        .with_transport(script.clone())
        .with_retry(ceiling());
    assert_eq!(api.get().with_timeout(sec(200)).await.unwrap(), "ok");
    assert_eq!(script.requests().len(), 2);
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

#[tokio::test(start_paused = true)]
async fn c4_3_a_zero_ceiling_retries_an_asked_wait_of_zero_and_no_positive_one() {
    // A rule asks for no wait: that is retried, after the random part alone.
    let script = Script::new();
    script.answer(418);
    script.answer(200).body("ok");
    let api = Data::new().with_transport(script.clone()).with_retry(
        Retry::none()
            .tries(2)
            .max_retry_after(Duration::ZERO)
            .when(|_| Some(Duration::ZERO)),
    );
    let start = Instant::now();
    assert_eq!(api.get().await.unwrap(), "ok");
    let at = sent(&script, start);
    assert!(at[1] <= sec(1), "{at:?}");

    // A rule asks for one nanosecond: no further attempt.
    let script = Script::new();
    script.answer(418);
    script.answer(200).body("unused");
    let api = Data::new().with_transport(script.clone()).with_retry(
        Retry::none()
            .tries(2)
            .max_retry_after(Duration::ZERO)
            .when(|_| Some(Duration::from_nanos(1))),
    );
    let error = api.get().await.unwrap_err();
    assert!(error.is_status(418));
    assert_eq!(error.tries(), 1);
    assert_eq!(error.retry_after(), Some(Duration::from_nanos(1)));
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_wait_the_timer_cannot_represent_ends_the_retries_whatever_the_ceiling() {
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", &u64::MAX.to_string());
    script.answer(200).body("unused");
    let api = Data::new()
        .with_transport(script.clone())
        .with_retry(Retry::standard().max_retry_after(Duration::MAX));
    let start = Instant::now();
    let error = api.get().await.unwrap_err();
    assert!(error.is_status(429));
    assert_eq!(error.tries(), 1);
    assert_eq!(error.retry_after(), Some(sec(u64::MAX)));
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_wait_beyond_the_timer_s_milliseconds_ends_the_retries() {
    use futures_util::FutureExt;
    // It fits an Instant, but not the timer's 64-bit count of milliseconds.
    let wait = sec(u64::MAX / 1000 + 1);
    assert!(Instant::now().checked_add(wait).is_some());
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", &wait.as_secs().to_string());
    script.answer(200).body("unused");
    let api = Data::new()
        .with_transport(script.clone())
        .with_retry(Retry::standard().max_retry_after(wait));
    let mut call = Box::pin(api.get().send());
    let mut result = call.as_mut().now_or_never();
    for _ in 0..5 {
        if result.is_some() {
            break;
        }
        tokio::task::yield_now().await;
        result = call.as_mut().now_or_never();
    }
    let error = result.expect("the call ends at once").unwrap_err();
    assert!(error.is_status(429));
    assert_eq!(error.tries(), 1);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_deadline_past_the_timer_s_range_ends_the_retries_instead_of_firing_early() {
    use futures_util::FutureExt;
    // At this age of the runtime, even 120 s more is past the last millisecond the timer counts.
    tokio::time::advance(sec(u64::MAX / 1000 + 3)).await;
    let script = Script::new();
    script.answer(429).header("retry-after", "120");
    script.answer(200).body("unused");
    let api = Data::new()
        .with_transport(script.clone())
        .with_retry(Retry::standard().max_retry_after(sec(600)));
    let start = Instant::now();
    let mut call = Box::pin(api.get().send());
    let mut result = call.as_mut().now_or_never();
    for _ in 0..8 {
        if result.is_some() {
            break;
        }
        tokio::task::yield_now().await;
        result = call.as_mut().now_or_never();
    }
    assert_eq!(sent(&script, start), [Duration::ZERO]);
    let error = result.expect("the call ends at once").unwrap_err();
    assert!(error.is_status(429));
    assert_eq!(error.tries(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_wait_longer_than_the_runtime_s_longest_sleep_ends_the_retries() {
    use futures_util::FutureExt;
    // A runtime one second old; a wait that 64 bits of milliseconds hold, but far longer than
    // the longest sleep the runtime documents (68,719,476,734 ms, about 2.2 years).
    tokio::time::advance(sec(1)).await;
    let wait = sec(u64::MAX / 1000);
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", &wait.as_secs().to_string());
    script.answer(200).body("unused");
    let api = Data::new()
        .with_transport(script.clone())
        .with_retry(Retry::standard().max_retry_after(wait));
    let mut call = Box::pin(api.get().send());
    let mut result = call.as_mut().now_or_never();
    for _ in 0..10 {
        if result.is_some() {
            break;
        }
        tokio::task::yield_now().await;
        result = call.as_mut().now_or_never();
    }
    let error = result.expect("the call ends at once").unwrap_err();
    assert!(error.is_status(429));
    assert_eq!(error.tries(), 1);
    assert_eq!(script.unused(), 1);

    // The longest sleep itself is waited for.
    let longest = Duration::from_millis(68_719_476_734);
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", &(longest.as_secs()).to_string());
    script.answer(200).body("ok");
    let api = Data::new()
        .with_transport(script.clone())
        .with_retry(Retry::standard().max_retry_after(longest));
    assert_eq!(api.get().await.unwrap(), "ok");
    assert_eq!(script.requests().len(), 2);
}
