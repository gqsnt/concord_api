//! C12.11 to C12.14, read from outside: `Body::get_content_type`, `Body::repeatable`,
//! `Body::is_repeatable` and `Body::map_stream` (docs/reference/contracts.md).
//!
//! Written from the public surface and its rustdoc alone, beside the author's own tests
//! (`c12_repeatable.rs`), which this file does not use. Every test goes through the facade, and
//! where a promise is about what is sent, through a client of `concord::api!` over a
//! `concord_testkit::Script` or over a transport of this file, on Tokio's paused clock.
//!
//! The source of bytes is this file's own: it counts how many times it was opened, when, and
//! how many chunks each opening gave.

mod common;

use concord::error::{Class, Kind};
use concord::limit::Limit;
use concord::sign::{CanonicalPath, Credentials, SigV4, SignError};
use concord::transport::{Transport, TransportError, TryInfo};
use concord::{bytes::Bytes, reqwest, Body, Middleware, Next, Retry};
use concord_testkit::script::UploadState;
use concord_testkit::Script;
use serde::Deserialize;
use std::collections::VecDeque;
use std::error::Error as _;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{sleep, Instant, Sleep};

// The descriptions -------------------------------------------------------------------------

/// A page of a search: its items, and the cursor of the next page.
#[derive(Debug, Deserialize)]
struct Page {
    items: Vec<u32>,
    #[serde(default)]
    next: Option<String>,
}

impl Page {
    fn next_cursor(&self) -> Option<String> {
        self.next.clone()
    }
}

impl concord::page::IntoItems for Page {
    type Item = u32;
    fn into_items(self) -> Vec<u32> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

/// A writer of the caller's own that wraps another body and keeps its content type.
fn shouted(body: &Body) -> std::io::Result<Body> {
    let bytes = body
        .as_bytes()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "a buffered body"))?;
    let mut output = Body::from(bytes.to_ascii_uppercase());
    if let Some(label) = body.get_content_type() {
        output = output.content_type(label.to_owned());
    }
    Ok(output)
}

concord::api! {
    client Store {
        base "https://store.example.test";
        PUT put(file: Body) { path "objects/a"; body file; }
        PUT sized(file: Body, size: u64) {
            path "objects/b";
            headers { "content-length" = size; }
            body file;
        }
        POST post(file: Body) { path "objects"; body file; }
        PUT once(file: Body) { path "objects/c"; body file; idempotent false; }
        PUT labelled(file: Body) {
            path "objects/d";
            headers { "content-type" = "application/x-special"; }
            body file;
        }
        POST shout(text: Body) { path "shout"; body shouted(text); }
        POST search(file: Body, cursor?: String) -> Page {
            path "search";
            query { cursor; }
            body file;
            read concord::Json;
            paginate concord::by_cursor(cursor, Page::next_cursor);
        }
    }
}

concord::api! {
    client Bucket(signer: SigV4) {
        base "https://photos.s3.eu-west-1.amazonaws.com";
        middleware signer.clone();
        PUT put(file: Body) { path "reports/a.bin"; body file; }
    }
}

// The source of bytes ----------------------------------------------------------------------

/// What a source fails with: at its opening number `n`, or while read in opening `n`.
#[derive(Debug, PartialEq, Eq)]
enum SourceError {
    Open(usize),
    Read(usize),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the source of the test failed")
    }
}

impl std::error::Error for SourceError {}

/// The plan of a source: what each opening gives, and how.
struct Plan {
    chunks: Vec<Bytes>,
    /// The pause before chunk `i`, on Tokio's clock; missing: none.
    pauses: Vec<Duration>,
    /// How long an opening takes before it gives its stream.
    opening_takes: Duration,
    /// The openings (counted from one) that fail.
    failing_openings: Vec<usize>,
    /// Every opening fails after this many chunks.
    fails_after: Option<usize>,
    /// After the chunks, this chunk again and again, for ever.
    tail: Option<Bytes>,
}

impl Plan {
    fn new(chunks: &[&[u8]]) -> Plan {
        Plan {
            chunks: chunks.iter().map(|c| Bytes::copy_from_slice(c)).collect(),
            pauses: Vec::new(),
            opening_takes: Duration::ZERO,
            failing_openings: Vec::new(),
            fails_after: None,
            tail: None,
        }
    }
    fn pauses(mut self, pauses: &[Duration]) -> Plan {
        self.pauses = pauses.to_vec();
        self
    }
    fn every_chunk_after(mut self, pause: Duration) -> Plan {
        self.pauses = vec![pause; self.chunks.len()];
        self
    }
    fn opening_takes(mut self, wait: Duration) -> Plan {
        self.opening_takes = wait;
        self
    }
    fn failing_openings(mut self, openings: &[usize]) -> Plan {
        self.failing_openings = openings.to_vec();
        self
    }
    fn fails_after(mut self, chunks: usize) -> Plan {
        self.fails_after = Some(chunks);
        self
    }
    fn endless(mut self, tail: &'static [u8]) -> Plan {
        self.tail = Some(Bytes::from_static(tail));
        self
    }
    fn source(self) -> Source {
        Source(Arc::new(Shared {
            plan: self,
            start: Instant::now(),
            opens: Mutex::new(Vec::new()),
            pulls: Mutex::new(Vec::new()),
        }))
    }
}

struct Shared {
    plan: Plan,
    start: Instant,
    /// When each opening began (the call of the function given to `repeatable`).
    opens: Mutex<Vec<Duration>>,
    /// How many chunks each opening gave.
    pulls: Mutex<Vec<usize>>,
}

/// A source of bytes that counts.
#[derive(Clone)]
struct Source(Arc<Shared>);

impl Source {
    /// A `Body::repeatable` over this source. Opening = the call of `make`.
    fn body(&self) -> Body {
        let source = self.clone();
        Body::repeatable(move || {
            let number = source.opened();
            let source = source.clone();
            async move { source.open(number).await }
        })
    }

    fn opened(&self) -> usize {
        let mut opens = self.0.opens.lock().expect("opens");
        opens.push(self.0.start.elapsed());
        self.0.pulls.lock().expect("pulls").push(0);
        opens.len()
    }

    async fn open(self, number: usize) -> Result<Chunks, SourceError> {
        if !self.0.plan.opening_takes.is_zero() {
            sleep(self.0.plan.opening_takes).await;
        }
        if self.0.plan.failing_openings.contains(&number) {
            return Err(SourceError::Open(number));
        }
        Ok(Chunks {
            source: self,
            opening: number,
            index: 0,
            sleep: None,
            done: false,
        })
    }

    /// When each opening began, from the creation of the source.
    fn opens(&self) -> Vec<Duration> {
        self.0.opens.lock().expect("opens").clone()
    }

    fn opened_times(&self) -> usize {
        self.opens().len()
    }

    /// How many chunks each opening gave.
    fn pulls(&self) -> Vec<usize> {
        self.0.pulls.lock().expect("pulls").clone()
    }

    /// Every byte of one opening, as planned.
    fn bytes(&self) -> Vec<u8> {
        self.0.plan.chunks.iter().flatten().copied().collect()
    }
}

/// The stream of one opening.
struct Chunks {
    source: Source,
    opening: usize,
    index: usize,
    sleep: Option<Pin<Box<Sleep>>>,
    done: bool,
}

impl futures_core::Stream for Chunks {
    type Item = Result<Bytes, SourceError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        if this.done {
            return Poll::Ready(None);
        }
        let plan = &this.source.0.plan;
        if plan.fails_after == Some(this.index) {
            this.done = true;
            return Poll::Ready(Some(Err(SourceError::Read(this.opening))));
        }
        let next = plan
            .chunks
            .get(this.index)
            .cloned()
            .or_else(|| plan.tail.clone());
        let Some(next) = next else {
            this.done = true;
            return Poll::Ready(None);
        };
        let pause = plan.pauses.get(this.index).copied().unwrap_or_default();
        if !pause.is_zero() {
            let timer = this.sleep.get_or_insert_with(|| Box::pin(sleep(pause)));
            if timer.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }
            this.sleep = None;
        }
        this.index += 1;
        this.source.0.pulls.lock().expect("pulls")[this.opening - 1] += 1;
        Poll::Ready(Some(Ok(next)))
    }
}

