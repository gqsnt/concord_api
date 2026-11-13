//! C6.2: every text that the engine can print, in one table. Each row is produced through the
//! public surface, by a client of `api/tour` (and, for a transport, by a transport or an HTTP
//! client that a caller gives it), and compared as a whole line.
//!
//! The rows: docs/messages.md section 2, with ANSWERS.md A1 (one pattern for a shape that a
//! place cannot hold), A15 (an entry whose expression fails), A19 (five texts), A20 (a clock
//! length with a decimal part), A33 (silence after a head, and the hint of a clock that the
//! description set), A36 (an empty body), A37 (`with_base`), and the texts of the engine that
//! no page lists: the connect timeout of the HTTP client, "the request has too many headers"
//! and "the description is not valid (…)". A74 (a URL too long to be sent) and A76 (the label
//! of a body is a header value) are rows of the table. The two texts of A75 (a head that
//! cannot be read, a body of the request that failed) need a real connection to a socket of
//! `127.0.0.1`: they are a second table, on the real clock. A46 (a next URL that is no URL) is
//! a test of its own.
//!
//! The texts that no client of `api/tour` can produce, and why:
//!
//! - `build error: the base is not a URL that can be used, in <endpoint>` (row 1, without
//!   `with_base`): every base of `api/tour` is fixed text, which the macro checks (messages
//!   26 and 27), or a host hole, which fails as a host hole (A17); no client has a port hole.
//! - `input <name> cannot be written in a path`, `… in a host`, `… in a header` (A1, and A49
//!   for a value nested deeper than 128 levels): every input of `api/tour` that stands in a
//!   path, a host or a header is a number, a text or a `Secret`; only `shapes.search` has an
//!   input that a place refuses (`filter`, in a query).
//! - `build error: the request has too many headers, in <endpoint>`: it needs more than 32,768
//!   field lines (the capacity of `http::HeaderMap`); no header of `api/tour` takes a list.
//! - `build error: the description is not valid (a header name)` and `(fixed path text)`:
//!   only a description that the macro refuses (messages 27 and 30) gives them, through the
//!   hidden `Draft`.
//! - `build error: the HTTP client could not be built, in <endpoint>`: only when `reqwest`
//!   cannot make its default client (its TLS cannot start), which a test cannot cause.
//! - `transport error: no connection after 30 s (the connect timeout of the HTTP client), in
//!   <endpoint>`: the length is printed only when it is known, that is with `concord::Http`
//!   made with no `Http::with` closure (which may change it); such a client takes no resolver,
//!   so its connection is stalled only by a socket: `c7_1_real_stalled_tls_handshake_reports_
//!   the_30_s_http_connect_timeout` in `concord-core`. The text is the engine's: the paper
//!   gives none for the connect timeout.
//! - `transport error: connection refused`, through Concord's own `Http`: it needs a socket;
//!   `c8_4_a_connection_that_was_refused_ends_the_pages` has it. Here the same words come from
//!   a transport of the caller, whose source is the `io::Error` of that kind (A13).

mod common;

use common::engine::*;
use common::stream;
use concord::error::Class;
use concord::transport::{Transport, TransportError, TryInfo};
use concord::{reqwest, MIB};
use concord_testkit::Script;
use std::time::Duration;
use tour::{Filter, Item, Trace};

const COLUMN: &str = "printed text";

/// The first error of the pages, as its printed line.
async fn first_line<T: Send, E: Send>(mut pages: concord::Pages<T, E>) -> Result<String, String> {
    loop {
        match pages.try_next().await {
            Ok(Some(_)) => continue,
            Ok(None) => return Err("the end, and no error".to_owned()),
            Err(error) => return Ok(error.to_string()),
        }
    }
}

/// A row: `check` gives the line that the call printed, compared with `expected`.
fn row<F>(row: &'static str, expected: &'static str, check_line: F) -> Cell
where
    F: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    cell(row, COLUMN, async move {
        let line = check_line.await?;
        check(line == expected, || format!("{line:?}"))
    })
}

/// The printed line of the error of a call, or a text that says it gave a value.
fn printed<T, E>(result: Result<T, concord::Error<E>>) -> Result<String, String> {
    match result {
        Ok(_) => Err("a value, not an error".to_owned()),
        Err(error) => Ok(error.to_string()),
    }
}

/// The next try answers this status with a JSON label and this body.
fn give(script: &Script, status: u16, body: &'static str) {
    script
        .answer(status)
        .header("content-type", "application/json")
        .body(body);
}

