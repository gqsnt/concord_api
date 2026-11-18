//! Contracts for shared limits, using scripted HTTP and virtual time.
#![cfg(feature = "limit")]
// Test fixtures intentionally fail on invalid URLs, headers or error types.
#![allow(clippy::unwrap_used)]

use concord::http::{self, HeaderValue, StatusCode, Version};
use concord::limit::{counts, remaining, Bucket, Limit, LimitError, Window};
use concord::reqwest::{Method, Request, Response};
use concord::transport::{TransportError, TryInfo};
use concord::{Middleware, Next};
use concord_testkit::Script;
use futures_util::{future::join_all, FutureExt};
use std::collections::VecDeque;
use std::error::Error as _;
use std::panic::AssertUnwindSafe;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;

concord::api! {
    client Data {
        base "https://one.test";
        read concord::Text;
        GET get() -> String { path "data"; }
        POST post(body: concord::Body) -> String { path "data"; body body; }
        GET stream() -> concord::ByteStream { path "stream"; read concord::read::Stream; }
    }
}

fn sec(n: u64) -> Duration {
    Duration::from_secs(n)
}
fn bucket(capacity: u64, seconds: u64) -> Bucket {
    Bucket::new(|_| Some("allowance".into())).window(Window::rolling(capacity, sec(seconds)))
}
fn client(script: &Script, limit: Limit) -> Data {
    Data::new()
        .with_transport(script.clone())
        .with_middleware(limit)
}
async fn call(
    limit: &Limit,
    script: &Script,
    endpoint: &'static str,
) -> Result<Response, TransportError> {
    call_at(limit, script, endpoint, "one.test", "red").await
}
async fn call_at(
    limit: &Limit,
    script: &Script,
    endpoint: &'static str,
    host: &str,
    tenant: &str,
) -> Result<Response, TransportError> {
    let mut request = Request::new(Method::GET, format!("https://{host}/data").parse().unwrap());
    request
        .headers_mut()
        .insert("x-tenant", HeaderValue::from_str(tenant).unwrap());
    limit
        .handle(request, TryInfo::new(endpoint, 1), Next::to(script))
        .await
}
fn cause(error: &TransportError) -> &LimitError {
    error.source().unwrap().downcast_ref().unwrap()
}
fn times(script: &Script, start: Instant, expected: &[u64]) {
    assert_eq!(script.requests().len(), expected.len());
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

type RawResponse = http::Response<concord::reqwest::Body>;
type BodyFrame = Result<
    http_body::Frame<concord::bytes::Bytes>,
    <concord::reqwest::Body as http_body::Body>::Error,
>;

async fn next_frame(response: &mut RawResponse) -> Option<BodyFrame> {
    use http_body::Body as _;
    futures_util::future::poll_fn(|cx| std::pin::Pin::new(response.body_mut()).poll_frame(cx)).await
}

async fn body_ending_releases_permit_before_drop(cut: bool) {
    let script = Script::new();
    let answer = script
        .answer(200)
        .header("x-kept", "one")
        .header("x-kept", "two")
        .body("first")
        .pause(sec(4));
    if cut {
        answer.cut();
    } else {
        answer.body("last");
    }
    script.answer(200).body("next");
    let api = client(&script, Limit::new().max_in_flight(1));
    let start = Instant::now();
    let mut raw: RawResponse = api.get().send_raw().await.unwrap().into();
    assert_eq!(
        next_frame(&mut raw)
            .await
            .unwrap()
            .unwrap()
            .into_data()
            .unwrap(),
        "first"
    );
    let mut next = Box::pin(api.get().send());
    assert!(next.as_mut().now_or_never().is_none());
    assert_eq!(script.requests().len(), 1);
    if cut {
        assert!(next_frame(&mut raw).await.unwrap().unwrap_err().is_body());
    } else {
        assert_eq!(
            next_frame(&mut raw)
                .await
                .unwrap()
                .unwrap()
                .into_data()
                .unwrap(),
            "last"
        );
        assert!(next_frame(&mut raw).await.is_none());
    }
    assert_eq!(next.await.unwrap(), "next");
    times(&script, start, &[0, 4]);
    // The response and its body still exist after the peer has completed.
    assert_eq!(raw.headers().get_all("x-kept").iter().count(), 2);
    assert_eq!(raw.status(), 200);
    drop(raw);
}

#[tokio::test(start_paused = true)]
async fn c11_4_never_resends_and_only_the_engine_numbers_retries() {
    let script = Script::new();
    script.answer(429).header("retry-after", "5");
    script.answer(200).body("unused");
    let api = client(&script, Limit::new());
    let error = api.get().await.unwrap_err();
    assert!(error.is_status(429));
    assert_eq!(error.tries(), 1);
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.requests()[0].info().number(), 1);
    assert_eq!(script.unused(), 1);

    let script = Script::new();
    script.answer(429).header("retry-after", "5");
    script.answer(200).body("ok");
    let api =
        client(&script, Limit::new()).with_retry(concord::Retry::none().tries(2).when(|tried| {
            (tried.status() == Some(StatusCode::TOO_MANY_REQUESTS)).then_some(Duration::ZERO)
        }));
    assert_eq!(api.get().await.unwrap(), "ok");
    assert_eq!(script.requests().len(), 2);
    assert_eq!(
        script
            .requests()
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        script.requests()[1].at() - script.requests()[0].at(),
        sec(5)
    );
}

