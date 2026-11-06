//! Wire-level protocol tests: the official SigV4 examples, exact requests on the wire, the
//! documented success and failure shapes, and the negative cases of the brief (perturbed keys,
//! ranges and content headers; unknown XML; refused heads; truncated pages and bodies;
//! cancellation). Everything runs offline against the loopback test server.

mod common;

use std::time::Duration;

use common::*;
use concord::ErrorKind;
use concord::reqwest::{self, header::HeaderValue};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_pagination::{PagesError, Resume};
use concord_sdk_amazon_s3::sigv4::sign_request;
use concord_sdk_amazon_s3::{
    AnyObject, BodyCompletion, BodyErrorKind, BucketName, ByteRange, Bytes, Clock, ContentType,
    ContinuationToken, Credentials, ETag, ErrorBody, HttpDate, MaxKeys, Metadata, ObjectKey,
    ProtocolErrorKind, Region, S3Client, S3Error, SignedRequest, StorageClass, UrlEncoding,
};

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn key(text: &str) -> ObjectKey {
    ObjectKey::new(text).unwrap()
}

fn bucket_name(text: &str) -> BucketName {
    BucketName::new(text).unwrap()
}

/// A client signing like AWS's examples: `https://s3.amazonaws.com`, `us-east-1`, the
/// documentation example key, `20130524T000000Z`.
fn doc_client() -> S3Client {
    S3Client::new(
        reqwest::Client::new(),
        Region::new("us-east-1").unwrap(),
        Credentials::new(DOC_ACCESS_KEY, DOC_SECRET_KEY).unwrap(),
    )
    .unwrap()
    .with_endpoint("https://s3.amazonaws.com")
    .unwrap()
    .with_clock(Clock::fixed(doc_time()))
}

fn header_str<'a>(request: &'a reqwest::Request, name: &str) -> &'a str {
    request.headers()[name].to_str().unwrap()
}

/// A documented response head (`name: value` lines) as reply headers.
fn head_fixture(name: &str) -> Vec<(String, String)> {
    fixture_text(name)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (n, v) = l.split_once(": ").unwrap();
            (n.to_owned(), v.to_owned())
        })
        .collect()
}

fn with_head(mut reply: Reply, head: &[(String, String)]) -> Reply {
    for (n, v) in head {
        reply = reply.header(n, v.clone());
    }
    reply
}

fn wire(recorded: &Recorded) -> Result<Verified, Refusal> {
    verify(&WireRequest::from_recorded(recorded), known_secret)
}

/// The signer's `Authorization` against a documented one: equal components, and equal text once
/// aws-sigv4's `, ` separators are written as the documentation's `,` (the only difference
/// observed; aws-sigv4 is the AWS SDK for Rust's signer, so its form is what AWS's own SDK
/// sends — an inference, not something an offline test can check against S3).
fn assert_same_authorization(sent: &str, documented: &str) {
    assert_eq!(parse_authorization(sent), parse_authorization(documented));
    assert!(parse_authorization(documented).is_some());
    assert_eq!(sent.replace(", ", ","), documented);
}

// ---------------------------------------------------------------------------------------------
// Official SigV4 examples (AWS S3 developer guide, "Signature Calculations for the Authorization
// Header: Transferring Payload in a Single Chunk", section "Examples: Signature Calculations").
// ---------------------------------------------------------------------------------------------

#[test]
fn official_get_object_example_is_reproduced_by_the_sdk() {
    let s3 = doc_client();
    let request = s3
        .prepare(
            s3.bucket(bucket_name("examplebucket"))
                .get_object(key("test.txt"))
                .set_range(ByteRange::inclusive(0, 9).unwrap()),
        )
        .unwrap();
    let native = request.request();
    assert_eq!(native.method(), reqwest::Method::GET);
    assert_eq!(
        native.url().as_str(),
        "https://examplebucket.s3.amazonaws.com/test.txt"
    );

    // The SDK's (aws-sigv4's) header carries the documented credential, signed headers and
    // signature; it separates the components with `, ` where the documentation writes `,`.
    assert_same_authorization(
        header_str(native, "authorization"),
        &fixture_text("sigv4/get-object.auth"),
    );
    assert_eq!(header_str(native, "x-amz-date"), "20130524T000000Z");
    assert_eq!(header_str(native, "x-amz-content-sha256"), EMPTY_SHA256);
    assert_eq!(header_str(native, "host"), "examplebucket.s3.amazonaws.com");
    assert_eq!(header_str(native, "range"), "bytes=0-9");
    let mut names: Vec<&str> = native.headers().keys().map(|n| n.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "authorization",
            "host",
            "range",
            "x-amz-content-sha256",
            "x-amz-date"
        ]
    );

    // The independent computation from the same request reaches the documented intermediate
    // values and the documented signature.
    let verified = verify_native(native).unwrap();
    assert_eq!(
        verified.canonical_request,
        fixture_text("sigv4/get-object.creq")
    );
    assert_eq!(
        verified.string_to_sign,
        fixture_text("sigv4/get-object.sts")
    );
    assert_eq!(
        verified.authorization.signature,
        "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
    );
}

/// The three other documented examples, computed independently from the documented inputs and
/// by the SDK's public signer on a native request carrying exactly the documented headers.
#[test]
fn official_put_lifecycle_and_list_examples() {
    struct Example {
        name: &'static str,
        method: reqwest::Method,
        url: &'static str,
        headers: &'static [(&'static str, &'static str)],
        body: &'static [u8],
    }
    let payload = fixture("sigv4/put-object.payload");
    let payload: &'static [u8] = Box::leak(payload.into_boxed_slice());
    let examples = [
        Example {
            name: "put-object",
            method: reqwest::Method::PUT,
            url: "https://examplebucket.s3.amazonaws.com/test%24file.text",
            headers: &[
                ("date", "Fri, 24 May 2013 00:00:00 GMT"),
                ("x-amz-storage-class", "REDUCED_REDUNDANCY"),
            ],
            body: payload,
        },
        Example {
            name: "get-bucket-lifecycle",
            method: reqwest::Method::GET,
            url: "https://examplebucket.s3.amazonaws.com/?lifecycle",
            headers: &[],
            body: b"",
        },
        Example {
            name: "list-objects",
            method: reqwest::Method::GET,
            url: "https://examplebucket.s3.amazonaws.com/?max-keys=2&prefix=J",
            headers: &[],
            body: b"",
        },
    ];
    // The documented payload of the PUT example hashes to the documented value.
    assert_eq!(
        sha256_hex(payload),
        "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072"
    );
    let credentials = Credentials::new(DOC_ACCESS_KEY, DOC_SECRET_KEY).unwrap();
    let region = Region::new("us-east-1").unwrap();
    for example in examples {
        let mut request =
            reqwest::Request::new(example.method.clone(), example.url.parse().unwrap());
        for (name, value) in example.headers {
            request
                .headers_mut()
                .insert(*name, HeaderValue::from_static(value));
        }
        if !example.body.is_empty() {
            *request.body_mut() = Some(reqwest::Body::from(example.body.to_vec()));
        }
        sign_request(&mut request, &credentials, &region, doc_time()).unwrap();
        let expected_auth = fixture_text(&format!("sigv4/{}.auth", example.name));
        assert_same_authorization(header_str(&request, "authorization"), &expected_auth);

        let verified = verify_native(&request).unwrap();
        assert_eq!(
            verified.canonical_request,
            fixture_text(&format!("sigv4/{}.creq", example.name)),
            "{}",
            example.name
        );
        assert_eq!(
            verified.string_to_sign,
            fixture_text(&format!("sigv4/{}.sts", example.name)),
            "{}",
            example.name
        );
        // The documented signature, from the independent HMAC chain alone.
        let key = signing_key(DOC_SECRET_KEY, "20130524", "us-east-1", "s3");
        let documented = parse_authorization(&expected_auth).unwrap().signature;
        assert_eq!(
            signature(&key, &verified.string_to_sign),
            documented,
            "{}",
            example.name
        );
    }
}

