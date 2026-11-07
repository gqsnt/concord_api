//! Secrets: Credentials secret_access_key and session_token (src/sigv4.rs:59,89), and the
//! sensitive continuation token (src/api.rs:58, types.rs:207). Each has a distinct canary.
//! The session travels in x-amz-security-token; the cursor travels in the query and is echoed
//! through the XML decoder. Only the signing key is exempt from the wire check: every success
//! request must carry AWS4-HMAC-SHA256 Authorization with Signature= and must omit the raw key.
//! List (XML) and put cover seven scenarios, plus streamed get bodies and pagination reports.
//! PutObject has no response-body schema or media check: malformed/wrong-media bodies are
//! deliberately accepted, while truncated bodies still fail. Invalid signing/credential inputs are
//! checked separately. Excludes native reqwest, explicit raw accessors, clear object/metadata
//! fields and typed Display wire values. The access-key ID is documented as printed by
//! Credentials Debug (src/sigv4.rs:36-37): a fixed public ID is used, not a secret canary.
//! The private generated S3Api/config and Signer have no public construction/access path.

mod common; // only its independent SigV4 verifier is used here

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{net::TcpListener, time::Duration};

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_amazon_s3::{
    BucketName, Bytes, Clock, ContentType, ContinuationToken, Credentials, Metadata, ObjectKey,
    Region, S3Client, S3Error,
};
use std::time::UNIX_EPOCH;

