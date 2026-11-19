//! Limits against a real server (C11.4, C11.5, C11.7): a server that counts the requests in
//! flight for each key, and one that counts the requests of its own rolling window and
//! reports it in its headers. The server's own counters never exceed what the limit allows.
//!
//! The window runs on the paused clock, which the server shares: its window and the limit's
//! are measured on one clock. A reset written as a date (`remaining_rfc3339`) is counted from
//! the response's own `Date`, so its wait is exact on the paused clock too; a reset in Unix
//! seconds (`remaining`) is counted from the system clock, which the paused clock does not
//! hold, so its wait is known to within a second.

use crate::server::{on_paused_clock, Feed, Frozen, Gate, Probe, Server, Wire};
use axum::extract::{Request, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use concord::limit::{counts, remaining, remaining_rfc3339, Bucket, Limit};
use concord::read::{JsonLines, RecordStream};
use concord::Text;
use std::collections::HashMap;
use std::future::IntoFuture;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

concord::api! {
    client Tenants {
        base "http://tenants.test";
        read Text;
        GET work(tenant: String) -> String {
            path "work";
            headers { "x-tenant" = tenant; }
        }
        GET budget() -> String { path "budget"; }
        GET reset() -> String { path "reset"; }
        GET watch() -> RecordStream<Row> { path "watch"; read JsonLines::new(64); }
    }
}

/// A line of a watch.
#[derive(Debug, serde::Deserialize, PartialEq)]
pub struct Row {
    n: u32,
}

// A cap on the calls in flight for each key ----------------------------------------------

/// What the server counts for each tenant: the requests in flight now, and the most at once.
#[derive(Clone, Default)]
struct InFlight {
    now: Arc<Mutex<HashMap<String, (usize, usize)>>>,
    gate: Gate,
}

impl InFlight {
    fn now(&self, tenant: &str) -> usize {
        self.now
            .lock()
            .unwrap()
            .get(tenant)
            .map_or(0, |count| count.0)
    }

    fn most(&self, tenant: &str) -> usize {
        self.now
            .lock()
            .unwrap()
            .get(tenant)
            .map_or(0, |count| count.1)
    }
}

async fn work(State(counted): State<InFlight>, request: Request) -> Response {
    let tenant = request.headers()["x-tenant"].to_str().unwrap().to_owned();
    {
        let mut now = counted.now.lock().unwrap();
        let count = now.entry(tenant.clone()).or_default();
        count.0 += 1;
        count.1 = count.1.max(count.0);
    }
    counted.gate.wait().await;
    counted.now.lock().unwrap().get_mut(&tenant).unwrap().0 -= 1;
    "done".into_response()
}

async fn in_flight(wire: Wire) {
    let counted = InFlight::default();
    let server = Server::start(
        Router::new()
            .route("/work", get(work))
            .with_state(counted.clone()),
    )
    .await;
    let per_tenant = Bucket::new(|call| {
        Some(
            call.request()
                .headers()
                .get("x-tenant")?
                .to_str()
                .ok()?
                .to_owned(),
        )
    })
    .max_in_flight(2);
    // `entered` sees every call that reaches the limit, `left` every call that it lets go.
    let (entered, left) = (Probe::default(), Probe::default());
    let tenants = Tenants::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .with_middleware(entered.clone())
        .with_middleware(Limit::new().bucket(per_tenant))
        .with_middleware(left.clone());

    let calls: Vec<_> = (0..12)
        .map(|n| {
            let tenant = if n % 2 == 0 { "a" } else { "b" };
            tokio::spawn(tenants.work(tenant.to_owned()).into_future())
        })
        .collect();
    // Every call has reached the limit, and every call that it let go has reached the server.
    entered.until_sent(12).await;
    let gone = left.seen().sent.len();
    server.seen.until(gone).await;
    assert_eq!(gone, 4, "two calls of each tenant leave at once");
    assert_eq!((counted.now("a"), counted.now("b")), (2, 2));

    gate_open_and_finish(&counted.gate, calls).await;
    assert_eq!(server.seen.len(), 12);
    // The server's own count never went over the cap.
    assert_eq!((counted.most("a"), counted.most("b")), (2, 2));
}

async fn gate_open_and_finish(
    gate: &Gate,
    calls: Vec<tokio::task::JoinHandle<Result<String, tenants::Error>>>,
) {
    gate.open();
    for call in calls {
        assert_eq!(call.await.unwrap().unwrap(), "done");
    }
}

#[tokio::test]
async fn c11_7_a_cap_for_each_key_holds_the_servers_count_of_calls_in_flight_over_http1() {
    in_flight(Wire::One).await;
}

#[tokio::test]
async fn c11_7_a_cap_for_each_key_holds_the_servers_count_of_calls_in_flight_over_http2() {
    in_flight(Wire::Two).await;
}

// A window that the server reports --------------------------------------------------------

/// The server allows 3 requests in any second, and reports how many of them its window holds,
/// with the request just received: `x-rate-limit: 3:1`, `x-rate-limit-count: n:1`.
#[derive(Clone, Default)]
struct Window {
    arrivals: Arc<Mutex<Vec<Instant>>>,
    most: Arc<Mutex<usize>>,
}

async fn budget(State(window): State<Window>) -> Response {
    let now = Instant::now();
    let held = {
        let mut arrivals = window.arrivals.lock().unwrap();
        arrivals.push(now);
        arrivals
            .iter()
            .filter(|at| now.duration_since(**at) < Duration::from_secs(1))
            .count()
    };
    let mut most = window.most.lock().unwrap();
    *most = (*most).max(held);
    (
        [
            ("x-rate-limit", "3:1".to_owned()),
            ("x-rate-limit-count", format!("{held}:1")),
        ],
        "spent",
    )
        .into_response()
}

async fn learned_window(wire: Wire) {
    let _clock = Frozen::hold();
    let window = Window::default();
    let server = Server::start(
        Router::new()
            .route("/budget", get(budget))
            .with_state(window.clone()),
    )
    .await;
    let limit = Limit::new().bucket(
        Bucket::new(|_| Some("application".into()))
            .learn(counts("x-rate-limit", "x-rate-limit-count")),
    );
    let probe = Probe::default();
    let tenants = Tenants::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .with_middleware(limit)
        .with_middleware(probe.clone());

    let calls: Vec<_> = (0..9).map(|_| tenants.budget().into_future()).collect();
    let all = async move {
        let calls: Vec<_> = calls.into_iter().map(tokio::spawn).collect();
        let mut answers = Vec::new();
        for call in calls {
            answers.push(call.await.unwrap());
        }
        answers
    };
    let step = Duration::from_millis(50);
    let answers = on_paused_clock(&probe, step, Duration::from_secs(10), all).await;
    assert!(answers
        .iter()
        .all(|answer| matches!(answer.as_deref(), Ok("spent"))));

    // The server's own window never held more than 3.
    assert_eq!(*window.most.lock().unwrap(), 3);
    let seen = probe.seen();
    // Before any response, the bucket knows no window: the first call goes alone.
    assert!(seen.sent[1] >= seen.back[0]);
    // Then the calls leave at the times that the reported window allows: 3 in any second,
    // and no longer a wait than that.
    let at = window.arrivals.lock().unwrap().clone();
    for k in 3..at.len() {
        let apart = at[k] - at[k - 3];
        assert!(
            apart >= Duration::from_secs(1),
            "calls {} and {k}: {apart:?}",
            k - 3
        );
    }
    let all = at[8] - at[0];
    assert!(
        all < Duration::from_secs(2) + 2 * step,
        "nine calls took {all:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_window_that_the_server_reports_spaces_the_calls_on_the_paused_clock_over_http1() {
    learned_window(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_window_that_the_server_reports_spaces_the_calls_on_the_paused_clock_over_http2() {
    learned_window(Wire::Two).await;
}

// A 429 that holds the next calls --------------------------------------------------------

async fn held_after_a_429(wire: Wire) {
    let _clock = Frozen::hold();
    let server = Server::start(crate::scripted(
        "/budget",
        [
            crate::Answer::Status(429, &[("retry-after", "2")]),
            crate::Answer::Text("after the wait"),
        ],
    ))
    .await;
    let probe = Probe::default();
    let tenants = Tenants::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .with_middleware(Limit::new())
        .with_middleware(probe.clone());
    let both = async move {
        let first = tenants.budget().await;
        let second = tenants.budget().await;
        (first, second)
    };
    let step = Duration::from_millis(50);
    let (first, second) = on_paused_clock(&probe, step, Duration::from_secs(10), both).await;

    // The call that got the 429 gets it; the next call waits for its Retry-After (C11.4).
    assert_eq!(
        first.unwrap_err().status().map(|status| status.as_u16()),
        Some(429)
    );
    assert_eq!(second.unwrap(), "after the wait");
    let seen = probe.seen();
    let waited = seen.sent[1] - seen.back[0];
    assert!(waited >= Duration::from_secs(2), "waited {waited:?}");
    assert!(waited < Duration::from_secs(2) + step, "waited {waited:?}");
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_429_with_retry_after_holds_the_next_call_on_the_paused_clock_over_http1() {
    held_after_a_429(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c11_4_a_429_with_retry_after_holds_the_next_call_on_the_paused_clock_over_http2() {
    held_after_a_429(Wire::Two).await;
}

// A stream that holds its place -----------------------------------------------------------

/// How the first stream lets its place go.
#[derive(Clone, Copy, Debug)]
enum Release {
    /// The server ends the body, and the reader reads to its end.
    End,
    /// The server breaks the body, and the reader meets the failure.
    Fail,
    /// The caller drops the stream with the body still open.
    Drop,
}

async fn stream_holds_its_place(wire: Wire, release: Release) {
    let (feeder, feed) = Feed::<Bytes>::new();
    let feeds = Arc::new(Mutex::new(vec![feed]));
    let router = Router::new()
        .route(
            "/watch",
            get(
                |State(feeds): State<Arc<Mutex<Vec<Feed<Bytes>>>>>| async move {
                    match feeds.lock().unwrap().pop() {
                        Some(feed) => axum::body::Body::from_stream(feed).into_response(),
                        None => "{\"n\":2}\n".into_response(),
                    }
                },
            ),
        )
        .with_state(feeds);
    let server = Server::start(router).await;
    let (entered, left) = (Probe::default(), Probe::default());
    let tenants = Tenants::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .with_middleware(entered.clone())
        .with_middleware(Limit::new().max_in_flight(1))
        .with_middleware(left.clone());

    // The first watch keeps its body open on the server.
    let mut first = tenants.watch().await.unwrap();
    feeder.send(Bytes::from_static(b"{\"n\":1}\n"));
    assert_eq!(first.try_next().await.unwrap(), Some(Row { n: 1 }));

    // The second waits in the limit for as long as the first holds its place.
    let second = tokio::spawn(tenants.watch().into_future());
    entered.until_sent(2).await;
    assert_eq!(left.seen().sent.len(), 1);
    assert_eq!(server.seen.len(), 1);

    // The place is held until the body ends, fails or is dropped (C11.7, C11.10). In the
    // first two cases the stream is still held when the second call leaves.
    let kept = match release {
        Release::End => {
            drop(feeder);
            assert_eq!(first.try_next().await.unwrap(), None);
            Some(first)
        }
        Release::Fail => {
            feeder.fail();
            assert!(first.try_next().await.is_err());
            Some(first)
        }
        Release::Drop => {
            drop(first);
            None
        }
    };
    let mut second = second.await.unwrap().unwrap();
    assert_eq!(second.try_next().await.unwrap(), Some(Row { n: 2 }));
    assert_eq!(server.seen.len(), 2, "{release:?}");
    drop(kept);
}

async fn streams_hold_their_place(wire: Wire) {
    for release in [Release::End, Release::Fail, Release::Drop] {
        stream_holds_its_place(wire, release).await;
    }
}

#[tokio::test]
async fn c11_7_a_record_stream_holds_its_place_until_its_body_ends_fails_or_is_dropped_over_http1()
{
    streams_hold_their_place(Wire::One).await;
}

#[tokio::test]
async fn c11_7_a_record_stream_holds_its_place_until_its_body_ends_fails_or_is_dropped_over_http2()
{
    streams_hold_their_place(Wire::Two).await;
}

// Resets written as dates -------------------------------------------------------------------

/// The civil date and time of a number of seconds since 1970, and the day of the week
/// (0 is Sunday), by the days-from-civil algorithm of H. Hinnant.
fn civil(seconds: u64) -> (i64, u32, u32, u32, u32, u32, usize) {
    let days = (seconds / 86_400) as i64;
    let time = (seconds % 86_400) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted + 2) / 5 + 1) as u32;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let weekday = (days + 4).rem_euclid(7) as usize;
    (
        year,
        month,
        day,
        time / 3600,
        time / 60 % 60,
        time % 60,
        weekday,
    )
}

/// An HTTP date (RFC 9110, IMF-fixdate).
fn http_date(seconds: u64) -> String {
    let (year, month, day, hour, minute, second, weekday) = civil(seconds);
    let weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{}, {day:02} {} {year} {hour:02}:{minute:02}:{second:02} GMT",
        weekdays[weekday],
        months[month as usize - 1]
    )
}

/// An RFC 3339 date-time in UTC.
fn rfc3339(seconds: u64) -> String {
    let (year, month, day, hour, minute, second, _) = civil(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// How the server writes its reset.
#[derive(Clone, Copy)]
enum Reset {
    /// `x-reset` as an RFC 3339 date, with the response's `Date` set by the handler.
    Dated,
    /// `x-reset` in Unix seconds.
    Unix,
}

/// The server allows one request until its reset, 3 seconds after its own clock (the
/// system's, as a real server's is): `x-limit: 1`, `x-left: 0`, `x-reset`. Returns how long
/// the second call waited after the first came back, on the paused clock.
async fn reset_wait(wire: Wire, reset: Reset) -> Duration {
    let _clock = Frozen::hold();
    let router = Router::new().route(
        "/reset",
        get(move || async move {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mut response = "spent".into_response();
            let headers = response.headers_mut();
            headers.insert("x-limit", "1".parse().unwrap());
            headers.insert("x-left", "0".parse().unwrap());
            let written = match reset {
                Reset::Dated => {
                    headers.insert("date", http_date(now).parse().unwrap());
                    rfc3339(now + 3)
                }
                Reset::Unix => (now + 3).to_string(),
            };
            headers.insert("x-reset", written.parse().unwrap());
            response
        }),
    );
    let server = Server::start(router).await;
    let bucket = Bucket::new(|_| Some("resets".into())).learn(match reset {
        Reset::Dated => remaining_rfc3339("x-limit", "x-left", "x-reset"),
        Reset::Unix => remaining("x-limit", "x-left", "x-reset"),
    });
    let probe = Probe::default();
    let tenants = Tenants::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .with_middleware(Limit::new().bucket(bucket))
        .with_middleware(probe.clone());
    let both = async move { (tenants.reset().await, tenants.reset().await) };
    let (first, second) = on_paused_clock(&probe, STEP, Duration::from_secs(10), both).await;
    assert_eq!(first.unwrap(), "spent");
    assert_eq!(second.unwrap(), "spent");
    assert_eq!(server.seen.len(), 2);
    let seen = probe.seen();
    seen.sent[1] - seen.back[0]
}

const STEP: Duration = Duration::from_millis(50);

async fn dated_reset(wire: Wire) {
    // Counted from the response's `Date`: the reset is 3 seconds after it, exactly.
    let waited = reset_wait(wire, Reset::Dated).await;
    assert!(waited >= Duration::from_secs(3), "waited {waited:?}");
    assert!(waited < Duration::from_secs(3) + STEP, "waited {waited:?}");
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_reset_written_as_a_date_holds_the_next_call_from_the_responses_date_over_http1() {
    dated_reset(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_reset_written_as_a_date_holds_the_next_call_from_the_responses_date_over_http2() {
    dated_reset(Wire::Two).await;
}

async fn unix_reset(wire: Wire) {
    // Counted from the system clock, which has moved on by the fraction of a second that the
    // server's whole seconds leave out, and by the few milliseconds of the exchange.
    let waited = reset_wait(wire, Reset::Unix).await;
    assert!(waited > Duration::from_millis(1900), "waited {waited:?}");
    assert!(waited < Duration::from_secs(3) + STEP, "waited {waited:?}");
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_reset_in_unix_seconds_holds_the_next_call_to_within_a_second_over_http1() {
    unix_reset(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c11_5_a_reset_in_unix_seconds_holds_the_next_call_to_within_a_second_over_http2() {
    unix_reset(Wire::Two).await;
}
