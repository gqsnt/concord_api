//! Acceptance specifications for A1's native decoder, in addition to existing SDK tests.
mod common;

use concord::{Document, ErrorKind};
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_amazon_s3::{BucketName, MAX_ERROR_BODY, ObjectKey};

#[tokio::test]
async fn native_error_decoder_keeps_the_five_states_and_authoritative_status() {
    let document = common::fixture("error-no-such-key.xml");
    let raw = document.clone();
    let server = TestServer::start(move |request| {
        let reply = Reply::new(503).header("x-amz-request-id", "native-failure");
        match request.path() {
            "/parsed" => reply.body(document.clone()), // no media label: explicitly ignored
            "/empty" => reply,
            "/bad" => reply.body(b"<Error>broken"),
            "/long" => reply.body(vec![b'x'; MAX_ERROR_BODY + 1]),
            _ => reply.with_body(Body::FailAfter(vec!["<Error>prefix".into()])),
        }
    })
    .await;
    let s3 = common::client_for(server.addr(), "us-east-1", &["mybucket"]);
    let bucket = s3.bucket(BucketName::new("mybucket").unwrap());
    for (path, expected) in [
        ("parsed", "Read"),
        ("empty", "Empty"),
        ("bad", "Unrecognized"),
        ("long", "TooLong"),
        ("broken", "Broken"),
    ] {
        let signed = s3
            .prepare(bucket.get_object(ObjectKey::new(path).unwrap()))
            .unwrap();
        let (http, request, decoder) = signed.into_parts();
        let error = decoder
            .execute(&http, request)
            .await
            .unwrap()
            .decode()
            .await
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Status);
        assert_eq!(error.status().unwrap().as_u16(), 503);
        assert_eq!(error.delivery(), concord::Delivery::Answered);
        assert_eq!(error.head().unwrap().status().as_u16(), 503);
        let failure = error.provider().unwrap();
        assert_eq!(
            failure.operation().unwrap().name(),
            "S3Api.bucket.get_object"
        );
        assert_eq!(
            failure.meta().headers()["x-amz-request-id"],
            "native-failure"
        );
        assert!(failure.body().len() <= MAX_ERROR_BODY);
        let state = match failure.state() {
            Document::Read(_) => "Read",
            Document::Empty => "Empty",
            Document::Unrecognized(_) => "Unrecognized",
            Document::TooLong { limit } => {
                assert_eq!(*limit, MAX_ERROR_BODY);
                "TooLong"
            }
            Document::Broken(_) => "Broken",
            _ => panic!("unexpected document state"),
        };
        assert_eq!(state, expected);
        assert_eq!(failure.document().is_some(), expected == "Read");
        if path == "parsed" {
            assert_eq!(failure.body().as_ref(), raw.as_slice());
        }
        if path == "bad" {
            assert_eq!(failure.body().as_ref(), b"<Error>broken");
        }
        assert!(error.into_response().is_err());
    }
    assert_eq!(server.requests().len(), 5);
    server.shutdown().await;
}

#[tokio::test]
async fn native_head_failure_is_empty_even_with_a_large_representation_length() {
    let server = TestServer::start(|_| Reply::new(404).header("content-length", "999999")).await;
    let s3 = common::client_for(server.addr(), "us-east-1", &["mybucket"]);
    let bucket = s3.bucket(BucketName::new("mybucket").unwrap());
    let signed = s3
        .prepare(bucket.head_object(ObjectKey::new("absent").unwrap()))
        .unwrap();
    let (http, request, decoder) = signed.into_parts();
    let error = decoder
        .execute(&http, request)
        .await
        .unwrap()
        .decode()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status().unwrap().as_u16(), 404);
    assert!(matches!(error.provider().unwrap().state(), Document::Empty));
    assert!(error.provider().unwrap().body().is_empty());
    server.shutdown().await;
}
