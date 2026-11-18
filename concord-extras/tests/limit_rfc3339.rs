//! Contracts for resets given as RFC 3339 dates, using scripted HTTP and virtual time.
#![cfg(feature = "limit")]
// Test fixtures intentionally fail on invalid URLs, headers or error types, and index fixed tables.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord::limit::{remaining, remaining_rfc3339, Bucket, Limit, LimitError, Seen};
use concord::reqwest::{Method, Request, Response};
use concord::transport::{TransportError, TryInfo};
use concord::{Middleware, Next};
use concord_testkit::Script;
use std::error::Error as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;

fn sec(n: u64) -> Duration {
    Duration::from_secs(n)
}
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// 2030-01-01T00:00:00Z, a server clock unrelated to this machine's.
const T: u64 = 1_893_456_000;

fn civil(seconds: u64) -> (u64, u64, u64, u64, u64, u64) {
    let days = seconds / 86_400;
    let rest = seconds % 86_400;
    // Days to a civil date (proleptic Gregorian), for days since 1970.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day, rest / 3600, rest % 3600 / 60, rest % 60)
}
fn rfc3339(seconds: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(seconds);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}
fn http_date(seconds: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(seconds);
    let weekday =
        ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(seconds / 86_400 % 7) as usize];
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(mo - 1) as usize];
    format!("{weekday}, {d:02} {month} {y:04} {h:02}:{mi:02}:{s:02} GMT")
}
fn local_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn call(limit: &Limit, script: &Script) -> Result<Response, TransportError> {
    let request = Request::new(Method::GET, "https://one.test/data".parse().unwrap());
    limit
        .handle(request, TryInfo::new("data", 1), Next::to(script))
        .await
}
fn cause(error: &TransportError) -> &LimitError {
    error.source().unwrap().downcast_ref().unwrap()
}
fn sent(script: &Script, start: Instant) -> Vec<Duration> {
    script.requests().iter().map(|r| r.at() - start).collect()
}
fn requests() -> Limit {
    Limit::new().max_hold(sec(120)).bucket(
        Bucket::new(|_| Some("requests".into())).learn(remaining_rfc3339(
            "x-limit",
            "x-remaining",
            "x-reset",
        )),
    )
}

/// What `until_rfc3339` returned for the `x-reset` of each response.
fn recorder() -> (Limit, Arc<Mutex<Vec<Option<Duration>>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    let limit = Limit::new().bucket(Bucket::new(|_| Some("k".into())).learn(
        move |seen: &Seen<'_>| {
            let until = seen
                .header("x-reset")
                .and_then(|text| seen.until_rfc3339(text));
            kept.lock().unwrap().push(until);
            Vec::new()
        },
    ));
    (limit, seen)
}

