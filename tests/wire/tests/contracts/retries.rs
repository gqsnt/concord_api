//! C4.1 to C4.4, C6.2 and C9.5 against a real server: what `Retry::standard()` sends again,
//! what an unmarked `POST` never sends twice, a `Retry-After` on the paused clock, and the two
//! ways a connection can end too early, before the head and after it.

use crate::server::{on_paused_clock, Frozen, Probe, Server, Wire};
use crate::{scripted, Answer};
use concord::error::{Class, Kind};
use concord::Text;
use std::future::IntoFuture;
use std::time::Duration;

concord::api! {
    client Shop {
        base "http://shop.test";
        read Text;
        GET item(name: String) -> String { path "items/{name}"; }
        POST order(name: String) -> String {
            path "items/{name}";
            body concord::Body::new(name.clone(), "text/plain");
        }
    }
}

const STEP: Duration = Duration::from_millis(50);
const MOST: Duration = Duration::from_secs(10);

/// A client of `server` over `wire`, whose attempts `probe` watches.
fn shop(server: &Server, wire: Wire, probe: &Probe) -> Shop {
    Shop::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .with_middleware(probe.clone())
}

// A 503 then a 200 ----------------------------------------------------------------------

async fn a_503_then_a_200(wire: Wire) {
    let _clock = Frozen::hold();
    let server = Server::start(scripted(
        "/items/{name}",
        [Answer::Status(503, &[]), Answer::Text("in stock")],
    ))
    .await;
    let probe = Probe::default();
    let call = shop(&server, wire, &probe)
        .with_retry(shop::Retry::standard())
        .item("lamp".to_owned());
    let value = on_paused_clock(&probe, STEP, MOST, call.into_future()).await;

    assert_eq!(value.unwrap(), "in stock");
    // Two requests on the wire, and two attempts counted by the engine: each attempt is
    // handed to the middleware once (C4.4), which is what `tries()` counts.
    assert_eq!(server.seen.len(), 2);
    assert_eq!(probe.seen().sent.len(), 2);
    assert!(server
        .seen
        .all()
        .iter()
        .all(|seen| seen.version == wire.version()));
}