// ---------------------------------------------------------------------------------------------
// ListObjectsV2.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn list_objects_v2_request_and_documented_response() {
    let server = TestServer::start(|_| xml_reply(fixture("list-start-after.xml"))).await;
    let s3 = client_for(server.addr(), "us-east-1", &["quotes"]);
    let page = s3
        .bucket(bucket_name("quotes"))
        .list_objects_v2()
        .set_max_keys(MaxKeys::new(3).unwrap())
        .set_prefix("E")
        .set_start_after(key("ExampleGuide.pdf"))
        .send()
        .await
        .unwrap();

    let recorded = server.single();
    assert_eq!(recorded.method, "GET");
    assert_eq!(
        recorded.target,
        "/?max-keys=3&prefix=E&start-after=ExampleGuide.pdf&list-type=2"
    );
    assert_eq!(
        recorded.host,
        format!("quotes.s3.test:{}", server.addr().port())
    );
    assert!(recorded.body.is_empty());
    assert_eq!(recorded.header("x-amz-date"), Some("20261106T120000Z"));
    assert_eq!(recorded.header("x-amz-content-sha256"), Some(EMPTY_SHA256));
    let verified = wire(&recorded).unwrap();
    assert_eq!(verified.authorization.access_key, SYNTHETIC_ACCESS_KEY);
    assert_eq!(verified.authorization.region, "us-east-1");
    assert_eq!(verified.authorization.service, "s3");
    assert_eq!(
        verified.canonical_request.lines().nth(2),
        Some("list-type=2&max-keys=3&prefix=E&start-after=ExampleGuide.pdf")
    );

    assert_eq!(page.name.as_deref(), Some("quotes"));
    assert_eq!(page.prefix.as_deref(), Some("E"));
    assert_eq!(page.start_after.as_deref(), Some("ExampleGuide.pdf"));
    assert_eq!(
        (page.key_count, page.max_keys, page.is_truncated),
        (Some(1), Some(3), false)
    );
    let [object] = &page.contents[..] else {
        panic!("{page:?}")
    };
    assert_eq!(object.key, "ExampleObject.txt");
    assert_eq!(
        object.last_modified.as_deref(),
        Some("2013-09-17T18:07:53.000Z")
    );
    assert_eq!(
        object.etag.as_ref().unwrap().as_str(),
        "\"599bab3ed2c697f1d26842727561fd94\""
    );
    assert_eq!(object.size, Some(857));
    assert_eq!(object.storage_class, Some(StorageClass::ReducedRedundancy));
    assert_eq!(page.meta.request_id(), Some("FAKE-REQUEST-ID"));
}

#[tokio::test]
async fn delimiter_listing_returns_common_prefixes() {
    let server = TestServer::start(|_| xml_reply(fixture("list-delimiter.xml"))).await;
    let s3 = client_for(server.addr(), "us-east-1", &["example-bucket"]);
    let page = s3
        .bucket(bucket_name("example-bucket"))
        .list_objects_v2()
        .set_delimiter("/")
        .send()
        .await
        .unwrap();
    assert_eq!(server.single().target, "/?delimiter=%2F&list-type=2");
    assert_eq!(page.prefix.as_deref(), Some(""));
    assert_eq!(page.delimiter.as_deref(), Some("/"));
    assert_eq!(page.contents.len(), 1);
    assert_eq!(page.contents[0].key, "sample.jpg");
    assert_eq!(page.common_prefixes, ["photos/"]);
}

#[tokio::test]
async fn unknown_xml_elements_do_not_prevent_decoding() {
    let server = TestServer::start(|_| xml_reply(fixture("list-unknown-elements.xml"))).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let page = s3
        .bucket(bucket_name("examplebucket"))
        .list_objects_v2()
        .set_fetch_owner(true)
        .send()
        .await
        .unwrap();
    assert_eq!(server.single().target, "/?fetch-owner=true&list-type=2");
    assert_eq!(page.name.as_deref(), Some("examplebucket"));
    assert_eq!(page.prefix.as_deref(), Some(""));
    let [object] = &page.contents[..] else {
        panic!("{page:?}")
    };
    assert_eq!(object.key, "a&b <c>.txt");
    assert_eq!(object.size, Some(5));
    assert_eq!(object.checksum_algorithms, ["CRC32", "SHA256"]);
    assert_eq!(
        object.owner.as_ref().unwrap().id.as_deref(),
        Some("synthetic-owner-id")
    );
    assert_eq!(
        object.storage_class,
        Some(StorageClass::Other("FUTURE_CLASS".into()))
    );
}

#[tokio::test]
async fn url_encoded_listing_is_decoded() {
    let server = TestServer::start(|_| xml_reply(fixture("list-url-encoded.xml"))).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let page = s3
        .bucket(bucket_name("examplebucket"))
        .list_objects_v2()
        .set_prefix("test/")
        .set_delimiter("/")
        .set_encoding_type(UrlEncoding)
        .send()
        .await
        .unwrap();
    assert_eq!(
        server.single().target,
        "/?delimiter=%2F&encoding-type=url&prefix=test%2F&list-type=2"
    );
    assert_eq!(page.encoding_type.as_deref(), Some("url"));
    assert_eq!(page.prefix.as_deref(), Some("test/"));
    assert_eq!(page.delimiter.as_deref(), Some("/"));
    let keys: Vec<&str> = page.contents.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, ["test/test_file(3).png", "test/märz report+1.txt"]);
    assert_eq!(page.common_prefixes, ["test/sub dir/"]);
}

