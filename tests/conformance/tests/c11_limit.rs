//! C11.4 to C11.7: the limit, `concord::limit` (feature `limit`).
//!
//! `Limit`, `Bucket`, `Window`,
//! `Seen`, `counts`, `remaining` and `LimitError`, attached to a client of `concord::api!`
//! over a `concord_testkit::Script`, on paused `tokio` time. Callers keep response
//! metadata and share bounded quotas across calls, including after cancellation.

mod common;

use common::engine::{http_date, DateForm};
use concord::error::{Class, Kind};
use concord::limit::{counts, remaining, Bucket, Limit, LimitError, Window};
use concord::transport::TransportError;
use concord::{ByteStream, Retry};
use concord_testkit::Script;
use std::error::Error as _;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::task::Poll;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, timeout, Instant};

concord::api! {
    client Shop {
        base "https://shop.example.test/api";
        read concord::Text;
        GET item(id: u64) -> String { path "items/{id}"; }
        GET search() -> String { path "search"; }
        POST order(text: String) -> String {
            path "orders";
            body concord::Body::new(text.clone(), "text/plain");
        }
        GET download() -> ByteStream {
            path "download";
            read concord::read::Stream;
        }
    }
}

// What the tests share ----------------------------------------------------------------

/// The text that keys and header values hold, so that a printed line can be searched for it.
const MARK: &str = "pr1vate";

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn shop(script: &Script, limit: Limit) -> Shop {
    Shop::new()
        .with_middleware(limit)
        .with_transport(script.clone())
}

/// `n` answers `200 ok`.
fn oks(script: &Script, n: usize) {
    for _ in 0..n {
        script.answer(200).body("ok");
    }
}

/// When each request reached the script, from `start`.
fn sent(script: &Script, start: Instant) -> Vec<Duration> {
    script
        .requests()
        .iter()
        .map(|request| request.at() - start)
        .collect()
}

/// The endpoint of each request that reached the script.
fn endpoints(script: &Script) -> Vec<&'static str> {
    script
        .requests()
        .iter()
        .map(|request| request.info().endpoint())
        .collect()
}

/// A bucket charged once per call, keyed by the path of the request.
fn by_path() -> Bucket {
    Bucket::new(|call| Some(call.request().url().path().to_owned()))
}

/// A bucket charged once per call, with one key for every call.
fn one_key(key: &'static str) -> Bucket {
    Bucket::new(move |_| Some(key.to_owned()))
}

#[track_caller]
fn success<T, E>(result: Result<T, concord::Error<E>>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!(
            "expected a value, got an error {:?} (class {:?}, status {:?})",
            error.kind(),
            error.class(),
            error.status()
        ),
    }
}

#[track_caller]
fn failure<T, E>(result: Result<T, concord::Error<E>>) -> concord::Error<E> {
    match result {
        Ok(_) => panic!("expected an error, got a value"),
        Err(error) => error,
    }
}

/// The limit error that a call ended with, before anything was sent.
#[track_caller]
fn limit_error(error: &concord::Error) -> &LimitError {
    assert_eq!(error.kind(), Kind::Transport, "the kind of the error");
    assert_eq!(
        error.class(),
        Some(Class::Blocked),
        "the class of the error"
    );
    error
        .source()
        .and_then(|source| source.downcast_ref::<LimitError>())
        .expect("the source of the error is a LimitError")
}

/// Everything an error prints: its line, its `Debug`, and the same for each of its sources.
fn printed(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = format!("{error}\n{error:?}");
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(&format!("\n{cause}\n{cause:?}"));
        source = cause.source();
    }
    text
}

#[track_caller]
fn prints_no_value(text: &str) {
    assert!(!text.contains(MARK), "a printed text holds a value");
}

/// Every byte of a stream, and the error that ended it if any.
async fn drain(mut body: ByteStream) -> (Vec<u8>, bool) {
    let mut bytes = Vec::new();
    loop {
        match body.try_next().await {
            Ok(Some(chunk)) => bytes.extend_from_slice(&chunk),
            Ok(None) => return (bytes, false),
            Err(_) => return (bytes, true),
        }
    }
}

/// Seconds since 1970, from the clock of the machine.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("after 1970")
        .as_secs()
}

/// Drives a future to its end, and tells whether it panicked on the way.
async fn panics<F: Future>(future: F) -> bool {
    let mut future = Box::pin(future);
    std::future::poll_fn(|cx| {
        match std::panic::catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx))) {
            Ok(Poll::Ready(_)) => Poll::Ready(false),
            Ok(Poll::Pending) => Poll::Pending,
            Err(_) => Poll::Ready(true),
        }
    })
    .await
}

/// The `MaxHold` of an error, as `(required, retry_after, waited)`.
#[track_caller]
fn max_hold(error: &concord::Error) -> (Option<Duration>, Option<Duration>, Duration) {
    match limit_error(error) {
        LimitError::MaxHold {
            required,
            retry_after,
            waited,
            ..
        } => (*required, *retry_after, *waited),
        other => panic!("expected MaxHold, got {other:?}"),
    }
}