/// A transport of the caller's: every try fails as a connection, with this `io` kind behind.
#[derive(Debug)]
struct Fails(Option<std::io::ErrorKind>);

impl Transport for Fails {
    async fn send(
        &self,
        _request: reqwest::Request,
        _info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        Err(match self.0 {
            Some(kind) => TransportError::connect().with_source(std::io::Error::from(kind)),
            None => TransportError::connect(),
        })
    }
}

/// A resolver of names that knows none.
struct NoName;

impl reqwest::dns::Resolve for NoName {
    fn resolve(&self, _name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async { Err("no such name".into()) })
    }
}

/// A resolver of names that never answers: the connection is never made.
struct NeverResolves;

impl reqwest::dns::Resolve for NeverResolves {
    fn resolve(&self, _name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(std::future::pending())
    }
}

fn titled(title: &str) -> Item {
    Item {
        id: 1,
        title: title.to_owned(),
    }
}

/// What the build of a request prints (messages section 2, rows 1 to 9, 21, 25; A1; A15; A37).
fn the_texts_of_a_request() -> Vec<Cell> {
    vec![
        row(
            "1. Invalid base, with A37: a with_base that is no base",
            "build error: the base is not a URL that can be used (with_base), in shapes.again",
            async { printed(tour::Shapes::new().with_base("not a url").again(1).request()) },
        ),
        row(
            "2. Invalid host hole",
            "build error: input tenant is not one label of a host, in shapes.hosted",
            async { printed(tour::Shapes::new().hosted("a.b").request()) },
        ),
        row(
            "3. An empty path value, or a finished segment that is . or ..",
            "build error: input path_key gives an empty or dot path segment, in shapes.guarded",
            async {
                let lines: Vec<_> = ["", ".", ".."]
                    .into_iter()
                    .map(|key| printed(tour::Shapes::new().guarded(key).request()))
                    .collect();
                if lines.windows(2).any(|pair| pair[0] != pair[1]) {
                    return Err(format!("{lines:?}"));
                }
                printed(tour::Shapes::new().guarded("..").request())
            },
        ),
        row(
            "4 and A1. A shape that cannot be written at that place (a query)",
            "build error: input filter cannot be written in a query, in shapes.search",
            async {
                let filter = Filter {
                    owner: "ann".into(),
                    open: true,
                };
                printed(tour::Shapes::new().search(1.5).filter(filter).request())
            },
        ),
        row(
            "5. Nonfinite number",
            "build error: input ratio is not a finite number, in shapes.search",
            async { printed(tour::Shapes::new().search(f64::NAN).request()) },
        ),
        row(
            "6. Invalid final header value",
            "build error: the value of header x-cursor is not a legal header value, in shapes.headed",
            async { printed(tour::Shapes::new().headed().cursor("a\nb").send().await) },
        ),
        row(
            "6 and A76. The label of a body that is not a legal header value",
            "build error: the value of header content-type is not a legal header value, in shapes.upload",
            async {
                let body = concord::Body::new("abc", "text/plain\r\nx-injected: 1");
                printed(tour::Shapes::new().upload(body).request())
            },
        ),
        row(
            "A74. A URL longer than 65,534 bytes",
            "build error: the URL is too long to be sent, in files.readme",
            async {
                let files = tour::Files::new("key");
                printed(files.readme("a".repeat(65_535)).request())
            },
        ),
        row(
            "7. A body that could not be written",
            "build error: the body could not be written, in shapes.measured",
            async { printed(tour::Shapes::new().measured(titled(""), 20).request()) },
        ),
        row(
            "A15. An entry whose expression fails",
            "build error: input n could not be written, in shapes.parsed",
            async { printed(tour::Shapes::new().parsed("twelve").request()) },
        ),
        row(
            "8. request() on a call whose body is a one-shot stream",
            "build error: the body is a stream; into_request takes it, in shapes.upload",
            async { printed(tour::Shapes::new().upload(stream("abc")).request()) },
        ),
        row(
            "9. A body that is a one-shot stream, sent a second time",
            "build error: the body is a stream that was already sent, in shapes.streamed",
            async {
                let script = Script::new();
                script
                    .answer(200)
                    .read_upload()
                    .header("content-type", "application/json")
                    .body(r#"{"items":[],"next":"b"}"#);
                first_line(shapes(&script).streamed(stream("abc")).pages()).await
            },
        ),
        row(
            "21. A page that leaves the origin of the first request",
            "build error: the URL is at another origin than the call, in shapes.roaming",
            async {
                let script = Script::new();
                give(&script, 200, r#"{"items":[],"next":"other"}"#);
                first_line(shapes(&script).roaming("first").pages()).await
            },
        ),
        row(
            "25. A content-length written by the description that is not the length",
            "build error: the content-length is not the length of the body, in shapes.measured",
            async { printed(tour::Shapes::new().measured(titled("t"), 3).request()) },
        ),
    ]
}

/// What a transport prints (rows 10, 18, 22 to 24; A13; A19; A20; A33; the connect timeout).
fn the_texts_of_a_transport() -> Vec<Cell> {
    vec![
        row(
            "10. A middleware or a transport that will not send",
            "transport error: blocked before it was sent, in shapes.again",
            async {
                let script = Script::new();
                printed(
                    shapes(&script)
                        .with_middleware(Blocks::default())
                        .again(1)
                        .await,
                )
            },
        ),
        row(
            "18. A body that breaks or ends early",
            "transport error: the body broke after status 200, in shapes.again",
            async {
                let script = Script::new();
                script
                    .answer(200)
                    .header("content-type", "application/json")
                    .body("{")
                    .cut();
                printed(shapes(&script).again(1).await)
            },
        ),
        row(
            "22. A connection that failed: connection refused",
            "transport error: connection refused, in shapes.again",
            async {
                let shapes = tour::Shapes::new()
                    .with_transport(Fails(Some(std::io::ErrorKind::ConnectionRefused)));
                printed(shapes.again(1).await)
            },
        ),
        row(
            "22 and A13. A connection that failed: connection reset",
            "transport error: connection reset, in shapes.again",
            async {
                let shapes = tour::Shapes::new()
                    .with_transport(Fails(Some(std::io::ErrorKind::ConnectionReset)));
                printed(shapes.again(1).await)
            },
        ),
        row(
            "22. A connection that failed: no connection",
            "transport error: no connection, in shapes.again",
            async { printed(tour::Shapes::new().with_transport(Fails(None)).again(1).await) },
        ),
        row(
            "22. A connection that failed: name not found",
            "transport error: name not found, in shapes.again",
            async {
                let http = concord::Http::with(|builder| builder.no_proxy().dns_resolver(NoName))
                    .map_err(|error| error.to_string())?;
                printed(tour::Shapes::new().with_transport(http).again(1).await)
            },
        ),
        row(
            "The connect timeout of Concord's own HTTP client, changed by Http::with: its length is not known",
            "transport error: no connection (the connect timeout of the HTTP client), in shapes.again",
            async {
                let http =
                    concord::Http::with(|builder| builder.no_proxy().dns_resolver(NeverResolves))
                        .map_err(|error| error.to_string())?;
                let shapes = tour::Shapes::new()
                    .with_transport(http)
                    .with_idle_timeout(secs(600));
                printed(shapes.again(1).await)
            },
        ),
        row(
            "The connect timeout of a caller's HTTP client, whose length is not known",
            "transport error: no connection (the connect timeout of the HTTP client), in shapes.again",
            async {
                let client = reqwest::Client::builder()
                    .no_proxy()
                    .dns_resolver(NeverResolves)
                    .connect_timeout(secs(5))
                    .build()
                    .map_err(|error| error.to_string())?;
                let shapes = tour::Shapes::new()
                    .with_transport(client)
                    .with_idle_timeout(secs(600));
                printed(shapes.again(1).await)
            },
        ),
        row(
            "23. Silence for longer than idle_timeout",
            "transport error: no answer for 30 s (with_idle_timeout changes it), in shapes.again",
            async {
                let script = Script::new();
                script.never();
                printed(shapes(&script).again(1).await)
            },
        ),
        row(
            "23 and A20. A clock length with a decimal part",
            "transport error: no answer for 1.5 s (with_idle_timeout changes it), in shapes.again",
            async {
                let script = Script::new();
                script.never();
                let shapes = shapes(&script).with_idle_timeout(Duration::from_millis(1500));
                printed(shapes.again(1).await)
            },
        ),
        row(
            "A33. Silence before a head, on a clock that the description set",
            "transport error: no answer for 5 s (with_idle_timeout on the call changes it), in shapes.order",
            async {
                let script = Script::new();
                script.never();
                printed(shapes(&script).order(Trace::new(false, secs(5))).await)
            },
        ),
        row(
            "A33. A body that was silent after its head",
            "transport error: the body was silent for 30 s after status 200 (with_idle_timeout changes it), in shapes.again",
            async {
                let script = Script::new();
                script
                    .answer(200)
                    .header("content-type", "application/json")
                    .body("{")
                    .stall();
                printed(shapes(&script).again(1).await)
            },
        ),
        row(
            "A33. A body that was silent after its head, on a clock that the description set",
            "transport error: the body was silent for 5 s after status 200 (with_idle_timeout on the call changes it), in shapes.order",
            async {
                let script = Script::new();
                script
                    .answer(200)
                    .header("content-type", "application/json")
                    .body("{")
                    .stall();
                printed(shapes(&script).order(Trace::new(false, secs(5))).await)
            },
        ),
        row(
            "24. The whole call longer than timeout",
            "transport error: the call took longer than 60 s (with_timeout), in shapes.again",
            async {
                let script = Script::new();
                script.never();
                let shapes = shapes(&script)
                    .with_timeout(secs(60))
                    .with_idle_timeout(secs(600));
                printed(shapes.again(1).await)
            },
        ),
        row(
            "A19. Class Other",
            "transport error: the transport failed, in shapes.again",
            async {
                let shapes = tour::Shapes::new().with_transport(FailsWith(Class::Other));
                printed(shapes.again(1).await)
            },
        ),
        row(
            "A19. A timeout that a transport reported",
            "transport error: a timeout was reported by the transport, in shapes.again",
            async {
                let shapes = tour::Shapes::new().with_transport(FailsWith(Class::Timeout));
                printed(shapes.again(1).await)
            },
        ),
        row(
            "A19. A body that broke with no head known",
            "transport error: the body broke, in shapes.again",
            async {
                let shapes = tour::Shapes::new().with_transport(FailsWith(Class::Body));
                printed(shapes.again(1).await)
            },
        ),
    ]
}

/// What an answer prints (rows 11 to 17, 19, 20; A19; A25; A36).
fn the_texts_of_an_answer() -> Vec<Cell> {
    const TALLY: &str = r#"{"counts":{"key":[{"kind":"a","n":1},{"kind":"b","n":"x"}]}}"#;
    vec![
        row(
            "11. A status with no arm, not a 2xx",
            "refused: status 404, in shapes.again",
            async {
                let script = Script::new();
                script.answer(404);
                printed(shapes(&script).again(1).await)
            },
        ),
        row(
            "12. The same, after tries",
            "refused: status 503, in shapes.again, after 3 tries",
            async {
                let script = Script::new();
                for _ in 0..3 {
                    script.answer(503);
                }
                let shapes = shapes(&script).with_retry(concord::Retry::standard());
                printed(shapes.again(1).await)
            },
        ),
        row(
            "13. A reader that holds the server to its label",
            "mismatch: status 200, the content type is not the one the reader expects, in shapes.strict",
            async {
                let script = Script::new();
                script
                    .answer(200)
                    .header("content-type", "text/plain")
                    .body(ITEM);
                printed(shapes(&script).strict().await)
            },
        ),
        row(
            "14. A body larger than its limit",
            "mismatch: status 200, the body is larger than the limit of the reader, in shapes.again",
            async {
                let script = Script::new();
                script
                    .answer(200)
                    .header("content-type", "application/json")
                    .declare_length(10 * MIB as u64 + 1)
                    .stall();
                printed(shapes(&script).again(1).await)
            },
        ),
        row(
            "15. A body that does not decode, with its place",
            "mismatch: status 200, counts[key][1].n is not what the type says, in shapes.counts",
            async {
                let script = Script::new();
                give(&script, 200, TALLY);
                printed(shapes(&script).counts().await)
            },
        ),
        row(
            "15 and A25. A place, when the label was not the reader's either",
            "mismatch: status 200, counts[key][1].n is not what the type says and its content type is not the reader's, in shapes.counts",
            async {
                let script = Script::new();
                script
                    .answer(200)
                    .header("content-type", "text/plain")
                    .body(TALLY);
                printed(shapes(&script).counts().await)
            },
        ),
        row(
            "16. A body that does not read, when its label was not the reader's either",
            "mismatch: status 200, the body is not what the type says and its content type is not the reader's, in shapes.counts",
            async {
                let script = Script::new();
                script
                    .answer(200)
                    .header("content-type", "text/html")
                    .body("<html></html>");
                printed(shapes(&script).counts().await)
            },
        ),
        row(
            "A19. A Mismatch at the root with the reader's label",
            "mismatch: status 200, the body is not what the type says, in shapes.counts",
            async {
                let script = Script::new();
                give(&script, 200, "not json");
                printed(shapes(&script).counts().await)
            },
        ),
        row(
            "A36. A body of no bytes that a reader refuses",
            "mismatch: status 204, the body is empty, in shapes.again",
            async {
                let script = Script::new();
                script.answer(204);
                printed(shapes(&script).again(1).await)
            },
        ),
        row(
            "A36. A body of no bytes, whose label was not the reader's",
            "mismatch: status 200, the body is empty and its content type is not the reader's, in shapes.again",
            async {
                let script = Script::new();
                script.answer(200).header("content-type", "text/plain");
                printed(shapes(&script).again(1).await)
            },
        ),
        row(
            "17. A 2xx that no arm takes, under the arrow of a HEAD",
            "mismatch: status 200, no arm takes this status, in shapes.present",
            async {
                let script = Script::new();
                script.answer(200);
                printed(shapes(&script).present(1).await)
            },
        ),
        row(
            "A19. mismatch(…) or require(…) in an arm",
            "mismatch: status 201, the status arm refused the answer, in shapes.create",
            async {
                let script = Script::new();
                script.answer(201);
                printed(shapes(&script).create(item()).await)
            },
        ),
        row(
            "19. A pager that refuses a page",
            "mismatch: status 200, the next page cannot be asked, in shapes.guarded",
            async {
                let script = Script::new();
                give(&script, 200, r#"{"items":[],"more":true}"#);
                first_line(shapes(&script).guarded("pk").pages()).await
            },
        ),
        row(
            "20. A pager that fails with the API's problem",
            "refused: status 200, in shapes.guarded",
            async {
                let script = Script::new();
                give(
                    &script,
                    200,
                    r#"{"items":[],"refusal":{"code":"stop","message":"no"}}"#,
                );
                first_line(shapes(&script).guarded("pk").pages()).await
            },
        ),
    ]
}

/// What a request that failed on the wire prints (A75): a real connection to a server on
/// `127.0.0.1`, through Concord's own client, so these rows run on the real clock.
fn the_texts_on_the_wire() -> Vec<Cell> {
    fn shapes_on(base: &str) -> tour::Shapes {
        tour::Shapes::new()
            .with_transport(concord::Http::with(|builder| builder.no_proxy()).expect("an Http"))
            .with_base(base)
    }
    vec![
        row(
            "A75. A head of the answer that cannot be read",
            "transport error: the head of the answer could not be read, in shapes.again",
            async {
                let (base, _) = raw_server(Some(b"not HTTP at all\r\n\r\n".to_vec()));
                printed(shapes_on(&base).again(1).await)
            },
        ),
        row(
            "A75. The caller's body of the request that fails while it is sent",
            "transport error: the body of the request failed, in shapes.upload",
            async {
                let (base, _) = raw_server(None);
                let body = concord::Body::stream(BreaksAfterOnePiece(0));
                printed(shapes_on(&base).upload(body).await)
            },
        ),
    ]
}

#[tokio::test(start_paused = true)]
async fn c6_2_every_text_that_the_engine_prints_through_a_client_of_tour() {
    let mut cells = Vec::new();
    cells.extend(the_texts_of_a_request());
    cells.extend(the_texts_of_a_transport());
    cells.extend(the_texts_of_an_answer());
    run_cells("the texts at run time", cells).await;
}

#[tokio::test(start_paused = true)]
async fn c6_2_a46_a_next_url_that_is_no_url() {
    // ANSWERS.md A46: "A next URL that is no URL: `build error: the next URL is not a URL, in
    // <endpoint>`."
    let script = Script::new();
    give(&script, 200, r#"{"items":[],"next":"http://["}"#);
    let line = first_line(shapes(&script).addressed().pages()).await;
    assert_eq!(
        line.as_deref(),
        Ok("build error: the next URL is not a URL, in shapes.addressed")
    );
}

#[tokio::test]
async fn c6_2_a75_the_texts_of_a_request_that_failed_on_the_wire() {
    run_cells("the texts on the wire", the_texts_on_the_wire()).await;
}
