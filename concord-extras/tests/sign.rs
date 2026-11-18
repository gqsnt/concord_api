#![cfg(feature = "sign")]
//! Signing through the public facade, without a public clock or live sockets.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord::http::{HeaderValue, Method};
use concord::reqwest::{Request, Response};
use concord::sign::{body_bytes, check_origin, CanonicalPath, Credentials, SigV4, SignError};
use concord::transport::{Transport, TransportError, TryInfo};
use concord::{Base, Body, Retry, Secret};
use concord_testkit::Script;
use futures_util::FutureExt;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::error::Error as _;
use std::future::IntoFuture;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

concord::api! {
    client Api(base: Base, signer: SigV4) {
        base base;
        middleware signer.clone();
        GET get() -> String { path "a/b"; query { "a" = "+"; "flag" = ""; } read concord::Text; }
        PUT put(bytes: Body) { path "object"; body bytes; }
        GET list(after?: String) -> Page {
            path "list";
            query { after; }
            read concord::Json;
            paginate concord::by_cursor(after, Page::next);
        }
    }
}
#[derive(serde::Deserialize)]
struct Page {
    next: Option<String>,
}
impl Page {
    fn next(&self) -> Option<String> {
        self.next.clone()
    }
}

fn base() -> Base {
    "https://s3.test/prefix".parse().unwrap()
}
fn credentials() -> Credentials {
    Credentials::new("ACCESS", "secret-key").session_token("session-secret")
}
fn signer() -> SigV4 {
    SigV4::new(
        &base(),
        credentials(),
        "us-east-1",
        "s3",
        CanonicalPath::Preserve,
    )
}
fn mac(key: &[u8], text: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(text.as_bytes());
    mac.finalize().into_bytes().to_vec()
}
// Independent reconstruction from the wire record. Path/query are supplied as
// known canonical inputs, rather than calling the implementation under test.
fn verify(
    request: &concord_testkit::script::Request,
    path: &str,
    query: &str,
    access: &str,
    secret: &str,
    token: Option<&str>,
    names: &str,
) {
    let timestamp = request.header("x-amz-date").unwrap();
    let date = &timestamp[..8];
    let auth = request.header("authorization").unwrap();
    let fields = names
        .split(';')
        .map(|name| {
            let values = request
                .headers()
                .get_all(name)
                .iter()
                .map(|value| {
                    value
                        .to_str()
                        .unwrap()
                        .split([' ', '\t'])
                        .filter(|word| !word.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{name}:{values}\n")
        })
        .collect::<String>();
    let payload = request.header("x-amz-content-sha256").unwrap();
    if payload != "UNSIGNED-PAYLOAD" {
        assert_eq!(
            payload,
            hex::encode(Sha256::digest(request.body().unwrap_or(b"")))
        );
    }
    let canonical = format!(
        "{}\n{path}\n{query}\n{fields}\n{names}\n{payload}",
        request.method()
    );
    let scope = format!("{date}/us-east-1/s3/aws4_request");
    let text = format!(
        "AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical))
    );
    let key = mac(format!("AWS4{secret}").as_bytes(), date);
    let key = mac(&key, "us-east-1");
    let key = mac(&key, "s3");
    let key = mac(&key, "aws4_request");
    let expected = format!(
        "AWS4-HMAC-SHA256 Credential={access}/{scope}, SignedHeaders={names}, Signature={}",
        hex::encode(mac(&key, &text))
    );
    assert!(
        auth == expected,
        "the signature must match the recorded request"
    );
    assert_eq!(request.header("x-amz-security-token"), token);
    assert!(request.headers()["authorization"].is_sensitive());
    if token.is_some() {
        assert!(request.headers()["x-amz-security-token"].is_sensitive());
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_each_attempt_signs_the_edited_request_again_after_time_moves() {
    let script = Script::new();
    script.answer(503).header("retry-after", "2");
    script.answer(200).body("ok");
    let edits = Arc::new(AtomicUsize::new(0));
    let api = Api::new(base(), signer())
        .with_transport(script.clone())
        .with_retry(Retry::standard().tries(2));
    let call = api.get().edit_request({
        let edits = edits.clone();
        move |request| {
            edits.fetch_add(1, Ordering::SeqCst);
            *request.method_mut() = Method::PUT;
            *request.body_mut() = Some("edited body".into());
            request
                .headers_mut()
                .insert("x-amz-meta-owner", HeaderValue::from_static("finance"));
        }
    });
    let exported = api.get().request().unwrap();
    assert!(!exported.headers().contains_key("authorization"));
    assert!(script.requests().is_empty());
    assert_eq!(call.await.unwrap(), "ok");
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(script.unused(), 0);
    assert_eq!(edits.load(Ordering::SeqCst), 1);
    for (i, request) in requests.iter().enumerate() {
        assert_eq!(request.info().number(), u32::try_from(i + 1).unwrap());
        assert_eq!(request.body(), Some(b"edited body".as_slice()));
        verify(
            request,
            "/prefix/a/b",
            "a=%2B&flag=",
            "ACCESS",
            "secret-key",
            Some("session-secret"),
            "host;x-amz-content-sha256;x-amz-date;x-amz-meta-owner;x-amz-security-token",
        );
    }
    assert!(requests[1].at().duration_since(requests[0].at()) >= Duration::from_secs(2));
    assert_ne!(
        requests[0].header("x-amz-date"),
        requests[1].header("x-amz-date")
    );
    assert!(requests[0].header("authorization") != requests[1].header("authorization"));
}

#[tokio::test(start_paused = true)]
async fn c12_9_whole_credential_replacement_reaches_every_clone_on_the_next_attempt() {
    #[derive(Clone)]
    struct Rotate {
        script: Script,
        credentials: Credentials,
    }
    impl Transport for Rotate {
        async fn send(&self, request: Request, info: TryInfo) -> Result<Response, TransportError> {
            self.credentials
                .replace(Credentials::new("NEXT", "next-key"));
            self.script.send(request, info).await
        }
    }
    let credentials = credentials();
    let signer = SigV4::new(
        &base(),
        credentials.clone(),
        "us-east-1",
        "s3",
        CanonicalPath::Preserve,
    );
    let script = Script::new();
    script.answer(503);
    script.answer(200).body("ok");
    let api = Api::new(base(), signer.clone())
        .with_transport(Rotate {
            script: script.clone(),
            credentials,
        })
        .with_retry(Retry::standard().tries(2));
    api.get().await.unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    verify(
        &requests[0],
        "/prefix/a/b",
        "a=%2B&flag=",
        "ACCESS",
        "secret-key",
        Some("session-secret"),
        "host;x-amz-content-sha256;x-amz-date;x-amz-security-token",
    );
    verify(
        &requests[1],
        "/prefix/a/b",
        "a=%2B&flag=",
        "NEXT",
        "next-key",
        None,
        "host;x-amz-content-sha256;x-amz-date",
    );
    let mut request = Request::new(Method::GET, "https://s3.test".parse().unwrap());
    signer.sign(&mut request, "us-east-1").unwrap();
    assert!(request.headers()["authorization"]
        .to_str()
        .unwrap()
        .contains("Credential=NEXT/"));
}

#[tokio::test(start_paused = true)]
async fn c12_9_concurrent_replacement_keeps_each_signature_on_one_credential_set() {
    let first = Credentials::new("FIRST", "first-key").session_token("first-token");
    let second = Credentials::new("SECOND", "second-key").session_token("second-token");
    let credentials = Credentials::new("FIRST", "first-key").session_token("first-token");
    let signer = SigV4::new(
        &base(),
        credentials.clone(),
        "us-east-1",
        "s3",
        CanonicalPath::Preserve,
    );
    let start = std::sync::Barrier::new(2);
    let signed = std::thread::scope(|scope| {
        // Disconnecting also stops the writer if signing unwinds.
        let (done, running) = std::sync::mpsc::channel::<()>();
        let credentials = &credentials;
        let start = &start;
        scope.spawn(move || {
            start.wait();
            while matches!(
                running.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ) {
                credentials.replace(first.clone());
                std::thread::yield_now();
                credentials.replace(second.clone());
                std::thread::yield_now();
            }
        });
        start.wait();
        let requests = (0..512)
            .map(|_| {
                let mut request = Request::new(Method::GET, "https://s3.test/".parse().unwrap());
                signer.sign(&mut request, "us-east-1").unwrap();
                request
            })
            .collect::<Vec<_>>();
        drop(done);
        requests
    });
    let script = Script::new();
    for request in signed {
        script.answer(200);
        script
            .send(request, TryInfo::new("sign.get", 1))
            .await
            .unwrap();
    }
    let requests = script.requests();
    assert_eq!(requests.len(), 512);
    for request in requests.iter() {
        let authorization = request.header("authorization").unwrap();
        let (access, secret, token) = if authorization.contains("Credential=FIRST/") {
            ("FIRST", "first-key", "first-token")
        } else {
            ("SECOND", "second-key", "second-token")
        };
        verify(
            request,
            "/",
            "",
            access,
            secret,
            Some(token),
            "host;x-amz-content-sha256;x-amz-date;x-amz-security-token",
        );
    }
}

#[tokio::test(start_paused = true)]
async fn c12_9_stale_signing_fields_and_transport_headers_are_handled() {
    let script = Script::new();
    script.answer(200).body("ok");
    let signer = SigV4::new(
        &base(),
        Credentials::new("ACCESS", "secret-key"),
        "us-east-1",
        "s3",
        CanonicalPath::Preserve,
    );
    Api::new(base(), signer)
        .with_transport(script.clone())
        .get()
        .edit_request(|r| {
            for name in [
                "authorization",
                "x-amz-date",
                "x-amz-content-sha256",
                "x-amz-security-token",
            ] {
                r.headers_mut()
                    .insert(name, HeaderValue::from_static("stale"));
            }
            for name in [
                "keep-alive",
                "proxy-connection",
                "te",
                "trailer",
                "upgrade",
                "proxy-authorization",
                "proxy-authenticate",
                "user-agent",
                "expect",
                "transfer-encoding",
                "x-amzn-trace-id",
                "x-volatile",
            ] {
                r.headers_mut()
                    .insert(name, HeaderValue::from_static("volatile"));
            }
            r.headers_mut().insert(
                "connection",
                HeaderValue::from_static("keep-alive, X-Volatile"),
            );
            r.headers_mut()
                .append("x-amz-meta-many", HeaderValue::from_static("  one\t two  "));
            r.headers_mut()
                .append("x-amz-meta-many", HeaderValue::from_static("three"));
        })
        .await
        .unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    verify(
        &requests[0],
        "/prefix/a/b",
        "a=%2B&flag=",
        "ACCESS",
        "secret-key",
        None,
        "host;x-amz-content-sha256;x-amz-date;x-amz-meta-many",
    );
    let auth = requests[0].header("authorization").unwrap();
    assert!(auth.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date;x-amz-meta-many,"));
    for name in [
        "keep-alive",
        "proxy-connection",
        "te",
        "trailer",
        "upgrade",
        "proxy-authorization",
        "proxy-authenticate",
        "user-agent",
        "expect",
        "transfer-encoding",
        "x-amzn-trace-id",
        "x-volatile",
    ] {
        assert_eq!(requests[0].headers()[name].as_bytes(), b"volatile");
    }
    assert_eq!(
        requests[0].headers()["connection"].as_bytes(),
        b"keep-alive, X-Volatile"
    );
}

fn stream(polls: Arc<AtomicUsize>) -> Body {
    let mut remaining = Some(concord::bytes::Bytes::from_static(b"piece"));
    Body::stream(futures_util::stream::poll_fn(move |_| {
        polls.fetch_add(1, Ordering::SeqCst);
        std::task::Poll::Ready(remaining.take().map(Ok::<_, std::io::Error>))
    }))
    .content_type("application/octet-stream")
}

#[tokio::test(start_paused = true)]
async fn c12_10_streams_are_refused_without_polling_or_sending() {
    for known_length in [false, true] {
        let script = Script::new();
        let polls = Arc::new(AtomicUsize::new(0));
        let error = Api::new(base(), signer())
            .with_transport(script.clone())
            .put(stream(polls.clone()))
            .edit_request(move |r| {
                if known_length {
                    r.headers_mut()
                        .insert("content-length", HeaderValue::from_static("5"));
                }
            })
            .await
            .unwrap_err();
        assert!(matches!(
            SignError::find(&error),
            Some(SignError::StreamingBody)
        ));
        assert_eq!(error.tries(), 1);
        assert_eq!(script.requests().len(), 0);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn c12_10_unsigned_streams_require_explicit_https_and_are_sent_once() {
    let script = Script::new();
    script.answer(503).read_upload();
    let polls = Arc::new(AtomicUsize::new(0));
    let error = Api::new(base(), signer().allow_unsigned_payload())
        .with_transport(script.clone())
        .with_retry(Retry::standard().tries(3))
        .put(stream(polls.clone()))
        .await
        .unwrap_err();
    assert!(error.is_status(503));
    assert_eq!(error.tries(), 1);
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    // This fixture is a stream: its payload mode is known independently of the signer.
    assert_eq!(
        requests[0].header("x-amz-content-sha256"),
        Some("UNSIGNED-PAYLOAD")
    );
    verify(
        &requests[0],
        "/prefix/object",
        "",
        "ACCESS",
        "secret-key",
        Some("session-secret"),
        "content-type;host;x-amz-content-sha256;x-amz-date;x-amz-security-token",
    );
    assert_eq!(requests[0].info().number(), 1);
    assert_eq!(requests[0].upload().bytes(), b"piece");
    assert_eq!(requests[0].upload().pieces(), 1);
    assert_eq!(
        requests[0].upload().state(),
        &concord_testkit::script::UploadState::Ended
    );
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    polls.store(0, Ordering::SeqCst);
    let http: Base = "http://s3.test".parse().unwrap();
    let signer = SigV4::new(
        &http,
        credentials(),
        "us-east-1",
        "s3",
        CanonicalPath::Preserve,
    )
    .allow_unsigned_payload();
    let script = Script::new();
    let error = Api::new(http, signer)
        .with_transport(script.clone())
        .put(stream(polls.clone()))
        .await
        .unwrap_err();
    assert!(matches!(
        SignError::find(&error),
        Some(SignError::InsecureUnsignedPayload)
    ));
    assert!(script.requests().is_empty());
    assert_eq!(polls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_10_wrong_origin_is_refused_before_invalid_credentials_are_used() {
    for target in [
        "http://s3.test/prefix",
        "https://other.test/prefix",
        "https://s3.test:444/prefix",
    ] {
        let script = Script::new();
        let signer = SigV4::new(
            &base(),
            Credentials::new("", ""),
            "bad/scope",
            "s3",
            CanonicalPath::Preserve,
        );
        let error = Api::new(base(), signer)
            .with_transport(script.clone())
            .get()
            .edit_request(move |r| *r.url_mut() = target.parse().unwrap())
            .await
            .unwrap_err();
        assert!(matches!(
            SignError::find(&error),
            Some(SignError::WrongOrigin)
        ));
        assert_eq!(script.requests().len(), 0);
    }
    let request = Request::new(Method::GET, "https://s3.test:443/another".parse().unwrap());
    check_origin(&request, &base()).unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_10_invalid_requests_are_typed_and_never_reach_the_transport() {
    for mode in 0..7 {
        let script = Script::new();
        let error = Api::new(base(), signer())
            .with_transport(script.clone())
            .get()
            .edit_request(move |r| match mode {
                0 => {
                    r.headers_mut()
                        .insert("host", HeaderValue::from_static("foreign.test"));
                }
                1 => {
                    r.headers_mut()
                        .insert("connection", HeaderValue::from_static("host"));
                }
                2 => {
                    r.headers_mut()
                        .insert("connection", HeaderValue::from_static("X-Amz-Date"));
                }
                3 => {
                    r.url_mut().set_path("/literal+plus");
                }
                4 => {
                    r.url_mut().set_fragment(Some("private-fragment"));
                }
                5 => {
                    r.headers_mut().insert(
                        "x-amz-meta-private",
                        HeaderValue::from_bytes(b"\xff").unwrap(),
                    );
                }
                _ => {
                    r.headers_mut()
                        .insert("large", HeaderValue::from_str(&"x".repeat(65_536)).unwrap());
                }
            })
            .await
            .unwrap_err();
        let cause = SignError::find(&error).unwrap();
        match mode {
            5 => {
                assert!(matches!(cause, SignError::InvalidHeaderText(_)));
                assert!(cause.source().is_some());
            }
            6 => assert!(matches!(cause, SignError::TooLarge)),
            _ => assert!(matches!(cause, SignError::InvalidRequest)),
        }
        assert!(script.requests().is_empty());
        for text in [
            format!("{error:?}"),
            error.to_string(),
            format!("{cause:?}"),
            cause.to_string(),
        ] {
            for secret in [
                "secret-key",
                "session-secret",
                "private-fragment",
                "foreign.test",
            ] {
                assert!(!text.contains(secret));
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c12_10_large_payload_hashing_yields_and_drop_sends_nothing() {
    let script = Script::new();
    let api = Api::new(base(), signer()).with_transport(script.clone());
    let call = api.put(Body::new(vec![0; 256 * 1024], "application/octet-stream"));
    let mut sending = Box::pin(call.into_future());
    for _ in 0..3 {
        assert!(sending.as_mut().now_or_never().is_none());
    }
    drop(sending);
    assert!(script.requests().is_empty());
}

#[test]
fn c12_10_credentials_and_diagnostics_hide_values_and_retain_causes() {
    let input = Secret::new("snapshot-secret");
    let credentials = Credentials::new("ACCESS", input.clone());
    input.replace("changed-secret");
    let signer = SigV4::new(
        &base(),
        credentials.clone(),
        "us-east-1",
        "s3",
        CanonicalPath::Preserve,
    );
    assert_eq!(format!("{signer:?}"), "SigV4(..)");
    assert_eq!(format!("{credentials:?}"), "Credentials(..)");
    let cause = SignError::failed(std::io::Error::other("private-cause"));
    assert_eq!(format!("{cause:?}"), "Failed(..)");
    assert_eq!(cause.to_string(), "the request proof failed");
    assert_eq!(cause.source().unwrap().to_string(), "private-cause");
    // Like the other errors of the extras, a SignError can be cloned; the clone shares the cause.
    fn cloneable<T: Clone + Send + Sync + 'static>() {}
    cloneable::<SignError>();
    let copy = cause.clone();
    assert_eq!(copy.source().unwrap().to_string(), "private-cause");
    let mut request = Request::new(Method::GET, "https://s3.test".parse().unwrap());
    *request.body_mut() = Some("bytes".into());
    let borrowed = body_bytes(&request).unwrap();
    assert_eq!(
        borrowed.as_ptr(),
        request.body().unwrap().as_bytes().unwrap().as_ptr()
    );
}

#[tokio::test(start_paused = true)]
async fn c12_9_every_page_is_signed_with_its_own_query() {
    let script = Script::new();
    script.answer(200).body(r#"{"next":"second"}"#);
    script.answer(200).body(r#"{"next":null}"#);
    let api = Api::new(base(), signer()).with_transport(script.clone());
    let mut pages = api.list().pages();
    assert!(pages.try_next().await.unwrap().is_some());
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(pages.try_next().await.unwrap().is_some());
    assert!(pages.try_next().await.unwrap().is_none());
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        [1, 1]
    );
    verify(
        &requests[0],
        "/prefix/list",
        "",
        "ACCESS",
        "secret-key",
        Some("session-secret"),
        "host;x-amz-content-sha256;x-amz-date;x-amz-security-token",
    );
    verify(
        &requests[1],
        "/prefix/list",
        "after=second",
        "ACCESS",
        "secret-key",
        Some("session-secret"),
        "host;x-amz-content-sha256;x-amz-date;x-amz-security-token",
    );
    assert_ne!(
        requests[0].header("x-amz-date"),
        requests[1].header("x-amz-date")
    );
    assert!(requests[0].header("authorization") != requests[1].header("authorization"));
}

#[tokio::test(start_paused = true)]
async fn c12_9_query_spaces_literal_plus_and_empty_fields_are_signed_canonically() {
    let script = Script::new();
    script.answer(200).body("ok");
    Api::new(base(), signer())
        .with_transport(script.clone())
        .get()
        .edit_request(|request| {
            request
                .url_mut()
                .set_query(Some("&prefix=a+b&&plus=%2B&lower=%2f&raw=:@&"));
        })
        .await
        .unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].info().number(), 1);
    verify(
        &requests[0],
        "/prefix/a/b",
        "lower=%2F&plus=%2B&prefix=a%20b&raw=%3A%40",
        "ACCESS",
        "secret-key",
        Some("session-secret"),
        "host;x-amz-content-sha256;x-amz-date;x-amz-security-token",
    );
}

async fn refuses_framing(headers: &[(&'static str, &'static str)]) {
    let headers = headers.to_vec();
    let script = Script::new();
    script.answer(200);
    let result = Api::new(base(), signer())
        .with_transport(script.clone())
        .put(Body::new("hello", "text/plain"))
        .edit_request(move |request| {
            for (name, value) in &headers {
                request
                    .headers_mut()
                    .append(*name, HeaderValue::from_static(value));
            }
        })
        .await;
    assert!(
        result.is_err(),
        "framing that changes the signed request must be refused"
    );
    let error = result.unwrap_err();
    assert!(matches!(
        SignError::find(&error),
        Some(SignError::InvalidRequest)
    ));
    assert_eq!(script.requests().len(), 0);
    assert_eq!(error.tries(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_10_content_length_and_transfer_encoding_are_refused_together() {
    refuses_framing(&[("content-length", "5"), ("transfer-encoding", "chunked")]).await;
}
#[tokio::test(start_paused = true)]
async fn c12_10_content_length_must_equal_the_buffered_payload_length() {
    refuses_framing(&[("content-length", "3")]).await;
}
#[tokio::test(start_paused = true)]
async fn c12_10_connection_cannot_name_the_authorization_header() {
    refuses_framing(&[("connection", "Authorization")]).await;
}

#[tokio::test(start_paused = true)]
async fn c12_10_unsigned_payload_is_refused_for_other_services_without_polling_or_sending() {
    let script = Script::new();
    let polls = Arc::new(AtomicUsize::new(0));
    let signer = SigV4::new(
        &base(),
        credentials(),
        "us-east-1",
        "iam",
        CanonicalPath::NormalizeAndEncode,
    )
    .allow_unsigned_payload();
    let error = Api::new(base(), signer)
        .with_transport(script.clone())
        .put(stream(polls.clone()))
        .await
        .unwrap_err();
    assert!(matches!(
        SignError::find(&error),
        Some(SignError::StreamingBody)
    ));
    assert!(script.requests().is_empty());
    assert_eq!(polls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_9_a_host_matching_the_url_is_accepted_and_signed() {
    let script = Script::new();
    script.answer(200).body("ok");
    Api::new(base(), signer())
        .with_transport(script.clone())
        .get()
        .edit_request(|request| {
            request
                .headers_mut()
                .insert("host", HeaderValue::from_static("s3.test"));
        })
        .await
        .unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].header("host"), Some("s3.test"));
    verify(
        &requests[0],
        "/prefix/a/b",
        "a=%2B&flag=",
        "ACCESS",
        "secret-key",
        Some("session-secret"),
        "host;x-amz-content-sha256;x-amz-date;x-amz-security-token",
    );
}

#[tokio::test(start_paused = true)]
async fn c12_9_signer_clones_share_credentials_and_sample_each_signing_time() {
    let credentials = credentials();
    let signer = SigV4::new(
        &base(),
        credentials.clone(),
        "us-east-1",
        "s3",
        CanonicalPath::Preserve,
    );
    let script = Script::new();
    script.answer(200).body("first");
    script.answer(200).body("second");
    let first = Api::new(base(), signer.clone()).with_transport(script.clone());
    let second = Api::new(base(), signer).with_transport(script.clone());
    first.get().await.unwrap();
    tokio::time::advance(Duration::from_secs(2)).await;
    credentials.replace(Credentials::new("NEXT", "next-key"));
    second.get().await.unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    verify(
        &requests[0],
        "/prefix/a/b",
        "a=%2B&flag=",
        "ACCESS",
        "secret-key",
        Some("session-secret"),
        "host;x-amz-content-sha256;x-amz-date;x-amz-security-token",
    );
    verify(
        &requests[1],
        "/prefix/a/b",
        "a=%2B&flag=",
        "NEXT",
        "next-key",
        None,
        "host;x-amz-content-sha256;x-amz-date",
    );
    assert_ne!(
        requests[0].header("x-amz-date"),
        requests[1].header("x-amz-date")
    );
    assert_eq!(
        requests
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        [1, 1]
    );
}

async fn refuses_target(
    path: &'static str,
    query: Option<&'static str>,
    convention: CanonicalPath,
) {
    let script = Script::new();
    script.answer(200).body("unexpected send");
    let signer = SigV4::new(&base(), credentials(), "us-east-1", "s3", convention);
    let result = Api::new(base(), signer)
        .with_transport(script.clone())
        .get()
        .edit_request(move |request| {
            request.url_mut().set_path(path);
            request.url_mut().set_query(query);
        })
        .await;
    assert!(result.is_err(), "malformed signing input must be refused");
    let error = result.unwrap_err();
    assert!(matches!(
        SignError::find(&error),
        Some(SignError::InvalidRequest)
    ));
    assert_eq!(error.tries(), 1);
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_10_queries_refuse_non_utf8_escapes_before_sending() {
    refuses_target("/", Some("q=%FF"), CanonicalPath::Preserve).await;
}

#[tokio::test(start_paused = true)]
async fn c12_10_queries_refuse_incomplete_percent_escapes_before_sending() {
    refuses_target("/", Some("q=%"), CanonicalPath::Preserve).await;
}

#[tokio::test(start_paused = true)]
async fn c12_10_preserved_paths_refuse_incomplete_percent_escapes_before_sending() {
    refuses_target("/a%", None, CanonicalPath::Preserve).await;
}

#[tokio::test(start_paused = true)]
async fn c12_10_normalized_paths_refuse_incomplete_percent_escapes_before_sending() {
    refuses_target("/a%", None, CanonicalPath::NormalizeAndEncode).await;
}

#[tokio::test(start_paused = true)]
async fn c12_9_preserved_paths_sign_non_utf8_escapes_as_written() {
    let script = Script::new();
    script.answer(200).body("ok");
    Api::new(base(), signer())
        .with_transport(script.clone())
        .get()
        .edit_request(|request| {
            request.url_mut().set_path("/%00/%FF");
            request.url_mut().set_query(None);
        })
        .await
        .unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].info().number(), 1);
    verify(
        &requests[0],
        "/%00/%FF",
        "",
        "ACCESS",
        "secret-key",
        Some("session-secret"),
        "host;x-amz-content-sha256;x-amz-date;x-amz-security-token",
    );
}

#[tokio::test(start_paused = true)]
async fn c12_9_normalized_paths_sign_non_utf8_escapes_encoded_again() {
    let script = Script::new();
    script.answer(200).body("ok");
    let signer = SigV4::new(
        &base(),
        Credentials::new("ACCESS", "secret-key"),
        "us-east-1",
        "es",
        CanonicalPath::NormalizeAndEncode,
    );
    Api::new(base(), signer)
        .with_transport(script.clone())
        .get()
        .edit_request(|request| {
            request.url_mut().set_path("/%00/%FF");
            request.url_mut().set_query(None);
        })
        .await
        .unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].info().number(), 1);
    // The encoded bytes stay in S3's path and the
    // other convention encodes each '%' again, without interpreting the bytes as UTF-8.
    let timestamp = requests[0].header("x-amz-date").unwrap();
    let date = &timestamp[..8];
    let names = "host;x-amz-date";
    let canonical = format!("GET\n/%2500/%25FF\n\nhost:s3.test\nx-amz-date:{timestamp}\n\n{names}\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    let scope = format!("{date}/us-east-1/es/aws4_request");
    let text = format!(
        "AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical))
    );
    let key = mac(b"AWS4secret-key", date);
    let key = mac(&key, "us-east-1");
    let key = mac(&key, "es");
    let key = mac(&key, "aws4_request");
    let expected = format!(
        "AWS4-HMAC-SHA256 Credential=ACCESS/{scope}, SignedHeaders={names}, Signature={}",
        hex::encode(mac(&key, &text))
    );
    assert!(requests[0].header("authorization") == Some(expected.as_str()));
}