#[tokio::test]
async fn query_values_are_uri_encoded_once_and_signed() {
    let token = ContinuationToken::new("1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=").unwrap();
    let server = TestServer::start(|_| xml_reply(fixture("list-delimiter.xml"))).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    s3.bucket(bucket_name("examplebucket"))
        .list_objects_v2()
        .set_prefix("a b+c*~/é")
        .set_continuation_token(token)
        .send()
        .await
        .unwrap();
    let recorded = server.single();
    assert_eq!(
        recorded.target,
        "/?continuation-token=1ueGcxLPRx1Tr%2FXYExHnhbYLgveDs2J%2Fwm36Hy4vbOwM%3D&prefix=a%20b%2Bc%2A~%2F%C3%A9&list-type=2"
    );
    let verified = wire(&recorded).unwrap();
    assert_eq!(
        verified.canonical_request.lines().nth(2),
        Some(
            "continuation-token=1ueGcxLPRx1Tr%2FXYExHnhbYLgveDs2J%2Fwm36Hy4vbOwM%3D&list-type=2&prefix=a%20b%2Bc%2A~%2F%C3%A9"
        )
    );
}

#[tokio::test]
async fn listing_refusals_keep_the_response_metadata() {
    // A 2xx labelled text/html: refused before the body is read (Concord `Metadata`).
    let server = TestServer::start(|_| {
        Reply::text(200, "text/html", "<html>proxy page</html>")
            .header("x-amz-request-id", "HTML-1")
    })
    .await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let bucket = s3.bucket(bucket_name("examplebucket"));
    let error = bucket.list_objects_v2().send().await.unwrap_err();
    let S3Error::Response {
        error: concord,
        meta,
    } = &error
    else {
        panic!("{error:?}")
    };
    assert_eq!(concord.kind(), ErrorKind::Metadata);
    assert_eq!(
        concord.codec_error().unwrap().label(),
        "the response Content-Type is not application/xml or text/xml"
    );
    assert_eq!(meta.request_id(), Some("HTML-1"));
    assert_eq!(error.status(), Some(reqwest::StatusCode::OK));

    // Not well-formed XML in a 200: a `Decode` error with the position, never a partial page.
    let server = TestServer::start(|_| xml_reply(fixture("list-not-well-formed.xml"))).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let error = s3
        .bucket(bucket_name("examplebucket"))
        .list_objects_v2()
        .send()
        .await
        .unwrap_err();
    let S3Error::Response { error: concord, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(concord.kind(), ErrorKind::Decode);
    let codec = concord.codec_error().unwrap();
    assert_eq!(codec.label(), "the S3 XML document is not well-formed");
    assert!(codec.get("line").is_some());

    // An <Error> document inside a 200 is not a listing.
    let server = TestServer::start(|_| xml_reply(fixture("error-no-such-key.xml"))).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let error = s3
        .bucket(bucket_name("examplebucket"))
        .list_objects_v2()
        .send()
        .await
        .unwrap_err();
    let S3Error::Response { error: concord, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(
        concord.codec_error().unwrap().label(),
        "a 2xx listing response carried an S3 <Error> document"
    );
}

#[tokio::test]
async fn a_truncated_page_without_a_token_is_a_protocol_error() {
    let server =
        TestServer::start(|_| xml_reply(fixture("list-truncated-without-token.xml"))).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let mut pages = s3
        .bucket(bucket_name("examplebucket"))
        .list_objects_v2()
        .pages();
    let Err(PagesError::Provider { error, failed, .. }) = pages.next_page().await else {
        panic!("expected a provider error")
    };
    assert!(matches!(
        error,
        S3Error::Protocol {
            kind: ProtocolErrorKind::TruncatedWithoutToken,
            ..
        }
    ));
    assert_eq!(failed, Resume::Start);
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 1, "no guessed follow-up request");
}

#[tokio::test]
async fn a_page_repeating_its_token_is_a_protocol_error() {
    let token = "1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=";
    // Page 1 again, for every request: page 2 then repeats the token it was requested with.
    let server = TestServer::start(move |_| xml_reply(fixture("inventory-page-1.xml"))).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let mut pages = s3
        .bucket(bucket_name("examplebucket"))
        .list_objects_v2()
        .set_prefix("reports/2026/")
        .pages();
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(
        first.next.as_ref().map(ContinuationToken::as_str),
        Some(token)
    );
    let Err(PagesError::Provider { error, failed, .. }) = pages.next_page().await else {
        panic!("expected a provider error")
    };
    assert!(matches!(
        error,
        S3Error::Protocol {
            kind: ProtocolErrorKind::RepeatedToken,
            ..
        }
    ));
    assert_eq!(failed, Resume::At(ContinuationToken::new(token).unwrap()));
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn a_page_cap_stops_with_the_token_to_resume_from() {
    let server = TestServer::start(|r| {
        if r.query().unwrap_or("").contains("continuation-token") {
            xml_reply(fixture("inventory-page-2.xml"))
        } else {
            xml_reply(fixture("inventory-page-1.xml"))
        }
    })
    .await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let listing = s3
        .bucket(bucket_name("examplebucket"))
        .list_objects_v2()
        .set_prefix("reports/2026/");
    let incomplete = listing
        .clone()
        .pages()
        .max_pages(std::num::NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    let Some(Resume::At(resume)) = incomplete.error.resume().cloned() else {
        panic!("a bounded listing names where to resume")
    };
    // Resuming from the token reads the rest, and only the rest.
    let rest = listing
        .set_continuation_token(resume)
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].key, "reports/2026/Q2 forecast.txt");
    assert_eq!(server.requests().len(), 2);
}

// ---------------------------------------------------------------------------------------------
// HeadObject, GetObject, PutObject.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn head_object_documented_response() {
    let head = head_fixture("head-object.head");
    let server = TestServer::start(move |_| {
        head_reply(with_head(Reply::new(200), &head), &vec![0u8; 434_234])
    })
    .await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let object = s3
        .bucket(bucket_name("examplebucket"))
        .head_object(key("my-image.jpg"))
        .send()
        .await
        .unwrap();
    let recorded = server.single();
    assert_eq!(
        (recorded.method.as_str(), recorded.target.as_str()),
        ("HEAD", "/my-image.jpg")
    );
    assert!(recorded.body.is_empty());
    wire(&recorded).unwrap();

    assert_eq!(object.status, reqwest::StatusCode::OK);
    assert_eq!(object.content_length, Some(434_234));
    assert_eq!(object.content_type.as_deref(), Some("text/plain"));
    assert_eq!(
        object.etag.as_ref().unwrap().as_str(),
        "\"fba9dede5f27731c9771645a39863328\""
    );
    assert_eq!(
        object.version_id.as_ref().unwrap().as_str(),
        "3HL4kqtJlcpXroDTDmjVBH40Nrjfkd"
    );
    // The documented `Sun, 1 Jan 2006 12:00:00 GMT` (one-digit day) is read.
    assert_eq!(
        object.last_modified.unwrap(),
        HttpDate::parse("Sun, 01 Jan 2006 12:00:00 GMT").unwrap()
    );
    assert_eq!(object.meta.request_id(), Some("318BC8BC143432E5"));
    assert_eq!(
        object.meta.extended_request_id(),
        Some("ef8yU9AS1ed4OpIszj7UDNEHGran")
    );
}

