//! Building, sending and repeating a call with an optional whole body.
mod common;

use concord::body::{Entries, IntoBody};
use concord::error::Kind;
use concord::http::header::CONTENT_TYPE;
use concord::sign::{body_bytes, CanonicalPath, Credentials, SigV4};
use concord::transport::{TransportError, TryInfo};
use concord::{reqwest, Body, Middleware, Next, Retry};
use concord_testkit::Script;
use sha2::{Digest, Sha256};
use std::{any::Any, borrow::Borrow as _, error::Error as _, io, sync::Arc};

concord::api! {
    client Payloads {
        base "https://payloads.test";

        PUT upload(payload?: Body) { body payload.as_ref(); }
        POST create(payload?: Body) { body payload.as_ref(); }
        PUT labelled(payload?: Body) {
            headers { "content-type" = "application/example"; }
            body payload.as_ref();
        }
        PUT xml() { body empty_xml { "version" = 1; } }
        PUT optional_json(value?: Option<u32>) {
            body value.as_ref().map(concord::json).transpose();
        }
        PUT broken() { body broken_writer(); }
        PUT broken_header() {
            headers { "x-probe" = "bad\nvalue"; }
            body broken_writer();
        }
        PUT opaque() { body opaque_writer(); }
        PUT optional_opaque(present: bool) { body present.then(opaque_writer); }
        PUT borrowed(blob: Box<Body>) { body blob.borrow(); }
        PUT downcast(cache: Arc<dyn Any + Send + Sync>) {
            body cache.downcast_ref().unwrap();
        }
        POST page(payload?: Body) -> Vec<u32> {
            body payload.as_ref();
            read concord::Json;
            paginate |call: payloads::Call<payloads::Page>, _| Some(call.payload(Body::from("")));
        }
    }
}

fn empty_xml(_: Entries) -> Option<Body> {
    Some(Body::new("", "application/xml"))
}

fn broken_writer() -> io::Result<Option<Body>> {
    Err(io::Error::other("writer refused"))
}

fn opaque_writer() -> impl IntoBody {
    Body::new("opaque", "text/plain")
}

fn payloads(script: &Script) -> Payloads {
    Payloads::new().with_transport(script.clone())
}

#[tokio::test(start_paused = true)]
async fn omitted_body_is_absent_in_inspection_and_transport() {
    let script = Script::new();
    script.answer(204);
    let call = payloads(&script).upload();
    let request = call.request().unwrap();
    assert!(request.body().is_none());
    assert!(!request.headers().contains_key(CONTENT_TYPE));
    call.await.unwrap();
    let sent = script.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].body(), None);
    assert_eq!(sent[0].header("content-type"), None);
}

#[tokio::test(start_paused = true)]
async fn explicit_content_type_is_sent_without_a_body() {
    let script = Script::new();
    script.answer(204);
    let call = payloads(&script).labelled();
    let request = call.request().unwrap();
    assert!(request.body().is_none());
    assert_eq!(request.headers()[CONTENT_TYPE], "application/example");
    call.await.unwrap();
    let sent = script.requests();
    assert_eq!(sent[0].body(), None);
    assert_eq!(sent[0].header("content-type"), Some("application/example"));
}

#[tokio::test(start_paused = true)]
async fn empty_xml_is_a_present_body() {
    let script = Script::new();
    script.answer(204);
    let call = payloads(&script).xml();
    let request = call.request().unwrap();
    assert_eq!(request.body().unwrap().as_bytes(), Some(&b""[..]));
    assert_eq!(request.headers()[CONTENT_TYPE], "application/xml");
    call.await.unwrap();
    let sent = script.requests();
    assert_eq!(sent[0].body(), Some(&b""[..]));
    assert_eq!(sent[0].header("content-type"), Some("application/xml"));
}

#[tokio::test(start_paused = true)]
async fn optional_nullable_json_sends_absence_null_and_value() {
    let script = Script::new();
    for _ in 0..3 {
        script.answer(204);
    }
    let client = payloads(&script);
    client.optional_json().await.unwrap();
    client.optional_json().value(None::<u32>).await.unwrap();
    client.optional_json().value(Some(7)).await.unwrap();
    let sent = script.requests();
    assert_eq!(sent.len(), 3);
    assert_eq!(sent[0].body(), None);
    assert_eq!(sent[0].header("content-type"), None);
    assert_eq!(sent[1].body(), Some(&b"null"[..]));
    assert_eq!(sent[1].header("content-type"), Some("application/json"));
    assert_eq!(sent[2].body(), Some(&b"7"[..]));
    assert_eq!(sent[2].header("content-type"), Some("application/json"));
}

#[tokio::test(start_paused = true)]
async fn optional_writer_error_keeps_its_source_and_sends_nothing() {
    let script = Script::new();
    let error = payloads(&script).broken().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(error.tries(), 0);
    assert_eq!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::Error>()
            .unwrap()
            .to_string(),
        "writer refused"
    );
    assert!(script.requests().is_empty());
}

#[test]
fn invalid_header_takes_precedence_over_optional_writer_error() {
    let error = Payloads::new().broken_header().request().unwrap_err();
    assert_eq!(error.kind(), Kind::Build);
    assert_eq!(
        error.to_string(),
        "build error: the value of header x-probe is invalid, in payloads.broken_header"
    );
    assert!(error.source().is_none());
}