#[tokio::test(start_paused = true)]
async fn c4_2_a_503_then_a_200_is_two_requests_and_two_attempts_over_http1() {
    a_503_then_a_200(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c4_2_a_503_then_a_200_is_two_requests_and_two_attempts_over_http2() {
    a_503_then_a_200(Wire::Two).await;
}

async fn three_503(wire: Wire) {
    let _clock = Frozen::hold();
    let server = Server::start(scripted(
        "/items/{name}",
        [
            Answer::Status(503, &[]),
            Answer::Status(503, &[]),
            Answer::Status(503, &[]),
        ],
    ))
    .await;
    let probe = Probe::default();
    let call = shop(&server, wire, &probe)
        .with_retry(shop::Retry::standard())
        .item("lamp".to_owned());
    let error = on_paused_clock(&probe, STEP, MOST, call.into_future())
        .await
        .unwrap_err();

    // At most 3 attempts; the last outcome is the result, unchanged.
    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.status().map(|status| status.as_u16()), Some(503));
    assert_eq!(error.tries(), 3);
    assert_eq!(server.seen.len(), 3);
}

#[tokio::test(start_paused = true)]
async fn c4_4_three_503_are_three_requests_and_tries_says_3_over_http1() {
    three_503(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c4_4_three_503_are_three_requests_and_tries_says_3_over_http2() {
    three_503(Wire::Two).await;
}

// An unmarked POST ----------------------------------------------------------------------

async fn a_post_is_sent_once(wire: Wire) {
    let answers = [
        Answer::Status(503, &[]),
        Answer::Status(429, &[("retry-after", "1")]),
        Answer::Status(502, &[]),
        Answer::Status(504, &[]),
        Answer::Status(408, &[]),
        Answer::Drop,
        Answer::CutBody,
    ];
    for answer in answers {
        let _clock = Frozen::hold();
        let server = Server::start(scripted(
            "/items/{name}",
            [answer.clone(), Answer::Text("taken twice")],
        ))
        .await;
        let probe = Probe::default();
        let call = shop(&server, wire, &probe)
            .with_retry(shop::Retry::standard())
            .order("lamp".to_owned());
        let error = on_paused_clock(&probe, STEP, MOST, call.into_future())
            .await
            .unwrap_err();

        assert_eq!(
            server.seen.len(),
            1,
            "{answer:?}: the server saw one request"
        );
        assert_eq!(error.tries(), 1, "{answer:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn c4_1_an_unmarked_post_is_sent_once_whatever_the_server_answers_over_http1() {
    a_post_is_sent_once(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c4_1_an_unmarked_post_is_sent_once_whatever_the_server_answers_over_http2() {
    a_post_is_sent_once(Wire::Two).await;
}

// Retry-After ---------------------------------------------------------------------------

async fn retry_after(wire: Wire) {
    let _clock = Frozen::hold();
    let server = Server::start(scripted(
        "/items/{name}",
        [
            Answer::Status(429, &[("retry-after", "1")]),
            Answer::Text("now"),
        ],
    ))
    .await;
    let probe = Probe::default();
    let call = shop(&server, wire, &probe)
        .with_retry(shop::Retry::standard())
        .item("lamp".to_owned());
    let value = on_paused_clock(&probe, STEP, MOST, call.into_future()).await;

    assert_eq!(value.unwrap(), "now");
    let seen = probe.seen();
    let waited = seen.sent[1] - seen.back[0];
    // A wait is never less than a `Retry-After`; the random part of a first wait is below
    // one second, so the wait is the `Retry-After` itself (C4.3).
    assert!(waited >= Duration::from_secs(1), "waited {waited:?}");
    assert!(waited < Duration::from_secs(1) + STEP, "waited {waited:?}");
    assert_eq!(server.seen.len(), 2);
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_429_with_retry_after_1_waits_one_second_on_the_paused_clock_over_http1() {
    retry_after(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_429_with_retry_after_1_waits_one_second_on_the_paused_clock_over_http2() {
    retry_after(Wire::Two).await;
}

async fn retry_after_past_the_timeout(wire: Wire) {
    let _clock = Frozen::hold();
    let server = Server::start(scripted(
        "/items/{name}",
        [
            Answer::Status(429, &[("retry-after", "5")]),
            Answer::Text("too late"),
        ],
    ))
    .await;
    let probe = Probe::default();
    let call = shop(&server, wire, &probe)
        .with_retry(shop::Retry::standard())
        .item("lamp".to_owned())
        .with_timeout(Duration::from_secs(2));
    let start = tokio::time::Instant::now();
    let error = on_paused_clock(&probe, STEP, MOST, call.into_future())
        .await
        .unwrap_err();

    // A wait that would not end before the call's timeout is not started: the 429 is the
    // result at once, with the wait it asked for (C4.3).
    assert_eq!(error.status().map(|status| status.as_u16()), Some(429));
    assert_eq!(error.retry_after(), Some(Duration::from_secs(5)));
    assert_eq!(error.tries(), 1);
    assert!(start.elapsed() < STEP, "waited {:?}", start.elapsed());
    assert_eq!(server.seen.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_retry_after_past_the_call_timeout_returns_the_429_at_once_over_http1() {
    retry_after_past_the_timeout(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_retry_after_past_the_call_timeout_returns_the_429_at_once_over_http2() {
    retry_after_past_the_timeout(Wire::Two).await;
}

// A connection that ends too early ------------------------------------------------------

async fn dropped_before_the_head(wire: Wire) {
    let server = Server::start(scripted("/items/{name}", [Answer::Drop])).await;
    let error = Shop::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .item("lamp".to_owned())
        .await
        .unwrap_err();

    // A connection that failed before a head was received is Connect, also after it was
    // made (C6.2). The request was written, so nothing states that it was not sent (C9.5).
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Connect));
    assert!(!error.request_not_sent());
    assert_eq!(error.status(), None);
    assert_eq!(error.tries(), 1);
    assert_eq!(server.seen.len(), 1);
}

#[tokio::test]
async fn c6_2_a_connection_dropped_before_the_head_is_connect_and_may_have_been_sent_over_http1() {
    dropped_before_the_head(Wire::One).await;
}

#[tokio::test]
async fn c6_2_a_connection_dropped_before_the_head_is_connect_and_may_have_been_sent_over_http2() {
    dropped_before_the_head(Wire::Two).await;
}

async fn dropped_then_answered(wire: Wire) {
    let _clock = Frozen::hold();
    let server = Server::start(scripted(
        "/items/{name}",
        [Answer::Drop, Answer::Text("second")],
    ))
    .await;
    let probe = Probe::default();
    let call = shop(&server, wire, &probe)
        .with_retry(shop::Retry::standard())
        .item("lamp".to_owned());
    let value = on_paused_clock(&probe, STEP, MOST, call.into_future()).await;

    // Connect is in the standard table, and a GET may be repeated. The engine sent it again:
    // two requests on the wire, and two attempts handed to the middleware, the count that
    // `tries()` reports (C4.4). A resend below the engine would be one attempt.
    assert_eq!(value.unwrap(), "second");
    assert_eq!(server.seen.len(), 2);
    assert_eq!(probe.seen().sent.len(), 2);
    let connections: Vec<u64> = server
        .seen
        .all()
        .iter()
        .map(|seen| seen.connection)
        .collect();
    assert_ne!(
        connections[0], connections[1],
        "the second attempt is on a new connection"
    );
}

#[tokio::test(start_paused = true)]
async fn c4_2_a_get_whose_connection_was_dropped_is_sent_again_on_a_new_one_over_http1() {
    dropped_then_answered(Wire::One).await;
}

#[tokio::test(start_paused = true)]
async fn c4_2_a_get_whose_connection_was_dropped_is_sent_again_on_a_new_one_over_http2() {
    dropped_then_answered(Wire::Two).await;
}

async fn cut_after_the_head(wire: Wire) {
    let server = Server::start(scripted("/items/{name}", [Answer::CutBody])).await;
    let error = Shop::new()
        .with_transport(wire.http())
        .with_base(server.url())
        .item("lamp".to_owned())
        .await
        .unwrap_err();

    // A failure while the body is read is class body, and keeps the status and the headers
    // (C5.4, C6.2); it is never a shorter value.
    assert_eq!(error.kind(), Kind::Transport);
    assert_eq!(error.class(), Some(Class::Body));
    assert!(!error.request_not_sent());
    assert_eq!(error.status().map(|status| status.as_u16()), Some(200));
    assert_eq!(
        error
            .headers()
            .and_then(|headers| headers.get("content-length")),
        Some("12")
    );
    assert_eq!(error.tries(), 1);
}

#[tokio::test]
async fn c6_2_a_body_cut_after_the_head_is_class_body_with_its_status_over_http1() {
    cut_after_the_head(Wire::One).await;
}

#[tokio::test]
async fn c6_2_a_body_cut_after_the_head_is_class_body_with_its_status_over_http2() {
    cut_after_the_head(Wire::Two).await;
}