#[tokio::test]
async fn get_object_range_documented_206() {
    let head = head_fixture("get-object-range.head");
    let server =
        TestServer::start(move |_| with_head(Reply::new(206), &head).body(b"0123456789")).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let output = s3
        .bucket(bucket_name("examplebucket"))
        .get_object(key("example-object"))
        .set_range(ByteRange::inclusive(0, 9).unwrap())
        .send()
        .await
        .unwrap();
    let recorded = server.single();
    assert_eq!(recorded.target, "/example-object");
    assert_eq!(recorded.header("range"), Some("bytes=0-9"));
    let verified = wire(&recorded).unwrap();
    assert!(
        verified
            .authorization
            .signed_headers
            .contains(&"range".to_owned())
    );

    assert!(output.head.is_partial());
    let range = output.head.content_range.unwrap();
    assert_eq!(
        (range.first, range.last, range.complete_length),
        (0, 9, Some(443))
    );
    assert_eq!(output.head.metadata.get("title"), Some("the title"));
    assert_eq!(output.head.accept_ranges.as_deref(), Some("bytes"));
    assert_eq!(output.head.content_length, Some(10));
    let (_, bytes) = output.bytes(1024).await.unwrap();
    assert_eq!(&bytes[..], b"0123456789");
}

#[tokio::test]
async fn put_object_sends_exact_bytes_and_headers() {
    let head = head_fixture("put-object.head");
    let server = TestServer::start(move |_| with_head(Reply::new(200), &head)).await;
    let s3 = client_for(server.addr(), "eu-west-3", &["examplebucket"]);
    let body: Vec<u8> = vec![0x00, 0xff, b'\r', b'\n', 0x80, b'z'];
    let metadata = Metadata::new()
        .insert("Author", "Janet")
        .unwrap()
        .insert("purpose", "protocol test")
        .unwrap();
    let output = s3
        .bucket(bucket_name("examplebucket"))
        .put_object(
            key("my-image.jpg"),
            Bytes::from(body.clone()),
            ContentType::new("image/jpeg").unwrap(),
        )
        .set_cache_control("max-age=60")
        .set_content_disposition("attachment; filename=\"image.jpg\"")
        .set_storage_class(StorageClass::ReducedRedundancy)
        .set_if_none_match(AnyObject)
        .metadata(metadata)
        .send(&s3)
        .await
        .unwrap();
    let recorded = server.single();
    assert_eq!(
        (recorded.method.as_str(), recorded.target.as_str()),
        ("PUT", "/my-image.jpg")
    );
    assert_eq!(recorded.body, body);
    assert!(!recorded.chunked);
    assert_eq!(recorded.header("content-length"), Some("6"));
    assert_eq!(recorded.header("content-type"), Some("image/jpeg"));
    assert_eq!(recorded.header("x-amz-meta-author"), Some("Janet"));
    assert_eq!(recorded.header("x-amz-meta-purpose"), Some("protocol test"));
    assert_eq!(recorded.header("cache-control"), Some("max-age=60"));
    assert_eq!(
        recorded.header("content-disposition"),
        Some("attachment; filename=\"image.jpg\"")
    );
    assert_eq!(
        recorded.header("x-amz-storage-class"),
        Some("REDUCED_REDUNDANCY")
    );
    assert_eq!(recorded.header("if-none-match"), Some("*"));
    assert_eq!(
        recorded.header("x-amz-content-sha256"),
        Some(sha256_hex(&body).as_str())
    );
    let verified = wire(&recorded).unwrap();
    assert_eq!(verified.authorization.region, "eu-west-3");
    for signed in [
        "content-type",
        "host",
        "if-none-match",
        "x-amz-meta-author",
        "x-amz-meta-purpose",
    ] {
        assert!(
            verified
                .authorization
                .signed_headers
                .iter()
                .any(|h| h == signed),
            "{signed}"
        );
    }

    assert_eq!(
        output.etag.unwrap().as_str(),
        "\"fbacf535f27731c9771645a39863328\""
    );
    assert_eq!(
        output.version_id.unwrap().as_str(),
        "43jfkodU8493jnFJD9fjj3HHNVfdsQUIFDNsidf038jfdsjGFDSIRp"
    );
    assert_eq!(output.meta.request_id(), Some("0A49CE4060975EAC"));
}

#[tokio::test]
async fn documented_error_document_and_head_errors() {
    let server = TestServer::start(|r| {
        if r.method == "HEAD" {
            Reply::new(404).header("x-amz-request-id", "HEAD-404")
        } else {
            Reply::text(404, "application/xml", fixture("error-no-such-key.xml"))
                .header("x-amz-request-id", "4442587FB7D0A2F9")
                .header("x-amz-id-2", "synthetic-id-2")
        }
    })
    .await;
    let s3 = client_for(server.addr(), "us-east-1", &["mybucket"]);
    let bucket = s3.bucket(bucket_name("mybucket"));

    let error = bucket
        .get_object(key("myfoto.jpg"))
        .send()
        .await
        .unwrap_err();
    assert!(error.is_not_found());
    let S3Error::Api(api) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(api.status(), reqwest::StatusCode::NOT_FOUND);
    assert_eq!(api.body(), ErrorBody::Parsed);
    assert_eq!(api.code(), Some("NoSuchKey"));
    assert_eq!(
        api.message(),
        Some("The resource you requested does not exist")
    );
    assert_eq!(api.resource(), Some("/mybucket/myfoto.jpg"));
    assert_eq!(api.document_request_id(), Some("4442587FB7D0A2F9"));
    assert_eq!(api.meta().extended_request_id(), Some("synthetic-id-2"));
    assert_eq!(api.operation(), "S3Api.bucket.get_object");
    // Display names the operation, status, code and request id; never the message or resource.
    let shown = error.to_string();
    assert_eq!(
        shown,
        "S3Api.bucket.get_object: HTTP 404 NoSuchKey (request id 4442587FB7D0A2F9)"
    );
    assert!(!format!("{error:?}").contains("myfoto"));

    // S3 sends no document for HEAD errors: the status and request id are what there is.
    let error = bucket
        .head_object(key("myfoto.jpg"))
        .send()
        .await
        .unwrap_err();
    assert!(error.is_not_found());
    let S3Error::Api(api) = &error else {
        panic!("{error:?}")
    };
    assert_eq!((api.code(), api.body()), (None, ErrorBody::Empty));
    assert_eq!(error.request_id(), Some("HEAD-404"));
}

