//! Returned URLs through application descriptions and the scripted transport.

// Integration-test helpers fail immediately on invalid fixtures or assertions.
#![allow(clippy::unwrap_used, clippy::panic)]

use concord::bytes::Bytes;
use concord::error::Kind;
use concord::http::{HeaderMap, HeaderName, HeaderValue, Method};
use concord::reqwest::{Request, Response};
use concord::transfer::{at, TransferError};
use concord::transport::{TransportError, TryInfo};
use concord::{Base, Body, Middleware, Next, Retry, Secret};
use concord_testkit::Script;
use serde::Deserialize;
use std::error::Error as _;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

const ORIGIN: &str = "https://storage.test";
const SIGNED: &str = "https://storage.test/bucket/a%2Fb/c%2fd?sig=%2F%2B+=~%2f%2b%7e%7E&empty=&repeat=one&repeat=two&bare&eq=a=b==";

#[derive(Deserialize)]
struct Ticket {
    url: String,
    headers: Vec<(String, String)>,
}

concord::api! {
    client MainApi(key: Secret) {
        base "https://api.test/v1";
        headers { "authorization" = "Bearer {key}"; "x-description" = "api-only"; }
        query { "token" = key; "normal" = "api-query"; }
        middleware Credential;
        GET ticket() -> Ticket { path "ticket"; read concord::Json; }
        GET header_only_ticket() -> Ticket {
            query { remove "token"; }
            read concord::Json;
        }
    }
}

concord::api! {
    client Uploads(origin: Base) {
        base origin;
        GET fetch() -> String { read concord::Text; }
        GET download() -> concord::ByteStream { read concord::read::Stream; }
        PUT put(payload: Body) { body payload; }
        POST post(payload: Body) { body payload; }
        POST safe_post(payload: Body) { body payload; idempotent; }
    }
}

struct Credential;

impl Middleware for Credential {
    async fn handle(
        &self,
        mut request: Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<Response, TransportError> {
        request.headers_mut().insert(
            "x-middleware-key",
            HeaderValue::from_static("api-middleware-secret"),
        );
        next.send(request, info).await
    }
}

fn uploads(script: &Script) -> Uploads {
    Uploads::new(ORIGIN.parse::<Base>().unwrap()).with_transport(script.clone())
}

fn assert_attempts(script: &Script, expected: &[u32]) {
    let requests = script.requests();
    assert_eq!(requests.len(), expected.len());
    assert_eq!(
        requests
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        expected
    );
}

fn reason(error: &TransferError) -> &'static str {
    match error {
        TransferError::InvalidUrl(_) => "InvalidUrl",
        TransferError::InvalidUri(_) => "InvalidUri",
        TransferError::NotAbsolute => "NotAbsolute",
        TransferError::UnsupportedScheme => "UnsupportedScheme",
        TransferError::ForbiddenUrlPart => "ForbiddenUrlPart",
        TransferError::ChangedUrl => "ChangedUrl",
        TransferError::UrlTooLong => "UrlTooLong",
        _ => panic!("unexpected transfer error"),
    }
}

fn refused(url: &str, expected: &str) -> TransferError {
    let script = Script::new();
    script.answer(200).body("unused");
    let error = at(uploads(&script).fetch(), url).unwrap_err();
    assert_eq!(reason(&error), expected, "input: {url:?}");
    assert_attempts(&script, &[]);
    assert_eq!(script.unused(), 1);
    error
}

