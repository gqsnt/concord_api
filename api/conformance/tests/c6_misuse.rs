//! What a reader found holding when he used Concord wrongly, or against a server that
//! misbehaves (build/reports/m6-misuse-opus.md), kept as regression tests and named by
//! contract:
//!
//! - no secret, body or query value in `Display`, `Debug`, the `source()` chain or the logs,
//!   after each kind of failure (C6.2, C10.6, C10.7);
//! - the server cases that protect a caller: a body of 1 GiB announced is refused at once
//!   (C5.4); an error body of 100 MiB keeps 64 KiB (C5.6); a `301` to another host is not
//!   followed (C7.1); a `Link` to another origin is a Build error and the other host gets
//!   nothing (C8.4); a `Retry-After` of a year is one try (C4.3).
//!
//! What is logged is captured for the whole binary: every `tracing` event and span (Concord's
//! own event `retry`, and the HTTP library's), and every `log` record (the HTTP library's).

mod common;

use common::engine::*;
use concord::error::{Class, Kept, Kind};
use concord::{Body, Retry, Secret};
use concord_testkit::{Script, Server};
use std::sync::{Mutex, Once};

/// A secret: a key in a path, a query, a header or a body.
const SECRET: &str = "secret-value-4b7e";
/// A value of the query that is not a secret.
const QUERY: &str = "query-value-c21d";
/// A value of a body, of the request or of the answer.
const BODY: &str = "body-value-90fa";
/// A value of a header of the answer.
const HEADER: &str = "header-value-6e13";
/// A value of a path that is not a secret.
const PATH: &str = "path-value-d58c";
/// Every value that no printed line and no log may hold.
const VALUES: [&str; 5] = [SECRET, QUERY, BODY, HEADER, PATH];

// What is logged ----------------------------------------------------------------------

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INSTALL: Once = Once::new();

fn note(line: String) {
    LINES.lock().unwrap_or_else(|p| p.into_inner()).push(line);
}

/// Writes every field of an event or a span, with its `Debug`.
struct Fields(String);

impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(&format!(" {}={value:?}", field.name()));
    }
}

/// A `tracing` subscriber that keeps every event and span, at every level.
struct Capture;

impl tracing::Subscriber for Capture {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        let mut fields = Fields(format!("span {}:", span.metadata().target()));
        span.record(&mut fields);
        note(fields.0);
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, values: &tracing::span::Record<'_>) {
        let mut fields = Fields("span record:".to_owned());
        values.record(&mut fields);
        note(fields.0);
    }
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = Fields(format!(
            "event {} {}:",
            event.metadata().level(),
            event.metadata().target()
        ));
        event.record(&mut fields);
        note(fields.0);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

/// A `log` logger that keeps every record, at every level.
struct LogCapture;

impl log::Log for LogCapture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &log::Record<'_>) {
        note(format!(
            "log {} {}: {}",
            record.level(),
            record.target(),
            record.args()
        ));
    }
    fn flush(&self) {}
}

static LOGGER: LogCapture = LogCapture;

/// Capture what is logged, once for the binary.
fn capture() {
    INSTALL.call_once(|| {
        tracing::subscriber::set_global_default(Capture).expect("the only subscriber");
        log::set_logger(&LOGGER).expect("the only logger");
        log::set_max_level(log::LevelFilter::Trace);
    });
}

/// Every line logged so far.
fn logged() -> Vec<String> {
    LINES.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// The values that a text holds, case aside (a host is lowered by the URL parser).
fn values_in(text: &str) -> Vec<&'static str> {
    let text = text.to_ascii_lowercase();
    VALUES
        .into_iter()
        .filter(|value| text.contains(value))
        .collect()
}

/// `Display`, `Debug` and `{:#?}` of an error, and of each cause of its chain.
fn printed(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = format!("{error}\n{error:?}\n{error:#?}");
    let mut cause = error.source();
    while let Some(next) = cause {
        text.push_str(&format!("\n{next}\n{next:?}"));
        cause = next.source();
    }
    text
}