#[tokio::test]
async fn error_documents_are_read_up_to_the_bound() {
    // A documented `<Error>` padded with trailing whitespace to exactly `MAX_ERROR_BODY` bytes.
    let mut at_limit = fixture("error-no-such-key.xml");
    at_limit.resize(concord_sdk_amazon_s3::MAX_ERROR_BODY, b' ');
    let server = TestServer::start(move |r| {
        let xml = |body: Vec<u8>| Reply::text(503, "application/xml", body);
        match r.path() {
            "/at-limit" => xml(at_limit.clone()),
            "/over-limit" => xml(vec![b' '; concord_sdk_amazon_s3::MAX_ERROR_BODY + 1]),
            "/not-a-document" => Reply::text(502, "text/html", "<html>bad gateway</html>"),
            _ => Reply::new(500)
                .header("content-type", "application/xml")
                .with_body(Body::FailAfter(vec!["<Error><Code>Int".into()])),
        }
    })
    .await;
    let s3 = client_for(server.addr(), "us-east-1", &["mybucket"]);
    let bucket = s3.bucket(bucket_name("mybucket"));
    let api_error = |error: S3Error| match error {
        S3Error::Api(api) => api,
        other => panic!("{other:?}"),
    };
    for (path, status, body, code) in [
        ("at-limit", 503, ErrorBody::Parsed, Some("NoSuchKey")),
        ("over-limit", 503, ErrorBody::TooLarge, None),
        ("not-a-document", 502, ErrorBody::Unrecognized, None),
        ("cut", 500, ErrorBody::ReadFailed, None),
    ] {
        let error = bucket.get_object(key(path)).send().await.unwrap_err();
        let api = api_error(error);
        assert_eq!(api.status().as_u16(), status, "{path}");
        assert_eq!((api.body(), api.code()), (body, code), "{path}");
        assert_eq!(api.operation(), "S3Api.bucket.get_object");
    }
    assert_eq!(server.requests().len(), 4);
}

#[tokio::test]
async fn conditional_reads_304_and_412() {
    let server = TestServer::start(|r| {
        if r.header("if-none-match").is_some() {
            Reply::new(304)
                .header("etag", "\"fba9dede5f27731c9771645a39863328\"")
                .header("last-modified", "Mon, 12 Oct 2009 17:50:00 GMT")
                .header("x-amz-request-id", "NM-1")
        } else {
            Reply::text(
                412,
                "application/xml",
                fixture("error-precondition-failed.xml"),
            )
            .header("x-amz-request-id", "PF-1")
        }
    })
    .await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let bucket = s3.bucket(bucket_name("examplebucket"));
    let current = ETag::parse("\"fba9dede5f27731c9771645a39863328\"").unwrap();

    let error = bucket
        .get_object(key("my-image.jpg"))
        .set_if_none_match(current.clone())
        .send()
        .await
        .unwrap_err();
    let S3Error::NotModified(not_modified) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(not_modified.etag(), Some(&current));
    assert_eq!(
        not_modified.last_modified().unwrap().to_string(),
        "Mon, 12 Oct 2009 17:50:00 GMT"
    );
    assert_eq!(not_modified.meta().request_id(), Some("NM-1"));
    let recorded = &server.requests()[0];
    assert_eq!(
        recorded.header("if-none-match"),
        Some("\"fba9dede5f27731c9771645a39863328\"")
    );
    let verified = wire(recorded).unwrap();
    assert!(
        verified
            .authorization
            .signed_headers
            .contains(&"if-none-match".to_owned())
    );

    let stale = ETag::parse("\"stale\"").unwrap();
    let error = bucket
        .get_object(key("my-image.jpg"))
        .set_if_match(stale)
        .set_if_unmodified_since(HttpDate::parse("Mon, 12 Oct 2009 17:50:00 GMT").unwrap())
        .send()
        .await
        .unwrap_err();
    assert!(error.is_precondition_failed());
    assert_eq!(error.code(), Some("PreconditionFailed"));
    assert_eq!(error.request_id(), Some("PF-1"));
    let recorded = &server.requests()[1];
    assert_eq!(recorded.header("if-match"), Some("\"stale\""));
    assert_eq!(
        recorded.header("if-unmodified-since"),
        Some("Mon, 12 Oct 2009 17:50:00 GMT")
    );
}

#[tokio::test]
async fn refused_ranges_and_body_framing() {
    // Each reply answers `bytes=0-9` wrongly.
    let replies = |r: &Recorded| match r.path() {
        "/other-range" => Reply::new(206)
            .header("content-range", "bytes 5-9/443")
            .header("content-type", "text/plain")
            .body(b"56789"),
        "/multipart" => Reply::new(206)
            .header("content-type", "multipart/byteranges; boundary=x")
            .body(b"--x--"),
        "/length" => Reply::new(206)
            .header("content-range", "bytes 0-9/443")
            .body(b"0123"),
        "/no-range" => Reply::new(206).body(b"0123456789"),
        _ => Reply::new(500),
    };
    let server = TestServer::start(replies).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let bucket = s3.bucket(bucket_name("examplebucket"));
    let get = |path: &str| {
        bucket
            .get_object(key(path))
            .set_range(ByteRange::inclusive(0, 9).unwrap())
            .send()
    };

    let error = get("other-range").await.unwrap_err();
    assert!(matches!(
        error,
        S3Error::Protocol {
            kind: ProtocolErrorKind::RangeMismatch,
            ..
        }
    ));

    for (path, label) in [
        (
            "multipart",
            "a multipart/byteranges response (this SDK requests one range)",
        ),
        ("length", "Content-Length disagrees with Content-Range"),
        ("no-range", "a 206 response has no Content-Range"),
    ] {
        let error = get(path).await.unwrap_err();
        let S3Error::Response { error: concord, .. } = &error else {
            panic!("{error:?}")
        };
        assert_eq!(concord.kind(), ErrorKind::Metadata, "{path}");
        assert_eq!(concord.codec_error().unwrap().label(), label, "{path}");
    }
}

#[tokio::test]
async fn body_failures_are_reported_with_what_arrived() {
    let server = TestServer::start(|r| match r.path() {
        "/cut" => Reply::new(200)
            .header("etag", "\"opaque\"")
            .with_body(Body::FailAfter(vec![concord::bytes::Bytes::from_static(
                b"partial-",
            )])),
        _ => Reply::new(200).body(vec![7u8; 4096]),
    })
    .await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let bucket = s3.bucket(bucket_name("examplebucket"));

    let mut output = bucket.get_object(key("cut")).send().await.unwrap();
    let first = output.body.chunk().await.unwrap().unwrap();
    assert_eq!(&first[..], b"partial-");
    let error = output.body.chunk().await.unwrap_err();
    let S3Error::Body(body) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(body.kind(), BodyErrorKind::Transport);
    assert_eq!(body.received(), 8);
    assert_eq!(body.transport().unwrap().kind(), ErrorKind::Transport);
    assert!(
        output.body.chunk().await.unwrap().is_none(),
        "nothing after a failure"
    );

    // A declared length above the caller's limit: refused before reading.
    let output = bucket.get_object(key("big")).send().await.unwrap();
    let error = output.body.collect(1024).await.unwrap_err();
    let S3Error::Body(body) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(
        (body.kind(), body.received(), body.expected()),
        (BodyErrorKind::LimitExceeded, 0, Some(4096))
    );
}