#[tokio::test(start_paused = true)]
async fn c11_5_rfc3339_reset_counts_from_a_valid_date_whatever_the_local_clock() {
    let local = local_now();
    let script = Script::new();
    // Server clocks behind this one by ten minutes, ahead by ten minutes, and years ahead.
    for server in [local - 600, local + 600, T] {
        script
            .answer(200)
            .header("date", &http_date(server))
            .header("x-reset", &rfc3339(server + 90));
    }
    script
        .answer(200)
        .header("date", &http_date(T))
        .header("x-reset", "2030-01-01T00:00:01.0000000001Z");
    script
        .answer(200)
        .header("date", &http_date(T))
        .header("x-reset", &rfc3339(T - 1));
    let (limit, seen) = recorder();
    for _ in 0..5 {
        call(&limit, &script).await.unwrap();
    }
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Some(sec(90)),
            Some(sec(90)),
            Some(sec(90)),
            // A fraction finer than a nanosecond is rounded up.
            Some(Duration::new(1, 1)),
            // A reset in the past is zero.
            Some(Duration::ZERO),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_rfc3339_without_one_valid_date_counts_from_the_local_clock() {
    let local = local_now();
    let reset = rfc3339(local + 3600);
    let script = Script::new();
    script.answer(200).header("x-reset", &reset);
    // Repeated, in an obsolete form, and not a date: each falls back to this clock.
    script
        .answer(200)
        .header("date", &http_date(T))
        .header("date", &http_date(T))
        .header("x-reset", &reset);
    script
        .answer(200)
        .header("date", "Tuesday, 01-Jan-30 00:00:00 GMT")
        .header("x-reset", &reset);
    script
        .answer(200)
        .header("date", "yesterday")
        .header("x-reset", &reset);
    let (limit, seen) = recorder();
    for _ in 0..4 {
        call(&limit, &script).await.unwrap();
    }
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 4);
    for until in seen.iter() {
        let until = until.unwrap();
        // The wall clock moves a little while the test runs.
        assert!(until <= sec(3600) && until > sec(3590), "{until:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_learned_dated_reset_holds_the_next_call_until_it() {
    let script = Script::new();
    script
        .answer(200)
        .header("date", &http_date(T))
        .header("x-limit", "1")
        .header("x-remaining", "0")
        .header("x-reset", &rfc3339(T + 90));
    script.answer(200);
    let limit = requests();
    let start = Instant::now();
    call(&limit, &script).await.unwrap();
    call(&limit, &script).await.unwrap();
    assert_eq!(sent(&script, start), [sec(0), sec(90)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_wrong_local_clock_without_date_costs_in_each_direction() {
    let local = local_now();
    // This clock is ten minutes fast: the server's reset looks past and teaches nothing,
    // so the next call is not held (the server would refuse it with a 429).
    let server = local - 600;
    let script = Script::new();
    script
        .answer(200)
        .header("x-limit", "1")
        .header("x-remaining", "0")
        .header("x-reset", &rfc3339(server + 90));
    script.answer(200);
    let limit = requests();
    let start = Instant::now();
    call(&limit, &script).await.unwrap();
    call(&limit, &script).await.unwrap();
    assert_eq!(sent(&script, start), [sec(0), sec(0)]);

    // With the server's Date, the same reset holds for its 90 seconds.
    let script = Script::new();
    script
        .answer(200)
        .header("date", &http_date(server))
        .header("x-limit", "1")
        .header("x-remaining", "0")
        .header("x-reset", &rfc3339(server + 90));
    script.answer(200);
    let limit = requests();
    let start = Instant::now();
    call(&limit, &script).await.unwrap();
    call(&limit, &script).await.unwrap();
    assert_eq!(sent(&script, start), [sec(0), sec(90)]);

    // This clock is ten minutes slow: the reset looks 690 seconds away, longer than
    // max_hold, so the next call fails at once.
    let server = local + 600;
    let script = Script::new();
    script
        .answer(200)
        .header("x-limit", "1")
        .header("x-remaining", "0")
        .header("x-reset", &rfc3339(server + 90));
    let limit = requests();
    let start = Instant::now();
    call(&limit, &script).await.unwrap();
    let error = call(&limit, &script).await.unwrap_err();
    assert!(
        matches!(cause(&error), LimitError::MaxHold { required: Some(wait), .. }
            if *wait > sec(680) && *wait <= sec(690)),
        "{:?}",
        cause(&error)
    );
    assert_eq!(sent(&script, start), [sec(0)]);
    assert_eq!(start.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_past_or_unreadable_dated_reset_teaches_nothing() {
    let script = Script::new();
    for (limit, left, reset) in [
        ("1", "0", rfc3339(T - 1)),
        ("1", "0", rfc3339(T)),
        ("+1", "0", rfc3339(T + 90)),
        ("1", "+0", rfc3339(T + 90)),
        ("1", "2", rfc3339(T + 90)),
        ("1", "0", "2030-01-01X00:01:30Z".into()),
        ("1", "0", format!("{}{}", rfc3339(T + 90), " ".repeat(4097))),
    ] {
        script
            .answer(200)
            .header("date", &http_date(T))
            .header("x-limit", limit)
            .header("x-remaining", left)
            .header("x-reset", &reset);
    }
    script.answer(200);
    let limit = requests();
    let start = Instant::now();
    for _ in 0..8 {
        call(&limit, &script).await.unwrap();
    }
    assert_eq!(sent(&script, start), [Duration::ZERO; 8]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_late_head_naming_the_same_reset_grants_no_second_refill() {
    let script = Script::new();
    let answer = |after: Duration, date: u64, left: &str, reset: u64| {
        script
            .answer(200)
            .after(after)
            .header("date", &http_date(date))
            .header("x-limit", "2")
            .header("x-remaining", left)
            .header("x-reset", &rfc3339(reset));
    };
    // A: the budget is 2 until T + 2. B: written at T too, its head arrives after the reset.
    answer(Duration::ZERO, T, "1", T + 2);
    answer(sec(3), T, "0", T + 2);
    // C and D spend the refill; their heads, which name the next reset, come late.
    answer(sec(10), T + 2, "1", T + 62);
    answer(sec(10), T + 2, "0", T + 62);
    answer(Duration::ZERO, T + 72, "1", T + 122);
    answer(Duration::ZERO, T + 72, "0", T + 122);
    let limit = requests();
    let start = Instant::now();
    let spawn = |limit: &Limit| {
        let (limit, script) = (limit.clone(), script.clone());
        tokio::spawn(async move { call(&limit, &script).await.map(drop) })
    };
    call(&limit, &script).await.unwrap();
    let b = spawn(&limit);
    tokio::time::sleep_until(start + ms(2500)).await;
    let (c, d) = (spawn(&limit), spawn(&limit));
    // B's head arrives at 3 s: after the refill that its reset gave, it names that reset again.
    tokio::time::sleep_until(start + ms(3500)).await;
    let (e, f) = (spawn(&limit), spawn(&limit));
    for call in [b, c, d, e, f] {
        call.await.unwrap().unwrap();
    }
    // E and F wait for the reset that C's and D's heads name: 60 s after their Date, counted
    // from their arrival at 12.5 s. Two units per period, never four.
    assert_eq!(
        sent(&script, start),
        [ms(0), ms(0), ms(2500), ms(2500), ms(72_500), ms(72_500)]
    );
    assert!(script.requests().iter().all(|r| r.info().number() == 1));
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_lower_balance_with_a_newer_dated_reset_is_honoured_at_once() {
    // Two units left until T + 10; then none left until T + 11, three seconds after that
    // response's own Date: on this clock, its reset comes before the one already known.
    let script = Script::new();
    for (date, reset, left) in [(T, T + 10, "2"), (T + 8, T + 11, "0")] {
        script
            .answer(200)
            .header("date", &http_date(date))
            .header("x-limit", "3")
            .header("x-remaining", left)
            .header("x-reset", &rfc3339(reset));
    }
    script.answer(200);
    let limit = requests();
    let start = Instant::now();
    for _ in 0..3 {
        call(&limit, &script).await.unwrap();
    }
    // The zero is honoured at once, and the later of the two deadlines is kept.
    assert_eq!(sent(&script, start), [sec(0), sec(0), sec(10)]);
}

#[tokio::test(start_paused = true)]
async fn c11_5_remaining_in_unix_seconds_honours_a_lower_balance_on_its_one_clock() {
    // The reader of Unix seconds counts every reset from this process's clock, so a reset
    // that is the same or later never comes earlier: its zero is honoured at once.
    let local = local_now();
    let script = Script::new();
    for (reset, left) in [(local + 10, "2"), (local + 11, "0")] {
        script
            .answer(200)
            .header("x-limit", "3")
            .header("x-remaining", left)
            .header("x-reset", &reset.to_string());
    }
    script.answer(200);
    let limit = Limit::new().bucket(Bucket::new(|_| Some("requests".into())).learn(remaining(
        "x-limit",
        "x-remaining",
        "x-reset",
    )));
    let start = Instant::now();
    for _ in 0..3 {
        call(&limit, &script).await.unwrap();
    }
    let at = sent(&script, start);
    assert_eq!(&at[..2], [sec(0), sec(0)]);
    // The wall clock moves a little while the test runs.
    assert!(at[2] > sec(10) && at[2] <= sec(11), "{at:?}");
}