#[tokio::test(start_paused = true)]
async fn c11_4_429_bucket_selection_and_global_fallback() {
    // No definitions, defaults, all false, some true, a skipped key and zero cost.
    for case in 0..6 {
        let script = Script::new();
        script.answer(429).header("retry-after", "5");
        script.answer(200);
        script.answer(200);
        let limit = match case {
            0 => Limit::new(),
            1 => Limit::new().bucket(bucket(100, 1)),
            2 => Limit::new().bucket(bucket(100, 1).hit_when(|_| false)),
            3 => Limit::new()
                .bucket(bucket(100, 1).hit_when(|_| true))
                .bucket(
                    Bucket::new(|c| Some(c.info().endpoint().into()))
                        .window(Window::rolling(100, sec(1)))
                        .hit_when(|_| false),
                ),
            4 => Limit::new().bucket(bucket(100, 1).cost(|_| 0)),
            _ => Limit::new().bucket(Bucket::new(|_| None)),
        };
        let start = Instant::now();
        assert_eq!(call(&limit, &script, "a").await.unwrap().status(), 429);
        call(&limit, &script, "b").await.unwrap();
        call(&limit, &script, "a").await.unwrap();
        times(&script, start, &[0, 5, 5]);
    }
    // One true predicate holds only its own bucket, leaving other methods/hosts free.
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", "7")
        .header("x-hit", "method");
    for _ in 0..3 {
        script.answer(200);
    }
    let limit = Limit::new()
        .bucket(
            Bucket::new(|c| Some(c.request().url().host_str()?.into()))
                .window(Window::rolling(20, sec(1)))
                .hit_when(|s| s.header("x-hit") == Some("app")),
        )
        .bucket(
            Bucket::new(|c| {
                Some(format!(
                    "{} {}",
                    c.request().url().host_str()?,
                    c.info().endpoint()
                ))
            })
            .window(Window::rolling(20, sec(1)))
            .hit_when(|s| s.header("x-hit") == Some("method")),
        );
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    call(&limit, &script, "b").await.unwrap();
    call_at(&limit, &script, "a", "two.test", "red")
        .await
        .unwrap();
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 0, 0, 7]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_panicking_selector_holds_its_bucket_for_later_calls() {
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", "5")
        .header("x-response", "kept")
        .body("unchanged");
    script.answer(200);
    let limit = Limit::new()
        .bucket(bucket(10, 1).hit_when(|_| panic!("private selector payload")))
        .bucket(
            Bucket::new(|context| (context.info().endpoint() == "both").then(|| "second".into()))
                .window(Window::rolling(10, sec(1))),
        );
    let start = Instant::now();
    let response = call(&limit, &script, "both").await.unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(response.headers()["x-response"], "kept");
    assert_eq!(response.text().await.unwrap(), "unchanged");
    call(&limit, &script, "only_panicking_bucket")
        .await
        .unwrap();
    times(&script, start, &[0, 5]);
    assert_eq!(script.unused(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_4_absent_invalid_and_past_retry_after_teach_no_hold() {
    let script = Script::new();
    script.answer(429);
    script.answer(429).header("retry-after", "0");
    script.answer(429).header("retry-after", "invalid");
    script
        .answer(429)
        .header("retry-after", "Sun, 06 Nov 1994 08:49:37 GMT");
    script.answer(200).header("retry-after", "10");
    script.answer(200);
    let limit = Limit::new();
    let start = Instant::now();
    for _ in 0..6 {
        call(&limit, &script, "a").await.unwrap();
    }
    times(&script, start, &[0, 0, 0, 0, 0, 0]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_learning_never_fails_received_response_or_poison_later_calls() {
    for case in 0..10 {
        let script = Script::new();
        let answer = script.answer(201).body("written").header("x-limit", "2:5");
        match case {
            0 => {
                answer.header("x-count", "bad");
            }
            1 => {
                answer.header("x-count", "1:5,1:5");
            }
            2 => {
                answer.header("x-count", "1:5,1:9");
            }
            3 => {
                answer.header("x-count", &"1".repeat(5000));
            }
            4 => {
                answer.header("x-count", "1:5").header("x-count", "0:5");
            }
            9 => {
                answer.header("x-count", "é");
            }
            _ => {
                drop(answer);
            }
        }
        script.answer(200).body("next");
        script.answer(200).body("after expiry");
        let definition = bucket(2, 5);
        let definition = match case {
            5 => definition.learn(|_| panic!("author-private-value")),
            6 => definition.learn(|_| vec![Window::rolling(0, sec(5))]),
            7 => definition.learn(|_| vec![Window::rolling(1, sec(5)); 9]),
            8 => definition.learn(|_| vec![Window::fixed(1, sec(5))]),
            _ => definition.learn(counts("x-limit", "x-count")),
        };
        let api = client(&script, Limit::new().bucket(definition));
        assert_eq!(
            api.post(concord::Body::from("payload")).await.unwrap(),
            "written"
        );
        assert_eq!(api.get().await.unwrap(), "next");
        assert_eq!(api.get().await.unwrap(), "after expiry");
        assert_eq!(script.requests().len(), 3);
        assert_eq!(
            script.requests()[2].at() - script.requests()[0].at(),
            sec(5)
        );
        assert!(script.requests().iter().all(|r| r.info().number() == 1));
    }
    // A valid gate survives even a learner and selector that panic.
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", "3")
        .body("unchanged");
    script.answer(200);
    let limit = Limit::new().bucket(
        bucket(10, 1)
            .learn(|_| panic!("private learner"))
            .hit_when(|_| panic!("private selector")),
    );
    let start = Instant::now();
    let response = call(&limit, &script, "a").await.unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(response.text().await.unwrap(), "unchanged");
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 3]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_key_and_cost_panics_leave_no_admission_state() {
    for cost_panics in [false, true] {
        let script = Script::new();
        script.answer(200);
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let definition = if cost_panics {
            bucket(1, 10).cost(move |_| {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("private cost");
                }
                1
            })
        } else {
            Bucket::new(move |_| {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("private key");
                }
                Some("k".into())
            })
            .window(Window::rolling(1, sec(10)))
        };
        let limit = Limit::new().max_keys(1).max_in_flight(1).bucket(definition);
        assert!(AssertUnwindSafe(call(&limit, &script, "a"))
            .catch_unwind()
            .await
            .is_err());
        assert_eq!(script.requests().len(), 0);
        call(&limit, &script, "a").await.unwrap();
        assert_eq!(script.requests().len(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_4_diagnostics_never_print_values() {
    let secret = "private-key-header-and-url";
    let script = Script::new();
    let observed = Arc::new(AtomicUsize::new(0));
    let seen_count = observed.clone();
    let definition = Bucket::new(move |call| {
        assert_eq!(format!("{call:?}"), "RequestInfo { .. }");
        Some(secret.into())
    })
    .window(Window::rolling(1, sec(100)))
    .learn(move |seen| {
        assert_eq!(seen.key(), secret);
        assert_eq!(format!("{seen:?}"), "Seen { .. }");
        seen_count.fetch_add(1, Ordering::SeqCst);
        vec![]
    });
    assert_eq!(format!("{definition:?}"), "Bucket { .. }");
    assert_eq!(
        format!("{:?}", Window::fixed(10, sec(100))),
        "Window { .. }"
    );
    let limit = Limit::new().bucket(definition);
    assert_eq!(format!("{limit:?}"), "Limit { .. }");
    script.answer(200).header("x-private", secret);
    call(&limit, &script, "a").await.unwrap();
    let error = call(&limit, &script, "a").await.unwrap_err();
    let errors = [
        cause(&error).clone(),
        LimitError::InvalidPolicy,
        LimitError::TooManyKeys,
        LimitError::CostExceedsCapacity,
    ];
    for error in errors {
        assert!(!format!("{error} {error:?}").contains(secret));
        assert!(error.source().is_none());
    }
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_5_simultaneous_costs_reserve_all_four_windows() {
    let script = Script::new();
    for _ in 0..6 {
        script.answer(200);
    }
    let limit = Limit::new()
        .bucket(
            bucket(6, 1)
                .window(Window::rolling(10, sec(10)))
                .cost(|_| 2),
        )
        .bucket(bucket(4, 2).window(Window::rolling(8, sec(10))).cost(|_| 2));
    let start = Instant::now();
    for result in join_all((0..6).map(|_| call(&limit, &script, "a"))).await {
        result.unwrap();
    }
    times(&script, start, &[0, 0, 2, 2, 10, 10]);
    for (capacity, period) in [(6, 1), (10, 10), (4, 2), (8, 10)] {
        for request in script.requests() {
            let n = script
                .requests()
                .iter()
                .filter(|r| r.at() <= request.at() && r.at() + sec(period) > request.at())
                .count();
            assert!(n * 2 <= capacity);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_waiter_never_partially_debits_another_bucket() {
    let script = Script::new();
    for _ in 0..3 {
        script.answer(200);
    }
    let limit = Limit::new().bucket(bucket(2, 10)).bucket(
        Bucket::new(|c| Some(c.info().endpoint().into())).window(Window::rolling(1, sec(5))),
    );
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    let mut waiting = Box::pin(call(&limit, &script, "a"));
    assert!(waiting.as_mut().now_or_never().is_none());
    call(&limit, &script, "b").await.unwrap();
    waiting.await.unwrap();
    times(&script, start, &[0, 0, 10]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_costs_are_per_bucket_and_none_or_zero_skips_everything() {
    let script = Script::new();
    for _ in 0..4 {
        script.answer(200);
    }
    let limit = Limit::new()
        .max_keys(2)
        .bucket(bucket(6, 10).cost(|_| 3))
        .bucket(bucket(3, 5)) // Identical key text remains a distinct allowance.
        .bucket(Bucket::new(|_| None).cost(|_| panic!("not selected")))
        .bucket(
            Bucket::new(|_| Some("skipped".into()))
                .cost(|_| 0)
                .learn(|_| panic!("skipped")),
        );
    let start = Instant::now();
    for _ in 0..4 {
        call(&limit, &script, "a").await.unwrap();
    }
    times(&script, start, &[0, 0, 10, 10]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_key_can_use_host_endpoint_and_header() {
    let script = Script::new();
    script.answer(429).header("retry-after", "5");
    for _ in 0..4 {
        script.answer(200);
    }
    let limit = Limit::new().bucket(
        Bucket::new(|c| {
            Some(format!(
                "{}/{}/{}",
                c.request().url().host_str()?,
                c.info().endpoint(),
                c.request().headers().get("x-tenant")?.to_str().ok()?
            ))
        })
        .window(Window::rolling(20, sec(1))),
    );
    let start = Instant::now();
    call_at(&limit, &script, "a", "one.test", "red")
        .await
        .unwrap();
    call_at(&limit, &script, "a", "two.test", "red")
        .await
        .unwrap();
    call_at(&limit, &script, "b", "one.test", "red")
        .await
        .unwrap();
    call_at(&limit, &script, "a", "one.test", "blue")
        .await
        .unwrap();
    call_at(&limit, &script, "a", "one.test", "red")
        .await
        .unwrap();
    times(&script, start, &[0, 0, 0, 0, 5]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_dispatched_cost_is_never_refunded_on_failure_or_cancel() {
    for cancel in [false, true] {
        let script = Script::new();
        if cancel {
            script.never();
        } else {
            script.fail(TransportError::blocked());
        }
        script.answer(200);
        let limit = Limit::new().max_in_flight(1).bucket(bucket(1, 5));
        let start = Instant::now();
        if cancel {
            let mut first = Box::pin(call(&limit, &script, "a"));
            assert!(first.as_mut().now_or_never().is_none());
            drop(first);
        } else {
            call(&limit, &script, "a").await.unwrap_err();
        }
        call(&limit, &script, "a").await.unwrap();
        times(&script, start, &[0, 5]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_5_learns_four_headers_two_windows_each_in_sorted_order() {
    let script = Script::new();
    script
        .answer(200)
        .header("x-app-limit", "3:5,2:1")
        .header("x-app-count", "1:1,1:5")
        .header("x-method-limit", "2:1,3:5")
        .header("x-method-count", "1:5,1:1");
    for _ in 0..4 {
        script.answer(200);
    }
    let limit = Limit::new()
        .bucket(Bucket::new(|_| Some("same".into())).learn(counts("x-app-limit", "x-app-count")))
        .bucket(
            Bucket::new(|_| Some("same".into())).learn(counts("x-method-limit", "x-method-count")),
        );
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    for result in join_all((0..4).map(|_| call(&limit, &script, "a"))).await {
        result.unwrap();
    }
    times(&script, start, &[0, 0, 1, 5, 5]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_late_optimistic_headers_never_reopen_rolling_budget() {
    let script = Script::new();
    script.answer(200).after(sec(1)).header("x-left", "2");
    script.answer(200).header("x-left", "0");
    script.answer(200);
    let limit = Limit::new().bucket(bucket(3, 10).learn(|s| {
        s.header("x-left")
            .and_then(|v| v.parse().ok())
            .map(|n| vec![Window::rolling(3, sec(10)).remaining(n)])
            .unwrap_or_default()
    }));
    let start = Instant::now();
    let (a, b) = tokio::join!(call(&limit, &script, "a"), call(&limit, &script, "a"));
    a.unwrap();
    b.unwrap();
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 0, 10]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_learning_subtracts_other_in_flight_cost() {
    let script = Script::new();
    script.answer(200).after(sec(1)).header("x-left", "1");
    script.never();
    script.answer(200);
    let limit = Limit::new().bucket(bucket(3, 10).learn(|s| {
        s.header("x-left")
            .map(|_| vec![Window::rolling(3, sec(10)).remaining(1)])
            .unwrap_or_default()
    }));
    let start = Instant::now();
    let mut first = Box::pin(call(&limit, &script, "a"));
    let mut second = Box::pin(call(&limit, &script, "a"));
    assert!(first.as_mut().now_or_never().is_none());
    assert!(second.as_mut().now_or_never().is_none());
    first.await.unwrap();
    drop(second);
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 0, 10]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_fixed_reset_refills_whole_capacity_and_remains_known() {
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    script.never();
    script.answer(200);
    let limit = Limit::new()
        .max_hold(sec(8))
        .bucket(Bucket::new(|_| Some("k".into())).window(Window::fixed(2, sec(5))));
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    call(&limit, &script, "a").await.unwrap();
    let mut first = Box::pin(call(&limit, &script, "a"));
    assert!(first.as_mut().now_or_never().is_none());
    tokio::time::advance(sec(5)).await;
    assert!(first.as_mut().now_or_never().is_none());
    // The second refilled unit can leave before the peer's head: no discovery gate.
    call(&limit, &script, "a").await.unwrap();
    drop(first);
    // The known budget remains spent after that peer is dropped; no second
    // reset was announced, so a further wait is unknown and bounded.
    let error = call(&limit, &script, "a").await.unwrap_err();
    assert!(
        matches!(cause(&error), LimitError::MaxHold { required: None, waited, .. }
        if *waited == sec(8))
    );
    times(&script, start, &[0, 0, 5, 5]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_remaining_unix_reset_and_stale_fixed_headers() {
    let script = Script::new();
    let reset = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 10;
    script
        .answer(200)
        .header("x-limit", "2")
        .header("x-left", "1")
        .header("x-reset", &reset.to_string());
    script
        .answer(200)
        .header("x-limit", "2")
        .header("x-left", "2")
        .header("x-reset", &(reset - 1).to_string());
    script.answer(200);
    let limit = Limit::new().bucket(
        Bucket::new(|_| Some("points".into())).learn(remaining("x-limit", "x-left", "x-reset")),
    );
    let start = Instant::now();
    for _ in 0..3 {
        call(&limit, &script, "a").await.unwrap();
    }
    assert_eq!(script.requests().len(), 3);
    assert_eq!(script.requests()[0].at(), start);
    assert_eq!(script.requests()[1].at(), start);
    let elapsed = script.requests()[2].at() - start;
    assert!(elapsed > sec(9) && elapsed <= sec(10));
    assert!(script.requests().iter().all(|r| r.info().number() == 1));
}

#[tokio::test(start_paused = true)]
async fn c11_6_spent_keys_are_bounded_without_eviction() {
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    let limit = Limit::new().max_keys(1).bucket(
        Bucket::new(|c| Some(c.info().endpoint().into())).window(Window::rolling(1, sec(10))),
    );
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    let error = call(&limit, &script, "b").await.unwrap_err();
    assert!(matches!(cause(&error), LimitError::TooManyKeys));
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 10]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_full_history_merges_conservatively_at_30000_per_ten_minutes() {
    let script = Script::new();
    let n = 34_200;
    for _ in 0..n {
        script.answer(200);
    }
    let limit = Limit::new().max_hold(sec(700)).bucket(bucket(30_000, 600));
    let start = Instant::now();
    for i in 0..n {
        call(&limit, &script, "a").await.unwrap();
        if i < 30_000 {
            tokio::time::advance(Duration::from_millis(1)).await;
        }
    }
    assert_eq!(script.requests().len(), n);
    // An exact independent history of the actual sends must always have room.
    let mut exact = VecDeque::new();
    for request in script.requests() {
        while exact
            .front()
            .is_some_and(|at| *at + sec(600) <= request.at())
        {
            exact.pop_front();
        }
        assert!(exact.len() < 30_000);
        exact.push_back(request.at());
        assert_eq!(request.info().number(), 1);
    }
    // 30,000 distinct debits overflow the 4,096 slots. The exact earliest next
    // admission was t=600; merging postpones it, never advances it.
    assert!(script.requests()[30_000].at() - start >= sec(600));
    assert!(script.requests()[30_000].at() - start > sec(625));
}

#[tokio::test(start_paused = true)]
async fn c11_6_full_history_recovers_all_capacity_after_the_period() {
    // One more debit than the documented 4,096-entry history must be merged.
    let capacity = 4_097;
    let script = Script::new();
    for _ in 0..2 * capacity {
        script.answer(200);
    }
    let limit = Limit::new()
        .max_hold(Duration::ZERO)
        .bucket(bucket(capacity, 10));
    let start = Instant::now();
    for _ in 0..capacity {
        call(&limit, &script, "a").await.unwrap();
        tokio::time::advance(Duration::from_millis(1)).await;
    }
    tokio::time::advance(sec(10)).await;
    let recovered = Instant::now();
    for _ in 0..capacity {
        call(&limit, &script, "a").await.unwrap();
    }
    assert_eq!(Instant::now(), recovered);
    let requests = script.requests();
    assert_eq!(requests.len() as u64, 2 * capacity);
    for (index, request) in requests.iter().enumerate() {
        let expected = if (index as u64) < capacity {
            start + Duration::from_millis(index as u64)
        } else {
            recovered
        };
        assert_eq!(request.at(), expected);
        assert_eq!(request.info().number(), 1);
    }
    assert_eq!(script.unused(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_7_known_excessive_wait_fails_immediately_with_given_wait() {
    let script = Script::new();
    script.answer(429).header("retry-after", "10");
    let limit = Limit::new().max_hold(sec(3));
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    let error = call(&limit, &script, "a").await.unwrap_err();
    assert!(
        matches!(cause(&error),LimitError::MaxHold{required:Some(required),retry_after:Some(given),waited,..}
        if *required==sec(10)&&*given==sec(10)&&waited.is_zero())
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
    times(&script, start, &[0]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_unknown_wait_ends_at_max_hold_and_drop_unblocks_discovery() {
    let script = Script::new();
    script.never();
    script.answer(200);
    let limit = Limit::new()
        .max_hold(sec(3))
        .bucket(Bucket::new(|_| Some("unknown".into())));
    let start = Instant::now();
    let mut first = Box::pin(call(&limit, &script, "a"));
    assert!(first.as_mut().now_or_never().is_none());
    let error = call(&limit, &script, "a").await.unwrap_err();
    assert!(
        matches!(cause(&error),LimitError::MaxHold{required:None,waited,..} if *waited==sec(3))
    );
    assert_eq!(script.requests().len(), 1);
    assert_eq!(start.elapsed(), sec(3));
    drop(first);
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 3]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_body_eof_releases_permit_while_raw_body_is_kept() {
    body_ending_releases_permit_before_drop(false).await;
}

#[tokio::test(start_paused = true)]
async fn c11_7_body_error_releases_permit_while_raw_body_is_kept() {
    body_ending_releases_permit_before_drop(true).await;
}

/// A body that gives one frame, then an error, and never reports that it ended.
struct Breaks(u8);

impl http_body::Body for Breaks {
    type Data = concord::bytes::Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        self.0 = self.0.saturating_add(1);
        match self.0 {
            1 => std::task::Poll::Ready(Some(Ok(http_body::Frame::data("first".into())))),
            2 => std::task::Poll::Ready(Some(Err(std::io::Error::other("broken")))),
            _ => std::task::Poll::Pending,
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_body_error_releases_permit_even_when_the_body_reports_no_end() {
    let sent = Arc::new(AtomicUsize::new(0));
    let transport = concord::fake({
        let sent = sent.clone();
        move |_| {
            Ok(if sent.fetch_add(1, Ordering::SeqCst) == 0 {
                http::Response::new(concord::reqwest::Body::wrap(Breaks(0))).into()
            } else {
                concord::reply(200, "next")
            })
        }
    });
    let limit = Limit::new().max_in_flight(1);
    let request = || Request::new(Method::GET, "https://one.test/data".parse().unwrap());
    let response = limit
        .handle(request(), TryInfo::new("data", 1), Next::to(&transport))
        .await
        .unwrap();
    let mut raw: http::Response<concord::reqwest::Body> = response.into();
    let frame = |raw: &mut http::Response<concord::reqwest::Body>| {
        use http_body::Body as _;
        futures_util::future::poll_fn(|cx| std::pin::Pin::new(raw.body_mut()).poll_frame(cx))
            .now_or_never()
            .flatten()
    };
    assert_eq!(
        frame(&mut raw).unwrap().unwrap().into_data().unwrap(),
        "first"
    );
    let mut next = Box::pin(limit.handle(request(), TryInfo::new("data", 1), Next::to(&transport)));
    assert!(next.as_mut().now_or_never().is_none());
    assert!(frame(&mut raw).unwrap().is_err());
    // The response is kept, and its body has not said that it ended: the error alone frees
    // the place.
    assert!(!http_body::Body::is_end_stream(raw.body()));
    let next = tokio::time::timeout(sec(1), next)
        .await
        .expect("the error did not release the permit")
        .unwrap();
    assert_eq!(next.status(), 200);
    assert_eq!(sent.load(Ordering::SeqCst), 2);
    drop(raw);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_header_over_4096_bytes_teaches_nothing() {
    // `0` units left, after spaces that the reader trims: 4,096 bytes are read, 4,097 are not.
    for (padding, waits) in [(4095, true), (4096, false)] {
        let script = Script::new();
        let reset = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 10;
        let left = format!("{}0", " ".repeat(padding));
        script
            .answer(200)
            .header("x-limit", "2")
            .header("x-left", &left)
            .header("x-reset", &reset.to_string());
        script.answer(200);
        let limit = Limit::new().bucket(
            Bucket::new(|_| Some("points".into())).learn(remaining("x-limit", "x-left", "x-reset")),
        );
        let start = Instant::now();
        for _ in 0..2 {
            call(&limit, &script, "a").await.unwrap();
        }
        let elapsed = script.requests()[1].at() - start;
        if waits {
            assert!(
                elapsed > sec(9) && elapsed <= sec(10),
                "{} bytes: {elapsed:?}",
                left.len()
            );
        } else {
            assert_eq!(elapsed, Duration::ZERO, "{} bytes", left.len());
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_7_unread_body_and_canceled_pull_hold_permit_until_drop() {
    let script = Script::new();
    script.answer(200).body("first").stall();
    script.answer(200).body("next");
    let api = client(&script, Limit::new().max_in_flight(1));
    let start = Instant::now();
    let mut body = api.stream().await.unwrap();
    assert_eq!(body.try_next().await.unwrap().unwrap(), "first");
    let mut next = Box::pin(api.get().send());
    assert!(next.as_mut().now_or_never().is_none());
    let mut pull = Box::pin(body.try_next());
    assert!(pull.as_mut().now_or_never().is_none());
    drop(pull);
    assert!(next.as_mut().now_or_never().is_none());
    assert_eq!(script.requests().len(), 1);
    drop(body);
    assert_eq!(next.await.unwrap(), "next");
    times(&script, start, &[0, 0]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_canceling_waiters_spends_nothing_and_leaks_no_permit_or_key() {
    let script = Script::new();
    script.answer(200).body("first").stall();
    script.answer(200);
    let limit = Limit::new().max_in_flight(1).max_keys(1).bucket(
        Bucket::new(|c| Some(c.info().endpoint().into())).window(Window::rolling(2, sec(5))),
    );
    let start = Instant::now();
    let first = call(&limit, &script, "a").await.unwrap();
    let mut waiting = Box::pin(call(&limit, &script, "b"));
    // Key-space rejection is immediate; it never evicts the existing account.
    assert!(matches!(
        cause(&waiting.as_mut().await.unwrap_err()),
        LimitError::TooManyKeys
    ));
    drop(waiting);
    let mut waiting = Box::pin(call(&limit, &script, "a"));
    assert!(waiting.as_mut().now_or_never().is_none());
    drop(waiting);
    drop(first);
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 0]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_call_timeout_cancels_admission_and_idle_excludes_limit_wait() {
    let script = Script::new();
    script.answer(200).body("first");
    script.answer(200).body("next");
    let api = client(&script, Limit::new().bucket(bucket(1, 5)));
    let start = Instant::now();
    api.get().await.unwrap();
    let error = api.get().with_timeout(sec(1)).await.unwrap_err();
    assert!(error.is_timeout());
    assert_eq!(start.elapsed(), sec(1));
    assert_eq!(error.tries(), 1);
    assert_eq!(script.requests().len(), 1);
    assert_eq!(api.get().with_idle_timeout(sec(1)).await.unwrap(), "next");
    times(&script, start, &[0, 5]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_zero_concurrency_and_invalid_policies_are_typed_before_sending() {
    let script = Script::new();
    let start = Instant::now();
    for limit in [
        Limit::new().max_in_flight(0),
        Limit::new().max_keys(0),
        Limit::new().bucket(bucket(0, 1)),
        Limit::new().bucket(bucket(1, 0)),
        Limit::new().bucket(bucket(1, 1).window(Window::fixed(0, sec(1)))),
        Limit::new().bucket(Bucket::new(|_| Some("x".repeat(513)))),
        Limit::new().bucket(bucket(1, 1).window(Window::rolling(1, sec(1)).remaining(2))),
        Limit::new().max_hold(Duration::MAX),
    ] {
        let error = call(&limit, &script, "a").await.unwrap_err();
        assert!(matches!(cause(&error), LimitError::InvalidPolicy));
        assert_eq!(start.elapsed(), Duration::ZERO);
    }
    let error = call(&Limit::new().bucket(bucket(1, 1).cost(|_| 2)), &script, "a")
        .await
        .unwrap_err();
    assert!(matches!(cause(&error), LimitError::CostExceedsCapacity));
    assert_eq!(script.requests().len(), 0);
    assert_eq!(start.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn c11_7_peer_extending_a_gate_wakes_and_rechecks_waiters() {
    let script = Script::new();
    script.answer(429).after(sec(1)).header("retry-after", "4");
    script.answer(429).after(sec(2)).header("retry-after", "10");
    script.answer(200);
    let limit = Limit::new().max_hold(sec(6)).bucket(bucket(100, 1));
    let start = Instant::now();
    let mut first = Box::pin(call(&limit, &script, "a"));
    let mut second = Box::pin(call(&limit, &script, "a"));
    assert!(first.as_mut().now_or_never().is_none());
    assert!(second.as_mut().now_or_never().is_none());
    first.await.unwrap();
    let mut waiting = Box::pin(call(&limit, &script, "a"));
    assert!(waiting.as_mut().now_or_never().is_none());
    assert_eq!(second.await.unwrap().status(), 429);
    let error = waiting.await.unwrap_err();
    assert!(matches!(cause(&error), LimitError::MaxHold {
        required: Some(required), retry_after: Some(given), waited, ..
    } if *required == sec(10) && *given == sec(10) && *waited == sec(1)));
    assert_eq!(start.elapsed(), sec(2));
    assert_eq!(script.requests().len(), 2);
    tokio::time::advance(sec(10)).await;
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 0, 12]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_retry_after_dates_match_core_and_huge_waits_are_bounded() {
    let date = "Tue, 01 Jan 2030 00:00:00 GMT";
    let mut headers = http::HeaderMap::new();
    headers.insert("retry-after", date.parse().unwrap());
    let before = concord::Headers::from(headers.clone())
        .retry_after()
        .unwrap();
    let script = Script::new();
    script.answer(429).header("retry-after", date);
    let limit = Limit::new();
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    let after = concord::Headers::from(headers).retry_after().unwrap();
    let error = call(&limit, &script, "a").await.unwrap_err();
    assert!(
        matches!(cause(&error),LimitError::MaxHold{required:Some(required),retry_after:Some(given),waited,..}
        if *given<=before&&*given>=after&&*required==*given&&waited.is_zero())
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
    times(&script, start, &[0]);
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", "18446744073709551615");
    let limit = Limit::new();
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    let error = call(&limit, &script, "a").await.unwrap_err();
    assert!(
        matches!(cause(&error),LimitError::MaxHold{required:Some(required),waited,..}
        if *required==sec(u64::MAX)&&waited.is_zero())
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
    times(&script, start, &[0]);
}

// Script owns every HTTP exchange; this downstream layer adds metadata and a
// trailer frame so the limit's public response conversion is exercised fully.
struct Metadata;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Marker(u32);
struct Trailers {
    body: concord::reqwest::Body,
    trailer: Option<http::HeaderMap>,
}
impl http_body::Body for Trailers {
    type Data = concord::bytes::Bytes;
    type Error = <concord::reqwest::Body as http_body::Body>::Error;
    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        use std::task::Poll;
        match std::pin::Pin::new(&mut self.body).poll_frame(cx) {
            Poll::Ready(None) => Poll::Ready(
                self.trailer
                    .take()
                    .map(|t| Ok(http_body::Frame::trailers(t))),
            ),
            frame => frame,
        }
    }
}
impl Middleware for Metadata {
    async fn handle(
        &self,
        request: Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<Response, TransportError> {
        use concord::reqwest::ResponseBuilderExt;
        let response = next.send(request, info).await?;
        let url = response.url().clone();
        let mut raw: http::Response<concord::reqwest::Body> = response.into();
        raw.extensions_mut().extend(
            http::Response::builder()
                .url(url)
                .body(())
                .unwrap()
                .into_parts()
                .0
                .extensions,
        );
        raw.extensions_mut().insert(Marker(42));
        *raw.version_mut() = Version::HTTP_2;
        let mut trailer = http::HeaderMap::new();
        trailer.insert("x-trailer", HeaderValue::from_static("kept"));
        Ok(raw
            .map(|body| {
                concord::reqwest::Body::wrap(Trailers {
                    body,
                    trailer: Some(trailer),
                })
            })
            .into())
    }
}

#[tokio::test(start_paused = true)]
async fn c11_7_body_wrapper_preserves_status_url_version_extensions_and_trailers() {
    use http_body::Body as _;
    let script = Script::new();
    script
        .answer(201)
        .header("x-repeat", "first")
        .header("x-repeat", "second")
        .body("payload");
    script.answer(200).body("next");
    let api = client(&script, Limit::new().max_in_flight(1)).with_middleware(Metadata);
    let response = api.get().send_raw().await.unwrap();
    assert_eq!(response.status(), 201);
    assert_eq!(response.url().as_str(), "https://one.test/data");
    assert_eq!(response.version(), Version::HTTP_2);
    assert_eq!(response.extensions().get::<Marker>(), Some(&Marker(42)));
    assert_eq!(
        response
            .headers()
            .get_all("x-repeat")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    let mut raw: http::Response<concord::reqwest::Body> = response.into();
    let mut next = Box::pin(api.get().send());
    assert!(next.as_mut().now_or_never().is_none());
    let first =
        futures_util::future::poll_fn(|cx| std::pin::Pin::new(raw.body_mut()).poll_frame(cx))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(first.into_data().unwrap(), "payload");
    let trailer =
        futures_util::future::poll_fn(|cx| std::pin::Pin::new(raw.body_mut()).poll_frame(cx))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        trailer.into_trailers().unwrap().get("x-trailer").unwrap(),
        "kept"
    );
    assert!(
        futures_util::future::poll_fn(|cx| std::pin::Pin::new(raw.body_mut()).poll_frame(cx))
            .await
            .is_none()
    );
    assert_eq!(next.await.unwrap(), "next");
    assert_eq!(raw.extensions().get::<Marker>(), Some(&Marker(42)));
    drop(raw);
    assert_eq!(script.requests().len(), 2);
    assert!(script.requests().iter().all(|r| r.info().number() == 1));
}

#[tokio::test(start_paused = true)]
async fn c11_5_learning_capacity_changes_never_raise_active_availability() {
    for fixed in [false, true] {
        let script = Script::new();
        script.answer(200);
        script.answer(200);
        let initial = if fixed {
            Window::fixed(1, sec(5))
        } else {
            Window::rolling(1, sec(5))
        };
        let limit = Limit::new().bucket(Bucket::new(|_| Some("k".into())).window(initial).learn(
            move |_| {
                vec![if fixed {
                    Window::fixed(10, sec(5)).remaining(10)
                } else {
                    Window::rolling(10, sec(5)).remaining(10)
                }]
            },
        ));
        let start = Instant::now();
        call(&limit, &script, "a").await.unwrap();
        call(&limit, &script, "a").await.unwrap();
        times(&script, start, &[0, 5]);
    }
    // Lower capacity must retain locally spent units too.
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    let limit = Limit::new().bucket(
        Bucket::new(|_| Some("k".into()))
            .window(Window::fixed(10, sec(5)))
            .cost(|_| 2)
            .learn(|_| vec![Window::fixed(2, sec(5)).remaining(2)]),
    );
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 5]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_waiting_on_concurrency_does_not_retain_a_new_key() {
    let script = Script::new();
    script.answer(200).body("first").stall();
    script.answer(200);
    let limit = Limit::new().max_in_flight(1).max_keys(2).bucket(
        Bucket::new(|c| Some(c.info().endpoint().into()))
            .window(Window::rolling(10, sec(5)).remaining(9)),
    );
    // Initial debt makes an erroneously retained pending key unreclaimable.
    let start = Instant::now();
    let first = call(&limit, &script, "first").await.unwrap();
    let mut abandoned = Box::pin(call(&limit, &script, "abandoned"));
    assert!(abandoned.as_mut().now_or_never().is_none());
    drop(abandoned);
    drop(first);
    call(&limit, &script, "last").await.unwrap();
    times(&script, start, &[0, 0]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_zero_hold_admits_immediately_but_never_waits() {
    let script = Script::new();
    script.answer(200).body("held").stall();
    script.answer(200);
    let limit = Limit::new().max_hold(Duration::ZERO).max_in_flight(1);
    let start = Instant::now();
    let first = call(&limit, &script, "a").await.unwrap();
    let error = call(&limit, &script, "a").await.unwrap_err();
    assert!(
        matches!(cause(&error),LimitError::MaxHold{required:None,waited,..} if waited.is_zero())
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
    drop(first);
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 0]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_unknown_learner_without_remaining_counts_its_discovery_call() {
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    let limit = Limit::new()
        .bucket(Bucket::new(|_| Some("k".into())).learn(|_| vec![Window::rolling(1, sec(5))]));
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 5]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_clones_share_a_gate_and_concurrency_runs_to_each_head() {
    let script = Script::new();
    script.answer(429).header("retry-after", "5");
    for _ in 0..5 {
        script.answer(200).after(sec(3)).body("ok");
    }
    let api = client(&script, Limit::new().max_in_flight(2));
    let error = api.get().await.unwrap_err();
    assert!(error.is_status(429));
    assert_eq!(error.tries(), 1);
    let start = Instant::now();
    let clones: Vec<_> = (0..5).map(|_| api.clone()).collect();
    for result in join_all(clones.iter().map(|c| c.get().send())).await {
        assert_eq!(result.unwrap(), "ok");
    }
    times(&script, start, &[0, 5, 5, 8, 8, 11]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_late_wakeup_never_dispatches_after_max_hold() {
    let script = Script::new();
    script.answer(200).body("first").stall();
    script.answer(200);
    let limit = Limit::new().max_in_flight(1).max_hold(sec(3));
    let start = Instant::now();
    let first = call(&limit, &script, "a").await.unwrap();
    let mut waiting = Box::pin(call(&limit, &script, "a"));
    assert!(waiting.as_mut().now_or_never().is_none());
    tokio::time::advance(sec(4)).await;
    drop(first);
    let error = waiting.await.unwrap_err();
    assert!(
        matches!(cause(&error),LimitError::MaxHold{required:Some(required),waited,..} if required.is_zero() && *waited==sec(4))
    );
    assert_eq!(script.requests().len(), 1);
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 4]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_sixty_four_client_clones_obey_four_upfront_windows() {
    let script = Script::new();
    for _ in 0..64 {
        script.answer(200).body("ok");
    }
    let limit = Limit::new()
        .bucket(bucket(20, 1).window(Window::rolling(100, sec(120))))
        .bucket(bucket(20, 1).window(Window::rolling(100, sec(120))));
    let api = client(&script, limit);
    let start = Instant::now();
    let clones: Vec<_> = (0..64).map(|_| api.clone()).collect();
    for result in join_all(clones.iter().map(|c| c.get().send())).await {
        result.unwrap();
    }
    let expected: Vec<_> = (0..64).map(|i| i / 20).collect();
    times(&script, start, &expected);
}

#[tokio::test(start_paused = true)]
async fn c11_4_global_fallback_holds_calls_with_no_selected_buckets() {
    for global in [false, true] {
        let script = Script::new();
        script.answer(429).header("retry-after", "5");
        script.answer(200);
        let limit = Limit::new().bucket(
            Bucket::new(|c| (c.info().endpoint() == "limited").then(|| "k".into()))
                .window(Window::rolling(10, sec(1)))
                .hit_when(move |_| !global),
        );
        let start = Instant::now();
        call(&limit, &script, "limited").await.unwrap();
        call(&limit, &script, "free").await.unwrap();
        times(&script, start, &[0, if global { 5 } else { 0 }]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_5_body_only_budgets_are_not_learned() {
    let script = Script::new();
    for _ in 0..3 {
        script.answer(200).body(r#"{"remaining":0}"#);
    }
    let api = client(
        &script,
        Limit::new().bucket(
            Bucket::new(|_| Some("k".into())).learn(remaining("x-limit", "x-left", "x-reset")),
        ),
    );
    let start = Instant::now();
    for _ in 0..3 {
        assert_eq!(api.get().await.unwrap(), r#"{"remaining":0}"#);
    }
    times(&script, start, &[0, 0, 0]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_discovery_releases_after_learning_head_with_unread_body() {
    let script = Script::new();
    script.answer(200).after(sec(2)).body("unread").stall();
    script.answer(200);
    let heads = Arc::new(AtomicUsize::new(0));
    let learned = heads.clone();
    let limit = Limit::new().bucket(Bucket::new(|_| Some("unknown".into())).learn(move |_| {
        learned.fetch_add(1, Ordering::SeqCst);
        vec![]
    }));
    let start = Instant::now();
    let mut first = Box::pin(call(&limit, &script, "a"));
    let mut second = Box::pin(call(&limit, &script, "a"));
    assert!(first.as_mut().now_or_never().is_none());
    assert!(second.as_mut().now_or_never().is_none());
    assert_eq!(heads.load(Ordering::SeqCst), 0);
    assert_eq!(script.requests().len(), 1);
    let unread = first.await.unwrap();
    assert_eq!(heads.load(Ordering::SeqCst), 1);
    second.await.unwrap();
    assert_eq!(heads.load(Ordering::SeqCst), 2);
    times(&script, start, &[0, 2]);
    drop(unread);
}

#[tokio::test(start_paused = true)]
async fn c11_5_discovery_failure_or_cancel_releases_waiting_peer() {
    for cancel in [false, true] {
        let script = Script::new();
        if cancel {
            script.never();
        } else {
            script.fail(TransportError::blocked()).after(sec(2));
        }
        script.answer(200);
        let limit = Limit::new().bucket(Bucket::new(|_| Some("unknown".into())));
        let start = Instant::now();
        let mut first = Box::pin(call(&limit, &script, "a"));
        let mut second = Box::pin(call(&limit, &script, "a"));
        assert!(first.as_mut().now_or_never().is_none());
        assert!(second.as_mut().now_or_never().is_none());
        assert_eq!(script.requests().len(), 1);
        if cancel {
            drop(first);
        } else {
            first.await.unwrap_err();
        }
        second.await.unwrap();
        times(&script, start, &[0, if cancel { 0 } else { 2 }]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_7_max_hold_reports_current_wait_and_original_contributing_retry_after() {
    let script = Script::new();
    script.answer(429).after(sec(9)).header("retry-after", "4");
    script.answer(429).header("retry-after", "10");
    let limit = Limit::new().max_hold(sec(12)).bucket(bucket(100, 1));
    let start = Instant::now();
    let mut peer = Box::pin(call(&limit, &script, "a"));
    assert!(peer.as_mut().now_or_never().is_none());
    assert_eq!(call(&limit, &script, "a").await.unwrap().status(), 429);
    let mut waiting = Box::pin(call(&limit, &script, "a"));
    assert!(waiting.as_mut().now_or_never().is_none());
    let (head, error) = tokio::join!(peer, waiting);
    assert_eq!(head.unwrap().status(), 429);
    let error = error.unwrap_err();
    assert!(matches!(cause(&error), LimitError::MaxHold {
        required: Some(required), retry_after: Some(given), waited, ..
    } if *required == sec(4) && *given == sec(4) && *waited == sec(9)));
    assert_eq!(start.elapsed(), sec(9));
    times(&script, start, &[0, 0]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_max_hold_recomputes_remaining_gate_after_late_wakeup() {
    let script = Script::new();
    script.answer(429).header("retry-after", "5");
    script.never();
    let limit = Limit::new().max_hold(sec(10)).max_in_flight(2);
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    let mut waiting = Box::pin(call(&limit, &script, "a"));
    assert!(waiting.as_mut().now_or_never().is_none());
    tokio::time::advance(sec(11)).await;
    let error = waiting.await.unwrap_err();
    assert!(matches!(cause(&error), LimitError::MaxHold {
        required: Some(required), retry_after: None, waited, ..
    } if required.is_zero() && *waited == sec(11)));
    times(&script, start, &[0]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_response_callback_panic_discards_all_learning_but_keeps_gate() {
    for selector_panics in [false, true] {
        let script = Script::new();
        script
            .answer(429)
            .header("retry-after", "2")
            .body("unchanged");
        script.answer(200);
        let limit = Limit::new()
            .bucket(
                bucket(2, 5)
                    .learn(move |seen| {
                        if seen.status() != 429 {
                            return vec![];
                        }
                        if !selector_panics {
                            panic!("private learner payload");
                        }
                        vec![Window::rolling(2, sec(5)).remaining(0)]
                    })
                    .hit_when(move |_| {
                        if selector_panics {
                            panic!("private selector payload");
                        }
                        false
                    }),
            )
            .bucket(bucket(2, 5).learn(|seen| {
                if seen.status() == 429 {
                    vec![Window::rolling(2, sec(5)).remaining(0)]
                } else {
                    vec![]
                }
            }));
        let start = Instant::now();
        let response = call(&limit, &script, "a").await.unwrap();
        assert_eq!(response.status(), 429);
        assert_eq!(response.text().await.unwrap(), "unchanged");
        call(&limit, &script, "a").await.unwrap();
        times(&script, start, &[0, 2]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_5_engine_retry_debits_the_same_known_window() {
    let script = Script::new();
    script.answer(503);
    script.answer(200).body("ok");
    let api = client(&script, Limit::new().bucket(bucket(1, 5))).with_retry(
        concord::Retry::none().tries(2).when(|tried| {
            (tried.status() == Some(StatusCode::SERVICE_UNAVAILABLE)).then_some(Duration::ZERO)
        }),
    );
    let start = Instant::now();
    assert_eq!(api.get().await.unwrap(), "ok");
    assert_eq!(script.requests().len(), 2);
    assert_eq!(
        script
            .requests()
            .iter()
            .map(|r| r.at() - start)
            .collect::<Vec<_>>(),
        [sec(0), sec(5)]
    );
    assert_eq!(
        script
            .requests()
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_zero_fixed_reset_refills_immediately_without_discovery() {
    let script = Script::new();
    script.never();
    script.answer(200);
    let limit = Limit::new().max_hold(sec(3)).bucket(
        Bucket::new(|_| Some("k".into())).window(Window::fixed(2, Duration::ZERO).remaining(0)),
    );
    let start = Instant::now();
    let mut first = Box::pin(call(&limit, &script, "a"));
    assert!(first.as_mut().now_or_never().is_none());
    call(&limit, &script, "a").await.unwrap();
    drop(first);
    let error = call(&limit, &script, "a").await.unwrap_err();
    assert!(
        matches!(cause(&error), LimitError::MaxHold { required: None, waited, .. }
        if *waited == sec(3))
    );
    times(&script, start, &[0, 0]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_invalid_static_policy_precedes_too_many_keys() {
    let script = Script::new();
    let policies = [
        Limit::new().max_keys(1).max_in_flight(0),
        Limit::new().max_keys(1).max_keys(0),
        Limit::new().max_keys(1).max_hold(Duration::MAX),
        Limit::new().max_keys(1).bucket(bucket(0, 1)),
        Limit::new().max_keys(1).bucket(bucket(1, 0)),
        Limit::new()
            .max_keys(1)
            .bucket(Bucket::new(|_| Some("x".repeat(513)))),
        Limit::new().max_keys(1).bucket(
            Bucket::new(|_| Some("k".into())).window(Window::rolling(1, sec(1)).remaining(2)),
        ),
        Limit::new().max_keys(1).bucket(
            (0..9).fold(Bucket::new(|_| Some("k".into())), |bucket, _| {
                bucket.window(Window::rolling(1, sec(1)))
            }),
        ),
    ];
    for policy in policies {
        let policy = policy.bucket(bucket(1, 1)).bucket(bucket(1, 1));
        let error = call(&policy, &script, "a").await.unwrap_err();
        assert!(matches!(cause(&error), LimitError::InvalidPolicy));
    }
    let error = call(
        &Limit::new()
            .max_keys(1)
            .bucket(bucket(1, 1))
            .bucket(bucket(1, 1)),
        &script,
        "a",
    )
    .await
    .unwrap_err();
    assert!(matches!(cause(&error), LimitError::TooManyKeys));
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_7_required_shrinks_while_retry_after_keeps_server_value() {
    let script = Script::new();
    script.answer(429).header("retry-after", "10");
    let limit = Limit::new().max_hold(sec(3));
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    tokio::time::advance(sec(4)).await;
    let error = call(&limit, &script, "a").await.unwrap_err();
    assert!(matches!(cause(&error), LimitError::MaxHold {
        required: Some(required), retry_after: Some(given), waited, ..
    } if *required == sec(6) && *given == sec(10) && waited.is_zero()));
    assert_eq!(start.elapsed(), sec(4));
    times(&script, start, &[0]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_initial_rolling_debt_expires() {
    let script = Script::new();
    script.answer(200);
    let limit = Limit::new()
        .max_hold(sec(2))
        .bucket(Bucket::new(|_| Some("k".into())).window(Window::rolling(1, sec(1)).remaining(0)));
    let start = Instant::now();
    let result = call(&limit, &script, "a").await;
    if let Err(error) = &result {
        assert!(matches!(cause(error), LimitError::MaxHold {
            required: Some(required), waited, ..
        } if *required == sec(1) && *waited == sec(2)));
        assert_eq!(script.requests().len(), 0);
        assert_eq!(start.elapsed(), sec(2));
    }
    result.unwrap();
    times(&script, start, &[1]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_initial_debt_anchor_survives_other_bucket_wait_and_commit() {
    let script = Script::new();
    for _ in 0..3 {
        script.answer(200);
    }
    let limit = Limit::new()
        .bucket(
            Bucket::new(|c| (c.info().endpoint() != "free").then(|| "a".into()))
                .window(Window::rolling(1, sec(2))),
        )
        .bucket(
            Bucket::new(|c| (c.info().endpoint() != "prime").then(|| "b".into()))
                .window(Window::rolling(2, sec(1)).remaining(1)),
        );
    let start = Instant::now();
    call(&limit, &script, "prime").await.unwrap();
    call(&limit, &script, "both").await.unwrap();
    call(&limit, &script, "free").await.unwrap();
    times(&script, start, &[0, 2, 2]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_idle_keys_are_reclaimed_over_time() {
    let script = Script::new();
    let names = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
    for _ in names {
        script.answer(200);
    }
    let limit = Limit::new().max_keys(2).bucket(
        Bucket::new(|call| Some(call.info().endpoint().into())).window(Window::rolling(1, sec(2))),
    );
    let start = Instant::now();
    for name in names {
        call(&limit, &script, name).await.unwrap();
        tokio::time::advance(sec(2)).await;
    }
    times(&script, start, &[0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_reclaimed_key_starts_with_its_static_windows() {
    let script = Script::new();
    for _ in 0..4 {
        script.answer(200);
    }
    let limit = Limit::new().max_keys(1).bucket(
        Bucket::new(|call| Some(call.info().endpoint().into())).window(Window::rolling(1, sec(5))),
    );
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    tokio::time::advance(sec(5)).await;
    call(&limit, &script, "b").await.unwrap();
    tokio::time::advance(sec(5)).await;
    call(&limit, &script, "a").await.unwrap();
    // The recreated key is governed by its original window, rather than
    // falling back to serial discovery and sending the fourth call at once.
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 5, 10, 15]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_the_4097th_debit_merges_at_the_second_oldest_expiry() {
    let script = Script::new();
    for _ in 0..4_098 {
        script.answer(200);
    }
    let limit = Limit::new().bucket(bucket(4_097, 10));
    let start = Instant::now();
    for _ in 0..4_097 {
        call(&limit, &script, "a").await.unwrap();
        tokio::time::advance(Duration::from_millis(1)).await;
    }
    call(&limit, &script, "a").await.unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 4_098);
    // With 4,096 slots, precisely the first two debits share the later expiry.
    // Keeping more history would send at 10 s; merging earlier would wait longer.
    assert_eq!(
        requests.last().unwrap().at() - start,
        sec(10) + Duration::from_millis(1)
    );
    assert_eq!(script.unused(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_7_waits_follow_explicit_advances_of_tokios_timer() {
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    let limit = Limit::new().bucket(bucket(1, 5));
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    let mut waiting = Box::pin(call(&limit, &script, "a"));
    assert!(waiting.as_mut().now_or_never().is_none());
    tokio::time::advance(sec(4)).await;
    assert!(waiting.as_mut().now_or_never().is_none());
    assert_eq!(script.requests().len(), 1);
    tokio::time::advance(sec(1)).await;
    assert!(waiting.as_mut().now_or_never().unwrap().is_ok());
    times(&script, start, &[0, 5]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_default_allows_1024_live_keys_across_buckets() {
    let script = Script::new();
    for _ in 0..512 {
        script.answer(200);
    }
    let definition = Bucket::new(|call| {
        Some(
            call.request()
                .headers()
                .get("x-tenant")?
                .to_str()
                .ok()?
                .into(),
        )
    })
    .window(Window::rolling(1, sec(10)));
    let limit = Limit::new().bucket(definition.clone()).bucket(definition);
    let start = Instant::now();
    for n in 0..512 {
        call_at(&limit, &script, "a", "one.test", &n.to_string())
            .await
            .unwrap();
    }
    let error = call_at(&limit, &script, "a", "one.test", "one-more")
        .await
        .unwrap_err();
    assert!(matches!(
        LimitError::find(&error),
        Some(LimitError::TooManyKeys)
    ));
    times(&script, start, &vec![0; 512]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_spent_rolling_or_fixed_budget_prevents_reclamation() {
    for fixed in [false, true] {
        let script = Script::new();
        script.answer(200);
        script.answer(200);
        let window = if fixed {
            Window::fixed(1, sec(5))
        } else {
            Window::rolling(1, sec(5))
        };
        let limit = Limit::new()
            .max_keys(1)
            .max_hold(sec(3))
            .bucket(Bucket::new(|call| Some(call.info().endpoint().into())).window(window));
        let start = Instant::now();
        call(&limit, &script, "a").await.unwrap();
        let error = call(&limit, &script, "b").await.unwrap_err();
        assert!(matches!(
            LimitError::find(&error),
            Some(LimitError::TooManyKeys)
        ));
        let error = call(&limit, &script, "a").await.unwrap_err();
        assert!(
            matches!(LimitError::find(&error), Some(LimitError::MaxHold { required: Some(wait), .. })
            if *wait == sec(5))
        );
        tokio::time::advance(sec(5)).await;
        call(&limit, &script, "b").await.unwrap();
        times(&script, start, &[0, 5]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_6_reactive_hold_prevents_reclamation_without_known_windows() {
    let script = Script::new();
    script.answer(429).header("retry-after", "5");
    script.answer(200);
    let limit = Limit::new()
        .max_keys(1)
        .max_hold(sec(3))
        .bucket(Bucket::new(|call| Some(call.info().endpoint().into())));
    let start = Instant::now();
    assert_eq!(call(&limit, &script, "a").await.unwrap().status(), 429);
    let error = call(&limit, &script, "b").await.unwrap_err();
    assert!(matches!(
        LimitError::find(&error),
        Some(LimitError::TooManyKeys)
    ));
    let error = call(&limit, &script, "a").await.unwrap_err();
    assert!(
        matches!(LimitError::find(&error), Some(LimitError::MaxHold { required: Some(wait), .. })
        if *wait == sec(5))
    );
    tokio::time::advance(sec(5)).await;
    call(&limit, &script, "b").await.unwrap();
    times(&script, start, &[0, 5]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_known_in_flight_and_first_discovery_keys_are_never_forgotten() {
    for known in [false, true] {
        let script = Script::new();
        script.never();
        script.answer(200);
        let definition = Bucket::new(|call| Some(call.info().endpoint().into()));
        let definition = if known {
            definition.window(Window::rolling(10, sec(1)))
        } else {
            definition
        };
        let limit = Limit::new().max_keys(1).bucket(definition);
        let start = Instant::now();
        let mut first = Box::pin(call(&limit, &script, "a"));
        assert!(first.as_mut().now_or_never().is_none());
        tokio::time::advance(sec(2)).await; // All local rolling debt has expired.
        let error = call(&limit, &script, "b").await.unwrap_err();
        assert!(matches!(
            LimitError::find(&error),
            Some(LimitError::TooManyKeys)
        ));
        drop(first);
        call(&limit, &script, "b").await.unwrap();
        times(&script, start, &[0, 2]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_6_body_in_flight_protects_key_until_drop_eof_or_error() {
    for ending in 0..3 {
        let script = Script::new();
        let answer = script.answer(200).body("head");
        match ending {
            0 => {
                answer.stall();
            }
            1 => {
                answer.pause(sec(2)).body("tail");
            }
            _ => {
                answer.pause(sec(2)).cut();
            }
        }
        script.answer(200);
        let limit = Limit::new().max_keys(1).bucket(
            Bucket::new(|call| Some(call.info().endpoint().into()))
                .window(Window::rolling(10, sec(1))),
        );
        let start = Instant::now();
        let mut raw: RawResponse = call(&limit, &script, "a").await.unwrap().into();
        tokio::time::advance(sec(2)).await;
        let error = call(&limit, &script, "b").await.unwrap_err();
        assert!(matches!(
            LimitError::find(&error),
            Some(LimitError::TooManyKeys)
        ));
        if ending == 0 {
            drop(raw);
            call(&limit, &script, "b").await.unwrap();
        } else {
            assert_eq!(
                next_frame(&mut raw)
                    .await
                    .unwrap()
                    .unwrap()
                    .into_data()
                    .unwrap(),
                "head"
            );
            if ending == 1 {
                assert_eq!(
                    next_frame(&mut raw)
                        .await
                        .unwrap()
                        .unwrap()
                        .into_data()
                        .unwrap(),
                    "tail"
                );
                assert!(next_frame(&mut raw).await.is_none());
            } else {
                assert!(next_frame(&mut raw).await.unwrap().unwrap_err().is_body());
            }
            call(&limit, &script, "b").await.unwrap();
            assert_eq!(raw.status(), 200); // Still alive after the other key is admitted.
            drop(raw);
        }
        times(&script, start, &[0, if ending == 0 { 2 } else { 4 }]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_6_reclaimed_learned_key_returns_to_discovery() {
    let script = Script::new();
    script.answer(200).header("x-seed", "yes");
    script.answer(200);
    script.never();
    script.answer(200);
    let limit = Limit::new().max_keys(1).bucket(
        Bucket::new(|call| Some(call.info().endpoint().into())).learn(|seen| {
            if seen.header("x-seed").is_some() {
                vec![Window::rolling(2, sec(1)).remaining(0)]
            } else {
                vec![]
            }
        }),
    );
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    tokio::time::advance(sec(1)).await;
    call(&limit, &script, "b").await.unwrap();
    let mut first = Box::pin(call(&limit, &script, "a"));
    let mut second = Box::pin(call(&limit, &script, "a"));
    assert!(first.as_mut().now_or_never().is_none());
    assert!(second.as_mut().now_or_never().is_none());
    assert_eq!(script.requests().len(), 3);
    drop(first);
    second.await.unwrap();
    times(&script, start, &[0, 1, 1, 1]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_without_retry_after_applies_only_to_unreadable_429_delay() {
    for (header, expected) in [
        (None, 2),
        (Some("bad"), 2),
        (Some("é"), 2),
        (Some("0"), 0),
        (Some("Sun, 06 Nov 1994 08:49:37 GMT"), 0),
        (Some("3"), 3),
    ] {
        let script = Script::new();
        let answer = script.answer(429).body("kept");
        if let Some(header) = header {
            answer.header("retry-after", header);
        } else {
            drop(answer);
        }
        script.answer(200);
        let limit = Limit::new().without_retry_after(sec(2));
        let start = Instant::now();
        let response = call(&limit, &script, "a").await.unwrap();
        assert_eq!(response.status(), 429);
        assert_eq!(response.text().await.unwrap(), "kept");
        call(&limit, &script, "b").await.unwrap();
        times(&script, start, &[0, expected]);
    }
    let script = Script::new();
    script.answer(503);
    script.answer(200);
    let limit = Limit::new().without_retry_after(sec(2));
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[0, 0]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_without_retry_after_keeps_bucket_selection_and_global_fallback() {
    for hit in [false, true] {
        let script = Script::new();
        script.answer(429);
        script.answer(200);
        script.answer(200);
        let limit = Limit::new().without_retry_after(sec(2)).bucket(
            Bucket::new(|call| (call.info().endpoint() == "a").then(|| "limited".into()))
                .hit_when(move |_| hit),
        );
        let start = Instant::now();
        call(&limit, &script, "a").await.unwrap();
        call(&limit, &script, "b").await.unwrap();
        call(&limit, &script, "a").await.unwrap();
        times(&script, start, &[0, if hit { 0 } else { 2 }, 2]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_7_fallback_max_hold_has_required_but_no_server_retry_after() {
    let script = Script::new();
    script.answer(429);
    let api = client(
        &script,
        Limit::new().without_retry_after(sec(2)).max_hold(sec(1)),
    );
    let start = Instant::now();
    let error = api.get().await.unwrap_err();
    assert!(error.is_status(429));
    assert_eq!(error.tries(), 1);
    let error = api.get().await.unwrap_err();
    assert!(
        matches!(LimitError::find(&error), Some(LimitError::MaxHold {
        required: Some(wait), retry_after: None, waited, ..
    }) if *wait == sec(2) && waited.is_zero())
    );
    assert_eq!(error.tries(), 1);
    assert_eq!(start.elapsed(), Duration::ZERO);
    times(&script, start, &[0]);
}

#[test]
fn c11_7_find_follows_sources_and_preserves_error_identity() {
    #[derive(Debug)]
    struct Outer(Box<dyn std::error::Error + Send + Sync>);
    impl std::fmt::Display for Outer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("outer")
        }
    }
    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&*self.0)
        }
    }
    let inner = Outer(Box::new(LimitError::InvalidPolicy));
    let expected = LimitError::find(&inner).unwrap() as *const LimitError;
    let outer = Outer(Box::new(inner));
    assert!(std::ptr::eq(LimitError::find(&outer).unwrap(), expected));
    let direct = LimitError::TooManyKeys;
    assert!(std::ptr::eq(LimitError::find(&direct).unwrap(), &direct));
    assert!(LimitError::find(&std::io::Error::other("private value")).is_none());
    #[derive(Debug)]
    struct Cycle;
    impl std::fmt::Display for Cycle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("cycle")
        }
    }
    impl std::error::Error for Cycle {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(self)
        }
    }
    assert!(LimitError::find(&Cycle).is_none());
}

#[tokio::test(start_paused = true)]
async fn c11_7_canceling_transport_releases_permit_before_peer_dispatch() {
    let script = Script::new();
    script.never();
    script.answer(200);
    let limit = Limit::new().max_in_flight(1);
    let start = Instant::now();
    let mut first = Box::pin(call(&limit, &script, "a"));
    let mut next = Box::pin(call(&limit, &script, "b"));
    assert!(first.as_mut().now_or_never().is_none());
    assert!(next.as_mut().now_or_never().is_none());
    assert_eq!(script.requests().len(), 1);
    drop(first);
    next.await.unwrap();
    times(&script, start, &[0, 0]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_concurrency_wait_has_max_hold_and_canceled_timeout_leaks_no_permit() {
    let script = Script::new();
    script.answer(200).body("held").stall();
    script.answer(200).body("next");
    let api = client(&script, Limit::new().max_in_flight(1).max_hold(sec(3)));
    let start = Instant::now();
    let held = api.stream().await.unwrap();
    let error = api.get().with_idle_timeout(sec(1)).await.unwrap_err();
    assert!(
        matches!(LimitError::find(&error), Some(LimitError::MaxHold {
        required: None, waited, ..
    }) if *waited == sec(3))
    );
    assert_eq!(error.tries(), 1);
    assert_eq!(start.elapsed(), sec(3));
    assert_eq!(script.requests().len(), 1);
    let error = api.get().with_timeout(sec(1)).await.unwrap_err();
    assert!(error.is_timeout());
    assert_eq!(error.tries(), 1);
    assert_eq!(start.elapsed(), sec(4));
    assert_eq!(script.requests().len(), 1);
    drop(held);
    assert_eq!(api.get().await.unwrap(), "next");
    times(&script, start, &[0, 4]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_max_hold_counts_the_time_of_the_key_function() {
    // A key function that takes two seconds.
    let slow = |window: Option<Window>| {
        let bucket = Bucket::new(|_| {
            assert!(tokio::time::advance(sec(2)).now_or_never().is_none());
            Some("k".into())
        });
        match window {
            Some(window) => bucket.window(window),
            None => bucket,
        }
    };

    // A request that needs no wait leaves, whatever its key function took: under a max_hold
    // of one second, it leaves at two.
    let script = Script::new();
    script.answer(200);
    let limit = Limit::new().max_hold(sec(1)).bucket(slow(None));
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    times(&script, start, &[2]);

    // A request that must wait may wait only until max_hold has passed since the limit
    // received it. The second request arrives at 2 s, its key function ends at 4 s, and the
    // window frees at 6 s, after its max_hold of 3 s ends at 5 s: it fails at once, at 4 s.
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    let limit = Limit::new()
        .max_hold(sec(3))
        .bucket(slow(Some(Window::rolling(1, sec(4)))));
    let start = Instant::now();
    call(&limit, &script, "a").await.unwrap();
    let error = call(&limit, &script, "a").await.unwrap_err();
    assert!(
        matches!(cause(&error), LimitError::MaxHold { required: Some(wait), waited, .. }
            if *wait == sec(2) && *waited == sec(2)),
        "{:?}",
        cause(&error)
    );
    assert_eq!(start.elapsed(), sec(4));
    times(&script, start, &[2]);
}