#[test]
fn opaque_writer_works_with_and_without_an_option() {
    let client = Payloads::new();
    for request in [
        client.opaque().request(),
        client.optional_opaque(true).request(),
    ] {
        let request = request.unwrap();
        assert_eq!(request.body().unwrap().as_bytes(), Some(&b"opaque"[..]));
        assert_eq!(request.headers()[CONTENT_TYPE], "text/plain");
    }
    let request = client.optional_opaque(false).request().unwrap();
    assert!(request.body().is_none());
    assert!(!request.headers().contains_key(CONTENT_TYPE));
}

#[tokio::test(start_paused = true)]
async fn boxed_body_borrow_infers_body_and_sends_payload() {
    let script = Script::new();
    script.answer(204);
    payloads(&script)
        .borrowed(Box::new(Body::new("payload", "text/plain")))
        .await
        .unwrap();
    assert_eq!(script.requests()[0].body(), Some(&b"payload"[..]));
}

#[tokio::test(start_paused = true)]
async fn downcast_ref_infers_body_and_sends_payload() {
    let script = Script::new();
    script.answer(204);
    let cache: Arc<dyn Any + Send + Sync> = Arc::new(Body::new("cached", "text/plain"));
    payloads(&script).downcast(cache).await.unwrap();
    assert_eq!(script.requests()[0].body(), Some(&b"cached"[..]));
}

struct Checksum;

impl Middleware for Checksum {
    async fn handle(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> Result<reqwest::Response, TransportError> {
        let digest = Sha256::digest(body_bytes(&request).unwrap());
        request
            .headers_mut()
            .insert("x-checksum-sha256", hex::encode(digest).parse().unwrap());
        next.send(request, info).await
    }
}

#[tokio::test(start_paused = true)]
async fn checksum_and_sigv4_hash_zero_bytes_for_an_omitted_body() {
    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let signer = SigV4::new(
        &"https://payloads.test".parse().unwrap(),
        Credentials::new("access", "secret"),
        "us-east-1",
        "s3",
        CanonicalPath::Preserve,
    );
    let script = Script::new();
    script.answer(204);
    payloads(&script)
        .with_middleware(Checksum)
        .with_middleware(signer)
        .upload()
        .await
        .unwrap();
    let sent = script.requests();
    assert_eq!(sent[0].header("x-checksum-sha256"), Some(EMPTY_SHA256));
    assert_eq!(sent[0].header("x-amz-content-sha256"), Some(EMPTY_SHA256));
    assert!(sent[0]
        .header("authorization")
        .unwrap()
        .contains("x-checksum-sha256"));
    assert_eq!(sent[0].body(), None);
    assert_eq!(sent[0].header("content-type"), None);
}

#[tokio::test(start_paused = true)]
async fn bodyless_put_retries_without_a_body_or_content_type() {
    let script = Script::new();
    script.answer(503);
    script.answer(204);
    payloads(&script)
        .with_retry(Retry::standard())
        .upload()
        .await
        .unwrap();
    let sent = script.requests();
    assert_eq!(sent.len(), 2);
    for (request, attempt) in sent.iter().zip(1..=2) {
        assert_eq!(request.info().number(), attempt);
        assert_eq!(request.body(), None);
        assert_eq!(request.header("content-type"), None);
    }
}

#[tokio::test(start_paused = true)]
async fn unmarked_bodyless_post_does_not_retry() {
    let script = Script::new();
    script.answer(503);
    script.answer(204);
    let error = payloads(&script)
        .with_retry(Retry::standard())
        .create()
        .await
        .unwrap_err();
    assert!(error.is_status(503));
    assert_eq!(error.tries(), 1);
    let sent = script.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].body(), None);
    assert_eq!(sent[0].header("content-type"), None);
}

#[tokio::test(start_paused = true)]
async fn present_optional_bodies_keep_their_replay_rules() {
    let repeatable = Body::repeatable(|| async {
        let response = reqwest::Response::from(http::Response::new(reqwest::Body::from("payload")));
        Ok::<_, reqwest::Error>(response.bytes_stream())
    });
    for (body, attempts) in [
        (Body::from("payload"), 2),
        (repeatable, 2),
        (common::stream("payload"), 1),
    ] {
        let script = Script::new();
        script.answer(503).read_upload();
        script.answer(204).read_upload();
        let result = payloads(&script)
            .with_retry(Retry::standard())
            .upload()
            .payload(body.content_type("application/example"))
            .await;
        if attempts == 1 {
            assert!(result.unwrap_err().is_status(503));
        } else {
            result.unwrap();
        }
        let sent = script.requests();
        assert_eq!(sent.len(), attempts);
        for request in sent {
            assert_eq!(request.upload().bytes(), b"payload");
            assert_eq!(request.header("content-type"), Some("application/example"));
        }
    }
}

#[tokio::test(start_paused = true)]
async fn pager_treats_absence_and_empty_bytes_with_equal_headers_as_a_repeat() {
    let script = Script::new();
    script.answer(200).body("[]");
    let mut pages = payloads(&script).page().pages();
    assert!(pages.try_next().await.unwrap().is_some());
    assert_eq!(pages.try_next().await.unwrap_err().kind(), Kind::Mismatch);
    assert!(pages.try_next().await.unwrap().is_none());
    let sent = script.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].body(), None);
    assert_eq!(sent[0].header("content-type"), None);
}
