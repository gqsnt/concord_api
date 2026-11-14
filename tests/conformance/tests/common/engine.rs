//! What the tests of a send (C3 to C7, C9) share: the clients of `tests/tour` over a
//! `concord_testkit::Script`, small middlewares and transports of a caller, answers made in
//! memory, and the cells of a table.
//!
//! Everything here uses the public surface only, as a caller has it.

use concord::error::{Class, Kind};
use concord::read::StreamError;
use concord::transport::{Transport, TransportError, TryInfo};
use concord::{reqwest, Middleware, Next};
use concord_testkit::Script;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One second.
pub const SECOND: Duration = Duration::from_secs(1);

/// `n` seconds.
pub fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// A task, as the API of `Tasks` sends it.
pub const TASK: &str = r#"{"id":7,"title":"write","done":false}"#;

/// An account, as Riot sends it.
pub const ACCOUNT: &str = r#"{"puuid":"p-1","gameName":"Faker","tagLine":"KR1"}"#;

/// A problem, as Riot sends it.
pub const PROBLEM: &str = r#"{"status":{"message":"slow down","status_code":429}}"#;

/// `Tasks` over this script.
pub fn tasks(script: &Script) -> tour::Tasks {
    tour::Tasks::new().with_transport(script.clone())
}

/// `Files` over this script. Its middleware `Signed` stays under every middleware of a caller.
pub fn files(script: &Script) -> tour::Files {
    tour::Files::new("key").with_transport(script.clone())
}

/// `Riot` over this script.
pub fn riot(script: &Script) -> tour::Riot {
    tour::Riot::new("key").with_transport(script.clone())
}

/// `Shapes` over this script: the fifth client of `tests/tour`, for the shapes that the four
/// others do not have.
pub fn shapes(script: &Script) -> tour::Shapes {
    tour::Shapes::new().with_transport(script.clone())
}

/// An item, as the API of `Shapes` sends it.
pub const ITEM: &str = r#"{"id":1,"title":"one"}"#;

/// The item of [`ITEM`].
pub fn item() -> tour::Item {
    tour::Item {
        id: 1,
        title: "one".to_owned(),
    }
}

/// A JSON answer with this status.
pub fn json(script: &Script, status: u16, body: &'static str) {
    script
        .answer(status)
        .header("content-type", "application/json")
        .body(body);
}

/// A `NewTask`.
pub fn new_task() -> tour::NewTask {
    tour::NewTask {
        title: "write".to_owned(),
    }
}

/// The error of a result, or the test fails (a value is not printed: it may not be `Debug`).
#[track_caller]
pub fn failure<T, E>(result: Result<T, concord::Error<E>>) -> concord::Error<E> {
    match result {
        Ok(_) => panic!("expected an error, got a value"),
        Err(error) => error,
    }
}

/// The value of a result, or the test fails with the line of the error.
#[track_caller]
pub fn success<T, E>(result: Result<T, concord::Error<E>>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected a value, got: {error}"),
    }
}

/// A short account of an outcome, for the message of a cell that fails.
pub fn outcome<T, E>(result: &Result<T, concord::Error<E>>) -> String {
    match result {
        Ok(_) => "a value".to_owned(),
        Err(error) => format!(
            "an error {:?} (class {:?}, status {:?})",
            error.kind(),
            error.class(),
            error.status()
        ),
    }
}

/// The error of a piece of a stream, or the test fails. "`ByteStream::try_next()`
/// gives a `concord::read::StreamError`".
#[track_caller]
pub fn stream_failure<T>(result: Result<T, StreamError>) -> StreamError {
    match result {
        Ok(_) => panic!("expected an error of the stream, got a piece"),
        Err(error) => error,
    }
}

/// A piece of a stream, or the test fails with the line of the error.
#[track_caller]
pub fn stream_success<T>(result: Result<T, StreamError>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected a piece, got: {error}"),
    }
}

/// A short account of what a stream gave, for the message of a cell that fails.
pub fn stream_outcome<T>(result: &Result<T, StreamError>) -> String {
    match result {
        Ok(_) => "a piece".to_owned(),
        Err(error) => format!(
            "an error {:?} (class {:?}, status {:?})",
            error.kind(),
            error.class(),
            error.status()
        ),
    }
}