// C11.4 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_4_a_429_is_returned_as_it_is_and_sent_once() {
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", "5")
        .header("x-note", "kept")
        .body("slow down");
    oks(&script, 1);

    let error = failure(shop(&script, Limit::new()).item(1).await);

    assert!(error.is_status(429));
    assert_eq!(error.tries(), 1);
    assert_eq!(error.body(), Some(&b"slow down"[..]));
    assert_eq!(
        error.headers().and_then(|headers| headers.get("x-note")),
        Some("kept")
    );
    assert_eq!(error.retry_after(), Some(secs(5)));
    assert_eq!(script.requests().len(), 1, "the limit sends nothing again");
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_429_with_retry_after_and_no_bucket_holds_every_call() {
    let script = Script::new();
    script.answer(429).header("retry-after", "7");
    oks(&script, 2);
    let client = shop(&script, Limit::new());
    let start = Instant::now();

    failure(client.item(1).await);
    let (a, b) = tokio::join!(client.search().send(), client.item(2).send());
    success(a);
    success(b);

    assert_eq!(sent(&script, start), vec![secs(0), secs(7), secs(7)]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_429_without_a_readable_retry_after_holds_nothing() {
    // Zero, a date in the past, and a value that cannot be read hold nothing.
    let past = http_date(-30, DateForm::Imf);
    for retry_after in [
        None,
        Some("soon"),
        Some("-3"),
        Some("0"),
        Some(past.as_str()),
    ] {
        let script = Script::new();
        {
            let answer = script.answer(429);
            if let Some(value) = retry_after {
                answer.header("retry-after", value);
            }
        }
        oks(&script, 1);
        let client = shop(&script, Limit::new().bucket(one_key("all")));
        let start = Instant::now();

        failure(client.item(1).await);
        success(client.item(1).await);

        assert_eq!(
            sent(&script, start),
            vec![secs(0), secs(0)],
            "retry-after {retry_after:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_retry_after_that_is_an_http_date_holds_until_that_date() {
    // Read as the engine reads it. The date is in whole seconds of the wall clock, 10 s
    // ahead: the hold ends within the last second before it.
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", &http_date(10, DateForm::Imf));
    oks(&script, 1);
    let client = shop(&script, Limit::new().bucket(one_key("all")));
    let start = Instant::now();

    failure(client.item(1).await);
    success(client.item(1).await);

    let second = sent(&script, start)[1];
    assert!(second > secs(8) && second <= secs(10), "sent at {second:?}");
}

#[tokio::test(start_paused = true)]
async fn c11_4_without_retry_after_holds_a_429_that_has_no_readable_retry_after() {
    // The bucket that was hit is held for the wait given; another key is not.
    for retry_after in [None, Some("soon"), Some("-3")] {
        let script = Script::new();
        {
            let answer = script.answer(429);
            if let Some(value) = retry_after {
                answer.header("retry-after", value);
            }
        }
        oks(&script, 2);
        let limit = Limit::new().without_retry_after(secs(3)).bucket(by_path());
        let client = shop(&script, limit);
        let start = Instant::now();

        failure(client.item(1).await);
        success(client.item(2).await);
        success(client.item(1).await);

        assert_eq!(
            sent(&script, start),
            vec![secs(0), secs(0), secs(3)],
            "retry-after {retry_after:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c11_4_without_retry_after_changes_nothing_for_a_readable_retry_after() {
    // A readable value is used as it is; a readable zero or past date still holds nothing.
    let past = http_date(-30, DateForm::Imf);
    for (retry_after, held) in [("7", secs(7)), ("0", secs(0)), (past.as_str(), secs(0))] {
        let script = Script::new();
        script.answer(429).header("retry-after", retry_after);
        oks(&script, 1);
        let limit = Limit::new()
            .without_retry_after(secs(3))
            .bucket(one_key("all"));
        let client = shop(&script, limit);
        let start = Instant::now();

        failure(client.item(1).await);
        success(client.item(1).await);

        assert_eq!(
            sent(&script, start),
            vec![secs(0), held],
            "retry-after {retry_after:?}"
        );
    }

    // Only a 429 is held: a 503 with no `Retry-After` holds nothing.
    let script = Script::new();
    script.answer(503);
    oks(&script, 1);
    let client = shop(
        &script,
        Limit::new()
            .without_retry_after(secs(3))
            .bucket(one_key("all")),
    );
    let start = Instant::now();
    failure(client.item(1).await);
    success(client.item(1).await);
    assert_eq!(sent(&script, start), vec![secs(0), secs(0)]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_without_retry_after_uses_the_same_selectors_and_global_hold() {
    // No bucket at all: every call waits.
    let script = Script::new();
    script.answer(429);
    oks(&script, 1);
    let client = shop(&script, Limit::new().without_retry_after(secs(4)));
    let start = Instant::now();
    failure(client.item(1).await);
    success(client.search().await);
    assert_eq!(sent(&script, start), vec![secs(0), secs(4)]);

    // Every `hit_when` says no: every call waits.
    let script = Script::new();
    script.answer(429);
    oks(&script, 1);
    let limit = Limit::new()
        .without_retry_after(secs(4))
        .bucket(by_path().hit_when(|_| false));
    let client = shop(&script, limit);
    let start = Instant::now();
    failure(client.item(1).await);
    success(client.item(2).await);
    assert_eq!(sent(&script, start), vec![secs(0), secs(4)]);

    // `hit_when` chooses the bucket: the other one is not held.
    let script = Script::new();
    script.answer(429).header("x-rate-limit-type", "method");
    oks(&script, 2);
    let limit = Limit::new()
        .without_retry_after(secs(4))
        .bucket(
            one_key("application")
                .hit_when(|seen| seen.header("x-rate-limit-type") == Some("application")),
        )
        .bucket(
            Bucket::new(|call| Some(call.info().endpoint().to_owned()))
                .hit_when(|seen| seen.header("x-rate-limit-type") != Some("application")),
        );
    let client = shop(&script, limit);
    let start = Instant::now();
    failure(client.item(1).await);
    success(client.search().await);
    success(client.item(1).await);
    assert_eq!(sent(&script, start), vec![secs(0), secs(0), secs(4)]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_without_retry_after_reports_its_wait_in_max_hold_and_no_retry_after() {
    let script = Script::new();
    script.answer(429);
    oks(&script, 1);
    let client = shop(
        &script,
        Limit::new().without_retry_after(secs(3)).max_hold(secs(1)),
    );

    failure(client.item(1).await);
    let error = failure(client.item(1).await);

    assert_eq!(max_hold(&error), (Some(secs(3)), None, Duration::ZERO));
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_4_limit_error_find_reaches_the_typed_error_from_any_level() {
    // From the error of a generated call, from the cause below it, from the limit error
    // itself, and through an error of the caller's that keeps the call error as its source.
    #[derive(Debug)]
    struct App(concord::Error);
    impl std::fmt::Display for App {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("the application failed")
        }
    }
    impl std::error::Error for App {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }

    let script = Script::new();
    script.answer(429).header("retry-after", "30");
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_hold(secs(10)));
    let refused = failure(client.item(1).await);
    let error = failure(client.item(1).await);

    let found = LimitError::find(&error).expect("found from the call error");
    assert!(matches!(
        found,
        LimitError::MaxHold {
            required: Some(_),
            ..
        }
    ));
    let below = error.source().expect("the call error has a source");
    assert!(matches!(
        LimitError::find(below),
        Some(LimitError::MaxHold { .. })
    ));
    let typed = limit_error(&error);
    assert!(matches!(
        LimitError::find(typed),
        Some(LimitError::MaxHold { .. })
    ));
    let wrapped = App(error);
    assert!(matches!(
        LimitError::find(&wrapped),
        Some(LimitError::MaxHold { .. })
    ));

    // A call error that is not the limit's: nothing is found.
    assert!(refused.is_status(429));
    assert!(LimitError::find(&refused).is_none());
    assert!(LimitError::find(&std::io::Error::other("other")).is_none());
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_key_or_a_cost_that_panics_unwinds_its_call_and_leaves_the_limit_usable() {
    // A panic before admission takes no permit and changes no shared state.
    for which in ["key", "cost"] {
        let script = Script::new();
        oks(&script, 2);
        let limit = Limit::new().max_in_flight(1).bucket(
            Bucket::new(move |call| {
                if which == "key" && call.info().endpoint() == "shop.search" {
                    panic!("a key of the author's");
                }
                Some("all".to_owned())
            })
            .window(Window::rolling(1, secs(10)))
            .cost(move |call| {
                if which == "cost" && call.info().endpoint() == "shop.search" {
                    panic!("a cost of the author's");
                }
                1
            }),
        );
        let client = shop(&script, limit);
        let start = Instant::now();

        assert!(panics(client.search().send()).await, "{which}: unwinds");
        success(client.item(1).await);
        success(client.item(2).await);

        assert_eq!(endpoints(&script), vec!["shop.item"; 2], "{which}");
        // The panicking call charged nothing: the first item goes at once, the second waits
        // for the window that the first filled.
        assert_eq!(sent(&script, start), vec![secs(0), secs(10)], "{which}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_learner_that_panics_teaches_nothing_from_that_response() {
    // One learner says the window is full, the other panics: nothing from that response
    // is learned, by either bucket, and the response is returned.
    let script = Script::new();
    script
        .answer(200)
        .header("x-limit", "1:60")
        .header("x-count", "1:60")
        .body("ok");
    oks(&script, 1);
    let limit = Limit::new()
        .bucket(one_key("learns").learn(counts("x-limit", "x-count")))
        .bucket(one_key("panics").learn(|seen| {
            if seen.header("x-limit").is_some() {
                panic!("a learner of the author's");
            }
            Vec::new()
        }));
    let client = shop(&script, limit);
    let start = Instant::now();

    assert_eq!(success(client.item(1).await), "ok");
    success(client.item(1).await);

    assert_eq!(sent(&script, start), vec![secs(0), secs(0)]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_function_that_panics_after_the_response_never_hangs_another_call() {
    // A discovery whose learner or 429 selector panics still lets the next call through,
    // and a panicking selector counts as a hit.
    let script = Script::new();
    script.answer(200).after(secs(2)).body("first");
    oks(&script, 1);
    let limit = Limit::new().bucket(one_key("unknown").learn(|seen| {
        if seen.status() == 200 && seen.header("x-never").is_none() {
            panic!("a learner of the author's");
        }
        Vec::new()
    }));
    let client = shop(&script, limit);
    let start = Instant::now();
    let (a, b) = tokio::join!(client.item(1).send(), async {
        sleep(ms(1)).await;
        client.item(2).send().await
    });
    assert_eq!(success(a), "first");
    success(b);
    assert_eq!(sent(&script, start), vec![secs(0), secs(2)]);

    let script = Script::new();
    script.answer(429).header("retry-after", "5").body("slow");
    oks(&script, 2);
    let limit = Limit::new().bucket(by_path().hit_when(|_| panic!("a selector of the author's")));
    let client = shop(&script, limit);
    let start = Instant::now();
    let error = failure(client.item(1).await);
    assert!(error.is_status(429), "the response is returned");
    assert_eq!(error.body(), Some(&b"slow"[..]));
    success(client.item(2).await); // another key: the hit was this bucket, not every call
    success(client.item(1).await);
    assert_eq!(sent(&script, start), vec![secs(0), secs(0), secs(5)]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_bucket_without_hit_when_is_held_for_its_key_only() {
    let script = Script::new();
    script.answer(429).header("retry-after", "10");
    oks(&script, 2);
    let client = shop(&script, Limit::new().bucket(by_path()));
    let start = Instant::now();

    failure(client.item(1).await);
    success(client.item(2).await); // another key: not held
    success(client.item(1).await); // the key that was hit: held

    assert_eq!(sent(&script, start), vec![secs(0), secs(0), secs(10)]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_hit_when_chooses_which_bucket_is_held() {
    // An application bucket for every call, a method bucket per endpoint; the response
    // says which one was hit.
    for (kind, search_waits) in [("method", false), ("application", true)] {
        let script = Script::new();
        script
            .answer(429)
            .header("retry-after", "10")
            .header("x-rate-limit-type", kind);
        oks(&script, 2);
        let limit = Limit::new()
            .bucket(
                one_key("application")
                    .hit_when(|seen| seen.header("x-rate-limit-type") == Some("application")),
            )
            .bucket(
                Bucket::new(|call| Some(call.info().endpoint().to_owned()))
                    .hit_when(|seen| seen.header("x-rate-limit-type") != Some("application")),
            );
        let client = shop(&script, limit);
        let start = Instant::now();

        failure(client.item(1).await);
        success(client.search().await);
        success(client.item(1).await);

        let expected_search = if search_waits { secs(10) } else { secs(0) };
        assert_eq!(
            sent(&script, start),
            vec![secs(0), expected_search, secs(10)],
            "{kind}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_429_that_holds_no_bucket_holds_every_call() {
    // Every `hit_when` says no.
    let script = Script::new();
    script.answer(429).header("retry-after", "6");
    oks(&script, 1);
    let limit = Limit::new().bucket(by_path().hit_when(|_| false));
    let client = shop(&script, limit);
    let start = Instant::now();

    failure(client.item(1).await);
    success(client.item(2).await);

    assert_eq!(sent(&script, start), vec![secs(0), secs(6)]);

    // The call that was hit has no bucket (its key is `None`).
    let script = Script::new();
    script.answer(429).header("retry-after", "6");
    oks(&script, 1);
    let limit = Limit::new().bucket(Bucket::new(|call| {
        (call.info().endpoint() == "shop.item").then(|| "items".to_owned())
    }));
    let client = shop(&script, limit);
    let start = Instant::now();

    failure(client.search().await);
    success(client.item(1).await);

    assert_eq!(sent(&script, start), vec![secs(0), secs(6)]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_header_that_cannot_be_read_never_fails_the_call() {
    // Not UTF-8 text, a malformed pair, a pair with no count: nothing is learned, the
    // response is returned, and the next call goes.
    for (limits, used) in [
        ("20:1é", "1:1"),
        ("20:abc", "1:1"),
        ("20:1,100:120", "1:1"),
        ("", ""),
    ] {
        let script = Script::new();
        script
            .answer(200)
            .header("x-limit", limits)
            .header("x-count", used)
            .body("written");
        oks(&script, 1);
        let limit = Limit::new().bucket(one_key("all").learn(counts("x-limit", "x-count")));
        let client = shop(&script, limit);
        let start = Instant::now();

        assert_eq!(success(client.order("a".to_owned()).await), "written");
        success(client.item(1).await);

        assert_eq!(sent(&script, start), vec![secs(0), secs(0)], "{limits:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_learner_that_panics_never_fails_the_call() {
    let script = Script::new();
    script.answer(201).body("written");
    oks(&script, 1);
    let limit = Limit::new().bucket(one_key("all").learn(|seen| {
        if seen.status() == 201 {
            panic!("a learner of the author's");
        }
        Vec::new()
    }));
    let client = shop(&script, limit);

    assert_eq!(success(client.order("a".to_owned()).await), "written");
    success(client.item(1).await);
    assert_eq!(script.requests().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_valid_retry_after_still_holds_when_learning_fails() {
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", "4")
        .header("x-limit", "garbage")
        .header("x-count", "garbage");
    oks(&script, 1);
    let limit = Limit::new().bucket(one_key("all").learn(counts("x-limit", "x-count")));
    let client = shop(&script, limit);
    let start = Instant::now();

    failure(client.item(1).await);
    success(client.item(1).await);

    assert_eq!(sent(&script, start), vec![secs(0), secs(4)]);
}

#[tokio::test(start_paused = true)]
async fn c11_4_no_key_header_or_request_value_is_printed() {
    // Keys and headers that hold the mark; every error that the limit can give.
    let key = |call: &concord::limit::RequestInfo<'_>| {
        Some(format!("{MARK}-{}", call.request().url().path()))
    };
    let script = Script::new();
    script
        .answer(429)
        .header("retry-after", "3600")
        .header("x-private", MARK);
    let limit = Limit::new()
        .max_keys(1)
        .max_hold(secs(1))
        .bucket(Bucket::new(key).learn(|seen| {
            let _ = seen.header("x-private");
            Vec::new()
        }));
    prints_no_value(&format!("{limit:?}"));
    let client = shop(&script, limit);

    failure(client.item(1).await); // the 429
    let held = failure(client.item(1).await);
    assert!(matches!(limit_error(&held), LimitError::MaxHold { .. }));
    prints_no_value(&printed(&held));
    let too_many = failure(client.item(2).await);
    assert!(matches!(limit_error(&too_many), LimitError::TooManyKeys));
    prints_no_value(&printed(&too_many));

    let costly = failure(
        shop(
            &Script::new(),
            Limit::new().bucket(
                Bucket::new(key)
                    .window(Window::rolling(1, secs(1)))
                    .cost(|_| 2),
            ),
        )
        .item(1)
        .await,
    );
    assert!(matches!(
        limit_error(&costly),
        LimitError::CostExceedsCapacity
    ));
    prints_no_value(&printed(&costly));

    let bucket = Bucket::new(key).window(Window::fixed(5, secs(60)).remaining(4));
    prints_no_value(&format!("{bucket:?}"));
    prints_no_value(&format!("{:?}", Window::rolling(10, secs(1)).remaining(3)));
    assert_eq!(script.requests().len(), 1);
}

// C11.5 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_5_a_known_rolling_window_spaces_the_calls() {
    let script = Script::new();
    oks(&script, 5);
    let limit = Limit::new().bucket(one_key("all").window(Window::rolling(2, secs(10))));
    let client = shop(&script, limit);
    let start = Instant::now();

    for _ in 0..5 {
        success(client.item(1).await);
    }

    assert_eq!(
        sent(&script, start),
        vec![secs(0), secs(0), secs(10), secs(10), secs(20)]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_cost_equal_to_the_capacity_goes_and_one_above_is_refused() {
    let script = Script::new();
    oks(&script, 2);
    let limit = Limit::new().bucket(
        one_key("points")
            .window(Window::rolling(3, secs(10)))
            .cost(|_| 3),
    );
    let client = shop(&script, limit);
    let start = Instant::now();

    success(client.item(1).await);
    success(client.item(1).await);
    assert_eq!(sent(&script, start), vec![secs(0), secs(10)]);

    let script = Script::new();
    oks(&script, 1);
    let limit = Limit::new().bucket(
        one_key("points")
            .window(Window::rolling(3, secs(10)))
            .cost(|_| 4),
    );
    let error = failure(shop(&script, limit).item(1).await);
    assert!(matches!(
        limit_error(&error),
        LimitError::CostExceedsCapacity
    ));
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_call_that_waits_for_one_bucket_holds_nothing_in_another() {
    // `items` (1 per 10 s) charges only `item`; `all` (2 per 10 s) charges every call.
    // The second `item` waits for `items`; if it held its share of `all` while it waits,
    // the `search` behind it would wait too.
    let script = Script::new();
    oks(&script, 3);
    let limit = Limit::new()
        .bucket(
            Bucket::new(|call| (call.info().endpoint() == "shop.item").then(|| "items".to_owned()))
                .window(Window::rolling(1, secs(10))),
        )
        .bucket(one_key("all").window(Window::rolling(2, secs(10))));
    let client = shop(&script, limit);
    let start = Instant::now();

    success(client.item(1).await);
    let (waiting, other) = tokio::join!(client.item(2).send(), async {
        sleep(ms(1)).await;
        client.search().send().await
    });
    success(waiting);
    success(other);

    assert_eq!(
        endpoints(&script),
        vec!["shop.item", "shop.search", "shop.item"]
    );
    assert_eq!(sent(&script, start), vec![secs(0), ms(1), secs(10)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_the_cost_is_per_bucket() {
    // Points: a search costs 5, an item 1, 11 per minute. Requests: 1 each, 4 per minute.
    // Two searches and an item fill the points; the fourth call waits for them alone, with
    // a request left. If every cost were one, the fourth would go at once; if the requests
    // bucket were charged 5, a search would not fit it.
    let script = Script::new();
    oks(&script, 4);
    let limit = Limit::new()
        .bucket(
            one_key("points")
                .window(Window::rolling(11, secs(60)))
                .cost(|call| {
                    if call.info().endpoint() == "shop.search" {
                        5
                    } else {
                        1
                    }
                }),
        )
        .bucket(one_key("requests").window(Window::rolling(4, secs(60))));
    let client = shop(&script, limit);
    let start = Instant::now();

    success(client.search().await);
    success(client.search().await);
    success(client.item(1).await);
    success(client.item(2).await);

    assert_eq!(
        sent(&script, start),
        vec![secs(0), secs(0), secs(0), secs(60)]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_cost_of_zero_or_a_key_of_none_leaves_the_bucket_out() {
    let script = Script::new();
    oks(&script, 6);
    let limit = Limit::new().bucket(
        Bucket::new(|call| (call.info().endpoint() != "shop.item").then(|| "x".to_owned()))
            .window(Window::rolling(1, secs(10)))
            .cost(|call| u64::from(call.info().endpoint() != "shop.search")),
    );
    let client = shop(&script, limit);
    let start = Instant::now();

    for _ in 0..3 {
        success(client.search().await); // cost 0
        success(client.item(1).await); // key None
    }

    assert_eq!(sent(&script, start), vec![secs(0); 6]);

    // Left out of a bucket with no known window too: no call waits for a discovery.
    let script = Script::new();
    script.answer(200).after(secs(5)).body("slow");
    oks(&script, 1);
    let limit = Limit::new().bucket(one_key("unknown").cost(|_| 0));
    let client = shop(&script, limit);
    let start = Instant::now();

    let (a, b) = tokio::join!(client.item(1).send(), client.item(2).send());
    success(a);
    success(b);
    assert_eq!(sent(&script, start), vec![secs(0), secs(0)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_cost_is_not_given_back_after_the_request_left() {
    // A failed send.
    let script = Script::new();
    script.fail(TransportError::connect());
    oks(&script, 1);
    let limit = Limit::new().bucket(one_key("all").window(Window::rolling(1, secs(10))));
    let client = shop(&script, limit);
    let start = Instant::now();

    failure(client.item(1).await);
    success(client.item(1).await);
    assert_eq!(sent(&script, start), vec![secs(0), secs(10)]);

    // A call dropped while it was sent.
    let script = Script::new();
    script.never();
    oks(&script, 1);
    let limit = Limit::new().bucket(one_key("all").window(Window::rolling(1, secs(10))));
    let client = shop(&script, limit);
    let start = Instant::now();

    assert!(timeout(secs(2), client.item(1).send()).await.is_err());
    success(client.item(1).await);
    assert_eq!(sent(&script, start), vec![secs(0), secs(10)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_what_a_response_shows_lowers_what_is_left() {
    // Known: 10 per minute. The server says all 10 are used.
    let script = Script::new();
    script
        .answer(200)
        .header("x-limit", "10:60")
        .header("x-count", "10:60")
        .body("ok");
    oks(&script, 1);
    let limit = Limit::new().bucket(
        one_key("all")
            .window(Window::rolling(10, secs(60)))
            .learn(counts("x-limit", "x-count")),
    );
    let client = shop(&script, limit);
    let start = Instant::now();

    success(client.item(1).await);
    success(client.item(1).await);

    // "All debt lasts a full period because phase is unknown."
    assert_eq!(sent(&script, start), vec![secs(0), secs(60)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_what_a_response_shows_never_raises_what_is_left() {
    // Known: 2 per minute. Each response says that nothing is used.
    let script = Script::new();
    for _ in 0..3 {
        script
            .answer(200)
            .header("x-limit", "2:60")
            .header("x-count", "0:60")
            .body("ok");
    }
    let limit = Limit::new().bucket(
        one_key("all")
            .window(Window::rolling(2, secs(60)))
            .learn(counts("x-limit", "x-count")),
    );
    let client = shop(&script, limit);
    let start = Instant::now();

    for _ in 0..3 {
        success(client.item(1).await);
    }

    assert_eq!(sent(&script, start), vec![secs(0), secs(0), secs(60)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_late_response_does_not_reopen_a_spent_budget() {
    // Two calls in flight under 2 per minute; the first answers late that 0 are used.
    let script = Script::new();
    script
        .answer(200)
        .after(secs(5))
        .header("x-limit", "2:60")
        .header("x-count", "0:60")
        .body("late");
    oks(&script, 2);
    let limit = Limit::new().bucket(
        one_key("all")
            .window(Window::rolling(2, secs(60)))
            .learn(counts("x-limit", "x-count")),
    );
    let client = shop(&script, limit);
    let start = Instant::now();

    let (a, b) = tokio::join!(client.item(1).send(), client.item(2).send());
    success(a);
    success(b);
    success(client.item(3).await);

    assert_eq!(sent(&script, start), vec![secs(0), secs(0), secs(60)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_bucket_with_no_known_window_lets_one_call_through_at_a_time() {
    // Each response takes 5 s: if the second and the third were let through together, the
    // third would go at 5 s.
    let script = Script::new();
    script.answer(200).after(secs(5)).body("first");
    script.answer(200).after(secs(5)).body("second");
    oks(&script, 1);
    let client = shop(&script, Limit::new().bucket(one_key("unknown")));
    let start = Instant::now();

    let (a, b, c) = tokio::join!(
        client.item(1).send(),
        client.item(2).send(),
        client.item(3).send()
    );
    success(a);
    success(b);
    success(c);

    let times = sent(&script, start);
    assert_eq!(times[0], secs(0));
    assert_eq!(times[1], secs(5), "the second waits for the first response");
    assert_eq!(
        times[2],
        secs(10),
        "the third waits for the response to the second"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_discovery_lets_the_next_call_through_at_the_head_not_at_the_end_of_its_body() {
    // The head arrives at once, the body ends 10 s later.
    let script = Script::new();
    script.answer(200).body("a").pause(secs(10)).body("b");
    oks(&script, 1);
    let limit = Limit::new().bucket(one_key("unknown").learn(counts("x-limit", "x-count")));
    let client = shop(&script, limit);
    let start = Instant::now();

    let (a, b) = tokio::join!(client.item(1).send(), async {
        sleep(ms(1)).await;
        client.item(2).send().await
    });
    assert_eq!(success(a), "ab");
    success(b);

    assert_eq!(sent(&script, start), vec![secs(0), ms(1)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_discovery_that_fails_lets_the_next_call_through() {
    // A failure before the head releases the next call. Were it not released, the next
    // call would wait for `max_hold`.
    let script = Script::new();
    script.fail(TransportError::connect());
    oks(&script, 1);
    let client = shop(&script, Limit::new().bucket(one_key("unknown")));
    let start = Instant::now();

    failure(client.item(1).await);
    success(client.item(2).await);

    assert_eq!(sent(&script, start), vec![secs(0), secs(0)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_an_attempt_that_the_engine_makes_again_waits_behind_the_same_gates() {
    // The second attempt of a retried call is charged and waits like any other call.
    let script = Script::new();
    script.answer(503).body("busy");
    oks(&script, 1);
    let limit = Limit::new().bucket(one_key("all").window(Window::rolling(1, secs(10))));
    let client = shop(&script, limit).with_retry(Retry::standard());
    let start = Instant::now();

    success(client.item(1).await);

    let attempts: Vec<u32> = script
        .requests()
        .iter()
        .map(|request| request.info().number())
        .collect();
    assert_eq!(attempts, vec![1, 2]);
    assert_eq!(sent(&script, start), vec![secs(0), secs(10)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_fixed_window_whose_reset_has_passed_is_whole_again() {
    // 2 until a reset at 10 s. At 10 s the budget is whole: two calls go together, not one
    // at a time as for a bucket with no known window (each response takes 1 s). Once that
    // budget is spent, no further reset is known: the wait is unknown and ends at `max_hold`.
    let script = Script::new();
    oks(&script, 2);
    script.answer(200).after(secs(1)).body("third");
    script.answer(200).after(secs(1)).body("fourth");
    oks(&script, 1);
    let limit = Limit::new()
        .max_hold(secs(15))
        .bucket(one_key("all").window(Window::fixed(2, secs(10))));
    let client = shop(&script, limit);
    let start = Instant::now();

    success(client.item(1).await);
    success(client.item(2).await);
    let (c, d) = tokio::join!(client.item(3).send(), client.item(4).send());
    success(c);
    success(d);
    assert_eq!(
        sent(&script, start),
        vec![secs(0), secs(0), secs(10), secs(10)]
    );

    let error = failure(client.item(5).await);
    assert_eq!(max_hold(&error), (None, None, secs(15)));
    assert_eq!(start.elapsed(), secs(11) + secs(15));
    assert_eq!(script.requests().len(), 4);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_spent_learned_fixed_refill_allows_discovery_again() {
    let script = Script::new();
    script
        .answer(200)
        .header("x-limit", "2")
        .header("x-left", "0")
        .header("x-reset", &(unix_now() + 30).to_string())
        .body("learned");
    script.fail(TransportError::connect());
    script.fail(TransportError::connect());
    oks(&script, 1);
    let limit = Limit::new()
        .max_hold(secs(3))
        .bucket(one_key("all").learn(remaining("x-limit", "x-left", "x-reset")));
    let client = shop(&script, limit);
    let start = Instant::now();

    success(client.item(1).await);
    sleep(secs(31)).await;
    for _ in 0..2 {
        let error = failure(client.item(1).await);
        assert_eq!(error.class(), Some(Class::Connect));
        assert_eq!(error.tries(), 1);
    }
    let value = client
        .item(1)
        .await
        .unwrap_or_else(|error| panic!("expected discovery, got {:?}", LimitError::find(&error)));

    assert_eq!(value, "ok");
    assert_eq!(
        sent(&script, start),
        vec![secs(0), secs(31), secs(31), secs(31)]
    );
    assert_eq!(script.unused(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_learned_fixed_refill_too_small_for_the_cost_allows_discovery() {
    let script = Script::new();
    script
        .answer(200)
        .header("x-limit", "3")
        .header("x-left", "0")
        .header("x-reset", &(unix_now() + 30).to_string());
    script.fail(TransportError::connect());
    oks(&script, 1);
    let client = shop(
        &script,
        Limit::new().max_hold(secs(3)).bucket(
            one_key("all")
                .cost(|_| 2)
                .learn(remaining("x-limit", "x-left", "x-reset")),
        ),
    );
    let start = Instant::now();

    success(client.item(1).await);
    sleep(secs(31)).await;
    assert_eq!(failure(client.item(1).await).class(), Some(Class::Connect));
    let value = client
        .item(1)
        .await
        .unwrap_or_else(|error| panic!("expected discovery, got {:?}", LimitError::find(&error)));

    assert_eq!(value, "ok");
    assert_eq!(sent(&script, start), vec![secs(0), secs(31), secs(31)]);
    assert_eq!(script.unused(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_cost_above_a_learned_fixed_capacity_is_still_refused() {
    let script = Script::new();
    script
        .answer(200)
        .header("x-limit", "3")
        .header("x-left", "0")
        .header("x-reset", &(unix_now() + 30).to_string());
    script.fail(TransportError::connect());
    oks(&script, 1);
    let client = shop(
        &script,
        Limit::new().max_hold(secs(3)).bucket(
            one_key("all")
                .cost(|call| {
                    if call.info().endpoint() == "shop.search" {
                        4
                    } else {
                        2
                    }
                })
                .learn(remaining("x-limit", "x-left", "x-reset")),
        ),
    );
    let start = Instant::now();

    success(client.item(1).await);
    for stage in 0..3 {
        if stage == 1 {
            sleep(secs(31)).await;
        } else if stage == 2 {
            assert_eq!(failure(client.item(1).await).class(), Some(Class::Connect));
        }
        let error = failure(client.search().await);
        assert!(matches!(
            limit_error(&error),
            LimitError::CostExceedsCapacity
        ));
        assert_eq!(error.tries(), 1);
        assert_eq!(sent(&script, start).len(), if stage == 2 { 2 } else { 1 });
    }

    assert_eq!(start.elapsed(), secs(31), "an oversized cost never waits");
    assert_eq!(
        script.unused(),
        1,
        "an oversized cost never reaches the transport"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_fixed_rediscovery_is_serial_releases_on_failure_and_learns_again() {
    let script = Script::new();
    script
        .answer(200)
        .header("x-limit", "2")
        .header("x-left", "0")
        .header("x-reset", &(unix_now() + 30).to_string());
    script.fail(TransportError::connect());
    script.fail(TransportError::connect());
    script.fail(TransportError::connect()).after(secs(5));
    script
        .answer(200)
        .after(secs(5))
        .header("x-limit", "2")
        .header("x-left", "2")
        .header("x-reset", &(unix_now() + 60).to_string());
    script.answer(200).after(secs(1));
    script.answer(200).after(secs(1));
    let client = shop(
        &script,
        Limit::new()
            .max_hold(secs(15))
            .bucket(one_key("all").learn(remaining("x-limit", "x-left", "x-reset"))),
    );
    let start = Instant::now();

    success(client.item(1).await);
    sleep(secs(31)).await;
    for _ in 0..2 {
        assert_eq!(failure(client.item(1).await).class(), Some(Class::Connect));
    }
    let (a, b, c, d) = tokio::join!(
        client.item(1).send(),
        client.item(1).send(),
        client.item(1).send(),
        client.item(1).send()
    );
    let mut failures = 0;
    for result in [a, b, c, d] {
        if let Err(error) = result {
            assert_eq!(error.class(), Some(Class::Connect));
            failures += 1;
        }
    }
    assert_eq!(failures, 1);

    assert_eq!(
        sent(&script, start),
        vec![
            secs(0),
            secs(31),
            secs(31),
            secs(31),
            secs(36),
            secs(41),
            secs(41)
        ]
    );
    assert_eq!(script.unused(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_fixed_refill_with_new_observations_stays_known() {
    let script = Script::new();
    script.answer(200).header("x-left", "0");
    for _ in 0..2 {
        script.answer(200).after(secs(1)).header("x-left", "2");
    }
    oks(&script, 1);
    let client = shop(
        &script,
        Limit::new()
            .max_hold(secs(15))
            .bucket(one_key("all").learn(|seen| {
                seen.header("x-left")
                    .and_then(|left| left.parse().ok())
                    .map(|left| vec![Window::fixed(2, secs(10)).remaining(left)])
                    .unwrap_or_default()
            })),
    );
    let start = Instant::now();

    success(client.item(1).await);
    let (a, b) = tokio::join!(client.item(1).send(), client.item(1).send());
    success(a);
    success(b);
    success(client.item(1).await);

    assert_eq!(
        sent(&script, start),
        vec![secs(0), secs(10), secs(10), secs(21)]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_fixed_refill_waits_for_outstanding_heads_before_forgetting() {
    let script = Script::new();
    script.answer(200).header("x-left", "0");
    for _ in 0..2 {
        script.answer(200).after(secs(5)).header("x-left", "2");
    }
    oks(&script, 1);
    let client = shop(
        &script,
        Limit::new()
            .max_hold(secs(20))
            .bucket(one_key("all").learn(|seen| {
                seen.header("x-left")
                    .and_then(|left| left.parse().ok())
                    .map(|left| vec![Window::fixed(2, secs(10)).remaining(left)])
                    .unwrap_or_default()
            })),
    );
    let start = Instant::now();

    success(client.item(1).await);
    sleep(secs(10)).await;
    let (a, b, waiting) = tokio::join!(client.item(1).send(), client.item(1).send(), async {
        sleep(secs(1)).await;
        client.item(1).send().await
    });
    success(a);
    success(b);
    success(waiting);

    assert_eq!(
        sent(&script, start),
        vec![secs(0), secs(10), secs(10), secs(25)]
    );
    assert_eq!(script.unused(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_5_forgetting_a_fixed_refill_preserves_other_windows() {
    let script = Script::new();
    script.answer(200).header("x-budget", "present");
    script.fail(TransportError::connect());
    script.answer(200).header("x-budget", "new");
    oks(&script, 1);
    let client = shop(
        &script,
        Limit::new().bucket(one_key("all").learn(|seen| {
            if seen.header("x-budget").is_some() {
                vec![
                    Window::fixed(
                        1,
                        secs(if seen.header("x-budget") == Some("new") {
                            12
                        } else {
                            5
                        }),
                    )
                    .remaining(0),
                    Window::rolling(2, secs(20)).remaining(1),
                ]
            } else {
                vec![]
            }
        })),
    );
    let start = Instant::now();

    success(client.item(1).await);
    sleep(secs(5)).await;
    assert_eq!(failure(client.item(1).await).class(), Some(Class::Connect));
    success(client.item(1).await);
    success(client.item(1).await);

    assert_eq!(
        sent(&script, start),
        vec![secs(0), secs(5), secs(20), secs(32)]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_discovery_call_that_is_dropped_lets_the_next_through() {
    let script = Script::new();
    script.never();
    oks(&script, 1);
    let client = shop(&script, Limit::new().bucket(one_key("unknown")));
    let start = Instant::now();

    let (dropped, next) = tokio::join!(timeout(secs(3), client.item(1).send()), async {
        sleep(ms(1)).await;
        client.item(2).send().await
    });
    assert!(dropped.is_err());
    success(next);

    assert_eq!(sent(&script, start), vec![secs(0), secs(3)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_counts_learns_every_window_of_a_pair_of_headers() {
    // 2 per second and 3 per 10 seconds; 2 and 1 used: the 1-second window is full, the
    // 10-second window has 2 left. The second and third calls go at 1 s, when the short
    // window frees; they spend the long window, so the fourth waits for it, until 10 s. If
    // the long window were ignored the fourth would go at 2 s; if the short one were, the
    // second would go at once.
    let script = Script::new();
    script
        .answer(200)
        .header("x-app-rate-limit", "2:1,3:10")
        .header("x-app-rate-limit-count", "2:1,1:10")
        .body("ok");
    oks(&script, 3);
    let limit = Limit::new()
        .bucket(one_key("application").learn(counts("x-app-rate-limit", "x-app-rate-limit-count")));
    let client = shop(&script, limit);
    let start = Instant::now();

    for _ in 0..4 {
        success(client.item(1).await);
    }

    assert_eq!(
        sent(&script, start),
        vec![secs(0), secs(1), secs(1), secs(10)]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_remaining_learns_a_budget_until_a_unix_reset() {
    let script = Script::new();
    let reset = unix_now() + 30;
    script
        .answer(200)
        .header("x-ratelimit-limit", "100")
        .header("x-ratelimit-remaining", "0")
        .header("x-ratelimit-reset", &reset.to_string())
        .body("ok");
    oks(&script, 1);
    let limit = Limit::new().bucket(one_key("points").learn(remaining(
        "x-ratelimit-limit",
        "x-ratelimit-remaining",
        "x-ratelimit-reset",
    )));
    let client = shop(&script, limit);
    let start = Instant::now();

    success(client.item(1).await);
    success(client.item(1).await);

    // A reset in whole seconds of the wall clock: within the last second before it.
    let second = sent(&script, start)[1];
    assert!(
        second > secs(29) && second <= secs(30),
        "sent at {second:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_window_that_cannot_hold_a_call_is_refused_before_sending() {
    for window in [
        Window::rolling(0, secs(1)),
        Window::rolling(1, Duration::ZERO),
        Window::fixed(0, secs(60)),
        Window::fixed(5, secs(60)).remaining(6),
    ] {
        let script = Script::new();
        oks(&script, 1);
        let limit = Limit::new().bucket(one_key("all").window(window));

        let error = failure(shop(&script, limit).item(1).await);

        assert!(matches!(limit_error(&error), LimitError::InvalidPolicy));
        assert_eq!(script.requests().len(), 0);
    }
}

// C11.6 ------------------------------------------------------------------------------

/// The URLs that reached the script, in order.
fn urls(script: &Script) -> Vec<String> {
    script
        .requests()
        .iter()
        .map(|request| request.url().as_str().to_owned())
        .collect()
}

fn item_url(id: u64) -> String {
    format!("https://shop.example.test/api/items/{id}")
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_key_that_holds_nothing_is_forgotten_when_room_is_needed() {
    // Two keys at most; each call has ended (its body read), and no window is known.
    let script = Script::new();
    oks(&script, 6);
    let client = shop(&script, Limit::new().max_keys(2).bucket(by_path()));

    for id in [1, 2, 3, 1, 2, 3] {
        success(client.item(id).await);
    }

    assert_eq!(
        urls(&script),
        [1, 2, 3, 1, 2, 3].map(item_url).to_vec(),
        "every call went"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_client_that_touches_many_more_keys_than_max_keys_one_after_another_never_fails() {
    // 4 keys at most, 100 keys touched one after another, with no known window.
    let script = Script::new();
    oks(&script, 100);
    let client = shop(&script, Limit::new().max_keys(4).bucket(by_path()));
    for id in 0..100 {
        success(client.item(id).await);
    }
    assert_eq!(script.requests().len(), 100);

    // The same with a known window per key: each key's spent budget has expired when the
    // next key comes, one second later. No call waits.
    let script = Script::new();
    oks(&script, 100);
    let limit = Limit::new()
        .max_keys(4)
        .bucket(by_path().window(Window::rolling(1, secs(1))));
    let client = shop(&script, limit);
    let start = Instant::now();
    for id in 0..100 {
        success(client.item(id).await);
        sleep(secs(1)).await;
    }
    let expected: Vec<Duration> = (0..100).map(secs).collect();
    assert_eq!(sent(&script, start), expected);
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_spent_learned_fixed_refill_can_be_forgotten() {
    for failed_discovery in [false, true] {
        let script = Script::new();
        script
            .answer(200)
            .header("x-limit", "1")
            .header("x-left", "0")
            .header("x-reset", &(unix_now() + 30).to_string());
        script.fail(TransportError::connect());
        if failed_discovery {
            script.fail(TransportError::connect());
        }
        oks(&script, 1);
        let client = shop(
            &script,
            Limit::new()
                .max_keys(1)
                .max_hold(secs(3))
                .bucket(by_path().learn(remaining("x-limit", "x-left", "x-reset"))),
        );
        let start = Instant::now();

        success(client.item(1).await);
        sleep(secs(31)).await;
        assert_eq!(failure(client.item(1).await).class(), Some(Class::Connect));
        if failed_discovery {
            assert_eq!(failure(client.item(1).await).class(), Some(Class::Connect));
        }
        success(client.item(2).await);

        assert_eq!(start.elapsed(), secs(31));
        assert_eq!(
            script.requests().len(),
            if failed_discovery { 4 } else { 3 }
        );
        assert_eq!(script.unused(), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_6_the_default_is_1024_keys() {
    // Every key holds spent budget for a minute: 1024 keys fit, the 1025th is refused.
    let script = Script::new();
    oks(&script, 1025);
    let client = shop(
        &script,
        Limit::new().bucket(by_path().window(Window::rolling(1, secs(60)))),
    );

    for id in 0..1024 {
        success(client.item(id).await);
    }
    let error = failure(client.item(1024).await);

    assert!(matches!(limit_error(&error), LimitError::TooManyKeys));
    assert_eq!(script.requests().len(), 1024);
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_key_with_spent_budget_that_has_not_expired_is_never_forgotten() {
    let script = Script::new();
    oks(&script, 3);
    let limit = Limit::new()
        .max_keys(1)
        .bucket(by_path().window(Window::rolling(1, secs(10))));
    let client = shop(&script, limit);
    let start = Instant::now();

    success(client.item(1).await);
    let error = failure(client.item(2).await);
    assert!(matches!(limit_error(&error), LimitError::TooManyKeys));

    // At 10 s the budget of the first key has expired: it holds nothing and is forgotten.
    sleep(secs(10)).await;
    success(client.item(2).await);
    let error = failure(client.item(1).await);
    assert!(matches!(limit_error(&error), LimitError::TooManyKeys));
    sleep(secs(10)).await;
    success(client.item(1).await);

    assert_eq!(urls(&script), [1, 2, 1].map(item_url).to_vec());
    assert_eq!(sent(&script, start), vec![secs(0), secs(10), secs(20)]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_key_held_by_a_429_is_never_forgotten() {
    // With `Retry-After`, and with the wait of `without_retry_after`.
    for retry_after in [Some("30"), None] {
        let script = Script::new();
        {
            let answer = script.answer(429);
            if let Some(value) = retry_after {
                answer.header("retry-after", value);
            }
        }
        oks(&script, 1);
        let limit = Limit::new()
            .max_keys(1)
            .without_retry_after(secs(30))
            .bucket(by_path());
        let client = shop(&script, limit);
        let start = Instant::now();

        failure(client.item(1).await);
        let error = failure(client.item(2).await);
        assert!(
            matches!(limit_error(&error), LimitError::TooManyKeys),
            "retry-after {retry_after:?}"
        );
        sleep(secs(30)).await;
        success(client.item(2).await);

        assert_eq!(
            sent(&script, start),
            vec![secs(0), secs(30)],
            "retry-after {retry_after:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_key_with_a_call_in_flight_is_never_forgotten() {
    // The window of 1 ms frees the spent budget at once: only the call itself holds the key.
    let script = Script::new();
    script.answer(200).after(secs(10)).body("slow");
    oks(&script, 1);
    let limit = Limit::new()
        .max_keys(1)
        .bucket(by_path().window(Window::rolling(1000, ms(1))));
    let client = shop(&script, limit);
    let start = Instant::now();

    let (first, second) = tokio::join!(client.item(1).send(), async {
        sleep(secs(5)).await;
        client.item(2).send().await
    });
    success(first);
    assert!(matches!(
        limit_error(&failure(second)),
        LimitError::TooManyKeys
    ));
    success(client.item(2).await);

    assert_eq!(urls(&script), [1, 2].map(item_url).to_vec());
    assert_eq!(sent(&script, start), vec![secs(0), secs(10)]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_key_whose_body_is_not_read_yet_is_never_forgotten() {
    // The call is in flight until its body ends or is dropped.
    let script = Script::new();
    script.answer(200).body("a").stall();
    oks(&script, 1);
    let limit = Limit::new()
        .max_keys(1)
        .bucket(by_path().window(Window::rolling(1000, ms(1))));
    let client = shop(&script, limit);

    let body = success(client.download().await);
    sleep(secs(1)).await;
    assert!(matches!(
        limit_error(&failure(client.item(1).await)),
        LimitError::TooManyKeys
    ));
    drop(body);
    success(client.item(1).await);

    assert_eq!(endpoints(&script), vec!["shop.download", "shop.item"]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_key_whose_first_call_is_not_answered_is_never_forgotten() {
    // No known window: the key is discovering, waiting for its first response.
    let script = Script::new();
    script.answer(200).after(secs(10)).body("first");
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_keys(1).bucket(by_path()));
    let start = Instant::now();

    let (first, second) = tokio::join!(client.item(1).send(), async {
        sleep(secs(5)).await;
        client.item(2).send().await
    });
    success(first);
    assert!(matches!(
        limit_error(&failure(second)),
        LimitError::TooManyKeys
    ));
    success(client.item(2).await);

    assert_eq!(sent(&script, start), vec![secs(0), secs(10)]);
}

#[tokio::test(start_paused = true)]
async fn c11_6_when_every_key_holds_something_one_more_is_a_typed_error_before_sending() {
    let script = Script::new();
    oks(&script, 4);
    let limit = Limit::new()
        .max_keys(3)
        .bucket(by_path().window(Window::rolling(1, secs(60))));
    let client = shop(&script, limit);

    for id in 1..=3 {
        success(client.item(id).await);
    }
    let error = failure(client.item(4).await);

    assert!(matches!(limit_error(&error), LimitError::TooManyKeys));
    assert!(matches!(
        LimitError::find(&error),
        Some(LimitError::TooManyKeys)
    ));
    assert_eq!(script.requests().len(), 3, "nothing is sent");
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_static_rule_that_is_broken_wins_over_one_key_too_many() {
    // Two keys for one key of room, and one bucket's window is invalid.
    let script = Script::new();
    oks(&script, 1);
    let limit = Limit::new()
        .max_keys(1)
        .bucket(one_key("a"))
        .bucket(one_key("b").window(Window::rolling(0, secs(1))));
    let error = failure(shop(&script, limit).item(1).await);
    assert!(matches!(limit_error(&error), LimitError::InvalidPolicy));
    assert_eq!(script.requests().len(), 0);

    // The only room is held, and the new key is longer than 512 bytes.
    let script = Script::new();
    oks(&script, 2);
    let limit = Limit::new().max_keys(1).bucket(
        Bucket::new(|call| {
            Some(if call.info().endpoint() == "shop.item" {
                "short".to_owned()
            } else {
                "x".repeat(513)
            })
        })
        .window(Window::rolling(1, secs(60))),
    );
    let client = shop(&script, limit);
    success(client.item(1).await);
    let error = failure(client.search().await);
    assert!(matches!(limit_error(&error), LimitError::InvalidPolicy));
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_6_the_keys_of_two_buckets_count_together() {
    // The same text from two definitions is two keys.
    let script = Script::new();
    oks(&script, 1);
    let limit = Limit::new()
        .max_keys(1)
        .bucket(one_key("same"))
        .bucket(one_key("same"));

    let error = failure(shop(&script, limit).item(1).await);

    assert!(matches!(limit_error(&error), LimitError::TooManyKeys));
    assert_eq!(
        script.requests().len(),
        0,
        "nothing is charged, nothing is sent"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_6_zero_max_keys_is_an_invalid_policy() {
    let script = Script::new();
    oks(&script, 1);

    let error = failure(
        shop(&script, Limit::new().max_keys(0).bucket(by_path()))
            .item(1)
            .await,
    );

    assert!(matches!(limit_error(&error), LimitError::InvalidPolicy));
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_key_is_at_most_512_bytes_of_utf_8() {
    let at_bound = "é".repeat(256);
    let over = format!("{at_bound}a");
    assert_eq!(at_bound.len(), 512);

    let script = Script::new();
    oks(&script, 1);
    let limit = Limit::new().bucket(Bucket::new(move |_| Some(at_bound.clone())));
    success(shop(&script, limit).item(1).await);

    let script = Script::new();
    oks(&script, 1);
    let limit = Limit::new().bucket(Bucket::new(move |_| Some(over.clone())));
    let error = failure(shop(&script, limit).item(1).await);
    assert!(matches!(limit_error(&error), LimitError::InvalidPolicy));
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_bucket_has_at_most_eight_windows() {
    let mut bucket = one_key("all");
    for n in 1..=8 {
        bucket = bucket.window(Window::rolling(1000, secs(n)));
    }
    let script = Script::new();
    oks(&script, 1);
    success(
        shop(&script, Limit::new().bucket(bucket.clone()))
            .item(1)
            .await,
    );

    let script = Script::new();
    oks(&script, 1);
    let ninth = bucket.window(Window::rolling(1000, secs(9)));
    let error = failure(shop(&script, Limit::new().bucket(ninth)).item(1).await);
    assert!(matches!(limit_error(&error), LimitError::InvalidPolicy));
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_full_history_never_fails_a_call_and_stays_on_the_safe_side() {
    // 5000 calls in a window of 5000 per 10 minutes: more than 4096 entries of history.
    const CALLS: usize = 5000;
    let script = Script::new();
    oks(&script, CALLS + 1);
    // The last call waits about 550 s: longer than the default `max_hold` of 60 s (C11.7).
    let limit = Limit::new()
        .max_hold(secs(700))
        .bucket(one_key("all").window(Window::rolling(CALLS as u64, secs(600))));
    let client = shop(&script, limit);
    let start = Instant::now();

    for _ in 0..CALLS {
        success(client.item(1).await);
        sleep(ms(10)).await;
    }
    success(client.item(1).await);

    let times = sent(&script, start);
    assert_eq!(times.len(), CALLS + 1);
    let last = times[CALLS];
    // The first entry leaves the window at 600 s; a merge may only make the wait longer,
    // and never longer than the expiry of the last call (650 s).
    assert!(
        last >= secs(600) && last <= secs(600) + ms(10 * (CALLS as u64 - 1)),
        "the 5001st call was sent at {last:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_6_a_learned_set_of_windows_that_is_too_large_teaches_nothing() {
    // Nine windows in the headers: ignored as a whole, the response is returned.
    let limits = (1..=9)
        .map(|n| format!("1:{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let used = (1..=9)
        .map(|n| format!("1:{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let script = Script::new();
    script
        .answer(200)
        .header("x-limit", &limits)
        .header("x-count", &used)
        .body("ok");
    oks(&script, 1);
    let limit = Limit::new().bucket(one_key("all").learn(counts("x-limit", "x-count")));
    let client = shop(&script, limit);
    let start = Instant::now();

    success(client.item(1).await);
    success(client.item(1).await);

    assert_eq!(sent(&script, start), vec![secs(0), secs(0)]);
}

// C11.7 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_7_the_permit_is_held_until_the_body_ends() {
    let script = Script::new();
    script.answer(200).body("a").pause(secs(5)).body("b");
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_in_flight(1));
    let start = Instant::now();

    let body = success(client.download().await);
    let ((bytes, failed), second) = tokio::join!(drain(body), async {
        sleep(ms(1)).await;
        client.item(1).send().await
    });
    success(second);

    assert_eq!((bytes, failed), (b"ab".to_vec(), false));
    assert_eq!(sent(&script, start), vec![secs(0), secs(5)]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_the_permit_is_released_when_the_body_fails() {
    let script = Script::new();
    script.answer(200).body("a").pause(secs(3)).cut();
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_in_flight(1));
    let start = Instant::now();

    let body = success(client.download().await);
    let ((bytes, failed), second) = tokio::join!(drain(body), async {
        sleep(ms(1)).await;
        client.item(1).send().await
    });
    success(second);

    assert_eq!((bytes, failed), (b"a".to_vec(), true));
    assert_eq!(sent(&script, start), vec![secs(0), secs(3)]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_the_permit_is_released_when_the_body_is_dropped() {
    let script = Script::new();
    script.answer(200).body("a").stall();
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_in_flight(1));
    let start = Instant::now();

    let body = success(client.download().await);
    let ((), second) = tokio::join!(
        async move {
            sleep(secs(3)).await;
            drop(body);
        },
        async {
            sleep(ms(1)).await;
            client.item(1).send().await
        }
    );
    success(second);

    assert_eq!(sent(&script, start), vec![secs(0), secs(3)]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_the_response_keeps_its_status_headers_url_and_body() {
    let script = Script::new();
    script
        .answer(203)
        .header("x-a", "1")
        .header("x-a", "2")
        .header("x-b", "three")
        .body("h\u{e9}")
        .pause(secs(1))
        .body("llo");
    let limit = Limit::new()
        .max_in_flight(1)
        .bucket(one_key("all").window(Window::rolling(10, secs(1))));

    let response = shop(&script, limit)
        .item(1)
        .send_raw()
        .await
        .expect("the response");

    assert_eq!(response.status(), 203);
    assert_eq!(
        response.url().as_str(),
        "https://shop.example.test/api/items/1"
    );
    let values: Vec<_> = response
        .headers()
        .get_all("x-a")
        .iter()
        .map(|value| value.as_bytes().to_vec())
        .collect();
    assert_eq!(values, vec![b"1".to_vec(), b"2".to_vec()]);
    assert_eq!(
        response.headers().get("x-b").map(|value| value.as_bytes()),
        Some(&b"three"[..])
    );
    let bytes = response.bytes().await.expect("the body reads");
    assert_eq!(&bytes[..], "h\u{e9}llo".as_bytes());
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_wait_known_to_be_longer_than_max_hold_fails_at_once() {
    let script = Script::new();
    script.answer(429).header("retry-after", "30");
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_hold(secs(10)));

    failure(client.item(1).await);
    let start = Instant::now();
    let error = failure(client.item(1).await);

    assert_eq!(start.elapsed(), Duration::ZERO, "at once");
    match limit_error(&error) {
        LimitError::MaxHold {
            required,
            retry_after,
            waited,
            ..
        } => {
            assert_eq!(*required, Some(secs(30)));
            assert_eq!(*retry_after, Some(secs(30)));
            assert_eq!(*waited, Duration::ZERO);
        }
        _ => panic!("expected MaxHold"),
    }
    assert_eq!(script.requests().len(), 1);
    assert_eq!(
        error.retry_after(),
        None,
        "MaxHold is not a response's Retry-After"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_7_max_hold_reports_the_wait_that_is_left_now_and_the_retry_after_given() {
    // 15 s after a `Retry-After: 30`, 15 s are left; the server gave 30.
    let script = Script::new();
    script.answer(429).header("retry-after", "30");
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_hold(secs(10)));

    failure(client.item(1).await);
    sleep(secs(15)).await;
    let error = failure(client.item(1).await);

    assert_eq!(
        max_hold(&error),
        (Some(secs(15)), Some(secs(30)), Duration::ZERO)
    );
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_wait_equal_to_max_hold_waits() {
    let script = Script::new();
    script.answer(429).header("retry-after", "10");
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_hold(secs(10)));
    let start = Instant::now();

    failure(client.item(1).await);
    success(client.item(1).await);

    assert_eq!(sent(&script, start), vec![secs(0), secs(10)]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_max_hold_is_60_seconds_by_default() {
    let script = Script::new();
    script.answer(429).header("retry-after", "60");
    script.answer(429).header("retry-after", "61");
    oks(&script, 1);
    let client = shop(&script, Limit::new());
    let start = Instant::now();

    failure(client.item(1).await);
    failure(client.item(1).await); // waits 60 s, then gets the second 429
    let error = failure(client.item(1).await);

    assert!(matches!(limit_error(&error), LimitError::MaxHold { .. }));
    assert_eq!(sent(&script, start), vec![secs(0), secs(60)]);
    assert_eq!(start.elapsed(), secs(60));
}

#[tokio::test(start_paused = true)]
async fn c11_7_an_unknown_wait_fails_when_max_hold_ends() {
    let script = Script::new();
    script.never();
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_in_flight(1).max_hold(secs(4)));
    let start = Instant::now();

    let (first, second) = tokio::join!(timeout(secs(10), client.item(1).send()), async {
        sleep(ms(1)).await;
        let result = client.item(2).send().await;
        (result, start.elapsed())
    });
    assert!(first.is_err());
    let (result, at) = second;
    let error = failure(result);

    assert_eq!(at, ms(1) + secs(4));
    match limit_error(&error) {
        LimitError::MaxHold {
            required, waited, ..
        } => {
            assert_eq!(*required, None, "the wait could not be known");
            assert_eq!(*waited, secs(4));
        }
        _ => panic!("expected MaxHold"),
    }
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_max_hold_of_zero_admits_only_at_once() {
    let script = Script::new();
    script.answer(200).after(secs(5)).body("slow");
    oks(&script, 1);
    let client = shop(
        &script,
        Limit::new().max_in_flight(1).max_hold(Duration::ZERO),
    );
    let start = Instant::now();

    let (first, second) = tokio::join!(client.item(1).send(), async {
        sleep(ms(1)).await;
        (client.item(2).send().await, start.elapsed())
    });
    success(first);
    let (result, at) = second;

    assert!(matches!(
        limit_error(&failure(result)),
        LimitError::MaxHold { .. }
    ));
    assert_eq!(at, ms(1));
    success(client.item(3).await);
    assert_eq!(sent(&script, start), vec![secs(0), secs(5)]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_call_dropped_while_it_waits_leaves_no_waiter() {
    let script = Script::new();
    script.answer(200).after(secs(10)).body("first");
    oks(&script, 2);
    let client = shop(&script, Limit::new().max_in_flight(1));
    let start = Instant::now();

    let (first, dropped, third) = tokio::join!(
        client.item(1).send(),
        async {
            sleep(ms(1)).await;
            timeout(secs(2), client.item(2).send()).await
        },
        async {
            sleep(ms(2)).await;
            client.item(3).send().await
        }
    );
    success(first);
    assert!(dropped.is_err());
    success(third);
    success(client.item(4).await);

    let urls: Vec<String> = script
        .requests()
        .iter()
        .map(|request| request.url().as_str().to_owned())
        .collect();
    assert_eq!(
        urls,
        vec![
            "https://shop.example.test/api/items/1",
            "https://shop.example.test/api/items/3",
            "https://shop.example.test/api/items/4",
        ]
    );
    assert_eq!(sent(&script, start), vec![secs(0), secs(10), secs(10)]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_call_dropped_while_it_is_sent_releases_its_permit() {
    let script = Script::new();
    script.never();
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_in_flight(1));
    let start = Instant::now();

    let (dropped, next) = tokio::join!(timeout(secs(3), client.item(1).send()), async {
        sleep(ms(1)).await;
        client.item(2).send().await
    });
    assert!(dropped.is_err());
    success(next);

    assert_eq!(sent(&script, start), vec![secs(0), secs(3)]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_a_call_dropped_while_its_body_is_read_releases_its_permit() {
    // `send()` of an endpoint that reads the whole body: dropped while the body arrives.
    let script = Script::new();
    script.answer(200).body("a").pause(secs(10)).body("b");
    oks(&script, 1);
    let client = shop(&script, Limit::new().max_in_flight(1));
    let start = Instant::now();

    let (dropped, next) = tokio::join!(timeout(secs(4), client.item(1).send()), async {
        sleep(ms(1)).await;
        client.item(2).send().await
    });
    assert!(dropped.is_err());
    success(next);

    assert_eq!(sent(&script, start), vec![secs(0), secs(4)]);
}

#[tokio::test(start_paused = true)]
async fn c11_7_zero_max_in_flight_is_an_invalid_policy() {
    let script = Script::new();
    oks(&script, 1);

    let error = failure(shop(&script, Limit::new().max_in_flight(0)).item(1).await);

    assert!(matches!(limit_error(&error), LimitError::InvalidPolicy));
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_7_clones_share_one_state() {
    // A clone attached to another client counts the same calls.
    let script = Script::new();
    script.answer(200).after(secs(5)).body("first");
    oks(&script, 1);
    let limit = Limit::new().max_in_flight(1);
    let one = shop(&script, limit.clone());
    let two = shop(&script, limit);
    let start = Instant::now();

    let (a, b) = tokio::join!(one.item(1).send(), async {
        sleep(ms(1)).await;
        two.item(2).send().await
    });
    success(a);
    success(b);

    assert_eq!(sent(&script, start), vec![secs(0), secs(5)]);
}
