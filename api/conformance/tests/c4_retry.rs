//! C4, retry: which requests may go again, which failures are worth it, the waits,
//! `Retry-After`, the rules `when` and `unless`, `not_applied_on`, `tries`
//! (docs/contracts.md C4; docs/engine.md section 3.2; docs/rules.md rule 18).
//!
//! The tables 6.1 to 6.4 of docs/cases.md are in `c4_tables.rs`.

mod common;

use bytes::Bytes;
use common::engine::*;
use common::stream;
use concord::error::{Class, Kind};
use concord::transport::{Transport, TransportError, TryInfo};
use concord::{reqwest, Retry};
use concord_testkit::Script;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

/// The gaps between the hand-overs of the recorded tries.
fn gaps(script: &Script) -> Vec<Duration> {
    let requests = script.requests();
    requests
        .windows(2)
        .map(|pair| pair[1].at() - pair[0].at())
        .collect()
}

/// A `503` with this `Retry-After`, then the task.
fn after(retry_after: &str) -> Script {
    let script = Script::new();
    script.answer(503).header("retry-after", retry_after);
    json(&script, 200, TASK);
    script
}

#[test]
fn the_dates_of_the_suite_are_the_examples_of_rfc_9110() {
    let at = midnight(1994, 11, 6) + 8 * 3600 + 49 * 60 + 37;
    assert_eq!(
        http_date_at(at, DateForm::Imf),
        "Sun, 06 Nov 1994 08:49:37 GMT"
    );
    assert_eq!(
        http_date_at(at, DateForm::Rfc850),
        "Sunday, 06-Nov-94 08:49:37 GMT"
    );
    assert_eq!(
        http_date_at(at, DateForm::Asctime),
        "Sun Nov  6 08:49:37 1994"
    );
}