/// After a failure, later `chunk()` calls return `Ok(None)` like the verified end does;
/// `completion()` is `EndOfInput` only at that end and keeps the failure's kind otherwise.
#[tokio::test]
async fn completion_tells_a_failed_body_from_a_clean_end() {
    let server =
        TestServer::start(|r| match r.path() {
            "/cut" => Reply::new(200).with_body(Body::FailAfter(vec![
                concord::bytes::Bytes::from_static(b"partial-"),
            ])),
            "/chunked" => Reply::new(200).with_body(Body::Chunks(vec![
                (Duration::ZERO, concord::bytes::Bytes::from_static(b"ab")),
                (Duration::ZERO, concord::bytes::Bytes::from_static(b"cd")),
            ])),
            _ => Reply::new(200).body(vec![7u8; 4096]),
        })
        .await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let bucket = s3.bucket(bucket_name("examplebucket"));

    let mut cut = bucket.get_object(key("cut")).send().await.unwrap();
    assert_eq!(cut.body.completion(), BodyCompletion::Reading);
    assert_eq!(&cut.body.chunk().await.unwrap().unwrap()[..], b"partial-");
    assert_eq!(cut.body.completion(), BodyCompletion::Reading);
    assert!(cut.body.chunk().await.is_err());
    for _ in 0..2 {
        assert!(cut.body.chunk().await.unwrap().is_none());
        assert!(cut.body.is_finished());
        assert_eq!(
            cut.body.completion(),
            BodyCompletion::Failed(BodyErrorKind::Transport)
        );
    }
    assert!(format!("{:?}", cut.body).contains("Failed(Transport)"));

    for (path, len) in [("whole", 4096), ("chunked", 4)] {
        let mut ok = bucket.get_object(key(path)).send().await.unwrap();
        assert_eq!(ok.body.completion(), BodyCompletion::Reading, "{path}");
        let mut read = 0;
        while let Some(chunk) = ok.body.chunk().await.unwrap() {
            read += chunk.len();
        }
        assert_eq!(read, len, "{path}");
        assert!(ok.body.chunk().await.unwrap().is_none());
        assert_eq!(ok.body.completion(), BodyCompletion::EndOfInput, "{path}");
    }
    assert_eq!(server.requests().len(), 3);
}

// ---------------------------------------------------------------------------------------------
// Signing context: Region, service, per-request credentials, session tokens.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn per_request_credentials_and_session_token() {
    let server = TestServer::start(|_| xml_reply(fixture("list-delimiter.xml"))).await;
    let s3 = client_for(server.addr(), "ap-southeast-2", &["examplebucket"]);
    let bucket = s3.bucket(bucket_name("examplebucket"));
    let temporary = other_credentials()
        .with_session_token(SYNTHETIC_SESSION_TOKEN)
        .unwrap();

    // One request's own credentials: a client snapshot with them.
    let own = s3.clone().with_credentials(temporary.clone());
    let signed = own
        .prepare(own.bucket(bucket_name("examplebucket")).list_objects_v2())
        .unwrap();
    let native = signed.request();
    assert!(native.headers()["authorization"].is_sensitive());
    assert!(native.headers()["x-amz-security-token"].is_sensitive());
    assert_eq!(
        header_str(native, "x-amz-security-token"),
        SYNTHETIC_SESSION_TOKEN
    );
    let shown = format!("{signed:?}");
    assert!(!shown.contains(SYNTHETIC_SESSION_TOKEN) && !shown.contains("Signature="));
    signed.send().await.unwrap();

    // The client's own credentials on the next request: per-request credentials do not stick.
    bucket.list_objects_v2().send().await.unwrap();

    let requests = server.requests();
    let first = wire(&requests[0]).unwrap();
    assert_eq!(first.authorization.access_key, OTHER_ACCESS_KEY);
    assert_eq!(first.authorization.region, "ap-southeast-2");
    assert_eq!(first.authorization.service, "s3");
    assert_eq!(first.authorization.date, "20261106");
    assert!(
        first
            .authorization
            .signed_headers
            .contains(&"x-amz-security-token".to_owned())
    );
    let second = wire(&requests[1]).unwrap();
    assert_eq!(second.authorization.access_key, SYNTHETIC_ACCESS_KEY);
    assert!(requests[1].header("x-amz-security-token").is_none());

    // Credentials replaced on the client: earlier handles keep theirs (snapshots).
    let rotated = s3.clone().with_credentials(other_credentials());
    rotated
        .bucket(bucket_name("examplebucket"))
        .list_objects_v2()
        .send()
        .await
        .unwrap();
    bucket.list_objects_v2().send().await.unwrap();
    let requests = server.requests();
    assert_eq!(
        wire(&requests[2]).unwrap().authorization.access_key,
        OTHER_ACCESS_KEY
    );
    assert_eq!(
        wire(&requests[3]).unwrap().authorization.access_key,
        SYNTHETIC_ACCESS_KEY
    );

    let shown = format!("{s3:?} {temporary:?}");
    assert!(!shown.contains(SYNTHETIC_SECRET) && !shown.contains(OTHER_SECRET));
    assert!(!shown.contains(SYNTHETIC_SESSION_TOKEN));
}

/// The generated builders hold the client's signing context (the send policy): their `Debug`
/// withholds it, and their own `prepare()` is the declared request, unsigned.
#[tokio::test]
async fn generated_builders_withhold_the_signing_context() {
    let server = TestServer::start(|_| Reply::new(200)).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]).with_credentials(
        other_credentials()
            .with_session_token(SYNTHETIC_SESSION_TOKEN)
            .unwrap(),
    );
    let bucket = s3.bucket(bucket_name("examplebucket"));
    let put = bucket
        .put_object(
            key("x"),
            Bytes::from_static(b"x"),
            ContentType::new("text/plain").unwrap(),
        )
        .metadata(Metadata::new().insert("author", "me").unwrap());
    let get = bucket.get_object(key("x")).set_range(ByteRange::from(1));
    let shown = format!(
        "{bucket:?} {get:?} {put:?} {:?} {:?}",
        bucket.list_objects_v2(),
        s3.prepare(get.clone()).unwrap()
    );
    for secret in [
        OTHER_SECRET,
        SYNTHETIC_SECRET,
        SYNTHETIC_SESSION_TOKEN,
        "Signature=",
    ] {
        assert!(!shown.contains(secret), "{secret} in {shown}");
    }
    let unsigned = get.prepare().unwrap();
    assert!(unsigned.request().headers().get("authorization").is_none());
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn unknown_credentials_are_answered_with_the_error_document() {
    let fake = FakeS3::default();
    let responder = fake.clone();
    let server = TestServer::start(move |r| responder.respond(r)).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"])
        .with_credentials(Credentials::new("SYNTHETIC-UNKNOWN-KEY", "synthetic-unknown").unwrap());
    let error = s3
        .bucket(bucket_name("examplebucket"))
        .list_objects_v2()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some("InvalidAccessKeyId"));
    assert_eq!(error.status(), Some(reqwest::StatusCode::FORBIDDEN));
    assert_eq!(fake.refusals(), [Refusal::UnknownAccessKey]);
}