/// A `GET` of an endpoint `tasks.get` whose reader is `reader`, made as generated code makes it
/// (docs/internals/engine.md 2.2 and 3.1): a 2xx is read by `read_body`, any other status is refused by
/// `refused_plain`. For the readers of a caller, which `tests/tour` has not.
pub fn prepared_get<T, R>(
    core: concord::__private::Core<()>,
    reader: R,
) -> concord::__private::Prepared<T, ()>
where
    T: Send + 'static,
    R: concord::read::Reader<T> + Clone + Send + Sync + 'static,
{
    use concord::__private as engine;
    let options = engine::Options::default();
    let run = engine::Run::start(&core, &options, "tasks.get");
    let mut draft = engine::Draft::new(http::Method::GET);
    draft.base(&[engine::Piece::Lit("https://tasks.example.com/api/tasks/7")]);
    let prepared = draft.prepare(
        run,
        core,
        options,
        engine::Idempotent::ByMethod,
        move |response, info| {
            let reader = reader.clone();
            async move {
                let status = response.status();
                let headers = concord::Headers::from(response.headers().clone());
                if status.is_success() {
                    let value = engine::read_body(&reader, response, &info).await?;
                    Ok(concord::Answer::new(status, headers, value))
                } else {
                    Err(engine::refused_plain(response, &info).await)
                }
            }
        },
    );
    match prepared {
        Ok(prepared) => prepared,
        Err(error) => panic!("the request is built: {error}"),
    }
}

/// A port of `127.0.0.1` on which every connection is refused, for as long as the socket is
/// kept: it is bound and does not listen, so no other process can take the port meanwhile.
pub fn refusing_port() -> (tokio::net::TcpSocket, u16) {
    let socket = tokio::net::TcpSocket::new_v4().expect("a socket");
    socket
        .bind("127.0.0.1:0".parse().expect("an address"))
        .expect("a port");
    let port = socket.local_addr().expect("its address").port();
    (socket, port)
}

/// An answer made in memory, as an HTTP library of the caller would give it.
pub fn response(status: u16, headers: &[(&str, &str)], body: &'static str) -> reqwest::Response {
    let mut builder = http::Response::builder().status(status);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    reqwest::Response::from(builder.body(reqwest::Body::from(body)).expect("a response"))
}

/// A one-shot stream body that gives `pieces`, each after `pause` of virtual time.
///
/// It is made from an answer of a `Script`, so that the suite needs no stream crate.
pub async fn paced_stream(pieces: &[&'static str], pause: Duration) -> concord::Body {
    let source = Script::new();
    {
        let mut answer = source.answer(200);
        for piece in pieces {
            answer = answer.pause(pause).body(*piece);
        }
    }
    let request = reqwest::Request::new(
        reqwest::Method::GET,
        "http://stream.test/".parse().expect("a URL"),
    );
    let response = source
        .send(request, TryInfo::new("stream.source", 1))
        .await
        .expect("the source of the stream answers");
    concord::Body::stream(response.bytes_stream())
}

/// A shared log of events, in order.
#[derive(Clone, Default, Debug)]
pub struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    /// Add an event.
    pub fn push(&self, event: impl Into<String>) {
        self.0.lock().expect("the log").push(event.into());
    }

    /// Every event, in order.
    pub fn events(&self) -> Vec<String> {
        self.0.lock().expect("the log").clone()
    }
}

/// A shared counter.
#[derive(Clone, Default, Debug)]
pub struct Count(Arc<AtomicUsize>);