/// Bytes that are not text: a NUL, an invalid UTF-8 sequence, a CRLF.
const PAYLOAD: [&[u8]; 3] = [b"\x00\x01\xff", b"not text \xc3\x28", b"end\r\n"];

fn payload() -> Vec<u8> {
    PAYLOAD.iter().copied().flatten().copied().collect()
}

// Transports and middlewares of the caller --------------------------------------------------

/// What a [`Wire`] does with one attempt.
#[derive(Clone, Copy, Debug)]
enum Then {
    Status(u16),
    Fail(Class),
}

#[derive(Clone, Copy, Debug)]
enum Step {
    /// Read the body to its end (or its error), then answer.
    ReadAll(Then),
    /// Read this many chunks, drop the rest, then answer.
    ReadChunks(usize, Then),
}

/// What a [`Wire`] read of one attempt.
#[derive(Clone, Debug, Default)]
struct Seen {
    bytes: Vec<u8>,
    chunks: usize,
    /// The body gave an error while it was read.
    failed: bool,
}

#[derive(Debug, Default)]
struct WireState {
    steps: VecDeque<Step>,
    seen: Vec<Seen>,
}

/// A transport of the caller's own that reads the body as it is told, and then reports
/// what it is told, whatever happened to the body.
#[derive(Clone, Debug, Default)]
struct Wire(Arc<Mutex<WireState>>);

impl Wire {
    fn new(steps: &[Step]) -> Wire {
        let wire = Wire::default();
        wire.0.lock().expect("wire").steps = steps.iter().copied().collect();
        wire
    }
    fn seen(&self) -> Vec<Seen> {
        self.0.lock().expect("wire").seen.clone()
    }
}

impl Transport for Wire {
    async fn send(
        &self,
        mut request: reqwest::Request,
        _info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        let step = self
            .0
            .lock()
            .expect("wire")
            .steps
            .pop_front()
            .unwrap_or(Step::ReadAll(Then::Status(599)));
        let (most, then) = match step {
            Step::ReadAll(then) => (usize::MAX, then),
            Step::ReadChunks(n, then) => (n, then),
        };
        let mut seen = Seen::default();
        if let Some(body) = request.body_mut().take() {
            let mut response = reqwest::Response::from(http::Response::new(body));
            while seen.chunks < most {
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        seen.chunks += 1;
                        seen.bytes.extend_from_slice(&chunk);
                    }
                    Ok(None) => break,
                    Err(_) => {
                        seen.failed = true;
                        break;
                    }
                }
            }
        }
        self.0.lock().expect("wire").seen.push(seen);
        match then {
            Then::Status(status) => Ok(concord::reply(status, "")),
            Then::Fail(class) => Err(TransportError::new(class)),
        }
    }
}

/// A middleware of the caller that reads the whole body of each attempt, keeps it, and
/// sends the same bytes on.
#[derive(Clone, Debug, Default)]
struct ReadsBody(Arc<Mutex<Vec<Vec<u8>>>>);

impl ReadsBody {
    fn seen(&self) -> Vec<Vec<u8>> {
        self.0.lock().expect("seen").clone()
    }
}

impl Middleware for ReadsBody {
    async fn handle(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        if let Some(body) = request.body_mut().take() {
            let response = reqwest::Response::from(http::Response::new(body));
            let bytes = response
                .bytes()
                .await
                .map_err(|e| TransportError::new(Class::Other).with_source(e))?;
            self.0.lock().expect("seen").push(bytes.to_vec());
            *request.body_mut() = Some(reqwest::Body::from(bytes));
        }
        next.send(request, info).await
    }
}