/// No value in what the error prints, in its chain, or in what was logged so far.
#[track_caller]
fn clean<E: 'static>(case: &str, error: &concord::Error<E>) {
    let text = printed(error);
    assert!(values_in(&text).is_empty(), "{case}: {text}");
    let leaks: Vec<_> = logged()
        .into_iter()
        .filter(|line| !values_in(line).is_empty())
        .collect();
    assert!(leaks.is_empty(), "{case}: logged {leaks:#?}");
}

/// `shapes.guarded` with a secret in its path, its query and a header, and a value in its
/// query.
fn guarded(shapes: &tour::Shapes) -> tour::shapes::Guarded {
    shapes
        .guarded(Secret::new(SECRET))
        .query_key(Secret::new(SECRET))
        .header_key(Secret::new(SECRET))
        .cursor(QUERY)
}

// No value, after each kind of failure ------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c6_2_c10_7_no_value_after_a_build_error() {
    capture();
    let shapes = tour::Shapes::new();
    let header = shapes
        .guarded(Secret::new(SECRET))
        .header_key(Secret::new(format!("{SECRET}\n")))
        .cursor(QUERY);
    let label = shapes.upload(Body::new(BODY, format!("text/{HEADER}\r\n")));
    let host = shapes.hosted(format!("{PATH}.{SECRET}"));
    let parsed = shapes.parsed(QUERY);
    let too_long = tour::Files::new(SECRET).readme(PATH.repeat(5_000));
    for (case, error) in [
        ("a header that is not legal", failure(header.send().await)),
        ("a label that is not legal", failure(label.await)),
        ("a host hole that is no label", failure(host.await)),
        ("an entry whose expression fails", failure(parsed.await)),
    ] {
        assert_eq!(error.kind(), Kind::Build, "{case}");
        clean(case, &error);
    }
    let error = failure(too_long.await);
    assert_eq!(error.kind(), Kind::Build);
    clean("a URL that is too long", &error);
}