impl Count {
    /// Add one.
    pub fn add(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    /// How many.
    pub fn get(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

/// A middleware that writes `<name> out <try>` on the way out, and `<name> back` when an
/// answer comes back through it. It sets the header `x-<name>` on the request.
#[derive(Debug)]
pub struct Logs {
    /// The log.
    pub log: Log,
    /// Its name.
    pub name: &'static str,
}

impl Middleware for Logs {
    async fn handle(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        self.log.push(format!(
            "{} out {} {}",
            self.name,
            info.number(),
            info.endpoint()
        ));
        let header = format!("x-{}", self.name);
        request.headers_mut().insert(
            http::HeaderName::from_bytes(header.as_bytes()).expect("a header name"),
            http::HeaderValue::from_static("1"),
        );
        let seen_signature = request.headers().contains_key("x-signature");
        self.log
            .push(format!("{} sees x-signature: {seen_signature}", self.name));
        let answer = next.send(request, info).await;
        if answer.is_ok() {
            self.log.push(format!("{} back", self.name));
        }
        answer
    }
}

/// A middleware that waits `before` it sends on and `after` the answer came back.
#[derive(Debug, Default)]
pub struct Waits {
    /// The wait before `next.send`.
    pub before: Duration,
    /// The wait after `next.send` gave its answer.
    pub after: Duration,
}

impl Middleware for Waits {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        tokio::time::sleep(self.before).await;
        let answer = next.send(request, info).await;
        tokio::time::sleep(self.after).await;
        answer
    }
}

/// A middleware that will not send: `TransportError::blocked()`, with a source that holds
/// text of the caller's own.
#[derive(Debug, Default)]
pub struct Blocks {
    /// How many times it was asked.
    pub count: Count,
}

impl Middleware for Blocks {
    async fn handle(
        &self,
        _request: reqwest::Request,
        _info: TryInfo,
        _next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        self.count.add();
        Err(TransportError::blocked().with_source(std::io::Error::other(CALLER_TEXT)))
    }
}

/// Text of the caller's own, which no printed line may hold.
pub const CALLER_TEXT: &str = "caller-text-7f3a";

/// A middleware that blocks from its `n`-th try on, and sends on before.
#[derive(Debug)]
pub struct BlocksFrom(pub u32);

impl Middleware for BlocksFrom {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        if info.number() >= self.0 {
            return Err(TransportError::blocked());
        }
        next.send(request, info).await
    }
}

/// A middleware that sends a copy of the request, drops its answer, and sends the request.
#[derive(Debug)]
pub struct SendsTwice;

impl Middleware for SendsTwice {
    async fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let copy = request.try_clone().expect("a request in memory");
        drop(next.send(copy, info.clone()).await?);
        next.send(request, info).await
    }
}

/// A middleware that renews a token after a `401`: it sends a copy again, with
/// `authorization: Bearer new`, inside the same try.
#[derive(Debug)]
pub struct Renews;

impl Middleware for Renews {
    async fn handle(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        request.headers_mut().insert(
            "authorization",
            http::HeaderValue::from_static("Bearer old"),
        );
        let again = request.try_clone();
        let answer = next.send(request, info.clone()).await?;
        if answer.status() == 401 {
            if let Some(mut request) = again {
                request.headers_mut().insert(
                    "authorization",
                    http::HeaderValue::from_static("Bearer new"),
                );
                return next.send(request, info).await;
            }
        }
        Ok(answer)
    }
}

/// A middleware that changes the method of the request.
#[derive(Debug)]
pub struct SetsMethod(pub http::Method);

impl Middleware for SetsMethod {
    async fn handle(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        *request.method_mut() = self.0.clone();
        next.send(request, info).await
    }
}

/// A transport of the caller's own: every try fails with this class, and a source.
#[derive(Debug)]
pub struct FailsWith(pub Class);

impl Transport for FailsWith {
    async fn send(
        &self,
        _request: reqwest::Request,
        _info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        Err(TransportError::new(self.0).with_source(std::io::Error::other(CALLER_TEXT)))
    }
}

/// The cells of a table: each one runs in a task of its own, so that a cell that panics
/// (a `todo!()`) fails alone; every cell runs, and the test fails once with the list.
pub type CellFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

/// A cell: its row, its column, and what it checks.
pub struct Cell {
    /// The row, as the table of `docs/reference/cases.md` names it.
    pub row: String,
    /// The column, as the table names it.
    pub column: String,
    /// What it checks.
    pub check: CellFuture,
}

/// A cell of a table.
pub fn cell(
    row: impl Into<String>,
    column: impl Into<String>,
    check: impl Future<Output = Result<(), String>> + Send + 'static,
) -> Cell {
    Cell {
        row: row.into(),
        column: column.into(),
        check: Box::pin(check),
    }
}