#[tokio::test(start_paused = true)]
async fn c11_12_nothing_of_api_client_travels() {
    let api_script = Script::new();
    api_script.answer(200).body(
        serde_json::json!({
            "url": SIGNED,
            "headers": [["x-ticket", "first"], ["x-ticket", "second"],
                        ["content-range", "bytes 0-6/7"]]
        })
        .to_string(),
    );
    let api = MainApi::new("api/key+~").with_transport(api_script.clone());
    let ticket = api.ticket().await.unwrap();
    assert_attempts(&api_script, &[1]);
    assert_eq!(api_script.unused(), 0);
    let api_requests = api_script.requests();
    let request = &api_requests[0];
    assert_eq!(
        request.url().as_str(),
        "https://api.test/v1/ticket?token=api%2Fkey%2B~&normal=api-query"
    );
    assert_eq!(request.header("authorization"), Some("Bearer api/key+~"));
    assert!(request.headers()["authorization"].is_sensitive());
    assert_eq!(request.header("x-description"), Some("api-only"));
    assert_eq!(
        request.header("x-middleware-key"),
        Some("api-middleware-secret")
    );

    let mut returned = HeaderMap::new();
    for (name, value) in ticket.headers {
        returned.append(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(&value).unwrap(),
        );
    }
    let mut expected = returned.clone();
    expected.insert(
        "content-type",
        HeaderValue::from_static("application/octet-stream"),
    );
    let transfer_script = Script::new();
    transfer_script.answer(200).read_upload();
    at(
        uploads(&transfer_script).put(Body::new("payload", "application/octet-stream")),
        &ticket.url,
    )
    .unwrap()
    .edit_request(move |request| request.headers_mut().extend(returned.clone()))
    .await
    .unwrap();
    assert_attempts(&transfer_script, &[1]);
    assert_eq!(transfer_script.unused(), 0);
    let requests = transfer_script.requests();
    let request = &requests[0];
    assert_eq!(request.url().as_str().as_bytes(), SIGNED.as_bytes());
    assert_eq!(request.method(), Method::PUT);
    assert_eq!(request.headers(), &expected);
    assert_eq!(request.body(), Some(b"payload".as_slice()));
    assert_eq!(request.upload().bytes(), b"payload");
    assert_attempts(&api_script, &[1]);
}

#[tokio::test(start_paused = true)]
async fn c11_11_signed_target_is_exact_for_every_preserved_query_class() {
    for (origin, url, target) in [
        (ORIGIN, SIGNED, "/bucket/a%2Fb/c%2fd?sig=%2F%2B+=~%2f%2b%7e%7E&empty=&repeat=one&repeat=two&bare&eq=a=b=="),
        (ORIGIN, "https://storage.test/p?&key=1&&key=2&", "/p?&key=1&&key=2&"),
        (ORIGIN, "https://storage.test/p?=&bare&&empty=&=value&", "/p?=&bare&&empty=&=value&"),
        (ORIGIN, "https://storage.test/%23/%00/%FF/%2eX?sig=%23%26%3D%3f%00%FF&plus=+&tilde=~", "/%23/%00/%FF/%2eX?sig=%23%26%3D%3f%00%FF&plus=+&tilde=~"),
        (ORIGIN, "https://storage.test//a///b/", "//a///b/"),
        (ORIGIN, "https://storage.test/", "/"),
        (ORIGIN, "https://storage.test/p?sig=%&other=%2&third=%zz", "/p?sig=%&other=%2&third=%zz"),
        (ORIGIN, "https://storage.test/p?key=a\\b", "/p?key=a\\b"),
        ("http://storage.test", "http://storage.test/p?x=1", "/p?x=1"),
        ("https://storage.test:444", "https://storage.test:444/p?x=1", "/p?x=1"),
        ("https://127.0.0.1", "https://127.0.0.1/p?x=1", "/p?x=1"),
        ("https://[::1]", "https://[::1]/p?x=1", "/p?x=1"),
        ("https://xn--bcher-kva.test", "https://xn--bcher-kva.test/p?x=1", "/p?x=1"),
    ] {
        let script = Script::new();
        script.answer(200).body("download");
        let client = Uploads::new(origin.parse::<Base>().unwrap()).with_transport(script.clone());
        assert_eq!(at(client.fetch(), url).unwrap().await.unwrap(), "download");
        assert_attempts(&script, &[1]);
        assert_eq!(script.unused(), 0);
        let requests = script.requests();
        assert_eq!(requests[0].url().as_str().as_bytes(), url.as_bytes());
        let uri: concord::http::Uri = requests[0].url().as_str().parse().unwrap();
        assert_eq!(uri.path_and_query().unwrap().as_str().as_bytes(), target.as_bytes());
        assert!(requests[0].headers().is_empty());
        assert_eq!(requests[0].method(), Method::GET);
    }
}