// C4.1 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c4_1_the_method_is_the_one_of_the_request_as_built_after_edit_request() {
    for (method, may_repeat) in [
        (http::Method::GET, true),
        (http::Method::HEAD, true),
        (http::Method::OPTIONS, true),
        (http::Method::PUT, true),
        (http::Method::DELETE, true),
        (http::Method::POST, false),
        (http::Method::PATCH, false),
    ] {
        // `tasks.create` is a `POST` with no `idempotent`: its edit decides.
        let script = Script::new();
        script.answer(503);
        json(&script, 200, TASK);
        let edited = method.clone();

        let result = tasks(&script)
            .with_retry(Retry::standard())
            .create(new_task())
            .edit_request(move |request| *request.method_mut() = edited.clone())
            .await;

        let requests = script.requests();
        assert_eq!(requests[0].method(), &method);
        assert_eq!(
            requests.len(),
            if may_repeat { 2 } else { 1 },
            "{method}: {}",
            outcome(&result)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c4_1_the_method_is_the_one_before_any_middleware() {
    // A middleware that makes a GET a POST: the GET may still be repeated.
    let script = Script::new();
    script.answer(503);
    json(&script, 200, TASK);
    success(
        tasks(&script)
            .with_middleware(SetsMethod(http::Method::POST))
            .with_retry(Retry::standard())
            .get(7)
            .await,
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method(), &http::Method::POST);

    // And a middleware that makes a POST a GET: the POST may not.
    let script = Script::new();
    script.answer(503);
    json(&script, 200, TASK);
    let error = failure(
        tasks(&script)
            .with_middleware(SetsMethod(http::Method::GET))
            .with_retry(Retry::standard())
            .create(new_task())
            .await,
    );
    assert!(error.is_status(503));
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_1_a_post_that_is_not_marked_is_not_sent_again_after_a_failed_connection() {
    let script = Script::new();
    script.fail(TransportError::connect());
    json(&script, 200, TASK);

    let error = failure(
        tasks(&script)
            .with_retry(Retry::standard().not_applied_on(429))
            .create(new_task())
            .await,
    );

    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Connect));
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_1_a_status_listed_with_not_applied_on_lets_a_post_go_again() {
    let script = Script::new();
    script.answer(429);
    json(&script, 200, TASK);

    success(
        tasks(&script)
            .with_retry(Retry::standard().not_applied_on(429))
            .create(new_task())
            .await,
    );

    assert_eq!(script.requests().len(), 2);
}

// C4.2 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c4_2_standard_makes_at_most_three_tries() {
    let script = Script::new();
    for _ in 0..4 {
        script.answer(503);
    }

    let error = failure(tasks(&script).with_retry(Retry::standard()).get(7).await);

    assert!(error.is_status(503));
    assert_eq!(error.tries(), 3);
    assert_eq!(script.requests().len(), 3);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_2_tries_sets_the_number_and_is_held_to_at_least_one() {
    for (tries, expected) in [(0, 1), (1, 1), (2, 2), (5, 5)] {
        let script = Script::new();
        for _ in 0..6 {
            script.answer(503);
        }

        let error = failure(
            tasks(&script)
                .with_retry(Retry::standard().tries(tries))
                .get(7)
                .await,
        );

        assert_eq!(error.tries(), expected, "tries({tries})");
        assert_eq!(script.requests().len(), expected as usize, "tries({tries})");
    }
}

#[tokio::test(start_paused = true)]
async fn c4_2_the_statuses_of_the_standard_table() {
    for (status, named) in [
        (408, true),
        (429, true),
        (502, true),
        (503, true),
        (504, true),
        (400, false),
        (401, false),
        (404, false),
        (409, false),
        (500, false),
        (501, false),
        (505, false),
    ] {
        let script = Script::new();
        script.answer(status);
        json(&script, 200, TASK);

        let result = tasks(&script).with_retry(Retry::standard()).get(7).await;

        assert_eq!(
            script.requests().len(),
            if named { 2 } else { 1 },
            "{status}: {}",
            outcome(&result)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c4_2_the_classes_of_the_standard_table() {
    for (class, named) in [
        (Class::Connect, true),
        (Class::Timeout, true),
        (Class::Body, true),
        (Class::Other, false),
        (Class::Blocked, false),
    ] {
        let script = Script::new();
        script.fail(TransportError::new(class));
        json(&script, 200, TASK);

        let result = tasks(&script).with_retry(Retry::standard()).get(7).await;

        assert_eq!(
            script.requests().len(),
            if named { 2 } else { 1 },
            "{class:?}: {}",
            outcome(&result)
        );
    }
}

// C4.3 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c4_3_the_waits_are_random_up_to_1_then_2_then_4_seconds_and_never_over_60() {
    // Many draws: each wait stays under the ceiling of its band, and each band has
    // at least one wait over zero, so an engine that never waits does not pass.
    // No distribution is asserted.
    const ROUNDS: usize = 32;
    let mut waited = [false; 8];
    for round in 0..ROUNDS {
        let script = Script::new();
        for _ in 0..9 {
            script.answer(503);
        }

        failure(
            tasks(&script)
                .with_retry(Retry::standard().tries(9))
                .get(7)
                .await,
        );

        let gaps = gaps(&script);
        assert_eq!(gaps.len(), 8);
        for (n, gap) in gaps.iter().enumerate() {
            // Before try n + 2: up to 2^n seconds, and never more than 60.
            let most = secs((1u64 << n).min(60));
            assert!(
                *gap <= most,
                "round {round}, before try {}: {gap:?} > {most:?}",
                n + 2
            );
            waited[n] |= *gap > Duration::ZERO;
        }
    }
    for (n, waited) in waited.iter().enumerate() {
        assert!(
            waited,
            "before try {}: no wait over zero in {ROUNDS} draws",
            n + 2
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c4_3_a56_each_wait_of_a_call_draws_its_own_random_part() {
    // ANSWERS.md A56: "The random part of a wait (the standard rule's jitter) is drawn again
    // for each wait. Two waits of one call are not tied to each other." The first wait is a
    // fraction of 1 s, the second a fraction of 2 s. One draw used twice would make the
    // second exactly twice the first (to the millisecond of the clock). Over 16 calls,
    // independent draws give that every time only by a chance far below one in a billion.
    const ROUNDS: usize = 16;
    let mut untied = 0;
    for _ in 0..ROUNDS {
        let script = Script::new();
        for _ in 0..3 {
            script.answer(503);
        }
        failure(tasks(&script).with_retry(Retry::standard()).get(7).await);

        let gaps = gaps(&script);
        assert_eq!(gaps.len(), 2);
        assert!(gaps[0] <= secs(1) && gaps[1] <= secs(2), "{gaps:?}");
        let twice = gaps[0] * 2;
        let apart = gaps[1].max(twice) - gaps[1].min(twice);
        if apart > Duration::from_millis(4) {
            untied += 1;
        }
    }
    assert!(
        untied > 0,
        "in {ROUNDS} calls, the second wait was always twice the first: one draw for both"
    );
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_wait_is_never_less_than_a_retry_after() {
    for seconds in [5, 60] {
        let script = after(&seconds.to_string());
        success(tasks(&script).with_retry(Retry::standard()).get(7).await);
        // The random part is at most 1 second here: the `Retry-After` is the wait.
        assert_eq!(gaps(&script), [secs(seconds)], "Retry-After: {seconds}");
    }
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_wait_is_never_less_than_what_a_rule_of_when_asked() {
    let script = Script::new();
    script.answer(503);
    json(&script, 200, TASK);

    success(
        tasks(&script)
            .with_retry(Retry::standard().when(|_| Some(secs(7))))
            .get(7)
            .await,
    );

    assert_eq!(gaps(&script), [secs(7)]);
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_retry_after_over_60_s_means_no_other_try() {
    let script = after("61");
    let start = Instant::now();

    let error = failure(tasks(&script).with_retry(Retry::standard()).get(7).await);

    assert!(error.is_status(503));
    assert_eq!(error.retry_after(), Some(secs(61)));
    assert_eq!(script.requests().len(), 1);
    assert_eq!(start.elapsed(), secs(0), "the error is the result, at once");
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_wait_asked_by_a_rule_over_60_s_means_no_other_try() {
    let script = Script::new();
    script.answer(503);
    json(&script, 200, TASK);

    let error = failure(
        tasks(&script)
            .with_retry(Retry::standard().when(|_| Some(secs(61))))
            .get(7)
            .await,
    );

    assert!(error.is_status(503));
    assert_eq!(error.retry_after(), Some(secs(61)));
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_3_retry_after_gives_the_longer_of_the_header_and_the_rule() {
    for (header, asked, longer) in [(90, 10, 90), (10, 90, 90), (70, 80, 80)] {
        let script = after(&header.to_string());
        let asks = Count::default();
        let rule = asks.clone();

        let error = failure(
            tasks(&script)
                .with_retry(Retry::standard().when(move |_| {
                    rule.add();
                    Some(secs(asked))
                }))
                .get(7)
                .await,
        );

        assert_eq!(
            error.retry_after(),
            Some(secs(longer)),
            "Retry-After {header}, asked {asked}"
        );
        assert_eq!(asks.get(), 1, "no rule is asked twice");
        assert_eq!(script.requests().len(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_retry_after_that_is_neither_seconds_nor_a_date_is_absent() {
    for text in ["soon", "-5", "5.5", "1 hour", ""] {
        let script = after(text);
        success(tasks(&script).with_retry(Retry::standard()).get(7).await);
        let gaps = gaps(&script);
        assert_eq!(gaps.len(), 1, "{text:?}");
        assert!(gaps[0] <= SECOND, "{text:?}: waited {:?}", gaps[0]);
    }
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_retry_after_date_in_the_past_is_a_wait_of_zero_in_each_form() {
    for form in [DateForm::Imf, DateForm::Rfc850, DateForm::Asctime] {
        for date in [
            http_date(-3600, form),
            http_date_at(midnight(1994, 11, 6), form),
        ] {
            let script = after(&date);
            success(tasks(&script).with_retry(Retry::standard()).get(7).await);
            let gaps = gaps(&script);
            assert_eq!(gaps.len(), 1, "{date}");
            assert!(gaps[0] <= SECOND, "{date}: waited {:?}", gaps[0]);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_retry_after_date_over_60_s_ahead_means_no_other_try_in_each_form() {
    // C4.3: "A date is compared with the clock of the machine". The wall clock is not paused
    // with Tokio: the wait is checked against the clock read before and after the call. A
    // date has whole seconds, so it can be up to 1 s nearer.
    for form in [DateForm::Imf, DateForm::Rfc850, DateForm::Asctime] {
        let before = std::time::SystemTime::now();
        let date = http_date(3600, form);
        let script = after(&date);

        let error = failure(tasks(&script).with_retry(Retry::standard()).get(7).await);

        let passed = before.elapsed().unwrap_or_default();
        assert_eq!(script.requests().len(), 1, "{date}");
        let wait = error.retry_after().expect("the wait of the date");
        assert!(
            wait <= secs(3600) && wait + passed + SECOND >= secs(3600),
            "{date}: {wait:?}, compared with the clock of the machine"
        );
    }
}

#[test]
fn c4_3_a_retry_after_date_is_compared_with_the_clock_that_is_given() {
    // engine 3.2: "The time left, the wall clock (for a `Retry-After` that is a date) and the
    // random number are given to it." A fixed clock: 2026-11-13 00:00:00 UTC.
    let now = at(midnight(2026, 11, 13));
    for form in [DateForm::Imf, DateForm::Rfc850, DateForm::Asctime] {
        let ahead = http_date_at(midnight(2026, 11, 13) + 3600, form);
        let decision = decide(&Retry::standard(), 503, Some(&ahead), 1, None, now, 0.0);
        assert_eq!(
            decision.wait, None,
            "{ahead}: over 60 s ahead, no other try"
        );
        assert_eq!(decision.retry_after(), Some(secs(3600)), "{ahead}");

        let near = http_date_at(midnight(2026, 11, 13) + 30, form);
        let decision = decide(&Retry::standard(), 503, Some(&near), 1, None, now, 0.0);
        assert_eq!(decision.wait, Some(secs(30)), "{near}");

        // "A date in the past is a wait of zero."
        let past = http_date_at(midnight(2026, 11, 13) - 3600, form);
        let decision = decide(&Retry::standard(), 503, Some(&past), 1, None, now, 0.0);
        assert_eq!(decision.retry_after(), Some(Duration::ZERO), "{past}");
        assert_eq!(decision.wait, Some(Duration::ZERO), "{past}");
    }
}

#[test]
fn c4_3_a_year_of_two_digits_follows_the_rule_of_fifty_years() {
    // C4.3: "a year of two digits by its rule of fifty years" (RFC 9110, 5.6.7: a year that
    // appears to be more than 50 years in the future is the most recent past year with the
    // same last two digits). Fixed clocks, so the test holds in every year.
    let now = at(midnight(2026, 11, 13));
    // `94` is 1994: 2094 would be more than fifty years ahead. A date in the past.
    let past = http_date_at(midnight(1994, 11, 6), DateForm::Rfc850);
    assert!(past.contains("-94 "), "{past}");
    let decision = decide(&Retry::standard(), 503, Some(&past), 1, None, now, 0.0);
    assert_eq!(
        decision.retry_after(),
        Some(Duration::ZERO),
        "{past} is in the past"
    );

    // `70` is 2070: less than fifty years ahead. A date in the future, in whatever year.
    let ahead = http_date_at(midnight(2070, 1, 1), DateForm::Rfc850);
    assert!(ahead.contains("-70 "), "{ahead}");
    let decision = decide(&Retry::standard(), 503, Some(&ahead), 1, None, now, 0.0);
    assert_eq!(decision.wait, None, "{ahead} is ahead");
    assert_eq!(
        decision.retry_after(),
        Some(secs(
            u64::try_from(midnight(2070, 1, 1) - midnight(2026, 11, 13)).unwrap()
        ))
    );

    // In 2045, `94` is 2094: 49 years ahead (its weekday is the one of 2094).
    let now = at(midnight(2045, 1, 1));
    let later = http_date_at(midnight(2094, 11, 6), DateForm::Rfc850);
    assert!(later.contains("-94 "), "{later}");
    let decision = decide(&Retry::standard(), 503, Some(&later), 1, None, now, 0.0);
    assert_eq!(
        decision.retry_after(),
        Some(secs(
            u64::try_from(midnight(2094, 11, 6) - midnight(2045, 1, 1)).unwrap()
        )),
        "{later} in 2045"
    );
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_year_of_two_digits_is_read_by_the_engine_against_the_clock_of_the_machine() {
    // The same rule through a send, with years taken from the clock of the machine.
    let past = http_date(-400 * 86_400, DateForm::Rfc850);
    let script = after(&past);
    success(tasks(&script).with_retry(Retry::standard()).get(7).await);
    assert_eq!(script.requests().len(), 2, "{past} is in the past");

    let ahead = http_date(10 * 366 * 86_400, DateForm::Rfc850);
    let script = after(&ahead);
    let error = failure(tasks(&script).with_retry(Retry::standard()).get(7).await);
    assert_eq!(script.requests().len(), 1, "{ahead} is ahead");
    assert!(error.retry_after().expect("a wait") > secs(9 * 365 * 86_400));
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_retry_after_date_in_the_past_is_kept_as_a_wait_of_zero() {
    // C4.3: "A date in the past is a wait of zero"; C6.1: `retry_after()` gives it. Retry is
    // off, so the refusal is the result and says what it asked.
    for form in [DateForm::Imf, DateForm::Rfc850, DateForm::Asctime] {
        let date = http_date_at(midnight(1994, 11, 6), form);
        let script = Script::new();
        script.answer(503).header("retry-after", &date);

        let error = failure(tasks(&script).get(7).await);

        assert_eq!(error.retry_after(), Some(Duration::ZERO), "{date}");
    }
}

#[test]
fn c4_3_the_random_wait_is_drawn_up_to_1_then_2_then_4_seconds_and_never_over_60() {
    // C4.3: "Before try `n+1`, a random time up to `2^(n−1)` seconds: up to 1 second, then
    // 2, then 4; never more than 60 seconds." engine 3.2: `random × min(60 s, 1 s × 2^(n-1))`,
    // the random number given.
    let now = at(midnight(2026, 11, 13));
    for (n, window) in [
        (1, 1),
        (2, 2),
        (3, 4),
        (4, 8),
        (6, 32),
        (7, 60),
        (8, 60),
        (20, 60),
    ] {
        let policy = Retry::standard().tries(30);
        let most = decide(&policy, 503, None, n, None, now, 1.0);
        assert_eq!(most.wait, Some(secs(window)), "after try {n}");
        let half = decide(&policy, 503, None, n, None, now, 0.5);
        assert_eq!(half.wait, Some(secs(window) / 2), "after try {n}");
        let none = decide(&policy, 503, None, n, None, now, 0.0);
        assert_eq!(none.wait, Some(Duration::ZERO), "after try {n}");
    }
}

#[test]
fn c4_3_a_wait_equal_to_the_time_left_means_no_other_try() {
    // engine 3.2, step 7: "wait >= time_left → None".
    let now = at(midnight(2026, 11, 13));
    let decision = decide(
        &Retry::standard(),
        503,
        Some("10"),
        1,
        Some(secs(10)),
        now,
        0.0,
    );
    assert_eq!(decision.wait, None);
    assert_eq!(decision.retry_after(), Some(secs(10)));
    let decision = decide(
        &Retry::standard(),
        503,
        Some("10"),
        1,
        Some(secs(10) + Duration::from_millis(1)),
        now,
        0.0,
    );
    assert_eq!(decision.wait, Some(secs(10)));
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_wait_that_ends_exactly_at_the_timeout_is_not_started() {
    // C4.3: "A wait that would not end before the `timeout` of the call also means no other
    // try": a `Retry-After` of 10 s under a `timeout` of 10 s.
    let script = after("10");
    let start = Instant::now();
    let error = failure(
        tasks(&script)
            .with_retry(Retry::standard())
            .get(7)
            .with_timeout(secs(10))
            .await,
    );
    assert!(error.is_status(503), "the last outcome, unchanged: {error}");
    assert_eq!(script.requests().len(), 1);
    assert_eq!(start.elapsed(), secs(0), "the wait is not started");
}

/// The pure decision of engine 3.2 for a `GET` answered with `status`, and maybe a
/// `Retry-After`, after try `n`.
fn decide(
    policy: &Retry,
    status: u16,
    retry_after: Option<&str>,
    n: u32,
    time_left: Option<Duration>,
    now: std::time::SystemTime,
    random: f64,
) -> concord::retry::Decision {
    let mut map = http::HeaderMap::new();
    if let Some(value) = retry_after {
        map.insert("retry-after", value.parse().unwrap());
    }
    let headers = concord::Headers::from(map);
    concord::retry::again(
        policy,
        concord::retry::Outcome {
            kind: Kind::Refused,
            status: Some(http::StatusCode::from_u16(status).unwrap()),
            class: None,
            headers: Some(&headers),
            problem: None,
        },
        n,
        &http::Method::GET,
        concord::__private::Idempotent::ByMethod,
        true,
        time_left,
        now,
        random,
    )
}

/// This second since 1970, as a `SystemTime`.
fn at(second: i64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + secs(u64::try_from(second).unwrap())
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_wait_that_would_not_end_before_the_timeout_means_no_other_try() {
    let script = after("20");
    let start = Instant::now();
    let error = failure(
        tasks(&script)
            .with_retry(Retry::standard())
            .get(7)
            .with_timeout(secs(10))
            .await,
    );
    assert!(error.is_status(503), "the last outcome, unchanged");
    assert_eq!(script.requests().len(), 1);
    assert_eq!(start.elapsed(), secs(0), "the wait is not started");

    // A wait that ends before it is made.
    let script = after("5");
    success(
        tasks(&script)
            .with_retry(Retry::standard())
            .get(7)
            .with_timeout(secs(10))
            .await,
    );
    assert_eq!(gaps(&script), [secs(5)]);
}

// C4.4 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c4_4_when_the_tries_run_out_the_last_outcome_is_the_result_unchanged() {
    let script = Script::new();
    script.answer(503).header("x-try", "1");
    script.answer(502).header("x-try", "2");
    script.answer(504).header("x-try", "3").body("third");

    let error = failure(tasks(&script).with_retry(Retry::standard()).get(7).await);

    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(504));
    assert_eq!(error.headers().unwrap().get("x-try"), Some("3"));
    assert_eq!(error.body(), Some(&b"third"[..]));
    assert_eq!(error.tries(), 3);
    assert_eq!(
        error.to_string(),
        "refused: status 504, in tasks.get, after 3 tries"
    );
}

#[tokio::test(start_paused = true)]
async fn c4_4_a_timeout_before_any_head_counts_its_try() {
    let script = Script::new();
    script.never();

    let error = failure(tasks(&script).get(7).await);

    assert!(error.is_timeout());
    assert_eq!(error.tries(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_4_what_a_middleware_sends_again_inside_a_try_is_not_counted() {
    let script = Script::new();
    json(&script, 200, TASK);
    script.answer(404);

    let error = failure(tasks(&script).with_middleware(SendsTwice).get(7).await);

    assert!(error.is_status(404));
    assert_eq!(error.tries(), 1);
    assert_eq!(script.requests().len(), 2);
    assert!(script
        .requests()
        .iter()
        .all(|request| request.info().number() == 1));
}

// C4.5 ------------------------------------------------------------------------------

/// The bytes of an answer that say when they are dropped.
struct Watched {
    bytes: &'static [u8],
    dropped: Arc<AtomicBool>,
}

impl AsRef<[u8]> for Watched {
    fn as_ref(&self) -> &[u8] {
        self.bytes
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

/// A transport of the caller's own: a `503` twice, then the task. At each send it notes
/// whether every answer it gave before was dropped.
#[derive(Default)]
struct Watch {
    given: Mutex<Vec<Arc<AtomicBool>>>,
    seen: Mutex<Vec<bool>>,
}

impl Transport for Watch {
    async fn send(
        &self,
        _request: reqwest::Request,
        _info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        let mut given = self.given.lock().unwrap();
        let all_dropped = given.iter().all(|flag| flag.load(Ordering::SeqCst));
        self.seen.lock().unwrap().push(all_dropped);
        let dropped = Arc::new(AtomicBool::new(false));
        given.push(dropped.clone());
        let (status, bytes): (u16, &'static [u8]) = if given.len() < 3 {
            (503, b"busy")
        } else {
            (200, TASK.as_bytes())
        };
        let body = Bytes::from_owner(Watched { bytes, dropped });
        let response = http::Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(reqwest::Body::from(body))
            .unwrap();
        Ok(reqwest::Response::from(response))
    }
}

#[tokio::test(start_paused = true)]
async fn c4_5_the_answer_of_a_try_that_is_given_up_is_dropped_before_the_next_is_sent() {
    let watch = Arc::new(Watch::default());

    success(
        tour::Tasks::new()
            .with_transport(watch.clone())
            .with_retry(Retry::standard())
            .get(7)
            .await,
    );

    assert_eq!(*watch.seen.lock().unwrap(), [true, true, true]);
}

#[tokio::test(start_paused = true)]
async fn c4_5_with_send_raw_the_answer_of_a_try_that_is_given_up_is_dropped_before_the_next() {
    // C4.5: "The answer of a try that is given up is dropped before the next one is sent",
    // for `send_raw()` too (C3.6: a head of 400 or above is looked at by the policy).
    let watch = Arc::new(Watch::default());

    let response = success(
        tour::Tasks::new()
            .with_transport(watch.clone())
            .with_retry(Retry::standard())
            .get(7)
            .send_raw()
            .await,
    );

    assert_eq!(response.status(), 200);
    assert_eq!(*watch.seen.lock().unwrap(), [true, true, true]);
}

// C4.6 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c4_6_a_policy_on_the_call_replaces_the_whole_policy_of_the_client() {
    let client_policy = || Retry::standard().tries(5).when(|_| Some(SECOND));

    // The client's rule is gone: a 500 is not in the standard table.
    let script = Script::new();
    script.answer(500);
    json(&script, 200, TASK);
    failure(
        tasks(&script)
            .with_retry(client_policy())
            .get(7)
            .with_retry(Retry::standard())
            .await,
    );
    assert_eq!(script.requests().len(), 1);

    // The client's number of tries is gone too.
    let script = Script::new();
    for _ in 0..5 {
        script.answer(503);
    }
    let error = failure(
        tasks(&script)
            .with_retry(client_policy())
            .get(7)
            .with_retry(Retry::standard())
            .await,
    );
    assert_eq!(error.tries(), 3);

    // `Retry::none()` on a call turns it off.
    let script = Script::new();
    script.answer(503);
    json(&script, 200, TASK);
    failure(
        tasks(&script)
            .with_retry(client_policy())
            .get(7)
            .with_retry(Retry::none())
            .await,
    );
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_6_a_caller_keeps_the_authors_policy_by_passing_it_on() {
    // The author's policy lists 429 for the unmarked POST `tasks.create`.
    let authors = || Retry::standard().not_applied_on(429);
    // The caller's rule: a wait of 3 seconds after a 429, and one after a 500.
    let mine = |tried: &concord::retry::Tried<'_, ()>| match tried.status().map(|s| s.as_u16()) {
        Some(429) => Some(secs(3)),
        Some(500) => Some(SECOND),
        _ => None,
    };

    // Passed on: the author's 429 lets the POST go again, after the caller's wait.
    // C4.1, three conditions all needed: 500 is not listed, so the unmarked POST is
    // not sent again after it, whatever the caller's rule asks.
    let script = Script::new();
    script.answer(429);
    script.answer(500);
    json(&script, 200, TASK);
    let error = failure(
        tasks(&script)
            .create(new_task())
            .with_retry(authors().when(mine))
            .await,
    );
    assert!(error.is_status(500));
    assert_eq!(error.tries(), 2);
    assert_eq!(script.requests().len(), 2);
    assert_eq!(gaps(&script), [secs(3)]);

    // Not passed on: the call's policy replaces the author's, and 429 is not listed.
    let script = Script::new();
    script.answer(429);
    json(&script, 200, TASK);
    let error = failure(
        tasks(&script)
            .create(new_task())
            .with_retry(Retry::standard().when(mine))
            .await,
    );
    assert!(error.is_status(429));
    assert_eq!(script.requests().len(), 1);
}

// C4.7 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c4_7_an_unless_that_gives_true_means_no_other_try() {
    let script = Script::new();
    script.answer(503);
    json(&script, 200, TASK);

    let error = failure(
        tasks(&script)
            .with_retry(
                Retry::standard()
                    .when(|_| Some(SECOND))
                    .unless(|tried| tried.status().is_some_and(|status| status == 503)),
            )
            .get(7)
            .await,
    );

    assert!(error.is_status(503));
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_7_every_unless_is_asked_before_any_when() {
    let log = Log::default();
    let (a, b, c) = (log.clone(), log.clone(), log.clone());
    let script = Script::new();
    script.answer(503);
    json(&script, 200, TASK);

    let error = failure(
        tasks(&script)
            .with_retry(
                Retry::standard()
                    .when(move |_| {
                        a.push("when");
                        Some(SECOND)
                    })
                    .unless(move |_| {
                        b.push("unless 1");
                        true
                    })
                    .unless(move |_| {
                        c.push("unless 2");
                        false
                    }),
            )
            .get(7)
            .await,
    );

    assert!(error.is_status(503));
    assert_eq!(log.events(), ["unless 1", "unless 2"]);

    // With every `unless` false, the `when` rules come after them.
    let log = Log::default();
    let (a, b) = (log.clone(), log.clone());
    let script = Script::new();
    script.answer(503);
    json(&script, 200, TASK);
    success(
        tasks(&script)
            .with_retry(
                Retry::standard()
                    .when(move |_| {
                        a.push("when");
                        None
                    })
                    .unless(move |_| {
                        b.push("unless");
                        false
                    }),
            )
            .get(7)
            .await,
    );
    assert_eq!(log.events(), ["unless", "when"]);
    assert_eq!(script.requests().len(), 2, "no wait given: C4.2 decides");
}

#[tokio::test(start_paused = true)]
async fn c4_7_the_when_rules_are_asked_in_order_and_the_first_wait_wins() {
    let log = Log::default();
    let (a, b) = (log.clone(), log.clone());
    let script = Script::new();
    script.answer(404);
    json(&script, 200, TASK);

    success(
        tasks(&script)
            .with_retry(
                Retry::standard()
                    .when(move |_| {
                        a.push("first");
                        None
                    })
                    .when(move |_| {
                        b.push("second");
                        Some(secs(3))
                    })
                    .when(|_| Some(secs(9))),
            )
            .get(7)
            .await,
    );

    assert_eq!(log.events(), ["first", "second"]);
    assert_eq!(gaps(&script), [secs(3)], "a rule adds a case: the 404");
}

#[tokio::test(start_paused = true)]
async fn c4_7_no_rule_of_when_after_the_first_wait_is_asked() {
    // C4.7: "the `when` rules are asked in the order they were added; the first that gives a
    // wait wins"; ANSWERS.md A22: they "stop at the first that gives a wait".
    let asked_after = Count::default();
    let third = asked_after.clone();
    let script = Script::new();
    script.answer(503);
    json(&script, 200, TASK);

    success(
        tasks(&script)
            .with_retry(Retry::standard().when(|_| Some(SECOND)).when(move |_| {
                third.add();
                None
            }))
            .get(7)
            .await,
    );

    assert_eq!(
        asked_after.get(),
        0,
        "the rule after the first wait was asked"
    );
    assert_eq!(script.requests().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn c4_7_a_rule_is_asked_only_when_another_try_is_still_possible() {
    let asks = Count::default();
    let policy = |asks: &Count| {
        let rule = asks.clone();
        let unless = asks.clone();
        Retry::standard()
            .when(move |_| {
                rule.add();
                Some(SECOND)
            })
            .unless(move |_| {
                unless.add();
                false
            })
    };

    // No try is left.
    let script = Script::new();
    script.answer(503);
    failure(
        tasks(&script)
            .with_retry(policy(&asks).tries(1))
            .get(7)
            .await,
    );
    // The request may not be repeated.
    let script = Script::new();
    script.answer(503);
    failure(
        tasks(&script)
            .with_retry(policy(&asks))
            .create(new_task())
            .await,
    );
    // The request cannot be copied.
    let script = Script::new();
    script.answer(503);
    failure(
        files(&script)
            .with_retry(policy(&asks))
            .upload("a", stream("abc"))
            .await,
    );
    // A value, and a Mismatch.
    let script = Script::new();
    json(&script, 200, TASK);
    success(tasks(&script).with_retry(policy(&asks)).get(7).await);
    let script = Script::new();
    json(&script, 200, "[");
    failure(tasks(&script).with_retry(policy(&asks)).get(7).await);
    // Blocked.
    let script = Script::new();
    script.fail(TransportError::blocked());
    failure(tasks(&script).with_retry(policy(&asks)).get(7).await);

    assert_eq!(asks.get(), 0);
}

#[tokio::test(start_paused = true)]
async fn c4_7_tried_gives_the_number_status_headers_class_and_problem() {
    let log = Log::default();
    let rule = log.clone();
    let script = Script::new();
    script
        .answer(503)
        .header("content-type", "application/json")
        .header("x-a", "1")
        .body(PROBLEM);
    script.fail(TransportError::connect());
    json(&script, 200, r#"{"id":"EUW1_1"}"#);

    success(
        riot(&script)
            .with_retry(Retry::standard().when(move |tried| {
                rule.push(format!(
                    "{} {:?} {:?} {:?} {:?}",
                    tried.number(),
                    tried.status().map(|status| status.as_u16()),
                    tried.class(),
                    tried.headers().and_then(|headers| headers.get("x-a")),
                    tried
                        .problem()
                        .map(|problem: &tour::RiotProblem| problem.status.message.as_str()),
                ));
                None
            }))
            .matches(tour::Europe)
            .get(tour::MatchId::from("EUW1_1"))
            .await,
    );

    assert_eq!(
        log.events(),
        [
            r#"1 Some(503) None Some("1") Some("slow down")"#,
            "2 None Some(Connect) None None",
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c4_7_for_send_raw_a_rule_gets_the_head_and_nothing_of_the_body() {
    let log = Log::default();
    let rule = log.clone();
    let script = Script::new();
    // A body that never ends: reading it would wait for the clock for silence.
    script
        .answer(503)
        .header("content-type", "application/json")
        .header("x-a", "1")
        .body(&PROBLEM.as_bytes()[..10])
        .stall();
    script.answer(200);
    let start = Instant::now();

    let response = success(
        riot(&script)
            .with_retry(Retry::standard().when(move |tried| {
                rule.push(format!(
                    "{:?} {:?} {}",
                    tried.status().map(|status| status.as_u16()),
                    tried.headers().and_then(|headers| headers.get("x-a")),
                    tried.problem().is_some(),
                ));
                Some(SECOND)
            }))
            .matches(tour::Europe)
            .get(tour::MatchId::from("EUW1_1"))
            .send_raw()
            .await,
    );

    assert_eq!(response.status(), 200);
    assert_eq!(log.events(), [r#"Some(503) Some("1") false"#]);
    assert_eq!(start.elapsed(), SECOND, "nothing of the body was read");
}

#[tokio::test(start_paused = true)]
async fn c4_7_with_a_rule_problem_is_tried_from_the_body_of_a_refusal() {
    // A refusal body that is not the problem type: `problem()` is `None`, and nothing fails.
    let log = Log::default();
    let rule = log.clone();
    let script = Script::new();
    script
        .answer(503)
        .header("content-type", "application/json")
        .body("{}");
    json(&script, 200, r#"{"id":"EUW1_1"}"#);

    success(
        riot(&script)
            .with_retry(Retry::standard().when(move |tried| {
                rule.push(format!("{}", tried.problem().is_some()));
                None
            }))
            .matches(tour::Europe)
            .get(tour::MatchId::from("EUW1_1"))
            .await,
    );

    assert_eq!(log.events(), ["false"]);
}
