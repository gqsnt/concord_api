//! Additive caller tests: the SDK's real signer and decoder, raw subscriber capture.
//! Signing keys never travel; the independent existing SigV4 verifier is their positive control.

#[path = "../../tests/common/mod.rs"]
mod common;
#[path = "../../../github-app/tests/log_capture/mod.rs"]
mod log_capture;

use std::time::{Duration, UNIX_EPOCH};

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_amazon_s3::{
    BucketName, Clock, ContinuationToken, Credentials, ObjectKey, Region, S3Client,
    http_client_builder,
};
use log_capture::Capture;

const BUCKET: &str = "canaryj1s3bucketz"; // Valid lower-case DNS label, deliberately private.

fn client(addr: std::net::SocketAddr, credentials: Credentials) -> S3Client {
    let http = http_client_builder()
        .no_proxy()
        .retry(concord::reqwest::retry::never())
        .resolve(&format!("{BUCKET}.s3.canary.invalid"), addr)
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    S3Client::new(http, Region::new("eu-west-3").unwrap(), credentials)
        .unwrap()
        .with_endpoint(&format!("http://s3.canary.invalid:{}", addr.port()))
        .unwrap()
        .with_clock(Clock::fixed(
            UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        ))
}

#[tokio::test(flavor = "current_thread")]
async fn seven_signed_listing_cases_emit_one_safe_event() {
    let mut canaries = Canaries::new("j1-s3");
    let signing_secret = canaries.issue();
    let session = canaries.issue();
    let cursor = canaries.issue();
    let credentials = Credentials::new("SYNTHETIC-PUBLIC-ID", &signing_secret)
        .unwrap()
        .with_session_token(&session)
        .unwrap();
    for case in 0..7 {
        let echo = session.clone();
        let server = TestServer::start(move |_: &Recorded| {
            let document = format!("<Error><Code>AccessDenied</Code><Message>{echo}</Message><Resource>{echo}</Resource></Error>");
            let reply = match case {
                0 | 6 => Reply::text(200, "application/xml",
                    "<ListBucketResult><Name>fixture</Name><IsTruncated>false</IsTruncated></ListBucketResult>"),
                1 => Reply::text(403, "application/xml", &document),
                2 => Reply::text(500, "application/xml", &document),
                3 => Reply::text(200, "application/xml", format!("<invalid>{echo}")),
                4 => Reply::text(200, "text/html", &echo),
                5 => Reply::text(200, "application/xml", "").with_body(Body::FailAfter(vec![
                    format!("<incomplete>{echo}").into(),
                ])),
                _ => unreachable!(),
            };
            reply.header("x-private", &echo).header("x-amz-request-id", &echo)
        }).await;
        let addr = if case == 6 {
            concord_example_testserver::refused_addr()
        } else {
            server.addr()
        };
        let s3 = client(addr, credentials.clone());
        let bucket = s3.bucket(BucketName::new(BUCKET).unwrap());
        let endpoint = bucket
            .list_objects_v2()
            .set_continuation_token(ContinuationToken::new(&cursor).unwrap());
        let capture = Capture::default();
        let dispatch = capture.dispatch();
        let _subscriber = tracing::dispatcher::set_default(&dispatch);
        let signed = s3.prepare(endpoint).unwrap();
        canaries.assert_debug_absent("signed request native summary", &signed);
        assert!(!format!("{signed:?}").contains(BUCKET));
        capture.assert_empty();
        let result = signed.send().await;
        assert_eq!(result.is_ok(), case == 0);
        // Do not format SDK response metadata: J1 makes no new promise about SDK-owned values.
        // The original SDK canary test continues to check its original, narrower contract.
        capture.assert_call(
            "S3Api.bucket.list_objects_v2",
            "GET",
            "",
            if case == 6 {
                None
            } else {
                Some(match case {
                    1 => 403,
                    2 => 500,
                    _ => 200,
                })
            },
        );
        canaries.assert_absent("all raw Concord events and spans", &capture.raw());
        capture.assert_absent(&[BUCKET]);
        if case == 0 {
            let requests = server.requests();
            assert_eq!(requests.len(), 1);
            canaries.assert_on_wire_except(&requests, &[&signing_secret]);
            assert!(requests[0].host.contains(BUCKET));
            let wire = common::WireRequest::from_recorded(&requests[0]);
            let secret = signing_secret.clone();
            common::verify(&wire, move |access| {
                (access == "SYNTHETIC-PUBLIC-ID").then(|| secret.clone())
            })
            .unwrap();
            assert!(common::verify(&wire, |_| Some("wrong-signing-secret".into())).is_err());
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn native_edits_cannot_reintroduce_host_path_query_method_or_header_secrets() {
    let mut canaries = Canaries::new("j1-s3-native");
    let signing_secret = canaries.issue();
    let session = canaries.issue();
    let key = canaries.issue();
    let query_name = canaries.issue();
    let query_value = canaries.issue();
    let method = canaries.issue();
    let header = canaries.issue();
    let echo = session.clone();
    let server = TestServer::start(move |_| {
        Reply::text(200, "application/octet-stream", "object bytes").header("x-private", &echo)
    })
    .await;
    let credentials = Credentials::new("SYNTHETIC-PUBLIC-ID", &signing_secret)
        .unwrap()
        .with_session_token(&session)
        .unwrap();
    let s3 = client(server.addr(), credentials);
    let bucket = s3.bucket(BucketName::new(BUCKET).unwrap());
    let signed = s3
        .prepare(bucket.get_object(ObjectKey::new(format!("private/{key}")).unwrap()))
        .unwrap();
    canaries.assert_debug_absent("signed object request", &signed);
    assert_eq!(
        format!("{signed:?}"),
        "S3Api.bucket.get_object: GET <withheld>"
    );
    let (http, mut request, decoder) = signed.into_parts();
    request
        .url_mut()
        .query_pairs_mut()
        .append_pair(&query_name, &query_value)
        .append_pair(&query_name, &session); // Duplicate keys, unknown to the declaration.
    *request.method_mut() = concord::reqwest::Method::from_bytes(method.as_bytes()).unwrap();
    // Both the native header NAME and its value must be absent from the summary and event.
    request
        .headers_mut()
        .insert("x-canaryj1s3nativenamez", header.parse().unwrap());
    s3.sign(&mut request).unwrap(); // Native edits require a fresh signature, as before.
    let summary = decoder.summary();
    assert_eq!(
        summary.to_string(),
        "S3Api.bucket.get_object: GET <withheld>"
    );
    canaries.assert_display_absent("decoder native summary", &summary);
    canaries.assert_debug_absent("decoder native summary", &summary);
    let capture = Capture::default();
    let dispatch = capture.dispatch();
    let _subscriber = tracing::dispatcher::set_default(&dispatch);
    let received = decoder.execute(&http, request).await.unwrap();
    canaries.assert_display_absent("received native summary", &received.summary());
    let output = received.decode().await.unwrap();
    output.bytes(1024).await.unwrap();
    capture.assert_call("S3Api.bucket.get_object", "GET", "{key}", Some(200));
    canaries.assert_absent("all raw Concord events and spans", &capture.raw());
    capture.assert_absent(&[BUCKET, "x-canaryj1s3nativenamez"]);
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    canaries.assert_on_wire_except(&requests, &[&signing_secret]);
    assert!(requests[0].host.contains(BUCKET));
    assert!(requests[0].header("x-canaryj1s3nativenamez").is_some());
    server.shutdown().await;
}