#[test]
fn c11_11_every_rewriting_class_is_refused_without_sending() {
    for (url, expected) in [
        // Relative, path, query, fragment, empty and authority references.
        ("next", "NotAbsolute"),
        ("../next", "NotAbsolute"),
        ("/absolute", "NotAbsolute"),
        ("?query", "NotAbsolute"),
        ("#fragment", "NotAbsolute"),
        ("", "NotAbsolute"),
        ("//storage.test/path", "NotAbsolute"),
        // Both fragment spellings; %23 is covered by the acceptance test.
        ("https://storage.test/p#fragment", "ForbiddenUrlPart"),
        ("https://storage.test/p#", "ForbiddenUrlPart"),
        // User information can be retained by at_url, and empty user info erased.
        ("https://user:password@storage.test/p", "ForbiddenUrlPart"),
        ("https://user@storage.test/p", "ForbiddenUrlPart"),
        ("https://:password@storage.test/p", "ForbiddenUrlPart"),
        ("https://@storage.test/p", "ForbiddenUrlPart"),
        // Scheme, host, IDNA, IP address and port normalization.
        ("HTTPS://storage.test/p", "ChangedUrl"),
        ("https://STORAGE.test/p", "ChangedUrl"),
        ("https://bücher.test/p", "ChangedUrl"),
        ("https://127.1/p", "ChangedUrl"),
        ("https://0x7f000001/p", "ChangedUrl"),
        ("https://[0:0:0:0:0:0:0:1]/p", "ChangedUrl"),
        ("https://[2001:DB8::1]/p", "ChangedUrl"),
        ("https://storage.test:443/p", "ChangedUrl"),
        ("https://storage.test:/p", "ChangedUrl"),
        ("https://storage.test:00444/p", "ChangedUrl"),
        ("https://storage.test:70000/p", "InvalidUrl"),
        ("https://[invalid]/p", "InvalidUrl"),
        ("https://sto rage.test/p", "InvalidUrl"),
        ("https://sto\u{1}rage.test/p", "InvalidUrl"),
        // Inserted root path and removed literal/encoded dot segments.
        ("https://storage.test", "ChangedUrl"),
        ("https://storage.test?sig=x", "ChangedUrl"),
        ("https://storage.test/a/./b", "ChangedUrl"),
        ("https://storage.test/a/../b", "ChangedUrl"),
        ("https://storage.test/a/%2e/b", "ChangedUrl"),
        ("https://storage.test/a/.%2E/b", "ChangedUrl"),
        ("https://storage.test/a/%2e./b", "ChangedUrl"),
        ("https://storage.test/a/%2E%2e/b", "ChangedUrl"),
        ("https://storage.test/a\\b", "ChangedUrl"),
        ("https:storage.test/p", "NotAbsolute"),
        ("https:/storage.test/p", "NotAbsolute"),
        ("https://", "NotAbsolute"),
        ("https:///storage.test/p", "NotAbsolute"),
        ("https:////storage.test/p", "NotAbsolute"),
        ("https:\\storage.test/p", "NotAbsolute"),
        (r"https:\\storage.test/p", "NotAbsolute"),
        // Trimming, removed controls, and percent encoding of literal data.
        (" https://storage.test/p", "ChangedUrl"),
        ("https://storage.test/p ", "ChangedUrl"),
        ("https://storage.test/a\tb", "ChangedUrl"),
        ("https://storage.test/p?q=a\nb", "ChangedUrl"),
        ("https://storage.test/p?q=a\rb", "ChangedUrl"),
        ("https://storage.test/a\u{1}b", "ChangedUrl"),
        ("https://storage.test/a b", "ChangedUrl"),
        ("https://storage.test/café", "ChangedUrl"),
        ("https://storage.test/p?q=a b", "ChangedUrl"),
        ("https://storage.test/p?q=café", "ChangedUrl"),
        // Unsupported and opaque URLs; a URL-parser/HTTP-URI boundary.
        ("ftp://storage.test/p", "UnsupportedScheme"),
        ("data://storage.test/p", "UnsupportedScheme"),
        ("data:text/plain,secret", "NotAbsolute"),
        ("mailto:secret@storage.test", "NotAbsolute"),
        ("https://sto`rage.test/p", "InvalidUri"),
    ] {
        refused(url, expected);
    }
}

#[test]
fn c11_11_documented_precedence_selects_one_reachable_variant() {
    // Every variant is reachable. Inputs with several refusal reasons pin the
    // first failing check, so callers can distinguish the documented cases.
    for (url, expected) in [
        ("a".repeat(65_535), "UrlTooLong"),
        ("https:///storage.test/p#fragment".to_owned(), "NotAbsolute"),
        (
            "ftp://@storage.test/p#fragment".to_owned(),
            "ForbiddenUrlPart",
        ),
        ("https://[invalid]/p?".to_owned(), "InvalidUrl"),
        ("ftp://STORAGE.test/p?".to_owned(), "UnsupportedScheme"),
        ("HTTPS://storage.test/p?".to_owned(), "ChangedUrl"),
        ("https://sto`rage.test/p".to_owned(), "InvalidUri"),
    ] {
        refused(&url, expected);
    }
}

