//! C11.11 and C11.12, the call on a URL that the API returned: `concord::transfer::at`
//! (docs/reference/contracts.md C11.11, C11.12 and C8.6).
//!
//! Every test goes through descriptions of `concord::api!` and generated clients over a
//! `concord_testkit::Script`, on Tokio's paused clock. The upload host is a second
//! description, with its own `base` and none of the API's credentials, as the module's
//! documentation shows.

mod common;

use common::engine::{failure, secs, success};
use common::stream;
use concord::error::Kind;
use concord::transfer::{at, TransferError};
use concord::transport::{TransportError, TryInfo};
use concord::{http, reqwest, Base, Body, Middleware, Next, Retry, Secret};
use concord_testkit::Script;
use serde::Deserialize;
use std::error::Error as _;
use std::future::IntoFuture;
use tokio::time::Instant;

/// What the API returns: where to upload, and the headers to send there.
#[derive(Debug, Deserialize)]
struct Ticket {
    url: String,
    headers: Vec<(String, String)>,
}

/// A middleware of the API client: it writes `x-api-stamp` on every request it sees.
#[derive(Debug)]
struct Stamp;

impl Middleware for Stamp {
    async fn handle(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        request
            .headers_mut()
            .insert("x-api-stamp", http::HeaderValue::from_static("stamped"));
        next.send(request, info).await
    }
}

concord::api! {
    client Api(token: Secret) {
        base "https://api.example.test";
        headers {
            "authorization" = token;
            "x-api-plain" = "plain";
        }
        query { "api_key" = token; }
        middleware Stamp;
        POST ticket() -> Ticket { path "uploads"; read concord::Json; }
    }
}

concord::api! {
    client Uploads(origin: Base) {
        base origin;
        headers { "x-upload" = "1"; }
        POST send(bytes: Body) { body bytes; }
        PUT put(bytes: Body) { body bytes; }
        GET fetch() -> Ticket { read concord::Json; }
    }
}

concord::api! {
    client Signed(origin: Base, key: Secret) {
        base origin;
        query { "sig" = key; }
        POST send(bytes: Body) { body bytes; }
    }
}

/// The upload host of most tests.
const ORIGIN: &str = "https://uploads.example.test";

fn base(text: &str) -> Base {
    match text.parse::<Base>() {
        Ok(base) => base,
        Err(_) => panic!("a base: {text}"),
    }
}

/// `Uploads` at this origin, over this script.
fn uploads_at(origin: &str, script: &Script) -> Uploads {
    Uploads::new(base(origin)).with_transport(script.clone())
}

/// `Uploads` at [`ORIGIN`], over this script.
fn uploads(script: &Script) -> Uploads {
    uploads_at(ORIGIN, script)
}

/// The call that `at` returns, or the test fails with the variant of the refusal.
#[track_caller]
fn accepted<C>(result: Result<C, TransferError>, url: &str) -> C {
    match result {
        Ok(call) => call,
        Err(error) => panic!("{url:?} was refused: {}", variant(&error)),
    }
}

/// The refusal of `at`, or the test fails.
#[track_caller]
fn refused<C>(result: Result<C, TransferError>, url: &str) -> TransferError {
    match result {
        Ok(_) => panic!("{url:?} was accepted"),
        Err(error) => error,
    }
}

/// The name of the variant, without the text of the error (which is under test).
fn variant(error: &TransferError) -> &'static str {
    match error {
        TransferError::InvalidUrl(_) => "InvalidUrl",
        TransferError::InvalidUri(_) => "InvalidUri",
        TransferError::NotAbsolute => "NotAbsolute",
        TransferError::UnsupportedScheme => "UnsupportedScheme",
        TransferError::ForbiddenUrlPart => "ForbiddenUrlPart",
        TransferError::ChangedUrl => "ChangedUrl",
        TransferError::EmptyQuery => "EmptyQuery",
        TransferError::UrlTooLong => "UrlTooLong",
        _ => "another variant",
    }
}

