//! Independent application consumer. See FIRST_USE.md and fixtures/first-use/PROVENANCE.md.
//! All responses are local, hand-authored fixtures; this is not AWS conformance evidence.

use std::fmt::Write;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use concord::reqwest::StatusCode;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_pagination::{PagesError, Resume};
use concord_sdk_amazon_s3::{
    BodyErrorKind, Bucket, BucketName, ByteRange, Clock, ContentType, ContinuationToken,
    Credentials, ETag, ErrorBody, MaxKeys, Metadata, ObjectKey, ProtocolErrorKind, Region,
    S3Client, S3Error, http_client_builder,
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

const ACCESS: &str = "SYNTHETIC-FIRST-USE-ACCESS";
const SECRET: &str = "NOT-A-REAL-SECRET-first-use-only";
const SESSION: &str = "SYNTHETIC-FIRST-USE-SESSION";
const DATE: &str = "20231114T221320Z";
const SCOPE: &str = "20231114/eu-west-3/s3/aws4_request";
const PREFIX: &str = "reports/2026/";
const KEY: &str = "reports/2026/naïve café+100%.bin";
const PATH: &str = "/reports/2026/na%C3%AFve%20caf%C3%A9%2B100%25.bin";
const ETAG: &str = "\"opaque-validator-7\"";
const FIRST_TOKEN: &str = "opaque+/= first";
const LAST_TOKEN: &str = "opaque+/= last";
const FIRST: &[u8] = include_bytes!("../fixtures/first-use/list-first.xml");
const EMPTY_NEXT: &[u8] = include_bytes!("../fixtures/first-use/list-empty-next.xml");
const LAST: &[u8] = include_bytes!("../fixtures/first-use/list-last.xml");
const PAYLOAD: &[u8] = include_bytes!("../fixtures/first-use/payload.bin");
const PRECONDITION: &[u8] = include_bytes!("../fixtures/first-use/precondition.xml");
const MISSING: &[u8] = include_bytes!("../fixtures/first-use/missing.xml");
const SLOW_DOWN: &[u8] = include_bytes!("../fixtures/first-use/slow-down.xml");

fn bucket(server: &TestServer) -> Bucket {
    // The SDK documents virtual-host addressing and reqwest's resolve hook for local fakes.
    let http = http_client_builder()
        .no_proxy()
        .resolve("first-use-bucket.s3.first-use.invalid", server.addr())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    S3Client::new(
        http,
        Region::new("eu-west-3").unwrap(),
        Credentials::new(ACCESS, SECRET)
            .unwrap()
            .with_session_token(SESSION)
            .unwrap(),
    )
    .unwrap()
    .with_endpoint(&format!(
        "http://s3.first-use.invalid:{}",
        server.addr().port()
    ))
    .unwrap()
    .with_clock(Clock::fixed(
        UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    ))
    .bucket(BucketName::new("first-use-bucket").unwrap())
}

fn key() -> ObjectKey {
    ObjectKey::new(KEY).unwrap()
}

fn listing(bucket: &Bucket) -> concord_sdk_amazon_s3::ListObjectsV2 {
    bucket
        .list_objects_v2()
        .prefix(PREFIX)
        .max_keys(MaxKeys::new(2).unwrap())
        .fetch_owner(true)
        .url_encoded_keys()
}

fn response(status: u16, id: &str) -> Reply {
    Reply::new(status)
        .header("x-amz-request-id", id)
        .header("x-amz-id-2", "synthetic-extended-id")
}

fn no_body_response(status: u16, id: &str) -> Reply {
    // Full always inserts Content-Length. HEAD/304 must omit a made-up zero length.
    response(status, id)
        .header("connection", "close")
        .with_body(Body::Stall)
}

fn xml(status: u16, id: &str, bytes: &[u8]) -> Reply {
    response(status, id)
        .header("content-type", "application/xml")
        .body(bytes)
}

fn object_reply(bytes: &[u8]) -> Reply {
    response(200, "synthetic-get-id")
        .header("content-type", "application/octet-stream")
        .header("etag", ETAG)
        .header("x-amz-meta-author", "first-use")
        .header("x-amz-version-id", "synthetic-version-1")
        .body(bytes)
}

fn assert_listing_wire(r: &Recorded) {
    assert_eq!(r.method, "GET");
    assert_eq!(r.path(), "/");
    assert_eq!(r.query_all("list-type"), ["2"]);
    assert_eq!(r.query_all("prefix"), [PREFIX]);
    assert_eq!(r.query_all("max-keys"), ["2"]);
    assert_eq!(r.query_all("fetch-owner"), ["true"]);
    assert_eq!(r.query_all("encoding-type"), ["url"]);
    assert_eq!(
        r.query_pairs().len(),
        5 + usize::from(r.target.contains("continuation-token="))
    );
    assert!(r.body.is_empty());
}

fn listing_reply(r: &Recorded) -> Reply {
    assert_listing_wire(r);
    match r
        .query_all("continuation-token")
        .first()
        .map(String::as_str)
    {
        None => xml(200, "synthetic-list-1", FIRST),
        Some(FIRST_TOKEN) => xml(200, "synthetic-list-2", EMPTY_NEXT),
        Some(LAST_TOKEN) => xml(200, "synthetic-list-3", LAST),
        other => panic!("unexpected continuation token: {other:?}"),
    }
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    let mut result = String::new();
    for b in bytes.as_ref() {
        write!(result, "{b:02x}").unwrap();
    }
    result
}

fn uri_encode(text: &str) -> String {
    let mut result = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            result.push(char::from(b));
        } else {
            write!(result, "%{b:02X}").unwrap();
        }
    }
    result
}