#[test]
fn c11_11_invalid_uri_is_reachable_with_a_canonical_url() {
    let url = "https://sto`rage.test/p";
    let parsed = concord::reqwest::Url::parse(url).unwrap();
    assert_eq!(parsed.as_str().as_bytes(), url.as_bytes());
    assert_eq!(
        parsed.join(url).unwrap().as_str().as_bytes(),
        url.as_bytes()
    );
    let parser_error = url.parse::<concord::http::Uri>().unwrap_err();
    let error = refused(url, "InvalidUri");
    let TransferError::InvalidUri(cause) = error else {
        panic!("canonical URL did not produce InvalidUri");
    };
    let cause = cause
        .downcast_ref::<concord::http::uri::InvalidUri>()
        .unwrap();
    assert_eq!(cause.to_string(), parser_error.to_string());
}

#[tokio::test(start_paused = true)]
async fn c11_11_url_length_bound_is_exact() {
    let prefix = "https://storage.test/";
    let url = format!("{prefix}{}", "a".repeat(65_534 - prefix.len()));
    let script = Script::new();
    script.answer(200).body("ok");
    assert_eq!(
        at(uploads(&script).fetch(), &url).unwrap().await.unwrap(),
        "ok"
    );
    assert_attempts(&script, &[1]);
    assert_eq!(
        script.requests()[0].url().as_str().as_bytes(),
        url.as_bytes()
    );
    refused(&format!("{url}a"), "UrlTooLong");
}