/// What an error prints, with its whole source chain.
fn printed(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = format!("{error} | {error:?}");
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(&format!(" | {cause} | {cause:?}"));
        source = cause.source();
    }
    text
}

/// A `200` with no body for the next attempt.
fn ok(script: &Script) {
    script.answer(200);
}

/// The URLs that reached the transport, in order.
fn urls(script: &Script) -> Vec<String> {
    script
        .requests()
        .iter()
        .map(|request| request.url().to_owned())
        .collect()
}

// C11.11: `at` refuses a URL that would not be sent as it is written ---------------------

#[tokio::test(start_paused = true)]
async fn c11_11_an_accepted_url_reaches_the_transport_byte_for_byte() {
    let cases = [
        "https://uploads.example.test/file?sig=%2f+&key=1&key=2",
        "https://uploads.example.test/a/b%2Fc/d%2fe?x=%7E~&y=a+b&z=&bare&&w==1&",
        "https://uploads.example.test/?&a=1",
        "https://uploads.example.test/",
        "https://uploads.example.test/file",
        "https://uploads.example.test/x?k=1&k=1&k=2&K=3",
        "https://uploads.example.test/%E6%97%A5%e6%9c%ac",
        "https://uploads.example.test/a;b=c,d@e:f!$()*'?q=%23%26%3D",
        "https://uploads.example.test/upload/%2E%2Ehidden?x-amz-signature=0a1B%2B%2f",
    ];
    for url in cases {
        let script = Script::new();
        ok(&script);
        let call = accepted(at(uploads(&script).send(Body::from("payload")), url), url);
        success(call.await);
        assert_eq!(urls(&script), [url]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_11_a_port_and_an_address_that_are_written_canonically_are_kept() {
    let cases = [
        (
            "https://uploads.example.test:8443",
            "https://uploads.example.test:8443/x?a=1",
        ),
        (
            "http://uploads.example.test:8080",
            "http://uploads.example.test:8080/x",
        ),
        (
            "http://uploads.example.test",
            "http://uploads.example.test/x",
        ),
        ("http://127.0.0.1:8080", "http://127.0.0.1:8080/x"),
        ("http://[::1]:8080", "http://[::1]:8080/x"),
    ];
    for (origin, url) in cases {
        let script = Script::new();
        ok(&script);
        let call = accepted(
            at(uploads_at(origin, &script).send(Body::from("p")), url),
            url,
        );
        success(call.await);
        assert_eq!(urls(&script), [url]);
    }
}

#[test]
fn c11_11_a_relative_url_is_not_absolute() {
    let script = Script::new();
    for url in [
        "",
        "file",
        "/file",
        "./file",
        "../file",
        "?a=1",
        "#part",
        "//uploads.example.test/file",
    ] {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert!(
            matches!(error, TransferError::NotAbsolute),
            "{url:?}: {}",
            variant(&error)
        );
    }
    assert!(script.requests().is_empty());
}

#[test]
fn c11_11_a_fragment_or_user_information_is_forbidden() {
    let script = Script::new();
    for url in [
        "https://uploads.example.test/file#part",
        "https://uploads.example.test/file?a=1#",
        "https://uploads.example.test/#",
        "https://user:pass@uploads.example.test/file",
        "https://user@uploads.example.test/file",
        "https://:pass@uploads.example.test/file",
    ] {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert!(
            matches!(error, TransferError::ForbiddenUrlPart),
            "{url:?}: {}",
            variant(&error)
        );
    }
    // An empty user information is refused as user information, though the parser
    // would erase it.
    for url in [
        "https://@uploads.example.test/file",
        "https://:@uploads.example.test/file",
    ] {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert_eq!(variant(&error), "ForbiddenUrlPart", "{url:?}");
    }
    // An encoded `#` is data, not a fragment.
    let url = "https://uploads.example.test/file?x=%23";
    let _ = accepted(at(uploads(&script).send(Body::from("p")), url), url);
    assert!(script.requests().is_empty());
}

#[test]
fn c11_11_a_spelling_that_parsing_would_change_is_refused() {
    let script = Script::new();
    for url in [
        "HTTPS://uploads.example.test/file",
        "https://UPLOADS.example.test/file",
        "https://uploads.example.test:443/file",
        "https://uploads.example.test:0443/file",
        "https://uploads.example.test:/file",
        "http://uploads.example.test:80/file",
        "https://uploads.example.test",
        "https://uploads.example.test?a=1",
        "https://uploads.example.test/a/../file",
        "https://uploads.example.test/./file",
        "https://uploads.example.test/a/%2e%2e/file",
        "https://uploads.example.test/a/%2E/file",
        "https://uploads.example.test/é",
        "https://uploads.example.test/a b",
        "https://uploads.example.test/x?a=b c",
        "https://uploads.example.test/x?a=<b>",
        "http://0x7f.0.0.1/x",
        "http://127.1/x",
        "http://[0:0::1]/x",
        "http://[::0001]/x",
    ] {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert!(
            matches!(error, TransferError::ChangedUrl),
            "{url:?}: {}",
            variant(&error)
        );
    }
    assert!(script.requests().is_empty());
}

#[test]
fn c11_11_whitespace_controls_slashes_and_backslashes_each_have_one_variant() {
    // One input, one variant, as the rustdoc of each variant says.
    let script = Script::new();
    let cases: [(&str, &str); 16] = [
        // Leading or trailing spaces and controls, removed tabs, LF and CR: the spelling.
        (" https://uploads.example.test/file", "ChangedUrl"),
        ("https://uploads.example.test/file ", "ChangedUrl"),
        ("\u{1}https://uploads.example.test/file", "ChangedUrl"),
        ("https://uploads.example.test/fi\tle", "ChangedUrl"),
        ("https://uploads.example.test/fi\nle", "ChangedUrl"),
        ("https://uploads.example.test/fi\rle", "ChangedUrl"),
        // A space or a control in the path is encoded: the spelling.
        ("https://uploads.example.test/fi le", "ChangedUrl"),
        ("https://uploads.example.test/fi\u{1}le", "ChangedUrl"),
        // A space or a control in the host does not parse.
        ("https://uploads.exa mple.test/file", "InvalidUrl"),
        ("https://uploads\u{1}.example.test/file", "InvalidUrl"),
        // Missing or extra slashes, a backslash in place of the authority's slashes.
        ("https:uploads.example.test/file", "NotAbsolute"),
        ("https:/uploads.example.test/file", "NotAbsolute"),
        ("https:///uploads.example.test/file", "NotAbsolute"),
        ("https:\\\\uploads.example.test\\file", "NotAbsolute"),
        // A backslash in the path: the spelling.
        ("https://uploads.example.test\\file", "ChangedUrl"),
        ("https://uploads.example.test/a\\b", "ChangedUrl"),
    ];
    for (url, expected) in cases {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert_eq!(variant(&error), expected, "{url:?}");
    }
    assert!(script.requests().is_empty());
}

#[test]
fn c11_11_a_query_of_separators_only_is_an_empty_query() {
    let script = Script::new();
    for url in [
        "https://uploads.example.test/file?",
        "https://uploads.example.test/file?&",
        "https://uploads.example.test/file?&&",
        "https://uploads.example.test/?&&&",
    ] {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert!(
            matches!(error, TransferError::EmptyQuery),
            "{url:?}: {}",
            variant(&error)
        );
    }
    // One pair that is not empty is a query: `?&=` has the pair `=`.
    for url in [
        "https://uploads.example.test/file?=",
        "https://uploads.example.test/file?&a",
    ] {
        let _ = accepted(at(uploads(&script).send(Body::from("p")), url), url);
    }
}

#[test]
fn c11_11_a_scheme_other_than_http_or_https_is_unsupported() {
    let script = Script::new();
    for url in [
        "ftp://uploads.example.test/file",
        "ws://uploads.example.test/file",
        "wss://uploads.example.test/file",
        "data://uploads.example.test/file",
        "httpx://uploads.example.test/file",
    ] {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert!(
            matches!(error, TransferError::UnsupportedScheme),
            "{url:?}: {}",
            variant(&error)
        );
    }
    // Without a literal `://` and a nonempty authority, the text is not absolute first.
    for url in [
        "mailto:someone@uploads.example.test",
        "data:text/plain,abc",
        "file:///etc/passwd",
    ] {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert_eq!(variant(&error), "NotAbsolute", "{url:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_11_the_engine_bound_of_65534_bytes_is_reached_exactly() {
    let prefix = "https://uploads.example.test/";
    let at_bound = format!("{prefix}{}", "a".repeat(65_534 - prefix.len()));
    let over = format!("{prefix}{}", "a".repeat(65_535 - prefix.len()));
    assert_eq!(at_bound.len(), 65_534);

    let script = Script::new();
    ok(&script);
    let call = accepted(
        at(uploads(&script).send(Body::from("p")), &at_bound),
        "65534 bytes",
    );
    success(call.await);
    assert_eq!(urls(&script), [at_bound]);

    let error = refused(
        at(uploads(&script).send(Body::from("p")), &over),
        "65535 bytes",
    );
    assert!(
        matches!(error, TransferError::UrlTooLong),
        "{}",
        variant(&error)
    );
    assert_eq!(script.requests().len(), 1);
}

#[test]
fn c11_11_a_url_that_does_not_parse_keeps_its_value_free_cause() {
    let script = Script::new();
    for url in [
        "https://[::1/s3cr3t",
        "https://uploads.example.test:99999/s3cr3t",
        "https://uploads..example.test%00/s3cr3t",
    ] {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert!(
            matches!(error, TransferError::InvalidUrl(_)),
            "{url:?}: {}",
            variant(&error)
        );
        assert!(
            error.source().is_some(),
            "{url:?}: the parser's error is kept"
        );
        assert!(!printed(&error).contains("s3cr3t"), "{url:?}");
    }
    // A refusal for a reason has no lower cause.
    for url in [
        "/s3cr3t",
        "https://uploads.example.test/s3cr3t#x",
        "HTTPS://uploads.example.test/s3cr3t",
        "https://uploads.example.test/s3cr3t?",
        "ftp://uploads.example.test/s3cr3t",
    ] {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert!(error.source().is_none(), "{url:?}: {}", variant(&error));
    }
}

#[test]
fn c11_11_a_url_that_parses_but_is_no_http_uri_is_invalid_uri() {
    // A backtick in the host keeps its spelling as a URL, and the HTTP URI parser refuses
    // it. The parser's error is kept, and holds no part of the URL.
    let script = Script::new();
    for url in [
        "https://sto`rage.test/p",
        "https://uploads`s3cr3t.example.test/file",
    ] {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        assert_eq!(variant(&error), "InvalidUri", "{url:?}");
        assert!(
            error.source().is_some(),
            "{url:?}: the parser's error is kept"
        );
        assert!(!printed(&error).contains("s3cr3t"), "{url:?}");
        assert!(!printed(&error).contains("rage"), "{url:?}");
    }
    assert!(script.requests().is_empty());
}

#[test]
fn c11_11_no_refusal_prints_or_holds_the_url() {
    let script = Script::new();
    let long = format!("https://uploads.example.test/s3cr3t/{}", "a".repeat(70_000));
    let cases = [
        "/s3cr3t",
        "https://user:s3cr3t@uploads.example.test/x",
        "https://uploads.example.test/x?sig=s3cr3t#s3cr3t",
        "HTTPS://uploads.example.test/x?sig=s3cr3t",
        "https://uploads.example.test/x/../s3cr3t",
        "https://uploads.example.test/s3cr3t?",
        "ftp://uploads.example.test/s3cr3t",
        "https://[::1/s3cr3t",
        long.as_str(),
    ];
    for url in cases {
        let error = refused(at(uploads(&script).send(Body::from("p")), url), url);
        let cloned = error.clone();
        for error in [&error, &cloned] {
            let text = printed(error);
            assert!(!text.contains("s3cr3t"), "{}: {text}", variant(error));
            assert!(
                !text.contains("uploads.example"),
                "{}: {text}",
                variant(error)
            );
        }
    }
}

// C11.12: a call on a returned URL carries only what its own description says -----------

#[tokio::test(start_paused = true)]
async fn c11_12_nothing_of_the_api_client_travels_with_the_upload() {
    // One transport for both clients, as an application often has.
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"url":"https://uploads.example.test/put/obj?sig=abc%2F&exp=1","headers":[]}"#);
    ok(&script);
    let api = Api::new("tok-s3cr3t").with_transport(script.clone());
    let ticket = success(api.ticket().await);
    let call = accepted(
        at(uploads(&script).send(Body::from("payload")), &ticket.url),
        &ticket.url,
    );
    success(call.await);

    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    // The API's own request did carry them: the test sees what it looks for.
    assert_eq!(requests[0].header("x-api-stamp"), Some("stamped"));
    assert_eq!(requests[0].header("x-api-plain"), Some("plain"));
    assert!(requests[0].headers().contains_key("authorization"));
    assert!(requests[0].url().contains("api_key="));

    let upload = &requests[1];
    assert_eq!(upload.url(), ticket.url);
    assert_eq!(upload.info().endpoint(), "uploads.send");
    assert_eq!(upload.header("x-upload"), Some("1"));
    assert_eq!(upload.body(), Some(&b"payload"[..]));
    for name in ["authorization", "x-api-stamp", "x-api-plain"] {
        assert!(!upload.headers().contains_key(name), "{name} travelled");
    }
    for (name, value) in upload.headers() {
        assert!(!value.is_sensitive(), "{name} is marked sensitive");
        assert!(
            !String::from_utf8_lossy(value.as_bytes()).contains("s3cr3t"),
            "{name} holds the API's token"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c11_12_headers_returned_at_run_time_are_added_through_edit_request() {
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(
            r#"{"url":"https://uploads.example.test/u","headers":[["x-upload","2"],["x-meta","a"],["x-meta","b"]]}"#,
        );
    ok(&script);
    let ticket = success(uploads(&script).fetch().await);
    let mut returned = http::HeaderMap::new();
    for (name, value) in &ticket.headers {
        returned.append(
            http::HeaderName::from_bytes(name.as_bytes()).expect("a name"),
            http::HeaderValue::from_str(value).expect("a value"),
        );
    }
    let call = accepted(
        at(uploads(&script).put(Body::from("p")), &ticket.url),
        &ticket.url,
    )
    .edit_request(move |request| {
        for name in returned.keys() {
            request.headers_mut().remove(name);
            for value in returned.get_all(name) {
                request.headers_mut().append(name.clone(), value.clone());
            }
        }
    });
    success(call.await);

    let upload = &script.requests()[1];
    assert_eq!(upload.url(), "https://uploads.example.test/u");
    let values = |name: &str| -> Vec<String> {
        upload
            .headers()
            .get_all(name)
            .iter()
            .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
            .collect()
    };
    assert_eq!(values("x-upload"), ["2"]);
    assert_eq!(values("x-meta"), ["a", "b"]);
}

#[tokio::test(start_paused = true)]
async fn c11_12_a_url_at_another_origin_is_refused_before_anything_is_sent() {
    let cases = [
        "https://other.example.test/file",
        "https://uploads.example.test.evil.test/file",
        "http://uploads.example.test/file",
        "https://uploads.example.test:8443/file",
        "https://example.test/file",
    ];
    for url in cases {
        let script = Script::new();
        ok(&script);
        // `at` cannot see the origin of an opaque call: the engine refuses it when it builds
        // the request.
        let call = accepted(at(uploads(&script).send(Body::from("p")), url), url);
        let error = failure(call.await);
        assert_eq!(error.kind(), Kind::Build, "{url}");
        assert_eq!(
            error.to_string(),
            "build error: the URL leaves the origin of the call, in uploads.send",
            "{url}"
        );
        assert!(!printed(&error).contains("example.test"), "{url}");
        assert!(script.requests().is_empty(), "{url}");
        assert_eq!(script.unused(), 1, "{url}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_12_the_path_of_a_base_neither_narrows_nor_widens_the_origin() {
    let script = Script::new();
    ok(&script);
    let url = "https://uploads.example.test/elsewhere/x?a=1";
    let call = accepted(
        at(
            uploads_at("https://uploads.example.test/v1/bucket", &script).send(Body::from("p")),
            url,
        ),
        url,
    );
    success(call.await);
    assert_eq!(urls(&script), [url]);

    let script = Script::new();
    ok(&script);
    let url = "https://other.example.test/v1/bucket/x";
    let call = accepted(
        at(
            uploads_at("https://uploads.example.test/v1/bucket", &script).send(Body::from("p")),
            url,
        ),
        url,
    );
    assert_eq!(failure(call.await).kind(), Kind::Build);
    assert!(script.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn c11_12_with_base_moves_the_origin_that_is_checked() {
    let script = Script::new();
    ok(&script);
    let url = "https://mirror.example.test/x";
    let client = uploads(&script).with_base("https://mirror.example.test");
    let call = accepted(at(client.send(Body::from("p")), url), url);
    success(call.await);
    assert_eq!(urls(&script), [url]);

    let script = Script::new();
    ok(&script);
    let url = "https://uploads.example.test/x";
    let client = uploads(&script).with_base("https://mirror.example.test");
    let call = accepted(at(client.send(Body::from("p")), url), url);
    assert_eq!(failure(call.await).kind(), Kind::Build);
    assert!(script.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn c11_12_the_call_keeps_its_retry_policy_and_the_exact_url_on_every_attempt() {
    let script = Script::new();
    script.answer(503).header("retry-after", "2");
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"url":"u","headers":[]}"#);
    let url = "https://uploads.example.test/t?sig=%2f&x=1";
    let start = Instant::now();
    let call = accepted(at(uploads(&script).fetch(), url), url).with_retry(Retry::standard());
    let ticket = success(call.await);
    assert_eq!(ticket.url, "u");
    let requests = script.requests();
    assert_eq!(urls(&script), [url, url]);
    assert_eq!(requests[0].info().number(), 1);
    assert_eq!(requests[1].info().number(), 2);
    assert_eq!(requests[1].at() - requests[0].at(), secs(2));
    assert_eq!(start.elapsed(), secs(2));
}

#[tokio::test(start_paused = true)]
async fn c11_12_an_unmarked_post_is_not_sent_again() {
    let script = Script::new();
    script.answer(503).header("retry-after", "1");
    ok(&script);
    let url = "https://uploads.example.test/u";
    let call = accepted(at(uploads(&script).send(Body::from("p")), url), url)
        .with_retry(Retry::standard());
    let error = failure(call.await);
    assert!(error.is_status(503));
    assert_eq!(error.tries(), 1);
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_12_a_stream_body_is_sent_once() {
    let script = Script::new();
    script.answer(503).read_upload().header("retry-after", "1");
    ok(&script);
    let url = "https://uploads.example.test/u?part=1";
    // `PUT` may be sent again, but not with a body that is a stream.
    let call = accepted(at(uploads(&script).put(stream("one-shot")), url), url)
        .with_retry(Retry::standard());
    let error = failure(call.await);
    assert!(error.is_status(503));
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].upload().bytes(), b"one-shot");
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_12_the_call_keeps_its_timeouts() {
    let url = "https://uploads.example.test/u";
    // The call timeout, set after `at`.
    let script = Script::new();
    script.answer(200).after(secs(20));
    let start = Instant::now();
    let call =
        accepted(at(uploads(&script).send(Body::from("p")), url), url).with_timeout(secs(10));
    let error = failure(call.await);
    assert!(error.is_timeout());
    assert_eq!(start.elapsed(), secs(10));

    // The call timeout, set before `at`.
    let script = Script::new();
    script.answer(200).after(secs(20));
    let start = Instant::now();
    let call = accepted(
        at(
            uploads(&script).send(Body::from("p")).with_timeout(secs(7)),
            url,
        ),
        url,
    );
    let error = failure(call.await);
    assert!(error.is_timeout());
    assert_eq!(start.elapsed(), secs(7));

    // The idle timeout of 30 s, by default.
    let script = Script::new();
    script.answer(200).after(secs(31));
    let start = Instant::now();
    let call = accepted(at(uploads(&script).send(Body::from("p")), url), url);
    let error = failure(call.await);
    assert!(error.is_timeout());
    assert_eq!(start.elapsed(), secs(30));
}

#[tokio::test(start_paused = true)]
async fn c11_12_the_call_keeps_its_reader_and_its_arms() {
    let url = "https://uploads.example.test/t";
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"url":"https://uploads.example.test/next","headers":[["a","b"]]}"#);
    let ticket = success(accepted(at(uploads(&script).fetch(), url), url).await);
    assert_eq!(ticket.url, "https://uploads.example.test/next");
    assert_eq!(ticket.headers, [("a".to_owned(), "b".to_owned())]);

    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"url":7}"#);
    let error = failure(accepted(at(uploads(&script).fetch(), url), url).await);
    assert_eq!(error.kind(), Kind::Mismatch);

    let script = Script::new();
    script.answer(404);
    let error = failure(accepted(at(uploads(&script).fetch(), url), url).await);
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(404));
}

#[tokio::test(start_paused = true)]
async fn c11_12_a_call_dropped_during_its_retry_wait_sends_nothing_more() {
    let script = Script::new();
    script.answer(503).header("retry-after", "10");
    ok(&script);
    let url = "https://uploads.example.test/t";
    let call = accepted(at(uploads(&script).fetch(), url), url).with_retry(Retry::standard());
    assert!(tokio::time::timeout(secs(5), call.into_future())
        .await
        .is_err());
    tokio::time::advance(secs(60)).await;
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_12_a_clone_of_the_returned_call_is_the_same_call() {
    // `at` returns the call's own type: it can be cloned and sent twice, as any call.
    let script = Script::new();
    ok(&script);
    ok(&script);
    let url = "https://uploads.example.test/c?n=1";
    let call = accepted(at(uploads(&script).send(Body::from("p")), url), url);
    let again = call.clone();
    success(call.await);
    success(again.await);
    assert_eq!(urls(&script), [url, url]);
    assert!(script
        .requests()
        .iter()
        .all(|request| request.body() == Some(&b"p"[..])));
}

#[tokio::test(start_paused = true)]
async fn c11_11_a_description_with_a_secret_query_entry_is_not_sent_as_written() {
    // The byte-for-byte promise holds for a description with no `Secret` query entry.
    // With one, the engine adds the entry again to the URL given to `at_url` (C8.6), in place
    // of a returned pair with the same key, even when the URL had no such key.
    for (url, returned) in [
        ("https://uploads.example.test/u?a=1", None),
        (
            "https://uploads.example.test/u?sig=returned&a=1",
            Some("sig=returned"),
        ),
    ] {
        let script = Script::new();
        ok(&script);
        let signed = Signed::new(base(ORIGIN), "k3y").with_transport(script.clone());
        let call = accepted(at(signed.send(Body::from("p")), url), url);
        success(call.await);

        let sent = urls(&script);
        assert_eq!(sent.len(), 1);
        let sent = &sent[0];
        assert_ne!(sent, url, "not sent as written");
        assert!(
            sent.starts_with("https://uploads.example.test/u?"),
            "{sent}"
        );
        assert!(sent.contains("a=1"), "{sent}");
        assert!(sent.contains("sig=k3y"), "{sent}: the entry is added again");
        if let Some(returned) = returned {
            assert!(
                !sent.contains(returned),
                "{sent}: the returned pair is replaced"
            );
        }
    }

    // The same URL on the description without a `Secret` entry is sent as written.
    let script = Script::new();
    ok(&script);
    let url = "https://uploads.example.test/u?sig=returned&a=1";
    success(accepted(at(uploads(&script).send(Body::from("p")), url), url).await);
    assert_eq!(urls(&script), [url]);
}