// ---------------------------------------------------------------------------------------------
// Perturbations: a key with spaces and Unicode, a signed range, a content header, the body.
// ---------------------------------------------------------------------------------------------

/// The signed requests of a small scenario: both listing pages, a HEAD of an unrelated object
/// and a GET of `target`.
fn scenario(s3: &S3Client, target: &str) -> Vec<reqwest::Request> {
    let bucket = s3.bucket(bucket_name("examplebucket"));
    let listing = bucket.list_objects_v2().set_prefix("reports/2026/");
    let token = ContinuationToken::new("1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=").unwrap();
    fn signed<T>(request: Result<SignedRequest<T>, S3Error>) -> reqwest::Request {
        request.unwrap().into_parts().1
    }
    vec![
        signed(s3.prepare(listing.clone())),
        signed(s3.prepare(listing.set_continuation_token(token))),
        signed(s3.prepare(bucket.head_object(key("reports/2026/January.csv")))),
        signed(
            s3.prepare(
                bucket
                    .get_object(key(target))
                    .set_range(ByteRange::inclusive(0, 9).unwrap()),
            ),
        ),
    ]
}

#[test]
fn perturbing_the_key_changes_only_its_own_request() {
    let s3 = doc_client();
    let plain = scenario(&s3, "reports/2026/March-summary.txt");
    let perturbed = scenario(&s3, "reports/2026/März summary.txt");

    // The perturbed key on the wire: `/` kept, space and UTF-8 percent-encoded once.
    assert_eq!(
        perturbed[3].url().path(),
        "/reports/2026/M%C3%A4rz%20summary.txt"
    );
    let verified = verify_native(&perturbed[3]).unwrap();
    assert_eq!(
        verified.canonical_request,
        format!(
            "GET\n/reports/2026/M%C3%A4rz%20summary.txt\n\nhost:examplebucket.s3.amazonaws.com\nrange:bytes=0-9\nx-amz-content-sha256:{EMPTY_SHA256}\nx-amz-date:20130524T000000Z\n\nhost;range;x-amz-content-sha256;x-amz-date\n{EMPTY_SHA256}"
        )
    );
    let auth = |r: &reqwest::Request| header_str(r, "authorization").to_owned();
    assert_ne!(auth(&plain[3]), auth(&perturbed[3]));

    // Both listing pages and the unrelated HEAD are byte-identical: same target, same signature.
    for i in 0..3 {
        assert_eq!(plain[i].url(), perturbed[i].url(), "request {i}");
        assert_eq!(auth(&plain[i]), auth(&perturbed[i]), "request {i}");
        verify_native(&perturbed[i]).unwrap();
    }
    assert_eq!(
        perturbed[1].url().query(),
        Some(
            "continuation-token=1ueGcxLPRx1Tr%2FXYExHnhbYLgveDs2J%2Fwm36Hy4vbOwM%3D&prefix=reports%2F2026%2F&list-type=2"
        )
    );
}

#[test]
fn perturbing_the_range_or_the_content_type_changes_the_signature() {
    let s3 = doc_client();
    let bucket = s3.bucket(bucket_name("examplebucket"));
    let get = |range| {
        s3.prepare(bucket.get_object(key("test.txt")).set_range(range))
            .unwrap()
            .into_parts()
            .1
    };
    let first = get(ByteRange::inclusive(0, 9).unwrap());
    let second = get(ByteRange::inclusive(0, 10).unwrap());
    let (a, b) = (
        verify_native(&first).unwrap(),
        verify_native(&second).unwrap(),
    );
    let differing: Vec<(&str, &str)> = a
        .canonical_request
        .lines()
        .zip(b.canonical_request.lines())
        .filter(|(x, y)| x != y)
        .collect();
    assert_eq!(differing, [("range:bytes=0-9", "range:bytes=0-10")]);
    assert_ne!(a.authorization.signature, b.authorization.signature);

    let put = |media: &str| {
        s3.prepare(bucket.put_object(
            key("test.txt"),
            Bytes::from_static(b"Welcome to Amazon S3."),
            ContentType::new(media).unwrap(),
        ))
        .unwrap()
        .into_parts()
        .1
    };
    let (a, b) = (
        verify_native(&put("text/plain")).unwrap(),
        verify_native(&put("text/csv")).unwrap(),
    );
    let differing: Vec<(&str, &str)> = a
        .canonical_request
        .lines()
        .zip(b.canonical_request.lines())
        .filter(|(x, y)| x != y)
        .collect();
    assert_eq!(
        differing,
        [("content-type:text/plain", "content-type:text/csv")]
    );
    assert_ne!(a.authorization.signature, b.authorization.signature);
}