#[tokio::test(start_paused = true)]
async fn c11_12_other_origin_is_refused_before_edits_or_sending() {
    for url in [
        "http://storage.test/p?sig=secret",
        "https://other.test/p?sig=secret",
        "https://storage.test:444/p?sig=secret",
        "https://storage.test./p?sig=secret",
    ] {
        let script = Script::new();
        script.answer(200).body("unused");
        let edits = Arc::new(AtomicUsize::new(0));
        let count = edits.clone();
        let call = at(uploads(&script).fetch(), url).unwrap();
        let error = call
            .edit_request(move |_| {
                count.fetch_add(1, Ordering::SeqCst);
            })
            .await
            .unwrap_err();
        assert_eq!(error.kind(), Kind::Build);
        assert_eq!(error.tries(), 0);
        assert_eq!(edits.load(Ordering::SeqCst), 0);
        assert_attempts(&script, &[]);
        assert_eq!(script.unused(), 1);
    }
    // Origin equality uses the built base, including its effective default port;
    // a path prefix does not restrict a same-origin sibling target.
    let script = Script::new();
    script.answer(200).body("ok");
    let client = Uploads::new("https://STORAGE.test:443/prefix".parse::<Base>().unwrap())
        .with_transport(script.clone());
    let url = "https://storage.test/sibling?sig=%2f";
    assert_eq!(at(client.fetch(), url).unwrap().await.unwrap(), "ok");
    assert_attempts(&script, &[1]);
    assert_eq!(script.requests()[0].url().as_str(), url);
    let script = Script::new();
    script.answer(200).body("ok");
    at(
        uploads(&script)
            .with_base("https://other.test".parse::<Base>().unwrap())
            .fetch(),
        "https://other.test/sibling?sig=%2f",
    )
    .unwrap()
    .await
    .unwrap();
    assert_attempts(&script, &[1]);
    assert_eq!(
        script.requests()[0].url().as_str(),
        "https://other.test/sibling?sig=%2f"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_11_no_secret_query_preserves_bytes_even_with_a_secret_header() {
    let url = "https://storage.test/p%2Fq?&sig=%2f+~&token=returned&to%6ben=alias&&bare&";
    let reattached = "https://storage.test/p%2Fq?&sig=%2f+~&&bare&&token=api%2Fkey%2B~";
    let script = Script::new();
    for _ in 0..2 {
        script.answer(200).body(r#"{"url":"unused","headers":[]}"#);
    }
    let api = MainApi::new("api/key+~")
        .with_base(ORIGIN.parse::<Base>().unwrap())
        .with_transport(script.clone());
    // Both calls have the same secret header and middleware. Only the first
    // endpoint removes the inherited Secret query entry from its description.
    at(api.header_only_ticket(), url).unwrap().await.unwrap();
    at(api.ticket(), url).unwrap().await.unwrap();
    assert_attempts(&script, &[1, 1]);
    assert_eq!(script.unused(), 0);
    let requests = script.requests();
    assert_eq!(requests[0].url().as_str().as_bytes(), url.as_bytes());
    assert_eq!(requests[1].url().as_str().as_bytes(), reattached.as_bytes());
    for request in requests {
        assert_eq!(request.header("authorization"), Some("Bearer api/key+~"));
        assert!(request.headers()["authorization"].is_sensitive());
        assert_eq!(request.header("x-description"), Some("api-only"));
        assert_eq!(
            request.header("x-middleware-key"),
            Some("api-middleware-secret")
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c11_11_opaque_secret_queries_are_a_documented_boundary() {
    // Deliberately violate the separate-description recipe. at cannot inspect
    // these inherited secrets; the engine replaces decoded aliases and appends.
    for (url, expected) in [
        (
            "https://storage.test/p?sig=%2f&token=old&to%6ben=again&token&token=&keep=+&&",
            "https://storage.test/p?sig=%2f&keep=+&&&token=api%2Fkey%2B~",
        ),
        (
            "https://storage.test/p?sig=%2f",
            "https://storage.test/p?sig=%2f&token=api%2Fkey%2B~",
        ),
    ] {
        let script = Script::new();
        script.answer(200).body(r#"{"url":"unused","headers":[]}"#);
        let api = MainApi::new("api/key+~")
            .with_base(ORIGIN.parse::<Base>().unwrap())
            .with_transport(script.clone());
        at(api.ticket(), url).unwrap().await.unwrap();
        assert_attempts(&script, &[1]);
        let requests = script.requests();
        assert_eq!(requests[0].url().as_str().as_bytes(), expected.as_bytes());
        assert_eq!(
            requests[0].header("authorization"),
            Some("Bearer api/key+~")
        );
        assert_eq!(requests[0].header("x-description"), Some("api-only"));
        assert_eq!(
            requests[0].header("x-middleware-key"),
            Some("api-middleware-secret")
        );
    }
}

#[test]
fn c11_12_header_map_extend_replaces_names_and_preserves_all_values() {
    let mut original = HeaderMap::new();
    original.append("x-replace", HeaderValue::from_static("old-first"));
    original.append("x-replace", HeaderValue::from_static("old-second"));
    original.append("x-keep", HeaderValue::from_static("keep-first"));
    original.append("x-keep", HeaderValue::from_static("keep-second"));
    let mut returned = HeaderMap::new();
    let mut sensitive = HeaderValue::from_static("new-first");
    sensitive.set_sensitive(true);
    returned.append("x-replace", sensitive);
    returned.append("x-replace", HeaderValue::from_static("new-second"));
    returned.append("x-added", HeaderValue::from_static("added-first"));
    returned.append("x-added", HeaderValue::from_static("added-second"));
    let mut loop_result = original.clone();
    for name in returned.keys() {
        loop_result.remove(name);
        for value in returned.get_all(name) {
            loop_result.append(name.clone(), value.clone());
        }
    }
    original.extend(returned);
    assert_eq!(original, loop_result);
    for (name, expected) in [
        ("x-replace", ["new-first", "new-second"]),
        ("x-keep", ["keep-first", "keep-second"]),
        ("x-added", ["added-first", "added-second"]),
    ] {
        let values = original
            .get_all(name)
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(values, expected);
    }
    assert_eq!(original.len(), 6);
    assert!(original["x-replace"].is_sensitive());
}

#[tokio::test(start_paused = true)]
async fn c11_11_errors_and_generated_calls_never_print_values() {
    let urls = [
        ("https://[SECRET]/p?sig=SECRET", "InvalidUrl"),
        ("https://secret`host.test/p?sig=SECRET", "InvalidUri"),
        ("SECRET/relative", "NotAbsolute"),
        ("ftp://storage.test/SECRET", "UnsupportedScheme"),
        ("https://SECRET@storage.test/p", "ForbiddenUrlPart"),
        ("https://storage.test/../SECRET", "ChangedUrl"),
    ];
    for (url, expected) in urls {
        let error = refused(url, expected);
        let cloned = error.clone();
        assert_eq!(format!("{error:?}"), expected);
        assert_eq!(format!("{cloned:?}"), expected);
        assert!(!error.to_string().contains("SECRET"));
        let mut cause = error.source();
        while let Some(source) = cause {
            assert!(!format!("{source:?} {source}").contains("SECRET"));
            cause = source.source();
        }
        assert_eq!(
            error.source().is_some(),
            matches!(expected, "InvalidUrl" | "InvalidUri")
        );
    }
    let long = format!("https://storage.test/SECRET{}", "a".repeat(65_534));
    let error = refused(&long, "UrlTooLong");
    assert_eq!(format!("{error:?}"), "UrlTooLong");
    assert!(!error.to_string().contains("SECRET"));
    assert!(error.source().is_none());

    let script = Script::new();
    script.answer(503).body("SECRET response");
    let call = at(
        uploads(&script).fetch(),
        "https://storage.test/SECRET?sig=SECRET",
    )
    .unwrap();
    assert!(!format!("{call:?}").contains("SECRET"));
    let error = call.await.unwrap_err();
    assert_eq!(error.tries(), 1);
    assert!(!format!("{error:?} {error}").contains("SECRET"));
    assert_attempts(&script, &[1]);
    let script = Script::new();
    let error = at(uploads(&script).fetch(), "https://secret.test/p?sig=SECRET")
        .unwrap()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(error.tries(), 0);
    assert!(!format!("{error:?} {error}").contains("SECRET"));
    let mut cause = error.source();
    while let Some(source) = cause {
        assert!(!format!("{source:?} {source}").contains("SECRET"));
        cause = source.source();
    }
    assert_attempts(&script, &[]);
}

#[test]
fn c11_11_preserves_value_free_parser_causes() {
    let url = refused("https://[invalid]/secret", "InvalidUrl");
    let TransferError::InvalidUrl(stored) = &url else {
        panic!("URL cause missing")
    };
    assert!(std::ptr::eq(
        stored.as_ref() as &dyn std::error::Error,
        url.source().unwrap()
    ));
    assert_eq!(
        url.source().unwrap().to_string(),
        concord::reqwest::Url::parse("https://[invalid]/different")
            .unwrap_err()
            .to_string()
    );
    let uri = refused("https://sto`rage.test/secret", "InvalidUri");
    let TransferError::InvalidUri(stored) = &uri else {
        panic!("URI cause missing")
    };
    assert!(std::ptr::eq(
        stored.as_ref() as &dyn std::error::Error,
        uri.source().unwrap()
    ));
    assert!(uri.source().unwrap().is::<concord::http::uri::InvalidUri>());
}

fn retry() -> Retry {
    Retry::none()
        .tries(2)
        .when(|_| Some(Duration::from_secs(2)))
}

#[tokio::test(start_paused = true)]
async fn c11_12_redirect_response_is_returned_without_another_send() {
    let script = Script::new();
    script
        .answer(302)
        .header("location", "https://other.test/result?sig=other");
    script.answer(200).body("unused");
    let error = at(uploads(&script).fetch(), SIGNED)
        .unwrap()
        .await
        .unwrap_err();
    assert!(error.is_status(302));
    assert_eq!(error.tries(), 1);
    assert_attempts(&script, &[1]);
    assert_eq!(script.requests()[0].url().as_str(), SIGNED);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_12_target_changing_edits_are_a_documented_boundary() {
    // Raw edits run after origin checking. at retains them, including edits
    // already installed when it receives the otherwise opaque generated call.
    let script = Script::new();
    script.answer(200).body("edited");
    let edited = uploads(&script).fetch().edit_request(|request| {
        *request.url_mut() = "https://other.test/changed?sig=%2F".parse().unwrap();
    });
    assert_eq!(at(edited, SIGNED).unwrap().await.unwrap(), "edited");
    assert_attempts(&script, &[1]);
    assert_eq!(
        script.requests()[0].url().as_str(),
        "https://other.test/changed?sig=%2F"
    );
    assert_eq!(script.unused(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_12_retry_idempotence_reader_and_transport_are_retained() {
    for (method, marked, expected) in [
        (Method::PUT, false, 2),
        (Method::POST, false, 1),
        (Method::POST, true, 2),
    ] {
        let script = Script::new();
        script.answer(503).read_upload();
        script.answer(200).read_upload();
        let client = uploads(&script).with_retry(retry());
        let body = Body::from("payload");
        let error = if method == Method::PUT {
            at(client.put(body), SIGNED).unwrap().await.err()
        } else if marked {
            at(client.safe_post(body), SIGNED).unwrap().await.err()
        } else {
            at(client.post(body), SIGNED).unwrap().await.err()
        };
        if expected == 1 {
            let error = error.unwrap();
            assert!(error.is_status(503));
            assert_eq!(error.tries(), 1);
            assert_attempts(&script, &[1]);
            assert_eq!(script.unused(), 1);
        } else {
            assert!(error.is_none());
            assert_attempts(&script, &[1, 2]);
            assert_eq!(script.unused(), 0);
            let requests = script.requests();
            assert_eq!(requests[1].at() - requests[0].at(), Duration::from_secs(2));
        }
        for request in script.requests() {
            assert_eq!(request.method(), method);
            assert_eq!(request.url().as_str().as_bytes(), SIGNED.as_bytes());
            assert_eq!(request.upload().bytes(), b"payload");
        }
    }
    let script = Script::new();
    script.answer(503).body("refused");
    script.answer(200).body("reader result");
    let value = at(uploads(&script).fetch().with_retry(retry()), SIGNED)
        .unwrap()
        .await
        .unwrap();
    assert_eq!(value, "reader result");
    assert_attempts(&script, &[1, 2]);
}

struct OneChunk {
    chunk: Option<Bytes>,
    dropped: Arc<AtomicUsize>,
}

impl futures_core::Stream for OneChunk {
    type Item = Result<Bytes, std::io::Error>;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.chunk.take().map(Ok))
    }
}

impl Drop for OneChunk {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

fn one_shot(dropped: &Arc<AtomicUsize>) -> Body {
    Body::stream(OneChunk {
        chunk: Some(Bytes::from_static(b"payload")),
        dropped: dropped.clone(),
    })
}

#[tokio::test(start_paused = true)]
async fn c11_12_stream_body_validation_never_claims_and_sends_once() {
    let script = Script::new();
    script.answer(503).read_upload();
    script.answer(200).read_upload();
    let dropped = Arc::new(AtomicUsize::new(0));
    let call = uploads(&script).put(one_shot(&dropped)).with_retry(retry());
    assert!(at(call.clone(), "relative").is_err());
    let checked = at(call.clone(), SIGNED).unwrap();
    assert_attempts(&script, &[]);
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    let inspect_error = checked.request().unwrap_err();
    assert_eq!(inspect_error.kind(), Kind::Build);
    assert_eq!(inspect_error.tries(), 0);
    let error = checked.await.unwrap_err();
    assert!(error.is_status(503));
    assert_eq!(error.tries(), 1);
    assert_attempts(&script, &[1]);
    assert_eq!(script.unused(), 1);
    assert_eq!(script.requests()[0].url().as_str(), SIGNED);
    assert_eq!(script.requests()[0].upload().bytes(), b"payload");
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    let error = at(call, SIGNED).unwrap().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(error.tries(), 0);
    assert_attempts(&script, &[1]);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_12_timeouts_and_stream_handoff_keep_engine_clocks() {
    for total in [false, true] {
        let script = Script::new();
        script.never();
        let call = uploads(&script).fetch();
        let call = if total {
            call.with_timeout(Duration::from_secs(3))
        } else {
            call.with_idle_timeout(Duration::from_secs(3))
        };
        let start = tokio::time::Instant::now();
        let error = at(call, SIGNED).unwrap().await.unwrap_err();
        assert!(error.is_timeout());
        assert_eq!(error.tries(), 1);
        assert_eq!(start.elapsed(), Duration::from_secs(3));
        assert_attempts(&script, &[1]);
    }
    let script = Script::new();
    script
        .answer(200)
        .header("x-result", "kept")
        .body("first")
        .pause(Duration::from_secs(4))
        .body("second");
    script.answer(200).body("unused");
    let mut stream = at(
        uploads(&script)
            .download()
            .with_timeout(Duration::from_secs(2))
            .with_retry(retry()),
        SIGNED,
    )
    .unwrap()
    .await
    .unwrap();
    assert_eq!(stream.status(), concord::http::StatusCode::OK);
    assert_eq!(stream.headers().get("x-result"), Some("kept"));
    assert_eq!(stream.try_next().await.unwrap().unwrap(), "first");
    let error = stream.try_next().await.unwrap_err();
    assert!(error.is_timeout());
    assert_attempts(&script, &[1]);
    assert_eq!(script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_12_drop_releases_body_and_cancels_retry_wait() {
    let script = Script::new();
    let dropped = Arc::new(AtomicUsize::new(0));
    let checked = at(uploads(&script).put(one_shot(&dropped)), SIGNED).unwrap();
    drop(checked);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_attempts(&script, &[]);
    script.never();
    let checked = at(uploads(&script).put(one_shot(&dropped)), SIGNED).unwrap();
    let mut sending = Box::pin(checked.into_future());
    let mut context = Context::from_waker(Waker::noop());
    assert!(sending.as_mut().poll(&mut context).is_pending());
    assert_attempts(&script, &[1]);
    drop(sending);
    assert_eq!(dropped.load(Ordering::SeqCst), 2);

    let script = Script::new();
    script.answer(503).body("retry");
    script.answer(200).body("next independent call");
    let checked = at(uploads(&script).fetch().with_retry(retry()), SIGNED).unwrap();
    let mut sending = Box::pin(checked.into_future());
    assert!(sending.as_mut().poll(&mut context).is_pending());
    assert_attempts(&script, &[1]);
    assert_eq!(script.unused(), 1);
    drop(sending);
    tokio::time::advance(Duration::from_secs(60)).await;
    assert_attempts(&script, &[1]);
    assert_eq!(
        at(uploads(&script).fetch(), SIGNED).unwrap().await.unwrap(),
        "next independent call"
    );
    assert_attempts(&script, &[1, 1]);
    assert_eq!(script.unused(), 0);
}

concord::api! {
    client Storage {
        base "https://storage.test";
        problem StorageProblem read concord::Json;
        scope objects {
            GET get() -> String { read concord::Text; }
        }
    }
}

#[derive(Debug, Deserialize)]
struct StorageProblem {}

#[test]
fn c11_11_at_call_keeps_the_generated_name_and_the_validator_source() {
    use concord::transfer::at_call;

    let script = Script::new();
    let storage = Storage::new().with_transport(script.clone());
    let long = format!("ftp://user@storage.test/#{}", "x".repeat(65_535));
    for (url, expected) in [
        (long.as_str(), "UrlTooLong"),
        ("relative?token=PRIVATE", "NotAbsolute"),
        ("ftp://user@storage.test/p#PRIVATE", "ForbiddenUrlPart"),
        ("ftp://storage.test/p?token=PRIVATE", "UnsupportedScheme"),
        ("https://[invalid]/PRIVATE", "InvalidUrl"),
        ("https://UPPER.test/p?token=PRIVATE", "ChangedUrl"),
        ("https://sto`rage.test/PRIVATE", "InvalidUri"),
    ] {
        let call = storage.objects().get();
        let legacy = at(call.clone(), url).unwrap_err();
        let error: storage::Error = at_call(call, url).unwrap_err();
        assert_eq!(error.kind(), Kind::Build);
        assert_eq!(error.endpoint(), "storage.objects.get");
        assert_eq!(error.tries(), 0);
        assert!(!error.request_not_sent());
        assert!(error.status().is_none());
        assert!(error.headers().is_none());
        assert!(error.problem().is_none());
        assert!(error.body().is_none());
        assert_eq!(
            error.to_string(),
            "build error: the transfer URL could not be used, in storage.objects.get"
        );
        let source = error
            .source()
            .unwrap()
            .downcast_ref::<TransferError>()
            .unwrap();
        assert_eq!(reason(source), expected);
        assert_eq!(reason(&legacy), expected);
        assert_eq!(
            source.source().map(ToString::to_string),
            legacy.source().map(ToString::to_string)
        );
        assert!(!format!("{error} {error:?} {}", error.details()).contains("PRIVATE"));
    }
    assert_attempts(&script, &[]);
}

#[tokio::test(start_paused = true)]
async fn c11_11_at_call_keeps_the_query_and_leaves_the_origin_check_to_the_call() {
    use concord::transfer::at_call;

    let script = Script::new();
    script.answer(200).body("ok");
    let uploads = uploads(&script);
    let error: uploads::Error = at_call(uploads.fetch(), "https://elsewhere.test/")
        .unwrap()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert!(error
        .to_string()
        .contains("the URL leaves the origin of the call"));
    assert_attempts(&script, &[]);

    async fn fetch(call: uploads::Call<uploads::Fetch>) -> uploads::Result<String> {
        at_call(call, SIGNED)?.await
    }
    assert_eq!(fetch(uploads.fetch()).await.unwrap(), "ok");
    assert_eq!(script.requests().first().unwrap().url().as_str(), SIGNED);
}

#[test]
fn c11_11_handwritten_calls_keep_at_and_generated_calls_supply_their_name() {
    use concord::page::{Call, NamedCall};

    #[derive(Clone)]
    struct Handwritten(String);

    impl Call for Handwritten {
        type Value = ();
        type Problem = ();

        fn at_url(self, url: &str) -> Self {
            Self(url.to_owned())
        }
    }

    fn name<C: NamedCall>(_: &C) -> &'static str {
        C::ENDPOINT
    }

    assert_eq!(at(Handwritten(String::new()), SIGNED).unwrap().0, SIGNED);
    assert_eq!(name(&Storage::new().objects().get()), "storage.objects.get");
}