// What the tests share ----------------------------------------------------------------------

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Retry a 5xx or a failed connection, after exactly 1, 2 and 4 seconds (each wait is at
/// least the random part of the wait, so it is exact), four attempts in all.
fn waits() -> Retry {
    Retry::none().tries(4).when(|tried| {
        let worth = tried.status().is_some_and(|s| s.is_server_error())
            || tried.class() == Some(Class::Connect);
        worth.then(|| secs(1 << (tried.number() - 1)))
    })
}

fn store(script: &Script) -> Store {
    Store::new().with_transport(script.clone())
}

fn store_on(wire: &Wire) -> Store {
    Store::new().with_transport(wire.clone())
}

/// `n` answers with this status that read the whole upload first.
fn answers(script: &Script, statuses: &[u16]) {
    for status in statuses {
        script.answer(*status).read_upload();
    }
}

/// What the script read of each upload, in order.
fn uploads(script: &Script) -> Vec<Vec<u8>> {
    script
        .requests()
        .iter()
        .map(|request| {
            assert_eq!(
                request.upload().state(),
                &UploadState::Ended,
                "the upload was read to its end"
            );
            request.upload().bytes().to_vec()
        })
        .collect()
}

/// When each request reached the script, from `start`.
fn sent_at(script: &Script, start: Instant) -> Vec<Duration> {
    script
        .requests()
        .iter()
        .map(|request| request.at() - start)
        .collect()
}

#[track_caller]
fn success<T, E>(result: Result<T, concord::Error<E>>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!(
            "expected a value, got {:?} (class {:?}, status {:?}): {error}",
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

/// The error is a Transport error of class `Other` whose `source()` is this failure of the
/// source, itself and not a wrapper.
#[track_caller]
fn is_the_source_failure(error: &concord::Error, expected: SourceError) {
    if let Err(why) = source_failure(error, expected) {
        panic!("{why}");
    }
}

/// [`is_the_source_failure`], as a result, for a test that runs several cases.
fn source_failure(error: &concord::Error, expected: SourceError) -> Result<(), String> {
    if (error.kind(), error.class()) != (Kind::Transport, Some(Class::Other)) {
        return Err(format!(
            "expected Transport (class Other), got {:?} (class {:?})",
            error.kind(),
            error.class()
        ));
    }
    let Some(source) = error.source() else {
        return Err("the error has no source".to_owned());
    };
    match source.downcast_ref::<SourceError>() {
        Some(found) if *found == expected => Ok(()),
        _ => Err(format!(
            "the source() of the error is not the failure of the source {expected:?}: {source:?}"
        )),
    }
}

/// The error is a Transport error of class `Other` whose `source()` is an `io::Error` of
/// kind `InvalidData`.
#[track_caller]
fn is_invalid_data(error: &concord::Error) {
    if let Err(why) = invalid_data(error) {
        panic!("{why}");
    }
}

/// [`is_invalid_data`], as a result, for a test that runs several cases.
fn invalid_data(error: &concord::Error) -> Result<(), String> {
    if (error.kind(), error.class()) != (Kind::Transport, Some(Class::Other)) {
        return Err(format!(
            "expected Transport (class Other), got {:?} (class {:?})",
            error.kind(),
            error.class()
        ));
    }
    let Some(source) = error.source() else {
        return Err("the error has no source".to_owned());
    };
    match source.downcast_ref::<std::io::Error>() {
        Some(io) if io.kind() == std::io::ErrorKind::InvalidData => Ok(()),
        _ => Err(format!(
            "the source() of the error is not an io::Error of kind InvalidData: {source:?}"
        )),
    }
}

/// The next item of a body taken as a stream.
async fn next_item(
    stream: &mut concord::body::BodyStream,
) -> Option<Result<Bytes, Box<dyn std::error::Error + Send + Sync>>> {
    std::future::poll_fn(|cx| futures_core::Stream::poll_next(stream.as_mut(), cx)).await
}

/// Every byte of a body taken as a stream, or the test fails.
async fn read_all(mut stream: concord::body::BodyStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    while let Some(item) = next_item(&mut stream).await {
        bytes.extend_from_slice(&item.expect("a chunk"));
    }
    bytes
}

/// Every byte of the body of a request (a stream included).
async fn read_request_body(request: &mut reqwest::Request) -> Vec<u8> {
    let body = request.body_mut().take().expect("the request has a body");
    let response = reqwest::Response::from(http::Response::new(body));
    response.bytes().await.expect("the body reads").to_vec()
}

// C12.11: `Body::get_content_type` ------------------------------------------------------------

#[test]
fn c12_11_the_label_is_returned_whole_with_its_parameters_and_unvalidated() {
    for label in [
        "multipart/form-data; boundary=\"a b\"; charset=UTF-8",
        "text/csv;charset=utf-8",
        "not a media type at all",
        "TEXT/PLAIN ;  q=0.5 ",
        "",
        "application/vnd.ex+json; v=\"é\"",
    ] {
        let body = Body::new("x", label);
        assert_eq!(body.get_content_type(), Some(label), "{label:?}");
        assert_eq!(body.clone().get_content_type(), Some(label), "a clone");
    }
}

#[test]
fn c12_11_a_stream_and_a_repeatable_carry_no_label_until_one_is_set() {
    let source = Plan::new(&PAYLOAD).source();
    let repeatable = source.body();
    assert_eq!(repeatable.get_content_type(), None);
    let labelled = repeatable.content_type("application/x-tar; v=2");
    assert_eq!(labelled.get_content_type(), Some("application/x-tar; v=2"));
    let relabelled = labelled.content_type("application/zip");
    assert_eq!(relabelled.get_content_type(), Some("application/zip"));
    let stream = common::stream("abc");
    assert_eq!(stream.get_content_type(), None);
    assert_eq!(
        stream.content_type("text/plain").get_content_type(),
        Some("text/plain")
    );
    assert_eq!(source.opened_times(), 0, "reading the label opens nothing");
}

#[test]
fn c12_11_gzip_keeps_the_label_whole_and_none_stays_none() {
    let label = "text/csv; charset=utf-8; header=present";
    let compressed = match concord::gzip(Body::new("a,b\n1,2\n", label)) {
        Ok(body) => body,
        Err(_) => panic!("gzip compresses bytes"),
    };
    assert_eq!(compressed.get_content_type(), Some(label));
    let unlabelled = match concord::gzip(Body::from(b"\x00\xff".to_vec())) {
        Ok(body) => body,
        Err(_) => panic!("gzip compresses bytes"),
    };
    assert_eq!(unlabelled.get_content_type(), None);
}

#[tokio::test(start_paused = true)]
async fn c12_11_the_label_is_sent_whole_and_a_header_of_the_description_wins() {
    let script = Script::new();
    answers(&script, &[200, 200]);
    let label = "text/plain; charset=utf-8; format=flowed";
    let body = Body::new("hello", label);
    success(store(&script).put(&body).await);
    success(store(&script).labelled(&body).await);
    let requests = script.requests();
    assert_eq!(
        requests[0]
            .headers()
            .get_all("content-type")
            .iter()
            .collect::<Vec<_>>(),
        [label]
    );
    assert_eq!(
        requests[1]
            .headers()
            .get_all("content-type")
            .iter()
            .collect::<Vec<_>>(),
        ["application/x-special"],
        "the description's header, once"
    );
    assert_eq!(
        body.get_content_type(),
        Some(label),
        "the body keeps its label"
    );
}

#[tokio::test(start_paused = true)]
async fn c12_11_a_wrapping_writer_of_the_caller_keeps_the_label_on_the_wire() {
    let script = Script::new();
    answers(&script, &[200]);
    let label = "text/x-shout; charset=us-ascii";
    success(store(&script).shout(Body::new("quiet words", label)).await);
    let requests = script.requests();
    assert_eq!(requests[0].header("content-type"), Some(label));
    assert_eq!(requests[0].body(), Some(&b"QUIET WORDS"[..]));
}

#[tokio::test(start_paused = true)]
async fn c12_11_the_label_of_a_repeatable_is_sent_on_every_attempt() {
    let script = Script::new();
    answers(&script, &[503, 200]);
    let source = Plan::new(&PAYLOAD).source();
    let label = "application/x-tar; v=2";
    success(
        store(&script)
            .put(source.body().content_type(label))
            .with_retry(waits())
            .await,
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.header("content-type"), Some(label));
    }
    assert_eq!(uploads(&script), [payload(), payload()]);
}

// C12.12: opened once for each attempt, from its first byte ------------------------------

#[test]
fn c12_12_building_cloning_inspecting_and_dropping_open_nothing() {
    let source = Plan::new(&PAYLOAD).source();
    let body = source.body();
    let clone = body.clone();
    assert!(body.as_bytes().is_none());
    assert_eq!(body.get_content_type(), None);
    let _ = format!("{body:?} {clone:?}");
    let from_ref = Body::from(&body);
    let concat = Body::concat([Body::from("a"), body.clone(), Body::from("b")]);
    assert!(concat.as_bytes().is_none());
    let unpolled = clone.into_stream().expect("the body is taken as a stream");
    drop((body, from_ref, concat, unpolled));
    // A call built, cloned, and dropped, with no runtime.
    let script = Script::new();
    let call = store(&script).put(source.body());
    let other = call.clone();
    let _ = format!("{call:?}");
    drop((call, other));
    assert_eq!(source.opened_times(), 0);
    assert!(script.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn c12_12_into_stream_opens_on_the_first_poll_with_an_independent_cursor_each() {
    let source = Plan::new(&PAYLOAD).source();
    let body = source.body();
    let mut first = body.clone().into_stream().expect("a stream");
    let second = body.into_stream().expect("a stream");
    assert_eq!(source.opened_times(), 0, "taking the streams opens nothing");
    let chunk = next_item(&mut first)
        .await
        .expect("a chunk")
        .expect("a chunk");
    assert_eq!(&chunk[..], PAYLOAD[0]);
    assert_eq!(source.opened_times(), 1);
    assert_eq!(
        read_all(second).await,
        payload(),
        "the second starts at byte zero"
    );
    assert_eq!(source.opened_times(), 2);
    let mut rest = chunk.to_vec();
    rest.extend(read_all(first).await);
    assert_eq!(rest, payload(), "the first goes on where it was");
    assert_eq!(source.pulls(), [3, 3]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_one_attempt_opens_once_and_sends_exact_bytes() {
    let script = Script::new();
    answers(&script, &[200]);
    let source = Plan::new(&PAYLOAD).source();
    success(store(&script).put(source.body()).with_retry(waits()).await);
    assert_eq!(source.opens(), [Duration::ZERO]);
    assert_eq!(source.pulls(), [3]);
    assert_eq!(uploads(&script), [payload()]);
    assert_eq!(script.requests()[0].upload().pieces(), 3);
}

#[tokio::test(start_paused = true)]
async fn c12_12_an_empty_source_sends_an_empty_body_at_each_attempt() {
    let script = Script::new();
    answers(&script, &[503, 200]);
    let source = Plan::new(&[]).source();
    success(store(&script).put(source.body()).with_retry(waits()).await);
    assert_eq!(source.opens(), [secs(0), secs(1)]);
    assert_eq!(uploads(&script), [Vec::<u8>::new(), Vec::new()]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_each_retry_opens_again_when_it_is_sent_and_starts_at_byte_zero() {
    let script = Script::new();
    answers(&script, &[503, 502, 504, 200]);
    let start = Instant::now();
    let source = Plan::new(&PAYLOAD).source();
    success(store(&script).put(source.body()).with_retry(waits()).await);
    let at = [secs(0), secs(1), secs(3), secs(7)];
    assert_eq!(sent_at(&script, start), at);
    assert_eq!(source.opens(), at, "each opening is at its attempt");
    assert_eq!(source.pulls(), [3, 3, 3, 3]);
    assert_eq!(uploads(&script), vec![payload(); 4]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_half_sent_upload_is_sent_again_from_byte_zero() {
    // The transport reads one chunk of three, then the connection fails.
    let wire = Wire::new(&[
        Step::ReadChunks(1, Then::Fail(Class::Connect)),
        Step::ReadAll(Then::Status(200)),
    ]);
    let source = Plan::new(&PAYLOAD).source();
    success(store_on(&wire).put(source.body()).with_retry(waits()).await);
    let seen = wire.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].bytes, PAYLOAD[0]);
    assert_eq!(seen[1].bytes, payload(), "never resumed: from byte zero");
    assert!(!seen[1].failed);
    assert_eq!(source.opens(), [secs(0), secs(1)]);
    assert_eq!(source.pulls()[1], 3);
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_body_read_whole_then_a_lost_connection_is_sent_whole_again() {
    let wire = Wire::new(&[
        Step::ReadAll(Then::Fail(Class::Connect)),
        Step::ReadAll(Then::Fail(Class::Connect)),
        Step::ReadAll(Then::Status(200)),
    ]);
    let source = Plan::new(&PAYLOAD).source();
    success(store_on(&wire).put(source.body()).with_retry(waits()).await);
    let seen = wire.seen();
    assert_eq!(seen.len(), 3);
    for attempt in &seen {
        assert_eq!(attempt.bytes, payload());
        assert!(!attempt.failed);
    }
    assert_eq!(source.opens(), [secs(0), secs(1), secs(3)]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_repeatable_body_makes_a_retry_possible_not_allowed() {
    // A POST that is not marked: a 503 is not retried, and a lost connection is not either.
    let script = Script::new();
    answers(&script, &[503, 200]);
    let source = Plan::new(&PAYLOAD).source();
    let error = failure(store(&script).post(source.body()).with_retry(waits()).await);
    assert!(error.is_status(503));
    assert_eq!(error.tries(), 1);
    assert_eq!(source.opened_times(), 1);
    assert_eq!(script.unused(), 1);

    let wire = Wire::new(&[
        Step::ReadAll(Then::Fail(Class::Connect)),
        Step::ReadAll(Then::Status(200)),
    ]);
    let source = Plan::new(&PAYLOAD).source();
    let error = failure(
        store_on(&wire)
            .post(source.body())
            .with_retry(Retry::standard())
            .await,
    );
    assert_eq!(error.class(), Some(Class::Connect));
    assert_eq!(wire.seen().len(), 1);
    assert_eq!(source.opened_times(), 1);

    // `idempotent false;` on a PUT.
    let script = Script::new();
    answers(&script, &[503, 200]);
    let source = Plan::new(&PAYLOAD).source();
    let error = failure(store(&script).once(source.body()).with_retry(waits()).await);
    assert!(error.is_status(503));
    assert_eq!(source.opened_times(), 1);

    // No policy at all.
    let script = Script::new();
    answers(&script, &[503, 200]);
    let source = Plan::new(&PAYLOAD).source();
    let error = failure(store(&script).put(source.body()).await);
    assert!(error.is_status(503));
    assert_eq!(source.opened_times(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_post_whose_status_is_listed_not_applied_is_sent_again_whole() {
    let script = Script::new();
    answers(&script, &[503, 200]);
    let source = Plan::new(&PAYLOAD).source();
    success(
        store(&script)
            .post(source.body())
            .with_retry(Retry::standard().not_applied_on(503))
            .await,
    );
    assert_eq!(source.opened_times(), 2);
    assert_eq!(uploads(&script), [payload(), payload()]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_call_polled_late_opens_when_it_is_polled() {
    let script = Script::new();
    answers(&script, &[200]);
    let start = Instant::now();
    let source = Plan::new(&PAYLOAD).source();
    let sending = store(&script).put(source.body()).send();
    sleep(secs(10)).await;
    assert_eq!(source.opened_times(), 0, "nothing before the first poll");
    assert!(script.requests().is_empty());
    success(sending.await);
    assert_eq!(source.opens(), [secs(10)]);
    assert_eq!(sent_at(&script, start), [secs(10)]);
    assert_eq!(uploads(&script), [payload()]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_two_clones_sent_at_once_each_read_their_own_stream() {
    let script = Script::new();
    script.answer(200).read_upload_paced(ms(300));
    script.answer(200).read_upload_paced(ms(700));
    let source = Plan::new(&PAYLOAD).every_chunk_after(ms(500)).source();
    let call = store(&script).put(source.body());
    let (a, b) = tokio::join!(call.clone().send(), call.send());
    success(a);
    success(b);
    assert_eq!(source.opens(), [secs(0), secs(0)]);
    assert_eq!(source.pulls(), [3, 3]);
    assert_eq!(uploads(&script), [payload(), payload()]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_the_same_body_in_two_calls_and_after_a_success_opens_each_time() {
    let script = Script::new();
    answers(&script, &[200, 200]);
    let source = Plan::new(&PAYLOAD).source();
    let body = source.body();
    success(store(&script).put(&body).await);
    success(store(&script).put(body).await);
    assert_eq!(source.opened_times(), 2);
    assert_eq!(uploads(&script), [payload(), payload()]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_large_binary_source_arrives_byte_for_byte_at_each_attempt() {
    let chunk: Vec<u8> = (0..16 * 1024).map(|i| (i % 251) as u8).collect();
    let chunks: Vec<&[u8]> = (0..64).map(|_| &chunk[..]).collect();
    let script = Script::new();
    answers(&script, &[503, 200]);
    let source = Plan::new(&chunks).source();
    success(store(&script).put(source.body()).with_retry(waits()).await);
    let expected = source.bytes();
    assert_eq!(expected.len(), 1024 * 1024);
    let sent = uploads(&script);
    assert_eq!(sent.len(), 2);
    assert!(sent.iter().all(|bytes| bytes == &expected), "byte for byte");
    assert_eq!(source.pulls(), [64, 64]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_multipart_body_with_a_repeatable_part_is_sent_three_times_the_same() {
    let script = Script::new();
    answers(&script, &[503, 503, 200]);
    let source = Plan::new(&PAYLOAD).source();
    let body = match concord::multipart()
        .text("note", "n")
        .file("blob", "b.bin", "application/octet-stream", source.body())
        .finish()
    {
        Ok(body) => body,
        Err(_) => panic!("the multipart body is built"),
    };
    assert_eq!(source.opened_times(), 0, "finish opens nothing");
    success(store(&script).put(body).with_retry(waits()).await);
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    let content_type = requests[0]
        .header("content-type")
        .expect("a content type")
        .to_owned();
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .expect("the multipart content type");
    let mut expected = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nn\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"blob\"; filename=\"b.bin\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    expected.extend(payload());
    expected.extend(format!("\r\n--{boundary}--\r\n").into_bytes());
    for request in &requests {
        assert_eq!(request.header("content-type"), Some(&content_type[..]));
    }
    assert_eq!(uploads(&script), vec![expected; 3]);
    assert_eq!(source.opens(), [secs(0), secs(1), secs(3)]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_one_shot_part_keeps_a_multipart_body_single_attempt() {
    let script = Script::new();
    answers(&script, &[503, 200]);
    let source = Plan::new(&PAYLOAD).source();
    let body = match concord::multipart()
        .file("a", "a.bin", "application/octet-stream", source.body())
        .file(
            "b",
            "b.bin",
            "application/octet-stream",
            common::stream("once"),
        )
        .finish()
    {
        Ok(body) => body,
        Err(_) => panic!("the multipart body is built"),
    };
    let error = failure(store(&script).put(body).with_retry(waits()).await);
    assert!(error.is_status(503));
    assert_eq!(error.tries(), 1);
    assert_eq!(source.opened_times(), 1);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_concatenation_of_bytes_and_a_repeatable_is_sent_whole_each_time() {
    let script = Script::new();
    answers(&script, &[503, 200]);
    let source = Plan::new(&PAYLOAD).source();
    let body = Body::concat([Body::from("<"), source.body(), Body::from(">")]);
    success(store(&script).put(body).with_retry(waits()).await);
    let mut expected = b"<".to_vec();
    expected.extend(payload());
    expected.push(b'>');
    assert_eq!(uploads(&script), [expected.clone(), expected]);
    assert_eq!(source.opened_times(), 2);
}

#[tokio::test(start_paused = true)]
async fn c12_12_each_page_opens_the_source_again() {
    let script = Script::new();
    for page in [
        r#"{"items":[1],"next":"a"}"#,
        r#"{"items":[2],"next":"b"}"#,
        r#"{"items":[3]}"#,
    ] {
        script
            .answer(200)
            .read_upload()
            .header("content-type", "application/json")
            .body(page);
    }
    let source = Plan::new(&PAYLOAD).source();
    let mut pages = store(&script).search(source.body()).pages();
    let mut items = Vec::new();
    while let Some(page) = success(pages.try_next().await) {
        items.extend(page.items);
    }
    assert_eq!(items, [1, 2, 3]);
    assert_eq!(source.opened_times(), 3);
    assert_eq!(uploads(&script), vec![payload(); 3]);
    let urls: Vec<String> = script
        .requests()
        .iter()
        .map(|r| r.url().as_str().to_owned())
        .collect();
    assert_eq!(
        urls,
        [
            "https://store.example.test/search",
            "https://store.example.test/search?cursor=a",
            "https://store.example.test/search?cursor=b",
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_large_upload_through_a_signer_is_signed_and_sent_whole_at_each_attempt() {
    let script = Script::new();
    answers(&script, &[503, 503, 200]);
    let base = "https://photos.s3.eu-west-1.amazonaws.com"
        .parse()
        .expect("a base");
    let signer = SigV4::new(
        &base,
        Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG"),
        "eu-west-1",
        "s3",
        CanonicalPath::Preserve,
    )
    .allow_unsigned_payload();
    let source = Plan::new(&PAYLOAD).source();
    success(
        Bucket::new(signer)
            .with_transport(script.clone())
            .put(source.body())
            .with_retry(waits())
            .await,
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    let mut dates = Vec::new();
    for request in &requests {
        assert_eq!(
            request.header("x-amz-content-sha256"),
            Some("UNSIGNED-PAYLOAD")
        );
        assert!(request
            .header("authorization")
            .is_some_and(|value| value.starts_with("AWS4-HMAC-SHA256 ")));
        dates.push(request.header("x-amz-date").expect("a date").to_owned());
    }
    assert!(
        dates[0] != dates[1] && dates[1] != dates[2],
        "each attempt is signed anew: {dates:?}"
    );
    assert_eq!(uploads(&script), vec![payload(); 3]);
    assert_eq!(source.opens(), [secs(0), secs(1), secs(3)]);
}

#[tokio::test(start_paused = true)]
async fn c12_12_a_signer_that_does_not_allow_an_unsigned_payload_opens_nothing() {
    let script = Script::new();
    answers(&script, &[200]);
    let base = "https://photos.s3.eu-west-1.amazonaws.com"
        .parse()
        .expect("a base");
    let signer = SigV4::new(
        &base,
        Credentials::new("AKIDEXAMPLE", "wJalrXUtnFEMI/K7MDENG"),
        "eu-west-1",
        "s3",
        CanonicalPath::Preserve,
    );
    let source = Plan::new(&PAYLOAD).source();
    let error = failure(
        Bucket::new(signer)
            .with_transport(script.clone())
            .put(source.body())
            .await,
    );
    assert!(matches!(
        SignError::find(&error),
        Some(SignError::StreamingBody)
    ));
    assert_eq!(source.opened_times(), 0, "refused without being opened");
    assert!(script.requests().is_empty());
}

// C12.13: a failure of the body is the attempt's failure -----------------------------------

#[tokio::test(start_paused = true)]
async fn c12_13_a_reading_error_is_class_other_whatever_the_transport_reports() {
    let mut causes = Vec::new();
    for class in [Class::Connect, Class::Timeout, Class::Body, Class::Blocked] {
        let wire = Wire::new(&[
            Step::ReadAll(Then::Fail(class)),
            Step::ReadAll(Then::Status(200)),
        ]);
        let source = Plan::new(&PAYLOAD).fails_after(1).source();
        let error = failure(
            store_on(&wire)
                .put(source.body())
                .with_retry(Retry::standard())
                .await,
        );
        assert_eq!(error.tries(), 1, "{class:?}: Other is not retried");
        assert_eq!(wire.seen().len(), 1, "{class:?}");
        assert_eq!(wire.seen()[0].bytes, PAYLOAD[0], "{class:?}");
        assert!(wire.seen()[0].failed, "{class:?}");
        assert_eq!(source.opened_times(), 1, "{class:?}");
        causes.push((class, source_failure(&error, SourceError::Read(1))));
    }
    let wrong: Vec<_> = causes.iter().filter(|(_, cause)| cause.is_err()).collect();
    assert!(wrong.is_empty(), "{wrong:?}");
}

#[tokio::test(start_paused = true)]
async fn c12_13_a_reading_error_fails_the_attempt_even_when_the_transport_answers() {
    let mut causes = Vec::new();
    for status in [200, 503] {
        let wire = Wire::new(&[
            Step::ReadAll(Then::Status(status)),
            Step::ReadAll(Then::Status(200)),
        ]);
        let source = Plan::new(&PAYLOAD).fails_after(2).source();
        let error = failure(store_on(&wire).put(source.body()).with_retry(waits()).await);
        assert_eq!(wire.seen().len(), 1, "{status}");
        assert_eq!(source.opened_times(), 1, "{status}");
        causes.push((status, source_failure(&error, SourceError::Read(1))));
    }
    let wrong: Vec<_> = causes.iter().filter(|(_, cause)| cause.is_err()).collect();
    assert!(wrong.is_empty(), "{wrong:?}");
}

#[tokio::test(start_paused = true)]
async fn c12_13_an_opening_error_is_class_other_with_its_cause_and_nothing_is_read() {
    let wire = Wire::new(&[
        Step::ReadAll(Then::Fail(Class::Connect)),
        Step::ReadAll(Then::Status(200)),
    ]);
    let source = Plan::new(&PAYLOAD).failing_openings(&[1]).source();
    let error = failure(store_on(&wire).put(source.body()).with_retry(waits()).await);
    assert_eq!(error.tries(), 1);
    assert_eq!(wire.seen().len(), 1);
    assert!(wire.seen()[0].bytes.is_empty());
    assert_eq!(source.opened_times(), 1);
    assert_eq!(source.pulls(), [0]);
    is_the_source_failure(&error, SourceError::Open(1));
}

#[tokio::test(start_paused = true)]
async fn c12_13_an_opening_error_on_a_retry_ends_the_call_with_that_cause() {
    let script = Script::new();
    answers(&script, &[503, 200, 200]);
    let source = Plan::new(&PAYLOAD).failing_openings(&[2]).source();
    let error = failure(store(&script).put(source.body()).with_retry(waits()).await);
    assert_eq!(error.tries(), 2);
    assert_eq!(source.opens(), [secs(0), secs(1)]);
    assert_eq!(script.requests().len(), 2);
    assert_eq!(script.unused(), 1);
    is_the_source_failure(&error, SourceError::Open(2));
}

#[tokio::test(start_paused = true)]
async fn c12_13_an_error_after_a_success_belongs_to_the_second_call_only() {
    let script = Script::new();
    answers(&script, &[200, 200]);
    let source = Plan::new(&PAYLOAD).failing_openings(&[2]).source();
    let body = source.body();
    success(store(&script).put(&body).await);
    let error = failure(store(&script).put(&body).await);
    assert_eq!(source.opened_times(), 2);
    assert_eq!(script.requests()[0].upload().bytes(), &payload()[..]);
    is_the_source_failure(&error, SourceError::Open(2));
}

#[tokio::test(start_paused = true)]
async fn c12_13_after_its_first_error_a_body_ends_and_opens_no_later_part() {
    // Through a client: the second part is never opened.
    let wire = Wire::new(&[Step::ReadAll(Then::Fail(Class::Connect))]);
    let first = Plan::new(&PAYLOAD).fails_after(1).source();
    let second = Plan::new(&PAYLOAD).source();
    let body = Body::concat([first.body(), Body::from("-"), second.body()]);
    let error = failure(store_on(&wire).put(body).with_retry(waits()).await);
    assert_eq!(wire.seen()[0].bytes, PAYLOAD[0]);
    assert_eq!(second.opened_times(), 0);

    // Taken as a stream: one chunk, the error, then the end, for good.
    let first = Plan::new(&PAYLOAD).fails_after(1).source();
    let second = Plan::new(&PAYLOAD).source();
    let mut stream = Body::concat([first.body(), second.body()])
        .into_stream()
        .expect("a stream");
    let chunk = next_item(&mut stream)
        .await
        .expect("an item")
        .expect("a chunk");
    assert_eq!(&chunk[..], PAYLOAD[0]);
    match next_item(&mut stream).await {
        Some(Err(_)) => {}
        Some(Ok(_)) => panic!("a chunk after the error of the source"),
        None => panic!("the end before the error of the source"),
    }
    assert!(next_item(&mut stream).await.is_none());
    assert!(next_item(&mut stream).await.is_none());
    assert_eq!(second.opened_times(), 0);

    // An opening that fails ends the body before the next part, too.
    let first = Plan::new(&PAYLOAD).failing_openings(&[1]).source();
    let second = Plan::new(&PAYLOAD).source();
    let mut stream = Body::concat([first.body(), second.body()])
        .into_stream()
        .expect("a stream");
    assert!(matches!(next_item(&mut stream).await, Some(Err(_))));
    assert!(next_item(&mut stream).await.is_none());
    assert_eq!(second.opened_times(), 0);
    is_the_source_failure(&error, SourceError::Read(1));
}

#[tokio::test(start_paused = true)]
async fn c12_13_a_source_of_exactly_the_declared_length_is_sent() {
    for (chunks, size) in [
        (&[&b"abc"[..], b"def"][..], 6),
        (&[&b"abcdef"[..]][..], 6),
        (&[][..], 0),
    ] {
        let wire = Wire::new(&[Step::ReadAll(Then::Status(200))]);
        let source = Plan::new(chunks).source();
        success(store_on(&wire).sized(source.body(), size).await);
        assert_eq!(wire.seen()[0].bytes, source.bytes(), "{size}");
        assert!(!wire.seen()[0].failed);
        assert_eq!(source.pulls(), [chunks.len()]);
    }
}

#[tokio::test(start_paused = true)]
async fn c12_13_one_byte_more_than_declared_fails_and_the_last_chunk_is_held() {
    // The declared length is reached by "def"; the lookahead finds "g": "def" is never sent.
    let wire = Wire::new(&[
        Step::ReadAll(Then::Fail(Class::Connect)),
        Step::ReadAll(Then::Status(200)),
    ]);
    let source = Plan::new(&[&b"abc"[..], b"def", b"g"]).source();
    let error = failure(
        store_on(&wire)
            .sized(source.body(), 6)
            .with_retry(waits())
            .await,
    );
    assert_eq!(wire.seen().len(), 1, "Other is not retried");
    assert_eq!(wire.seen()[0].bytes, b"abc");
    assert_eq!(source.pulls(), [3]);
    is_invalid_data(&error);
}

#[tokio::test(start_paused = true)]
async fn c12_13_an_endless_source_is_looked_at_one_chunk_past_the_length_and_not_drained() {
    let wire = Wire::new(&[Step::ReadAll(Then::Status(200))]);
    let source = Plan::new(&[&b"abc"[..], b"def"]).endless(b"x").source();
    let error = failure(store_on(&wire).sized(source.body(), 6).await);
    assert_eq!(
        source.pulls(),
        [3],
        "one chunk of lookahead, nothing drained"
    );
    assert_eq!(wire.seen()[0].bytes, b"abc");
    is_invalid_data(&error);
}

#[tokio::test(start_paused = true)]
async fn c12_13_a_source_longer_or_shorter_than_declared_fails() {
    let mut causes = Vec::new();
    for (chunks, size) in [
        (&[&b"abcdefg"[..]][..], 6),
        (&[&b"abc"[..]][..], 6),
        (&[&b"abc"[..], b"de"][..], 6),
        (&[][..], 1),
    ] {
        let wire = Wire::new(&[Step::ReadAll(Then::Status(200))]);
        let source = Plan::new(chunks).source();
        let error = failure(store_on(&wire).sized(source.body(), size).await);
        let sent = &wire.seen()[0].bytes;
        assert!(
            sent.len() as u64 <= size,
            "{size}: more than declared was sent"
        );
        causes.push((format!("{chunks:?}"), size, invalid_data(&error)));
    }
    let wrong: Vec<_> = causes
        .iter()
        .filter(|(_, _, cause)| cause.is_err())
        .collect();
    assert!(wrong.is_empty(), "{wrong:?}");

    // A declared length of zero and a source that gives a byte: refused before anything is
    // handed to the transport.
    let wire = Wire::new(&[Step::ReadAll(Then::Status(200))]);
    let source = Plan::new(&[&b"a"[..]]).source();
    let error = failure(store_on(&wire).sized(source.body(), 0).await);
    assert!(
        wire.seen().is_empty(),
        "zero declared: nothing reaches the transport"
    );
    is_invalid_data(&error);
}

#[tokio::test(start_paused = true)]
async fn c12_13_a_short_source_sends_what_it_has_before_it_fails() {
    let wire = Wire::new(&[Step::ReadAll(Then::Status(200))]);
    let source = Plan::new(&[&b"abc"[..]]).source();
    let error = failure(store_on(&wire).sized(source.body(), 6).await);
    assert_eq!(wire.seen()[0].bytes, b"abc");
    assert!(wire.seen()[0].failed);
    is_invalid_data(&error);
}

// C12.14: what sees a repeatable body -------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c12_14_an_edit_runs_once_on_an_unreadable_placeholder_and_its_headers_stay() {
    let script = Script::new();
    answers(&script, &[503, 503, 200]);
    let source = Plan::new(&PAYLOAD).source();
    let edits = Arc::new(AtomicUsize::new(0));
    let looked: Arc<Mutex<Vec<(bool, usize)>>> = Arc::default();
    let call = {
        let (edits, looked, source) = (edits.clone(), looked.clone(), source.clone());
        store(&script)
            .put(source.body())
            .edit_request(move |request| {
                edits.fetch_add(1, Ordering::SeqCst);
                let placeholder = request.body().is_some_and(|body| body.as_bytes().is_none());
                looked
                    .lock()
                    .expect("looked")
                    .push((placeholder, source.opened_times()));
                request
                    .headers_mut()
                    .insert("x-returned", http::HeaderValue::from_static("kept"));
            })
    };
    success(call.with_retry(waits()).await);
    assert_eq!(edits.load(Ordering::SeqCst), 1, "the edit runs once");
    assert_eq!(
        *looked.lock().expect("looked"),
        [(true, 0)],
        "a body that is not bytes, and nothing opened"
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_eq!(request.header("x-returned"), Some("kept"));
    }
    assert_eq!(uploads(&script), vec![payload(); 3]);
    assert_eq!(source.opened_times(), 3);
}

#[tokio::test(start_paused = true)]
async fn c12_14_an_edit_that_replaces_or_removes_the_body_is_a_build_error() {
    for replace in [true, false] {
        let script = Script::new();
        answers(&script, &[200]);
        let source = Plan::new(&PAYLOAD).source();
        let error = failure(
            store(&script)
                .put(source.body())
                .edit_request(move |request| {
                    if replace {
                        *request.body_mut() = Some(reqwest::Body::from("other"));
                    } else {
                        request.body_mut().take();
                    }
                })
                .with_retry(waits())
                .await,
        );
        assert_eq!(error.kind(), Kind::Build, "replace: {replace}");
        assert_eq!(error.tries(), 0, "replace: {replace}");
        assert!(script.requests().is_empty(), "replace: {replace}");
        assert_eq!(source.opened_times(), 0, "replace: {replace}");
    }
}

#[tokio::test(start_paused = true)]
async fn c12_14_a_middleware_sees_each_attempts_own_stream() {
    let script = Script::new();
    answers(&script, &[503, 503, 200]);
    let reads = ReadsBody::default();
    let source = Plan::new(&PAYLOAD).source();
    success(
        Store::new()
            .with_middleware(reads.clone())
            .with_transport(script.clone())
            .put(source.body())
            .with_retry(waits())
            .await,
    );
    assert_eq!(reads.seen(), vec![payload(); 3]);
    assert_eq!(source.opens(), [secs(0), secs(1), secs(3)]);
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_eq!(request.body(), Some(&payload()[..]));
    }
}

#[tokio::test(start_paused = true)]
async fn c12_14_request_succeeds_without_opening_and_the_call_can_still_be_sent() {
    let script = Script::new();
    answers(&script, &[200]);
    let source = Plan::new(&PAYLOAD).source();
    let call = store(&script)
        .put(source.body().content_type("application/x-tar"))
        .edit_request(|request| {
            request
                .headers_mut()
                .insert("x-returned", http::HeaderValue::from_static("kept"));
        });
    let request = call.request().expect("request() succeeds");
    assert!(request.body().is_some(), "the request holds a body");
    assert!(request.body().is_some_and(|body| body.as_bytes().is_none()));
    assert_eq!(
        common::header(&request, "x-returned"),
        ["kept"],
        "the edit ran"
    );
    let again = call.request().expect("request() succeeds twice");
    drop((request, again));
    assert_eq!(source.opened_times(), 0);
    success(call.await);
    assert_eq!(source.opened_times(), 1);
    assert_eq!(uploads(&script), [payload()]);
}

#[tokio::test(start_paused = true)]
async fn c12_14_into_request_exports_one_lazy_body_that_is_not_cloneable() {
    let script = Script::new();
    let source = Plan::new(&PAYLOAD).source();
    let mut request = store(&script)
        .put(source.body())
        .into_request()
        .expect("into_request() succeeds");
    assert_eq!(source.opened_times(), 0, "lazy");
    assert!(request.try_clone().is_none(), "not cloneable");
    assert_eq!(read_request_body(&mut request).await, payload());
    assert_eq!(source.opened_times(), 1);
    assert!(script.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn c12_14_two_pages_with_the_same_head_and_the_same_source_are_the_same_request() {
    let script = Script::new();
    for page in [
        r#"{"items":[1],"next":"a"}"#,
        r#"{"items":[2],"next":"a"}"#,
        r#"{"items":[3]}"#,
    ] {
        script
            .answer(200)
            .read_upload()
            .header("content-type", "application/json")
            .body(page);
    }
    let source = Plan::new(&PAYLOAD).source();
    let mut pages = store(&script).search(source.body()).pages();
    assert!(success(pages.try_next().await).is_some());
    assert!(success(pages.try_next().await).is_some());
    let error = match pages.try_next().await {
        Err(error) => error,
        Ok(Some(_)) => panic!("the same request was asked again"),
        Ok(None) => panic!("the pages looked complete"),
    };
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(script.requests().len(), 2, "the same request is not sent");
    assert_eq!(source.opened_times(), 2);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_14_a_call_dropped_during_the_wait_before_a_retry_opens_nothing_more() {
    let script = Script::new();
    answers(&script, &[503, 200]);
    let source = Plan::new(&PAYLOAD).source();
    let client = store(&script);
    let body = source.body();
    let task = tokio::spawn(async move { client.put(body).with_retry(waits()).await });
    sleep(ms(500)).await;
    task.abort();
    let _ = task.await;
    sleep(secs(60)).await;
    assert_eq!(source.opens(), [secs(0)]);
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_14_a_call_dropped_in_the_middle_of_an_upload_reads_and_opens_nothing_more() {
    let script = Script::new();
    answers(&script, &[200]);
    let source = Plan::new(&[&b"a"[..], b"b", b"c", b"d"])
        .every_chunk_after(secs(1))
        .source();
    let client = store(&script);
    let body = source.body();
    let task = tokio::spawn(async move { client.put(body).with_retry(waits()).await });
    sleep(ms(2500)).await;
    task.abort();
    let _ = task.await;
    sleep(secs(60)).await;
    assert_eq!(source.opens(), [secs(0)]);
    assert_eq!(source.pulls(), [2]);
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_14_the_idle_timeout_covers_an_opening_at_the_transport() {
    for (idle, expected) in [(None, secs(30)), (Some(secs(5)), secs(5))] {
        let script = Script::new();
        answers(&script, &[200]);
        let start = Instant::now();
        let source = Plan::new(&PAYLOAD).opening_takes(secs(100)).source();
        let mut call = store(&script).put(source.body());
        if let Some(idle) = idle {
            call = call.with_idle_timeout(idle);
        }
        let error = failure(call.await);
        assert!(error.is_timeout(), "{idle:?}");
        assert_eq!(start.elapsed(), expected, "{idle:?}");
        assert_eq!(source.opened_times(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn c12_14_each_chunk_sent_resets_the_idle_timeout() {
    let script = Script::new();
    answers(&script, &[200]);
    let start = Instant::now();
    let source = Plan::new(&PAYLOAD).every_chunk_after(secs(20)).source();
    success(store(&script).put(source.body()).await);
    assert_eq!(
        start.elapsed(),
        secs(60),
        "three silences of 20 s, under 30 s each"
    );
    assert_eq!(uploads(&script), [payload()]);

    let script = Script::new();
    answers(&script, &[200]);
    let start = Instant::now();
    let source = Plan::new(&PAYLOAD).pauses(&[secs(20), secs(31)]).source();
    let error = failure(store(&script).put(source.body()).await);
    assert!(error.is_timeout());
    assert_eq!(start.elapsed(), secs(50), "30 s after the last chunk");
}

#[tokio::test(start_paused = true)]
async fn c12_14_the_call_timeout_covers_an_opening_at_the_transport() {
    let script = Script::new();
    answers(&script, &[200]);
    let start = Instant::now();
    let source = Plan::new(&PAYLOAD).opening_takes(secs(100)).source();
    let error = failure(
        store(&script)
            .put(source.body())
            .with_timeout(secs(7))
            .await,
    );
    assert!(error.is_timeout());
    assert_eq!(start.elapsed(), secs(7));
}

#[tokio::test(start_paused = true)]
async fn c12_14_a_body_read_by_a_middleware_is_covered_by_the_call_timeout_only() {
    // No idle timeout while a middleware opens and reads: 40 s pass, the call succeeds.
    let script = Script::new();
    answers(&script, &[200]);
    let start = Instant::now();
    let source = Plan::new(&PAYLOAD).opening_takes(secs(40)).source();
    success(
        Store::new()
            .with_middleware(ReadsBody::default())
            .with_transport(script.clone())
            .put(source.body())
            .await,
    );
    assert_eq!(sent_at(&script, start), [secs(40)]);

    // The call timeout does cover it.
    let script = Script::new();
    answers(&script, &[200]);
    let start = Instant::now();
    let source = Plan::new(&PAYLOAD).opening_takes(secs(40)).source();
    let error = failure(
        Store::new()
            .with_middleware(ReadsBody::default())
            .with_transport(script.clone())
            .put(source.body())
            .with_timeout(secs(10))
            .await,
    );
    assert!(error.is_timeout());
    assert_eq!(start.elapsed(), secs(10));
    assert!(script.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn c12_14_a_limit_opens_nothing_while_a_call_waits_for_it() {
    let script = Script::new();
    script.never();
    answers(&script, &[200]);
    let client = Store::new()
        .with_middleware(Limit::new().max_in_flight(1).max_hold(secs(1000)))
        .with_transport(script.clone());
    let holder = Plan::new(&PAYLOAD).source();
    let waiter = Plan::new(&PAYLOAD).source();
    let first = {
        let (client, body) = (client.clone(), holder.body());
        tokio::spawn(async move { client.put(body).with_idle_timeout(secs(1000)).await })
    };
    sleep(secs(1)).await;
    assert_eq!(script.requests().len(), 1, "the first call holds the slot");
    let second = {
        let (client, body) = (client.clone(), waiter.body());
        tokio::spawn(async move { client.put(body).await })
    };
    sleep(secs(100)).await;
    assert_eq!(waiter.opened_times(), 0, "nothing opened while waiting");
    assert_eq!(script.requests().len(), 1);
    first.abort();
    let _ = first.await;
    success(second.await.expect("the task ends"));
    assert_eq!(waiter.opens(), [secs(101)]);
    assert_eq!(script.requests().len(), 2);
    assert_eq!(script.requests()[1].upload().bytes(), &payload()[..]);
}

// C12.14: two pages are compared by their flattened sources and bytes -----------------------

thread_local! {
    /// The bodies that the pager gives to the next pages, in order. Each test runs on a
    /// runtime of one thread, its own, which polls the pager.
    static NEXT_BODIES: std::cell::RefCell<VecDeque<Body>> = Default::default();
}

/// `paginate swap(file);`: the next page carries the next body of [`NEXT_BODIES`], and the
/// pages end when there is none.
fn swap<C, S>(file: S) -> impl Fn(C, &concord::Answer<Page>) -> Option<C> + Send + 'static
where
    C: concord::page::Call<Value = Page>,
    S: concord::page::Slot<C, Value = Body> + Send + 'static,
{
    move |call, _| {
        let next = NEXT_BODIES.with(|bodies| bodies.borrow_mut().pop_front())?;
        Some(file.set(call, next))
    }
}

concord::api! {
    client Swap {
        base "https://store.example.test";
        POST search(file: Body) -> Page {
            path "search";
            body file;
            read concord::Json;
            paginate swap(file);
        }
    }
}

/// What a caller sees of pages whose bodies are `first`, then `next`: how many requests
/// reached the transport, and what the second `try_next` gave (`None`: the end).
async fn two_pages(first: Body, next: Body) -> (usize, Option<concord::Error>) {
    let script = Script::new();
    for _ in 0..3 {
        script
            .answer(200)
            .read_upload()
            .header("content-type", "application/json")
            .body(r#"{"items":[1]}"#);
    }
    NEXT_BODIES.with(|bodies| *bodies.borrow_mut() = VecDeque::from([next]));
    let mut pages = Swap::new()
        .with_transport(script.clone())
        .search(first)
        .pages();
    assert!(success(pages.try_next().await).is_some(), "the first page");
    let second = match pages.try_next().await {
        Ok(Some(_)) => {
            assert!(success(pages.try_next().await).is_none(), "the end");
            None
        }
        Ok(None) => panic!("the pages ended before the second body"),
        Err(error) => Some(error),
    };
    (script.requests().len(), second)
}

/// Both pages were sent and none was stopped as a repeat.
#[track_caller]
fn both_sent(seen: (usize, Option<concord::Error>), case: &str) {
    let (requests, error) = seen;
    if let Some(error) = error {
        panic!("{case}: the second page was refused: {error}");
    }
    assert_eq!(requests, 2, "{case}: both pages reach the transport");
}

/// The second page was stopped as a repeat of the first, and not sent.
#[track_caller]
fn stopped(seen: (usize, Option<concord::Error>), case: &str) {
    let (requests, error) = seen;
    let error = error.unwrap_or_else(|| panic!("{case}: the repeat was sent"));
    assert_eq!(error.kind(), Kind::Mismatch, "{case}");
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, the next page cannot be requested, in swap.search",
        "{case}"
    );
    assert_eq!(
        requests, 1,
        "{case}: the repeat does not reach the transport"
    );
}

#[tokio::test(start_paused = true)]
async fn c12_14_pages_whose_bodies_share_a_first_source_and_differ_later_are_both_sent() {
    let (s, a, b) = (
        Plan::new(&PAYLOAD).source().body(),
        Plan::new(&PAYLOAD).source().body(),
        Plan::new(&PAYLOAD).source().body(),
    );
    both_sent(
        two_pages(
            Body::concat([s.clone(), a.clone()]),
            Body::concat([s.clone(), b.clone()]),
        )
        .await,
        "[s, a] then [s, b]",
    );
    both_sent(
        two_pages(
            Body::concat([s.clone(), a.clone(), b.clone()]),
            Body::concat([s.clone(), a.clone(), a.clone()]),
        )
        .await,
        "[s, a, b] then [s, a, a]",
    );
    both_sent(
        two_pages(
            Body::concat([Body::from("p"), a.clone()]),
            Body::concat([Body::from("p"), b.clone()]),
        )
        .await,
        "[\"p\", a] then [\"p\", b]",
    );
}

#[tokio::test(start_paused = true)]
async fn c12_14_pages_whose_bodies_differ_only_in_a_tail_of_bytes_are_both_sent() {
    let s = Plan::new(&PAYLOAD).source().body();
    both_sent(
        two_pages(
            Body::concat([s.clone(), Body::from("tail-x")]),
            Body::concat([s.clone(), Body::from("tail-y")]),
        )
        .await,
        "[s, \"tail-x\"] then [s, \"tail-y\"]",
    );
    both_sent(
        two_pages(
            Body::concat([s.clone(), Body::from("ab")]),
            Body::concat([s.clone(), Body::from("abc")]),
        )
        .await,
        "[s, \"ab\"] then [s, \"abc\"]",
    );
    both_sent(
        two_pages(
            Body::concat([Body::from("same"), s.clone(), Body::from("\x00")]),
            Body::concat([Body::from("same"), s.clone(), Body::from("\x01")]),
        )
        .await,
        "a byte after the source differs",
    );
}

#[tokio::test(start_paused = true)]
async fn c12_14_pages_whose_bodies_have_unequal_lengths_are_both_sent() {
    let (s, a) = (
        Plan::new(&PAYLOAD).source().body(),
        Plan::new(&PAYLOAD).source().body(),
    );
    both_sent(
        two_pages(s.clone(), Body::concat([s.clone(), a.clone()])).await,
        "[s] then [s, a]",
    );
    both_sent(
        two_pages(Body::concat([s.clone(), a.clone()]), s.clone()).await,
        "[s, a] then [s]",
    );
    both_sent(
        two_pages(
            Body::concat([s.clone()]),
            Body::concat([s.clone(), s.clone()]),
        )
        .await,
        "[s] then [s, s]",
    );
}

#[tokio::test(start_paused = true)]
async fn c12_14_pages_whose_bodies_flatten_to_the_same_sources_and_bytes_are_a_repeat() {
    let (s, a) = (
        Plan::new(&PAYLOAD).source().body(),
        Plan::new(&PAYLOAD).source().body(),
    );
    stopped(
        two_pages(
            Body::concat([s.clone(), a.clone()]),
            Body::concat([s.clone(), a.clone()]),
        )
        .await,
        "[s, a] then [s, a]",
    );
    stopped(
        two_pages(
            Body::concat([s.clone(), a.clone()]),
            Body::concat([Body::concat([s.clone()]), a.clone()]),
        )
        .await,
        "[s, a] then [[s], a]",
    );
    stopped(
        two_pages(
            Body::concat([s.clone(), Body::from("xy")]),
            Body::concat([s.clone(), Body::from("x"), Body::from("y")]),
        )
        .await,
        "[s, \"xy\"] then [s, \"x\", \"y\"]",
    );
}
