//! What every test of the suite builds a request with: the hidden module, as
//! `tests/tour` uses it, and a few ways to look at the `reqwest::Request` that comes out.
#![allow(dead_code)]

/// What the tests of a send share (C3 to C7, C9).
pub mod engine;

use concord::__private::{Core, Draft, Options};
use concord::error::Kind;
use concord::reqwest;

/// What the generated code of an endpoint gets from `Draft::finish`.
pub type Built = Result<reqwest::Request, concord::Error>;

/// `Piece::Val(name, value)`, through the adapter, as the generated code writes it.
/// The caller imports `concord::__private::AdaptValue as _`.
#[allow(unused_macros)]
macro_rules! val {
    ($name:expr, $value:expr) => {
        ::concord::__private::Piece::Val($name, &(&::concord::__private::ValueRef(&$value)).adapt())
    };
}

/// `Piece::Lit(text)`.
#[allow(unused_macros)]
macro_rules! lit {
    ($text:expr) => {
        ::concord::__private::Piece::Lit($text)
    };
}

/// A draft with this method and the fixed base `base "…";`.
pub fn draft(method: http::Method, base: &'static str) -> Draft {
    let mut draft = Draft::new(method);
    draft.base(&[concord::__private::Piece::Lit(base)]);
    draft
}

/// `GET` on `https://api.example.com/v1`.
pub fn get() -> Draft {
    draft(http::Method::GET, "https://api.example.com/v1")
}

/// `request()`: steps 2 and 3 with `Draft::finish`, on a client with no settings.
pub fn finish(draft: Draft, endpoint: &'static str) -> Built {
    draft.finish(&Core::<()>::default(), &Options::<()>::default(), endpoint)
}

/// `into_request()` and a send: steps 2 and 3 with `Draft::finish_for_send`.
pub fn finish_for_send(draft: Draft, endpoint: &'static str) -> Built {
    draft.finish_for_send(&Core::<()>::default(), &Options::<()>::default(), endpoint)
}

/// The request, or the test fails with the kind of the error (never its text: it may
/// be the very thing under test).
pub fn ok(built: Built) -> reqwest::Request {
    match built {
        Ok(request) => request,
        Err(error) => panic!(
            "expected a request, got an error of kind {:?}",
            error.kind()
        ),
    }
}

/// The printed text of a Build error, or the test fails.
pub fn build_error(built: Built) -> String {
    match built {
        Ok(request) => panic!("expected a Build error, got a request to {}", request.url()),
        Err(error) => {
            assert_eq!(error.kind(), Kind::Build, "the kind of the error");
            error.to_string()
        }
    }
}

/// The text of a shape that cannot be written at a place (one pattern for
/// every place; the name is the input, or the key of a keyed entry).
pub fn cannot_be_written(name: &str, place: &str, endpoint: &str) -> String {
    format!("build error: input {name} cannot be written in a {place}, in {endpoint}")
}

/// The URL as text.
pub fn url(request: &reqwest::Request) -> String {
    request.url().as_str().to_owned()
}

/// The query as text, `""` when there is none. A `?` with no pair after it fails the test:
/// a request with no query pair has no `?`.
#[track_caller]
pub fn query(request: &reqwest::Request) -> String {
    match request.url().query() {
        None => String::new(),
        Some("") => panic!("a `?` with no pair: {}", request.url()),
        Some(query) => query.to_owned(),
    }
}

/// Every value of one header, in order.
pub fn header(request: &reqwest::Request, name: &str) -> Vec<String> {
    request
        .headers()
        .get_all(name)
        .iter()
        .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
        .collect()
}

/// Whether every value of one header is marked sensitive (and there is one).
pub fn sensitive(request: &reqwest::Request, name: &str) -> bool {
    let values: Vec<_> = request.headers().get_all(name).iter().collect();
    !values.is_empty() && values.iter().all(|value| value.is_sensitive())
}

/// The bytes of a body held in memory.
pub fn body_bytes(request: &reqwest::Request) -> Option<Vec<u8>> {
    request
        .body()
        .and_then(reqwest::Body::as_bytes)
        .map(<[u8]>::to_vec)
}

/// Compare two URLs, as text: the hex digits of `%XX` are upper case.
#[track_caller]
pub fn assert_url(request: &reqwest::Request, expected: &str) {
    assert_eq!(url(request), expected);
}

/// Compare two queries, as text: the hex digits of `%XX` are upper case.
#[track_caller]
pub fn assert_query(request: &reqwest::Request, expected: &str) {
    assert_eq!(query(request), expected);
}

/// The encoding of one character for a path hole or a query (C2.3, C2.6): itself when it is
/// `A-Z a-z 0-9 - . _ ~`, else `%XX` for each byte of its UTF-8.
pub fn encoded(c: char) -> String {
    if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~') {
        c.to_string()
    } else {
        let mut buffer = [0; 4];
        c.encode_utf8(&mut buffer)
            .bytes()
            .map(|byte| format!("%{byte:02X}"))
            .collect()
    }
}

/// A one-shot stream that gives these bytes. (Made from a `reqwest::Response`, so the suite
/// needs no stream crate of its own.)
pub fn stream(text: &'static str) -> concord::Body {
    let response = reqwest::Response::from(http::Response::new(reqwest::Body::from(text)));
    concord::Body::stream(response.bytes_stream())
}

/// Every byte of a body taken as a stream.
pub async fn read(stream: concord::body::BodyStream) -> Vec<u8> {
    let response = reqwest::Response::from(http::Response::new(reqwest::Body::wrap_stream(stream)));
    response.bytes().await.expect("the stream reads").to_vec()
}

/// Every byte of the body of a request, also when it is a stream.
pub async fn read_request_body(request: &mut reqwest::Request) -> Vec<u8> {
    let body = request.body_mut().take().expect("the request has a body");
    let response = reqwest::Response::from(http::Response::new(body));
    response.bytes().await.expect("the body reads").to_vec()
}