fn hmac(key: &[u8], value: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(value.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

// An independent wire verifier from AWS's documented single-chunk algorithm. It never
// calls the SDK signer, reads its source, or imports its existing test verifier.
fn assert_signed(r: &Recorded) {
    assert!(r.host.starts_with("first-use-bucket.s3.first-use.invalid:"));
    assert_eq!(r.header("x-amz-date"), Some(DATE));
    assert_eq!(r.header("x-amz-security-token"), Some(SESSION));
    let payload_hash = hex(Sha256::digest(&r.body));
    assert_eq!(
        r.header("x-amz-content-sha256"),
        Some(payload_hash.as_str())
    );
    let auth = r.header("authorization").expect("SigV4 authorization");
    let attributes: Vec<_> = auth
        .strip_prefix("AWS4-HMAC-SHA256 ")
        .unwrap()
        .split(',')
        .map(|field| field.trim().split_once('=').unwrap())
        .collect();
    let field = |name| attributes.iter().find(|(k, _)| *k == name).unwrap().1;
    assert_eq!(field("Credential"), format!("{ACCESS}/{SCOPE}"));
    let signed = field("SignedHeaders");
    let names: Vec<_> = signed.split(';').collect();
    assert!(names.contains(&"host"));
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    for (name, _) in &r.headers {
        if name.starts_with("x-amz-") {
            assert!(names.contains(&name.as_str()), "unsigned {name}");
        }
    }
    let canonical_headers: String = names
        .iter()
        .map(|name| {
            let value = r.header(name).expect("signed header present");
            format!(
                "{name}:{}\n",
                value.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        })
        .collect();
    let mut query: Vec<_> = r
        .query_pairs()
        .iter()
        .map(|(k, v)| (uri_encode(k), uri_encode(v)))
        .collect();
    query.sort();
    let query = query
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let canonical = format!(
        "{}\n{}\n{query}\n{canonical_headers}\n{signed}\n{payload_hash}",
        r.method,
        r.path()
    );
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{DATE}\n{SCOPE}\n{}",
        hex(Sha256::digest(canonical.as_bytes()))
    );
    let date_key = hmac(format!("AWS4{SECRET}").as_bytes(), "20231114");
    let region_key = hmac(&date_key, "eu-west-3");
    let service_key = hmac(&region_key, "s3");
    let signing_key = hmac(&service_key, "aws4_request");
    assert_eq!(field("Signature"), hex(hmac(&signing_key, &to_sign)));
}

#[tokio::test]
async fn a_inventory_head_and_partial_download() {
    let server = TestServer::start(|r| {
        assert_signed(r);
        if r.path() == "/" {
            return listing_reply(r);
        }
        assert_eq!(r.path(), PATH);
        assert!(r.query_pairs().is_empty());
        assert!(r.body.is_empty());
        match r.method.as_str() {
            "HEAD" => no_body_response(200, "synthetic-head-id")
                .header("etag", ETAG)
                .header("last-modified", "Fri, 06 Nov 2026 00:00:00 GMT")
                .header("x-amz-meta-author", "first-use")
                .header("accept-ranges", "bytes"),
            "GET" => {
                assert_eq!(r.header("if-match"), Some(ETAG));
                assert_eq!(r.header("range"), Some("bytes=2-5"));
                response(206, "synthetic-range-id")
                    .header("content-type", "application/octet-stream")
                    .header("content-range", format!("bytes 2-5/{}", PAYLOAD.len()))
                    .header("etag", ETAG)
                    .body(&PAYLOAD[2..=5])
            }
            method => panic!("unexpected method {method}"),
        }
    })
    .await;
    let bucket = bucket(&server);
    let objects = listing(&bucket)
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(objects.len(), 2); // The empty middle page did not terminate traversal.
    let chosen = objects.iter().max_by_key(|o| o.size.unwrap_or(0)).unwrap();
    assert_eq!(chosen.key, KEY);
    assert_eq!(chosen.size, Some(PAYLOAD.len() as u64));
    let key = ObjectKey::new(chosen.key.clone()).unwrap();
    let head = bucket.head_object(key.clone()).send().await.unwrap();
    assert_eq!(head.status, StatusCode::OK);
    assert_eq!(head.metadata.get("author"), Some("first-use"));
    assert!(head.last_modified.is_some());
    assert_eq!(head.accept_ranges.as_deref(), Some("bytes"));
    assert_eq!(head.meta.request_id(), Some("synthetic-head-id"));
    let part = bucket
        .get_object(key)
        .if_match(head.etag.unwrap())
        .range(ByteRange::inclusive(2, 5).unwrap())
        .send()
        .await
        .unwrap();
    assert!(part.head.is_partial());
    let range = part.head.content_range.as_ref().unwrap();
    assert_eq!(
        (range.first, range.last, range.complete_length),
        (2, 5, Some(PAYLOAD.len() as u64))
    );
    assert_eq!(part.body.received(), 0);
    let (head, bytes) = part.bytes(4).await.unwrap();
    assert_eq!(bytes.as_ref(), &PAYLOAD[2..=5]);
    assert_eq!(head.content_length, Some(4));
    assert_eq!(server.requests().len(), 5);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_failed_page_retains_items_and_can_resume() {
    let failed_once = Arc::new(AtomicBool::new(false));
    let server = TestServer::start(move |r| {
        assert_signed(r);
        assert_listing_wire(r);
        if r.query_all("continuation-token") == [FIRST_TOKEN]
            && !failed_once.swap(true, Ordering::SeqCst)
        {
            xml(503, "synthetic-page-failure", SLOW_DOWN)
        } else {
            listing_reply(r)
        }
    })
    .await;
    let bucket = bucket(&server);
    let incomplete = listing(&bucket)
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].key, "reports/2026/summary.csv");
    let token = match incomplete.error {
        PagesError::Provider {
            error,
            failed: Resume::At(token),
            ..
        } => {
            assert_eq!(error.status(), Some(StatusCode::SERVICE_UNAVAILABLE));
            assert_eq!(error.code(), Some("SlowDown"));
            assert_eq!(error.request_id(), Some("synthetic-page-failure"));
            assert_eq!(token.as_str(), FIRST_TOKEN);
            token
        }
        other => panic!("expected a provider failure, got {other:?}"),
    };
    assert_eq!(server.requests().len(), 2); // No automatic retry.
    let rest = listing(&bucket)
        .continuation_token(token)
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].key, KEY);
    assert_eq!(server.requests().len(), 4);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_page_cap_is_distinct_from_exhaustion() {
    let server = TestServer::start(|r| {
        assert_signed(r);
        listing_reply(r)
    })
    .await;
    let bucket = bucket(&server);
    let mut pages = listing(&bucket)
        .pages()
        .max_pages(NonZeroUsize::new(1).unwrap());
    assert!(pages.next_page().await.unwrap().is_some());
    let token = match pages.next_page().await.unwrap_err() {
        PagesError::Bounded {
            resume: Resume::At(token),
            ..
        } => token,
        other => panic!("expected bounded traversal, got {other:?}"),
    };
    assert_eq!(token.as_str(), FIRST_TOKEN);
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 1);
    let mut resumed = listing(&bucket).continuation_token(token).pages();
    assert!(resumed.next_page().await.unwrap().is_some());
    assert!(resumed.next_page().await.unwrap().is_some());
    assert!(resumed.next_page().await.unwrap().is_none());
    assert!(resumed.next_page().await.unwrap().is_none());
    assert_eq!(server.requests().len(), 3);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_broken_continuation_contract_is_an_error() {
    for (bytes, expected) in [
        (
            include_bytes!("../fixtures/first-use/list-missing-token.xml").as_slice(),
            ProtocolErrorKind::TruncatedWithoutToken,
        ),
        (
            include_bytes!("../fixtures/first-use/list-repeated-token.xml").as_slice(),
            ProtocolErrorKind::RepeatedToken,
        ),
    ] {
        let server = TestServer::start(move |r| {
            assert_signed(r);
            assert_listing_wire(r);
            assert_eq!(r.query_all("continuation-token"), [LAST_TOKEN]);
            xml(200, "synthetic-broken-list", bytes)
        })
        .await;
        let mut pages = listing(&bucket(&server))
            .continuation_token(ContinuationToken::new(LAST_TOKEN).unwrap())
            .pages();
        match pages.next_page().await.unwrap_err() {
            PagesError::Provider {
                error: S3Error::Protocol { kind, meta, .. },
                ..
            } => {
                assert_eq!(kind, expected);
                assert_eq!(meta.request_id(), Some("synthetic-broken-list"));
            }
            other => panic!("expected protocol error, got {other:?}"),
        }
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended { .. })
        ));
        assert_eq!(server.requests().len(), 1);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn a_malformed_listing_never_becomes_successful_empty_inventory() {
    let server = TestServer::start(|r| {
        assert_signed(r);
        assert_listing_wire(r);
        xml(
            200,
            "synthetic-bad-xml",
            b"<ListBucketResult><IsTruncated>false</IsTruncated>",
        )
    })
    .await;
    let incomplete = listing(&bucket(&server))
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(incomplete.collected.is_empty());
    match incomplete.error {
        PagesError::Provider {
            error: S3Error::Response { error, meta },
            ..
        } => {
            assert_eq!(error.kind(), concord::ErrorKind::Decode);
            assert_eq!(meta.request_id(), Some("synthetic-bad-xml"));
        }
        other => panic!("expected XML decode error, got {other:?}"),
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_cached_copy_and_stale_validator_are_distinct() {
    let server = TestServer::start(|r| {
        assert_signed(r);
        assert_eq!((r.method.as_str(), r.path()), ("GET", PATH));
        if r.header("if-none-match") == Some(ETAG) {
            no_body_response(304, "synthetic-current")
                .header("etag", ETAG)
                .header("last-modified", "Fri, 06 Nov 2026 00:00:00 GMT")
        } else {
            assert_eq!(r.header("if-match"), Some("\"stale\""));
            xml(412, "synthetic-stale", PRECONDITION)
        }
    })
    .await;
    let bucket = bucket(&server);
    let current = bucket
        .get_object(key())
        .if_none_match(ETag::parse(ETAG).unwrap())
        .send()
        .await
        .unwrap_err();
    assert!(current.is_not_modified());
    match current {
        S3Error::NotModified(current) => {
            assert_eq!(current.etag().unwrap().as_str(), ETAG);
            assert!(current.last_modified().is_some());
            assert_eq!(current.meta().request_id(), Some("synthetic-current"));
        }
        other => panic!("expected cached-copy outcome, got {other:?}"),
    }
    let stale = bucket
        .get_object(key())
        .if_match(ETag::parse("\"stale\"").unwrap())
        .send()
        .await
        .unwrap_err();
    assert!(stale.is_precondition_failed());
    assert_eq!(stale.code(), Some("PreconditionFailed"));
    assert_eq!(stale.request_id(), Some("synthetic-stale"));
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_missing_get_document_and_bodyless_head_preserve_diagnostics() {
    let server = TestServer::start(|r| {
        assert_signed(r);
        assert_eq!(r.path(), PATH);
        match r.method.as_str() {
            "GET" => xml(404, "synthetic-missing-get", MISSING),
            "HEAD" => no_body_response(404, "synthetic-missing-head"),
            other => panic!("unexpected method {other}"),
        }
    })
    .await;
    let bucket = bucket(&server);
    let error = bucket.get_object(key()).send().await.unwrap_err();
    assert!(error.is_not_found());
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains("private-report.bin"));
    assert!(!rendered.contains("synthetic key does not exist"));
    assert!(!rendered.contains(SECRET));
    match error {
        S3Error::Api(error) => {
            assert_eq!(error.body(), ErrorBody::Parsed);
            assert_eq!(error.code(), Some("NoSuchKey"));
            assert_eq!(error.message(), Some("The synthetic key does not exist."));
            assert_eq!(
                error.resource(),
                Some("/first-use-bucket/private-report.bin")
            );
            assert_eq!(error.document_request_id(), Some("synthetic-document-id"));
            assert_eq!(error.meta().request_id(), Some("synthetic-missing-get"));
            assert_eq!(error.host_id(), Some("synthetic-document-host"));
        }
        other => panic!("expected provider document, got {other:?}"),
    }
    let error = bucket.head_object(key()).send().await.unwrap_err();
    assert!(error.is_not_found());
    match error {
        S3Error::Api(error) => {
            assert_eq!(error.body(), ErrorBody::Empty);
            assert_eq!(error.code(), None);
            assert_eq!(error.meta().request_id(), Some("synthetic-missing-head"));
        }
        other => panic!("expected bodyless HEAD error, got {other:?}"),
    }
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_range_mismatch_is_rejected_and_200_fallback_is_explicit() {
    let call = Arc::new(AtomicBool::new(false));
    let server = TestServer::start(move |r| {
        assert_signed(r);
        assert_eq!(r.header("range"), Some("bytes=2-5"));
        if !call.swap(true, Ordering::SeqCst) {
            response(206, "synthetic-wrong-range")
                .header("content-range", format!("bytes 0-3/{}", PAYLOAD.len()))
                .body(&PAYLOAD[..4])
        } else {
            object_reply(PAYLOAD) // A server may ignore Range and return the whole object.
        }
    })
    .await;
    let bucket = bucket(&server);
    let read = || {
        bucket
            .get_object(key())
            .range(ByteRange::inclusive(2, 5).unwrap())
    };
    match read().send().await.unwrap_err() {
        S3Error::Protocol {
            kind: ProtocolErrorKind::RangeMismatch,
            meta,
            ..
        } => {
            assert_eq!(meta.request_id(), Some("synthetic-wrong-range"));
        }
        other => panic!("expected wrong-range failure, got {other:?}"),
    }
    let full = read().send().await.unwrap();
    assert!(!full.head.is_partial());
    assert!(full.head.content_range.is_none());
    assert_eq!(full.bytes(1024).await.unwrap().1.as_ref(), PAYLOAD);
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_body_limit_refuses_collection_before_reading() {
    let server = TestServer::start(|r| {
        assert_signed(r);
        object_reply(PAYLOAD)
    })
    .await;
    let output = bucket(&server).get_object(key()).send().await.unwrap();
    assert_eq!(output.body.received(), 0);
    match output.bytes(2).await.unwrap_err() {
        S3Error::Body(error) => {
            assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
            assert_eq!(error.received(), 0);
            assert_eq!(error.expected(), Some(PAYLOAD.len() as u64));
            assert_eq!(error.meta().request_id(), Some("synthetic-get-id"));
        }
        other => panic!("expected body limit error, got {other:?}"),
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_interrupted_body_requires_caller_to_remember_failure() {
    let server = TestServer::start(|r| {
        assert_signed(r);
        response(200, "synthetic-cutoff").with_body(Body::FailAfter(vec![b"abc".to_vec().into()]))
    })
    .await;
    let mut output = bucket(&server).get_object(key()).send().await.unwrap();
    let mut staged = Vec::new();
    let error = loop {
        match output.body.chunk().await {
            Ok(Some(chunk)) => staged.extend_from_slice(&chunk),
            Ok(None) => panic!("a broken chunked body must not complete"),
            Err(error) => break error,
        }
    };
    match error {
        S3Error::Body(error) => {
            assert_eq!(error.kind(), BodyErrorKind::Transport);
            assert_eq!(error.received(), staged.len() as u64);
            assert_eq!(error.expected(), None); // Chunked fake, no Content-Length.
            assert_eq!(error.meta().request_id(), Some("synthetic-cutoff"));
            assert!(error.transport().is_some());
        }
        other => panic!("expected body transport error, got {other:?}"),
    }
    assert!(b"abc".starts_with(&staged));
    // Documented behavior: stopped and successful bodies share these later observations.
    // The application must retain the earlier Err and discard its staging buffer.
    assert!(output.body.is_finished());
    assert!(output.body.chunk().await.unwrap().is_none());
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_chunked_success_has_verified_eof_and_counts_bytes() {
    let server = TestServer::start(|r| {
        assert_signed(r);
        response(200, "synthetic-chunks").with_body(Body::Chunks(vec![
            (Duration::ZERO, PAYLOAD[..4].to_vec().into()),
            (Duration::from_millis(5), PAYLOAD[4..].to_vec().into()),
        ]))
    })
    .await;
    let mut output = bucket(&server).get_object(key()).send().await.unwrap();
    let mut bytes = Vec::new();
    while let Some(chunk) = output.body.chunk().await.unwrap() {
        bytes.extend_from_slice(&chunk);
    }
    assert_eq!(bytes, PAYLOAD);
    assert_eq!(output.body.received(), PAYLOAD.len() as u64);
    assert!(output.body.is_finished());
    assert!(output.body.chunk().await.unwrap().is_none());
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn b_put_retrieve_and_conditional_create_conflict() {
    let stored = Arc::new(Mutex::new(None::<Recorded>));
    let server = TestServer::start(move |r| {
        assert_signed(r);
        assert_eq!(r.path(), PATH);
        assert!(r.query_pairs().is_empty());
        let mut stored = stored.lock().unwrap();
        match r.method.as_str() {
            "PUT" => {
                assert_eq!(r.header("if-none-match"), Some("*"));
                assert_eq!(r.header("content-type"), Some("application/octet-stream"));
                assert!(!r.chunked);
                if stored.is_some() {
                    return xml(412, "synthetic-create-conflict", PRECONDITION);
                }
                assert_eq!(r.body, PAYLOAD);
                assert_eq!(r.body_len, PAYLOAD.len() as u64);
                assert_eq!(
                    r.header("content-length").unwrap(),
                    PAYLOAD.len().to_string()
                );
                assert_eq!(r.header("x-amz-meta-author"), Some("first-use"));
                assert_eq!(r.header("cache-control"), Some("private, max-age=60"));
                *stored = Some(r.clone());
                response(200, "synthetic-put-id")
                    .header("etag", ETAG)
                    .header("x-amz-version-id", "synthetic-version-1")
            }
            "GET" => {
                assert!(r.body.is_empty());
                let stored = stored.as_ref().expect("PUT precedes GET");
                object_reply(&stored.body)
                    .header("cache-control", stored.header("cache-control").unwrap())
            }
            method => panic!("unexpected method {method}"),
        }
    })
    .await;
    let bucket = bucket(&server);
    let put = bucket
        .put_object(
            key(),
            PAYLOAD.to_vec(),
            ContentType::new("application/octet-stream").unwrap(),
        )
        .metadata(Metadata::new().insert("author", "first-use").unwrap())
        .cache_control("private, max-age=60")
        .if_absent()
        .send()
        .await
        .unwrap();
    assert_eq!(put.etag.as_ref().unwrap().as_str(), ETAG);
    assert_eq!(
        put.version_id.as_ref().unwrap().as_str(),
        "synthetic-version-1"
    );
    assert_eq!(put.meta.request_id(), Some("synthetic-put-id"));
    let (head, bytes) = bucket
        .get_object(key())
        .send()
        .await
        .unwrap()
        .bytes(1024)
        .await
        .unwrap();
    assert_eq!(bytes.as_ref(), PAYLOAD);
    assert_eq!(head.etag, put.etag);
    assert_eq!(head.version_id, put.version_id);
    assert_eq!(head.metadata.get("author"), Some("first-use"));
    assert_eq!(head.cache_control.as_deref(), Some("private, max-age=60"));
    let error = bucket
        .put_object(
            key(),
            b"replacement".to_vec(),
            ContentType::new("application/octet-stream").unwrap(),
        )
        .if_absent()
        .send()
        .await
        .unwrap_err();
    assert!(error.is_precondition_failed());
    assert_eq!(error.code(), Some("PreconditionFailed"));
    assert_eq!(error.request_id(), Some("synthetic-create-conflict"));
    let bytes = bucket
        .get_object(key())
        .send()
        .await
        .unwrap()
        .bytes(1024)
        .await
        .unwrap()
        .1;
    assert_eq!(bytes.as_ref(), PAYLOAD);
    assert_eq!(server.requests().len(), 4);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn b_lost_put_response_is_reconciled_by_reading_without_retry() {
    let stored = Arc::new(Mutex::new(None::<Vec<u8>>));
    let server = TestServer::start(move |r| {
        assert_signed(r);
        assert_eq!(r.path(), PATH);
        let mut stored = stored.lock().unwrap();
        match r.method.as_str() {
            "PUT" => {
                assert!(
                    stored.is_none(),
                    "the SDK must not retry an ambiguous write"
                );
                assert_eq!(r.body, PAYLOAD);
                *stored = Some(r.body.clone()); // Applied before the response was lost.
                Reply::hang_up()
            }
            "GET" => object_reply(stored.as_ref().expect("write was applied")),
            other => panic!("unexpected method {other}"),
        }
    })
    .await;
    let bucket = bucket(&server);
    let error = bucket
        .put_object(
            key(),
            PAYLOAD.to_vec(),
            ContentType::new("application/octet-stream").unwrap(),
        )
        .if_absent()
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, S3Error::Transport(_)));
    assert!(!error.is_before_send());
    assert_eq!(error.status(), None);
    assert_eq!(server.requests().len(), 1);
    let bytes = bucket
        .get_object(key())
        .send()
        .await
        .unwrap()
        .bytes(1024)
        .await
        .unwrap()
        .1;
    assert_eq!(bytes.as_ref(), PAYLOAD);
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}