/// Run every cell, in order; fail once with each cell that failed, by row and column.
pub async fn run_cells(table: &str, cells: Vec<Cell>) {
    let total = cells.len();
    let mut failures = Vec::new();
    for Cell { row, column, check } in cells {
        let outcome = match tokio::spawn(check).await {
            Ok(outcome) => outcome,
            Err(join) => Err(match join.try_into_panic() {
                Ok(panic) => format!(
                    "panicked: {}",
                    panic
                        .downcast_ref::<&str>()
                        .map(|text| text.to_string())
                        .or_else(|| panic.downcast_ref::<String>().cloned())
                        .unwrap_or_default()
                ),
                Err(_) => "the task was cancelled".to_owned(),
            }),
        };
        if let Err(why) = outcome {
            failures.push(format!("[{row}] x [{column}]: {why}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{table}: {} of {total} cells fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// `Ok` when `holds`, else the message.
pub fn check(holds: bool, message: impl FnOnce() -> String) -> Result<(), String> {
    if holds {
        Ok(())
    } else {
        Err(message())
    }
}

/// What an error must be: its kind, and for a Transport error its class.
pub fn is<E>(error: &concord::Error<E>, kind: Kind, class: Option<Class>) -> Result<(), String> {
    check(error.kind() == kind && error.class() == class, || {
        format!(
            "expected {kind:?} (class {class:?}), got {:?} (class {:?})",
            error.kind(),
            error.class()
        )
    })
}

/// The days since 1970-01-01 of a civil date (proleptic Gregorian), for an HTTP date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let month = (month + 9) % 12;
    let of_year = (153 * month + 2) / 5 + day - 1;
    let of_era_day = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_era_day - 719_468
}

/// The civil date of a number of days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month = (5 * of_year + 2) / 153;
    let day = of_year - (153 * month + 2) / 5 + 1;
    let month = if month < 10 { month + 3 } else { month - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const LONG_DAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];

/// The three forms of an HTTP date (RFC 9110, section 5.6.7).
#[derive(Clone, Copy, Debug)]
pub enum DateForm {
    /// `Sun, 06 Nov 1994 08:49:37 GMT`
    Imf,
    /// `Sunday, 06-Nov-94 08:49:37 GMT`
    Rfc850,
    /// `Sun Nov  6 08:49:37 1994`
    Asctime,
}

/// An HTTP date `offset` seconds from the clock of the machine (negative: in the past).
pub fn http_date(offset: i64, form: DateForm) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    let at = i64::try_from(now).expect("a time") + offset;
    http_date_at(at, form)
}

/// An HTTP date for this second since 1970.
pub fn http_date_at(at: i64, form: DateForm) -> String {
    let days = at.div_euclid(86_400);
    let second = at.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let weekday = usize::try_from(days.rem_euclid(7)).expect("a weekday");
    let month_name = MONTHS[usize::try_from(month - 1).expect("a month")];
    let (h, m, s) = (second / 3600, second / 60 % 60, second % 60);
    match form {
        DateForm::Imf => format!(
            "{}, {day:02} {month_name} {year} {h:02}:{m:02}:{s:02} GMT",
            DAYS[weekday]
        ),
        DateForm::Rfc850 => format!(
            "{}, {day:02}-{month_name}-{:02} {h:02}:{m:02}:{s:02} GMT",
            LONG_DAYS[weekday],
            year % 100
        ),
        DateForm::Asctime => format!(
            "{} {month_name} {day:>2} {h:02}:{m:02}:{s:02} {year}",
            DAYS[weekday]
        ),
    }
}

/// The second since 1970 of midnight of a civil date.
pub fn midnight(year: i64, month: i64, day: i64) -> i64 {
    days_from_civil(year, month, day) * 86_400
}

/// A server on `127.0.0.1`, on threads of its own, that answers every connection with
/// `answer`, written as it is, once it has read the head of the request, and then reads
/// until the client closes; with no answer, it only reads until the client closes. It counts
/// the connections. For what the test kit cannot write: a head that is not HTTP, two
/// lengths.
pub fn raw_server(answer: Option<Vec<u8>>) -> (String, Count) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let base = format!("http://{}", listener.local_addr().expect("its address"));
    let connections = Count::default();
    let seen = connections.clone();
    std::thread::spawn(move || {
        for socket in listener.incoming() {
            let Ok(mut socket) = socket else { return };
            seen.add();
            let answer = answer.clone();
            std::thread::spawn(move || {
                let mut received = Vec::new();
                let mut buffer = [0; 4096];
                if let Some(answer) = answer {
                    while !received.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buffer) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => received.extend_from_slice(&buffer[..n]),
                        }
                    }
                    let _ = socket.write_all(&answer);
                    let _ = socket.shutdown(std::net::Shutdown::Write);
                }
                while let Ok(n) = socket.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                }
            });
        }
    });
    (base, connections)
}

/// A stream of the caller's: one piece, then an error that holds [`CALLER_TEXT`].
pub struct BreaksAfterOnePiece(pub u8);

impl futures_core::Stream for BreaksAfterOnePiece {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.0 += 1;
        std::task::Poll::Ready(match self.0 {
            1 => Some(Ok(bytes::Bytes::from_static(b"first piece"))),
            2 => Some(Err(std::io::Error::other(CALLER_TEXT))),
            _ => None,
        })
    }
}