fn error(c: &mut Canaries, error: &S3Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error metadata", &error.meta());
    match error {
        S3Error::Build(e) | S3Error::Transport(e) | S3Error::Response { error: e, .. } => {
            concord_error(c, e)
        }
        S3Error::Api(e) => c.assert_debug_absent("API error", e),
        S3Error::Signing(e) => {
            c.assert_debug_absent("signing error", e);
            c.assert_error_absent("signing error", e);
        }
        S3Error::Body(e) => {
            c.assert_debug_absent("body error", e);
            c.assert_debug_absent("body error kind", &e.kind());
            if let Some(e) = e.transport() {
                concord_error(c, e);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("amazon-s3");
    let signing_secret = c.issue();
    let session = c.issue();
    let cursor_text = c.issue();
    let cursor = ContinuationToken::new(&cursor_text).unwrap();
    let credentials = Credentials::new("SYNTHETIC-PUBLIC-ACCESS-ID", &signing_secret)
        .unwrap()
        .with_session_token(&session)
        .unwrap();
    c.assert_debug_absent("credentials", &credentials);
    c.assert_debug_absent("continuation token", &cursor);
    for case in 0..7 {
        let (echo, response_cursor) = (session.clone(), cursor_text.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let listing = r.path() == "/";
            let success = if r.method == "PUT" { String::new() }
                else if listing {
                    format!("<ListBucketResult><Name>canary-bucket</Name><IsTruncated>false</IsTruncated><ContinuationToken>{response_cursor}</ContinuationToken></ListBucketResult>")
                } else { "object bytes".into() };
            let problem = format!("<Error><Code>AccessDenied</Code><Message>{echo}</Message><Resource>{echo}</Resource><RequestId>synthetic-id</RequestId></Error>");
            let response = match case {
                0 | 6 => Reply::text(200, if listing {"application/xml"} else {"application/octet-stream"}, success),
                1 => Reply::text(403, "application/xml", problem),
                2 => Reply::text(500, "application/xml", problem),
                3 => Reply::text(200, "application/xml", format!("<invalid>{echo}")),
                4 => Reply::text(200, "text/html", &echo),
                5 => Reply::text(200, "application/xml", "").with_body(Body::FailAfter(vec![format!("<incomplete>{echo}").into()])),
                _ => unreachable!(),
            };
            response.header("x-private", &echo).header("x-amz-request-id", "synthetic-id")
        }).await;
        let addr = if case == 6 {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            drop(listener);
            addr
        } else {
            server.addr()
        };
        let http = concord_sdk_amazon_s3::http_client_builder()
            .no_proxy()
            .retry(concord::reqwest::retry::never())
            .resolve("canary-bucket.s3.canary.invalid", addr)
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let api = S3Client::new(http, Region::new("eu-west-3").unwrap(), credentials.clone())
            .unwrap()
            .with_endpoint(&format!("http://s3.canary.invalid:{}", addr.port()))
            .unwrap()
            .with_clock(Clock::fixed(
                UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            ));
        let bucket = api.bucket(BucketName::new("canary-bucket").unwrap());
        let read = bucket
            .list_objects_v2()
            .set_continuation_token(cursor.clone());
        let write = bucket.put_object(
            ObjectKey::new("one").unwrap(),
            Bytes::from_static(b"payload"),
            ContentType::new("text/plain").unwrap(),
        );
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("bucket scope", &bucket);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("unsigned read Prepared", &read.clone().prepare().unwrap());
        c.assert_debug_absent("unsigned write Prepared", &write.clone().prepare().unwrap());
        let signed = api.prepare(read.clone()).unwrap();
        c.assert_debug_absent("signed request", &signed);
        let native = read.clone().prepare().unwrap();
        // Sign through the public API, retaining the native Concord decoder/metadata path.
        let (http, mut request, decoder) = native.into_parts();
        api.sign(&mut request).unwrap();
        let received = decoder.execute(&http, request).await;
        c.assert_debug_absent("native Received Result", &received);
        match received {
            Ok(received) => {
                c.assert_debug_absent("native response metadata", &received.meta());
                let result = received.decode().await;
                c.assert_debug_absent("native decode Result", &result);
                if let Err(e) = result {
                    concord_error(&mut c, &e);
                }
            }
            Err(e) => concord_error(&mut c, &e),
        }
        let result = signed.send().await;
        c.assert_debug_absent("signed read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(page) => {
                c.assert_debug_absent("XML page", &page);
                c.assert_debug_absent("response metadata", &page.meta);
                c.assert_debug_absent("decoded continuation", &page.continuation_token);
                assert_eq!(page.continuation_token.unwrap().as_str(), cursor_text);
            }
            Err(e) => {
                error(&mut c, &e);
                if case == 1 {
                    assert!(e.to_string().contains("403"));
                    assert_eq!(e.code(), Some("AccessDenied"));
                    assert!(e.to_string().contains("S3Api.bucket.list_objects_v2"));
                }
            }
        }
        let result = write.clone().send().await;
        c.assert_debug_absent("write Result", &result);
        assert_eq!(result.is_ok(), matches!(case, 0 | 3 | 4));
        if let Err(e) = result {
            error(&mut c, &e);
        }
        let mut pages = read.pages();
        c.assert_debug_absent("listing traversal", &pages);
        let page = pages.next_page().await;
        c.assert_debug_absent("listing page Result", &page);
        if let Err(e) = page {
            c.assert_display_absent("PagesError", &e);
            c.assert_error_absent("PagesError and sources", &e);
            error(&mut c, e.provider_error().unwrap());
        }
        if matches!(case, 0 | 5) {
            let get = bucket.get_object(ObjectKey::new("one").unwrap());
            c.assert_debug_absent("streaming get endpoint", &get);
            let mut download = get.send().await.unwrap();
            c.assert_debug_absent("download", &download);
            c.assert_debug_absent("object body", &download.body);
            let mut failed = false;
            for _ in 0..3 {
                match download.body.chunk().await {
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(e) => {
                        error(&mut c, &e);
                        failed = true;
                        break;
                    }
                }
            }
            assert_eq!(failed, case == 5);
            c.assert_debug_absent("body completion", &download.body.completion());
            c.assert_debug_absent("consumed object body", &download.body);
        }
        if case == 0 {
            let metadata = Metadata::new().insert("author", "canary test").unwrap();
            let with_metadata = write.clone().metadata(metadata);
            c.assert_debug_absent("write with metadata", &with_metadata);
            c.assert_debug_absent(
                "metadata signed request",
                &with_metadata.clone().prepare(&api).unwrap(),
            );
            c.assert_debug_absent(
                "metadata write Result",
                &with_metadata.send(&api).await.unwrap(),
            );
            let requests = server.requests();
            c.assert_on_wire_except(&requests, &[&signing_secret]);
            for r in &requests {
                let authorization = r.header("authorization").unwrap();
                assert!(authorization.starts_with("AWS4-HMAC-SHA256 "));
                assert_eq!(r.header("x-amz-security-token"), Some(session.as_str()));
                assert!(!concord_check_canary::wire_text(r).contains(&signing_secret));
                // The signature verifies with this test's canary secret (independent verifier in
                // tests/common), so the canary key really signed the request ...
                let secret = signing_secret.clone();
                common::verify(&common::WireRequest::from_recorded(r), move |access| {
                    (access == "SYNTHETIC-PUBLIC-ACCESS-ID").then(|| secret.clone())
                })
                .expect("the recorded SigV4 signature verifies with the canary secret");
                // ... and not with any other secret.
                assert!(
                    common::verify(&common::WireRequest::from_recorded(r), |_| {
                        Some("ordinary-other-secret".to_owned())
                    })
                    .is_err()
                );
            }
        }
        let signing_error = api
            .prepare(write.set_content_language(format!("{session}é")))
            .unwrap_err();
        assert!(matches!(&signing_error, S3Error::Signing(_)));
        error(&mut c, &signing_error);
        let e = Credentials::new("public-id", format!("{signing_secret}\n")).unwrap_err();
        c.assert_debug_absent("invalid credential error", &e);
        c.assert_error_absent("invalid credential error", &e);
        server.shutdown().await;
    }
    assert!(c.checked() >= 200);
    println!(
        "amazon-s3: {} renderings; positive control passed (signing key exempt)",
        c.checked()
    );
}