#[tokio::test(start_paused = true)]
async fn c6_2_c10_6_c10_7_no_value_after_a_refusal_tried_three_times() {
    capture();
    let script = Script::new();
    for _ in 0..3 {
        script
            .answer(503)
            .header("x-value", HEADER)
            .header("content-type", "application/json")
            .body(format!(r#"{{"code":"{BODY}","message":"{BODY}"}}"#));
    }
    let shapes = shapes(&script).with_retry(Retry::standard());

    let error = failure(guarded(&shapes).send().await);

    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.tries(), 3);
    clean("a 503 three times", &error);
    // Concord's event `retry` was logged, twice, with no value.
    let retries = logged()
        .into_iter()
        .filter(|line| line.contains("concord") && line.contains("shapes.guarded"))
        .count();
    assert!(retries >= 2, "{:#?}", logged());
}

#[tokio::test(start_paused = true)]
async fn c6_2_c10_7_no_value_after_a_mismatch() {
    capture();
    let script = Script::new();
    json_of(
        &script,
        format!(r#"{{"counts":{{"{BODY}":[{{"kind":"{BODY}","n":"{BODY}"}}]}}}}"#),
    );
    json_of(&script, format!(r#"{{"id":"{BODY}","title":"{BODY}"}}"#));
    json_of(&script, format!(r#"{{"id":1,"title":"{BODY}""#));
    script
        .answer(200)
        .header("content-type", &format!("text/{HEADER}"))
        .body(format!(r#""{BODY}""#));
    let shapes = shapes(&script);
    for (case, error) in [
        ("a map whose key is a value", failure(shapes.counts().await)),
        ("a field of another type", failure(shapes.again(1).await)),
        ("a body cut short", failure(shapes.again(1).await)),
        ("a label that holds a value", failure(shapes.strict().await)),
    ] {
        assert_eq!(error.kind(), Kind::Mismatch, "{case}");
        clean(case, &error);
    }
}

#[tokio::test(start_paused = true)]
async fn c6_2_c10_7_no_value_after_a_failure_of_the_transport() {
    capture();
    let script = Script::new();
    script.never();
    script
        .answer(200)
        .header("x-value", HEADER)
        .header("content-type", "application/json")
        .body(BODY)
        .cut();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(BODY)
        .stall();
    let shapes = shapes(&script);
    for (case, error) in [
        (
            "silence before the head",
            failure(guarded(&shapes).send().await),
        ),
        ("a body that broke", failure(guarded(&shapes).send().await)),
        (
            "a body that was silent",
            failure(guarded(&shapes).send().await),
        ),
        (
            "a transport that failed",
            failure(
                guarded(&tour::Shapes::new().with_transport(FailsWith(Class::Connect)))
                    .send()
                    .await,
            ),
        ),
    ] {
        assert_eq!(error.kind(), Kind::Transport, "{case}");
        clean(case, &error);
    }
}

#[tokio::test]
async fn c6_2_c10_6_c10_7_no_value_after_a_failure_on_the_wire() {
    // Concord's own client, a real server: what the HTTP library logs is searched too.
    capture();
    let server = Server::start().await;
    let path = format!("/guarded/{SECRET}");
    for _ in 0..3 {
        server
            .on("GET", &path)
            .reply(503)
            .header("x-value", HEADER)
            .body(BODY);
    }
    for _ in 0..3 {
        server.on("GET", &path).close();
    }
    server
        .on("POST", "/posted")
        .reply(422)
        .header("content-type", "application/json")
        .body(format!(r#"{{"code":"{BODY}","message":"{BODY}"}}"#));
    let http = concord::Http::with(|builder| builder.no_proxy()).expect("an Http");
    let shapes = tour::Shapes::new()
        .with_transport(http.clone())
        .with_base(server.url())
        .with_retry(Retry::standard());
    let (_bound, port) = refusing_port();
    let refusing = tour::Shapes::new()
        .with_transport(http.clone())
        .with_base(format!("http://127.0.0.1:{port}"));
    let (unreadable, _) = raw_server(Some(
        format!("HTTP/1.1 200 OK\r\nx-value: {HEADER}\r\ncontent-length: 1\r\ncontent-length: 2\r\n\r\n{BODY}")
            .into_bytes(),
    ));
    let unreadable = tour::Shapes::new()
        .with_transport(http)
        .with_base(unreadable);

    for (case, kind, error) in [
        (
            "a 503 three times",
            Kind::Refused,
            failure(guarded(&shapes).send().await),
        ),
        (
            "a connection closed before the head, three times",
            Kind::Transport,
            failure(guarded(&shapes).send().await),
        ),
        (
            "a 422 to a body of entries that holds a secret",
            Kind::Refused,
            failure(
                shapes
                    .posted()
                    .token(Secret::new(SECRET))
                    .cursor(BODY)
                    .send()
                    .await,
            ),
        ),
        (
            "a connection refused",
            Kind::Transport,
            failure(guarded(&refusing).send().await),
        ),
        (
            "a head that cannot be read",
            Kind::Transport,
            failure(guarded(&unreadable).send().await),
        ),
    ] {
        assert_eq!(error.kind(), kind, "{case}");
        clean(case, &error);
    }
    assert_eq!(server.unused(), 0);
}

// The server cases that protect a caller ----------------------------------------------

/// `Tasks` on this server, through Concord's own client.
fn tasks_on(server: &Server) -> tour::Tasks {
    tour::Tasks::new()
        .with_transport(concord::Http::with(|builder| builder.no_proxy()).expect("an Http"))
        .with_base(format!("{}/api/tasks", server.url()))
}

#[tokio::test]
async fn c5_4_a_body_of_1_gib_announced_is_refused_before_it_is_read() {
    // C5.4: "an announced size over the limit is a Mismatch before anything is read". The
    // server announces 1 GiB, sends 8 bytes, and is then silent: a reader that waited for
    // the body would end on the clock for silence, a Transport error.
    capture();
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(200)
        .header("content-type", "application/json")
        .declare_length(1 << 30)
        .body(r#"{"id":7,"#)
        .silence();

    let error = failure(tasks_on(&server).with_idle_timeout(secs(5)).get(7).await);

    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, the body is larger than the limit of the reader, in tasks.get"
    );
}

#[tokio::test]
async fn c5_6_an_error_body_of_100_mib_keeps_64_kib() {
    // C5.6: "a Refused error and a Mismatch keep up to 64 KiB of the body". Announced, and
    // not announced.
    capture();
    let server = Server::start().await;
    server
        .on("GET", "/api/tasks/7")
        .reply(500)
        .header("content-type", "application/json")
        .large_body(100 << 20);
    server
        .on("GET", "/api/tasks/7")
        .reply(500)
        .header("content-type", "application/json")
        .chunked()
        .large_body(100 << 20);
    let tasks = tasks_on(&server);

    for case in ["announced", "chunked"] {
        let error = failure(tasks.get(7).await);
        assert_eq!(error.kind(), Kind::Refused, "{case}");
        assert!(error.is_status(500), "{case}");
        assert_eq!(error.body().map(<[u8]>::len), Some(64 * 1024), "{case}");
        assert_eq!(error.kept(), Some(Kept::Cut), "{case}");
    }
}

#[tokio::test]
async fn c7_1_a_301_to_another_host_is_not_followed() {
    // C7.1: "no redirect is followed (a `3xx` is the answer)". The other host gets nothing,
    // and the `location` stays in the headers of the error.
    capture();
    let server = Server::start().await;
    let other = Server::start().await;
    let location = format!("{}/api/tasks/7", other.url());
    server
        .on("GET", "/api/tasks/7")
        .reply(301)
        .header("location", &location);
    other
        .on("GET", "/api/tasks/7")
        .reply(200)
        .header("content-type", "application/json")
        .body(TASK);

    let error = failure(tasks_on(&server).get(7).await);

    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.to_string(), "refused: status 301, in tasks.get");
    assert_eq!(
        error.headers().and_then(|headers| headers.get("location")),
        Some(location.as_str())
    );
    assert!(other.requests().is_empty());
}

#[tokio::test]
async fn c8_4_a_link_to_another_origin_is_a_build_error_and_the_other_host_gets_nothing() {
    // C8.4, and messages section 2: "a page that leaves the origin of the first request of
    // its `.pages()`" is `build error: the URL is at another origin than the call`.
    capture();
    let server = Server::start().await;
    let other = Server::start().await;
    server
        .on("GET", "/linked-items")
        .reply(200)
        .header("content-type", "application/json")
        .header(
            "link",
            &format!(r#"<{}/linked-items?k={QUERY}>; rel="next""#, other.url()),
        )
        .body(r#"{"data":[],"next_cursor":null}"#);
    let feed = tour::Feed::new()
        .with_transport(concord::Http::with(|builder| builder.no_proxy()).expect("an Http"))
        .with_base(server.url());
    let mut pages = feed.linked().pages();

    assert!(success(pages.try_next().await).is_some());
    let error = failure(pages.try_next().await);

    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(
        error.to_string(),
        "build error: the URL is at another origin than the call, in feed.linked"
    );
    clean("a Link to another origin", &error);
    assert!(other.requests().is_empty());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_3_a_retry_after_of_a_year_is_one_try() {
    // C4.3: "A `Retry-After` [...] over 60 seconds means no other try: the error is the
    // result, and `e.retry_after()` says how long." A year, and 2^64 seconds.
    capture();
    for (value, wait) in [
        ("31536000", Some(secs(31_536_000))),
        ("99999999999999999999999", None),
    ] {
        let script = Script::new();
        for _ in 0..3 {
            script.answer(503).header("retry-after", value);
        }
        script
            .answer(200)
            .header("content-type", "application/json")
            .body(TASK);

        let error = failure(tasks(&script).with_retry(Retry::standard()).get(7).await);

        assert!(error.is_status(503), "{value}");
        assert_eq!(error.tries(), 1, "{value}");
        assert_eq!(script.requests().len(), 1, "{value}");
        if let Some(wait) = wait {
            assert_eq!(error.retry_after(), Some(wait), "{value}");
        }
    }
}

/// The next try answers `200` with this JSON body.
fn json_of(script: &Script, body: String) {
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(body);
}