#[tokio::test]
async fn changing_a_signed_part_after_signing_is_rejected() {
    let fake = FakeS3::default();
    fake.insert(
        "examplebucket",
        "test.txt",
        StoredObject {
            bytes: b"0123456789abcdef".to_vec(),
            content_type: "text/plain".into(),
            etag: "\"opaque\"".into(),
            last_modified: "Fri, 06 Nov 2026 12:00:00 GMT".into(),
            metadata: Default::default(),
            storage_class: None,
        },
    );
    let responder = fake.clone();
    let server = TestServer::start(move |r| responder.respond(r)).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let bucket = s3.bucket(bucket_name("examplebucket"));

    // Untouched: accepted.
    let signed = s3
        .prepare(
            bucket
                .get_object(key("test.txt"))
                .set_range(ByteRange::inclusive(0, 9).unwrap()),
        )
        .unwrap();
    let (client, request, decoder) = signed.into_parts();
    let ok = decoder.execute(&client, request).await.unwrap();
    assert_eq!(ok.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    drop(ok);

    // The signed range changed after signing.
    let signed = s3
        .prepare(
            bucket
                .get_object(key("test.txt"))
                .set_range(ByteRange::inclusive(0, 9).unwrap()),
        )
        .unwrap();
    let (client, mut request, decoder) = signed.into_parts();
    request
        .headers_mut()
        .insert("range", HeaderValue::from_static("bytes=0-15"));
    let refused = decoder.execute(&client, request).await.unwrap();
    assert_eq!(refused.status(), reqwest::StatusCode::FORBIDDEN);
    drop(refused);

    // A new x-amz-* header after signing: S3 requires every x-amz-* header to be signed.
    let (client, mut request, decoder) = s3
        .prepare(bucket.head_object(key("test.txt")))
        .unwrap()
        .into_parts();
    request
        .headers_mut()
        .insert("x-amz-request-payer", HeaderValue::from_static("requester"));
    let refused = decoder.execute(&client, request).await.unwrap();
    assert_eq!(refused.status(), reqwest::StatusCode::FORBIDDEN);
    drop(refused);

    // The body of a PUT changed after signing: the payload hash no longer matches.
    let (client, mut request, decoder) = s3
        .prepare(bucket.put_object(
            key("new.txt"),
            Bytes::from_static(b"original"),
            ContentType::new("text/plain").unwrap(),
        ))
        .unwrap()
        .into_parts();
    *request.body_mut() = Some(reqwest::Body::from("tampered"));
    let refused = decoder.execute(&client, request).await.unwrap();
    assert_eq!(refused.status(), reqwest::StatusCode::FORBIDDEN);
    drop(refused);

    // The signing time changed after signing.
    let (client, mut request, decoder) = s3
        .prepare(bucket.get_object(key("test.txt")))
        .unwrap()
        .into_parts();
    request
        .headers_mut()
        .insert("x-amz-date", HeaderValue::from_static("20261106T120001Z"));
    let refused = decoder.execute(&client, request).await.unwrap();
    assert_eq!(refused.status(), reqwest::StatusCode::FORBIDDEN);

    assert_eq!(
        fake.refusals(),
        [
            Refusal::SignatureDoesNotMatch,
            Refusal::AmzHeaderNotSigned,
            Refusal::PayloadHashMismatch,
            Refusal::SignatureDoesNotMatch,
        ]
    );
    assert!(
        fake.object("examplebucket", "new.txt").is_none(),
        "a refused PUT stores nothing"
    );
}

#[tokio::test]
async fn a_signature_refusal_reaches_the_caller_as_an_api_error() {
    let server = TestServer::start(|_| {
        Reply::text(
            403,
            "application/xml",
            fixture("error-signature-does-not-match.xml"),
        )
        .header("x-amz-request-id", "SYNTHETIC0000001")
    })
    .await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let error = s3
        .bucket(bucket_name("examplebucket"))
        .get_object(key("test.txt"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some("SignatureDoesNotMatch"));
    let S3Error::Api(api) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(api.host_id(), Some("synthetic-host-id"));
    assert_eq!(api.meta().request_id(), Some("SYNTHETIC0000001"));
}

// ---------------------------------------------------------------------------------------------
// Inputs refused before anything is sent.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn invalid_inputs_send_nothing() {
    assert!(BucketName::new("my.dotted.bucket").is_err());
    assert!(BucketName::new("UPPER").is_err());
    assert!(ObjectKey::new("a/../b").is_err());
    assert!(ObjectKey::new("").is_err());
    assert!(ByteRange::inclusive(9, 0).is_err());
    assert!(Metadata::new().insert("x", "non-ascii é").is_err());
    assert!(ContentType::new("not a media type").is_err());
    assert!(MaxKeys::new(0).is_err());
    assert!(Region::new("US-EAST-1").is_err());
    assert!(ETag::parse("unquoted").is_err());

    let server = TestServer::start(|_| Reply::new(200)).await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let bucket = s3.bucket(bucket_name("examplebucket"));
    // A header value that is not visible ASCII cannot be signed: refused, named, not sent.
    let error = bucket
        .put_object(
            key("x"),
            Bytes::from_static(b"x"),
            ContentType::new("text/plain").unwrap(),
        )
        .set_content_disposition("attachment; filename=\"café.txt\"")
        .send()
        .await
        .unwrap_err();
    let S3Error::Signing(signing) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(signing.header_name(), Some("content-disposition"));
    assert!(error.is_before_send());
    assert!(!error.to_string().contains("café"));
    // A line break in a header value: Concord refuses to build the request.
    let error = bucket
        .put_object(
            key("x"),
            Bytes::from_static(b"x"),
            ContentType::new("text/plain").unwrap(),
        )
        .set_cache_control("a\r\nx-injected: 1")
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(&error, S3Error::Build(e) if e.kind() == ErrorKind::Build),
        "{error:?}"
    );
    // Endpoints the addressing cannot use.
    for endpoint in [
        "http://127.0.0.1:9000",
        "https://s3.test/prefix",
        "ftp://s3.test",
        "https://s3.test/?x=1",
    ] {
        let error = s3.clone().with_endpoint(endpoint).unwrap_err();
        assert!(matches!(error, S3Error::Input(_)), "{endpoint}");
    }
    assert!(server.requests().is_empty());
}

// ---------------------------------------------------------------------------------------------
// Caller cancellation.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn dropping_a_body_or_a_pending_call_releases_the_connection() {
    let server = TestServer::start(|r| match r.path() {
        // An endless body, one chunk every 50 ms.
        "/endless" => Reply::new(200).with_body(Body::Generate(
            Box::new(|| Some(concord::bytes::Bytes::from_static(b"chunk"))),
            Some(Duration::from_millis(50)),
        )),
        _ => Reply::new(200)
            .body(b"late")
            .delay(Duration::from_millis(300)),
    })
    .await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let bucket = s3.bucket(bucket_name("examplebucket"));

    // The head arrived; the caller's deadline drops a pending read (nothing is lost: the next
    // read returns the chunk), then the caller drops the body.
    let mut output = bucket.get_object(key("endless")).send().await.unwrap();
    let pending = tokio::time::timeout(Duration::from_millis(5), output.body.chunk()).await;
    assert!(
        pending.is_err(),
        "the read was pending and its future was dropped"
    );
    assert_eq!(&output.body.chunk().await.unwrap().unwrap()[..], b"chunk");
    assert_eq!(output.body.received(), 5);
    assert!(output.body.expected().is_none());
    drop(output);
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.live_connections() == 0)
            .await,
        "dropping the body closes its connection: the server's next write fails"
    );

    // The head has not arrived: dropping the pending `send()` abandons the request.
    let call = bucket.get_object(key("slow-head")).send();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), call)
            .await
            .is_err()
    );
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.live_connections() == 0)
            .await,
        "dropping the call closes its connection"
    );
    assert_eq!(server.requests().len(), 2, "nothing was retried");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn redirects_reach_the_caller_unfollowed() {
    let server = TestServer::start(|r| match r.path() {
        "/temporary" => Reply::new(307)
            .header("location", "http://examplebucket.s3.test/elsewhere")
            .header("x-amz-request-id", "R-307"),
        _ => Reply::text(
            301,
            "application/xml",
            "<Error><Code>PermanentRedirect</Code><Message>use the bucket's Region</Message><Endpoint>examplebucket.s3.eu-west-1.amazonaws.com</Endpoint></Error>",
        )
        .header("x-amz-bucket-region", "eu-west-1"),
    })
    .await;
    let s3 = client_for(server.addr(), "us-east-1", &["examplebucket"]);
    let bucket = s3.bucket(bucket_name("examplebucket"));

    let error = bucket
        .get_object(key("permanent"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some("PermanentRedirect"));
    assert_eq!(error.meta().unwrap().bucket_region(), Some("eu-west-1"));
    let error = bucket
        .head_object(key("temporary"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        error.status(),
        Some(reqwest::StatusCode::TEMPORARY_REDIRECT)
    );
    assert_eq!(error.request_id(), Some("R-307"));
    // `http_client_builder()` follows no redirect: one request each, no signature forwarded.
    assert_eq!(server.requests().len(), 2);
}
