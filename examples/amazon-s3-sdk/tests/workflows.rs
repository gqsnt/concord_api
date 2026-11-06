//! The two workflows of the brief, end to end through the public API against the stateful fake
//! S3 (which verifies every request's SigV4 signature independently):
//!
//! - **A**: inventory keys under a prefix over two `ListObjectsV2` XML pages, select an object,
//!   `HEAD` it, then read it conditionally (`304`, `412`) and partially (one byte range),
//!   keeping its metadata.
//! - **B**: put a finite byte body with an explicit content type and metadata, then read it back
//!   and compare bytes and metadata with what the fake stored.

mod common;

use std::collections::BTreeMap;

use common::*;
use concord_example_testserver::TestServer;
use concord_pagination::Resume;
use concord_sdk_amazon_s3::{
    BucketName, ByteRange, ContentType, ETag, HttpDate, MaxKeys, Metadata, ObjectKey, S3Error,
    StorageClass,
};

const BUCKET: &str = "examplebucket";
const PREFIX: &str = "reports/2026/";
const TOKEN: &str = "1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=";

/// The objects the two fixture listing pages describe (same keys, sizes and entity tags).
fn inventory_store(fake: &FakeS3) {
    let objects = [
        (
            "reports/2026/January.csv",
            "\"70ee1738b6b21e2c8a43f3a5ab0eee71\"",
            "text/csv",
            b"month,total\njanuary,12000\n".to_vec(),
        ),
        (
            "reports/2026/März summary.txt",
            "\"9b2cf535f27731c974343645a3985328-2\"",
            "text/plain; charset=utf-8",
            "Summary for March: revenue up, costs flat.\n"
                .as_bytes()
                .to_vec(),
        ),
        (
            "reports/2026/Q2 forecast.txt",
            "\"opaque-not-a-digest\"",
            "text/plain",
            b"Q2 forecast: steady growth.\n\n\n\n".to_vec(),
        ),
    ];
    for (key, etag, content_type, bytes) in objects {
        let mut metadata = BTreeMap::new();
        metadata.insert("department".to_owned(), "finance".to_owned());
        metadata.insert("source".to_owned(), "synthetic fixture".to_owned());
        fake.insert(
            BUCKET,
            key,
            StoredObject {
                bytes,
                content_type: content_type.to_owned(),
                etag: etag.to_owned(),
                last_modified: "Wed, 01 Apr 2026 08:00:00 GMT".to_owned(),
                metadata,
                storage_class: None,
            },
        );
    }
    let mut next = BTreeMap::new();
    next.insert(TOKEN.to_owned(), fixture("inventory-page-2.xml"));
    fake.state.lock().unwrap().listings.insert(
        BUCKET.to_owned(),
        FixtureListing {
            prefix: PREFIX.to_owned(),
            first: fixture("inventory-page-1.xml"),
            next,
        },
    );
}

#[tokio::test]
async fn workflow_a_inventory_head_and_conditional_partial_reads() {
    let fake = FakeS3::default();
    inventory_store(&fake);
    let responder = fake.clone();
    let server = TestServer::start(move |r| responder.respond(r)).await;
    let s3 = client_for(server.addr(), "us-east-1", &[BUCKET]);
    let bucket = s3.bucket(BucketName::new(BUCKET).unwrap());

    // 1. Inventory: every key under the prefix, over S3's pages (opaque continuation token).
    let listing = bucket
        .list_objects_v2()
        .prefix(PREFIX)
        .max_keys(MaxKeys::new(2).unwrap());
    let mut pages = listing.pages();
    let mut inventory = Vec::new();
    let mut tokens = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        tokens.push(page.next.clone());
        assert_eq!(page.meta.response.request_id(), Some("FAKE-REQUEST-ID"));
        inventory.extend(page.items);
    }
    assert_eq!(
        pages.finish().outcome,
        concord_pagination::Outcome::Exhausted
    );
    let keys: Vec<&str> = inventory.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "reports/2026/January.csv",
            "reports/2026/März summary.txt",
            "reports/2026/Q2 forecast.txt"
        ]
    );
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].as_ref().map(|t| t.as_str()), Some(TOKEN));
    assert!(tokens[1].is_none(), "the last page has no next token");
    let requests = server.requests();
    assert_eq!(
        requests[0].target,
        "/?max-keys=2&prefix=reports%2F2026%2F&list-type=2"
    );
    assert_eq!(
        requests[1].target,
        "/?continuation-token=1ueGcxLPRx1Tr%2FXYExHnhbYLgveDs2J%2Fwm36Hy4vbOwM%3D&max-keys=2&prefix=reports%2F2026%2F&list-type=2"
    );

    // 2. Select: the largest object (the one whose ETag is a multipart-style tag, which this
    //    SDK treats as opaque, like every other tag).
    let selected = inventory
        .iter()
        .max_by_key(|o| o.size.unwrap_or(0))
        .unwrap();
    assert_eq!(selected.key, "reports/2026/März summary.txt");
    let key = ObjectKey::new(selected.key.clone()).unwrap();
    let listed_etag = selected.etag.clone().unwrap();
    assert_eq!(
        listed_etag.as_str(),
        "\"9b2cf535f27731c974343645a3985328-2\""
    );

    // 3. HEAD: metadata without the body.
    let head = bucket.head_object(key.clone()).send().await.unwrap();
    assert_eq!(head.content_length, selected.size);
    assert_eq!(head.etag.as_ref(), Some(&listed_etag));
    assert_eq!(
        head.content_type.as_deref(),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(head.metadata.get("department"), Some("finance"));
    assert_eq!(head.metadata.get("source"), Some("synthetic fixture"));
    let last_modified = head.last_modified.unwrap();
    let recorded = server.requests().pop().unwrap();
    assert_eq!(
        (recorded.method.as_str(), recorded.target.as_str()),
        ("HEAD", "/reports/2026/M%C3%A4rz%20summary.txt")
    );

    // 4. Conditional GET with the tag we hold: 304, nothing to download, validators kept.
    let error = bucket
        .get_object(key.clone())
        .if_none_match(listed_etag.clone())
        .send()
        .await
        .unwrap_err();
    let S3Error::NotModified(not_modified) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(not_modified.etag(), Some(&listed_etag));
    assert_eq!(not_modified.last_modified(), Some(last_modified));

    // Also by date: unchanged since its Last-Modified.
    let error = bucket
        .get_object(key.clone())
        .if_modified_since(last_modified)
        .send()
        .await
        .unwrap_err();
    assert!(error.is_not_modified());

    // 5. A write-after-read guard that no longer holds: 412.
    let stale = ETag::parse("\"an-older-version\"").unwrap();
    let error = bucket
        .get_object(key.clone())
        .if_match(stale)
        .range(ByteRange::inclusive(0, 6).unwrap())
        .send()
        .await
        .unwrap_err();
    assert!(error.is_precondition_failed());
    assert_eq!(error.code(), Some("PreconditionFailed"));

    // 6. One byte range of the version we inspected (If-Match guards it): 206 with metadata.
    let part = bucket
        .get_object(key.clone())
        .if_match(listed_etag.clone())
        .range(ByteRange::inclusive(12, 16).unwrap())
        .send()
        .await
        .unwrap();
    assert!(part.head.is_partial());
    let range = part.head.content_range.unwrap();
    assert_eq!(
        (range.first, range.last, range.complete_length),
        (12, 16, selected.size)
    );
    assert_eq!(part.head.etag.as_ref(), Some(&listed_etag));
    assert_eq!(part.head.metadata.get("department"), Some("finance"));
    let (_, bytes) = part.bytes(1024).await.unwrap();
    assert_eq!(&bytes[..], b"March");

    // The last 6 bytes, and the whole object when it changed after a given date.
    let tail = bucket
        .get_object(key.clone())
        .range(ByteRange::suffix(6).unwrap())
        .send()
        .await
        .unwrap();
    let (_, tail) = tail.bytes(1024).await.unwrap();
    assert_eq!(&tail[..], b"flat.\n");
    let earlier = HttpDate::parse("Sun, 01 Mar 2026 00:00:00 GMT").unwrap();
    let whole = bucket
        .get_object(key)
        .if_modified_since(earlier)
        .send()
        .await
        .unwrap();
    assert!(!whole.head.is_partial());
    let (head, bytes) = whole.bytes(1024).await.unwrap();
    assert_eq!(
        &bytes[..],
        "Summary for March: revenue up, costs flat.\n".as_bytes()
    );
    assert_eq!(head.content_length, Some(bytes.len() as u64));

    // Every request was signed and verified; nothing was refused.
    assert!(fake.refusals().is_empty(), "{:?}", fake.refusals());
    assert_eq!(fake.verified().len(), server.requests().len());
    assert_eq!(server.requests().len(), 2 + 1 + 2 + 1 + 1 + 2);
}

#[tokio::test]
async fn workflow_a_resumes_an_interrupted_inventory_from_its_token() {
    let fake = FakeS3::default();
    inventory_store(&fake);
    let responder = fake.clone();
    let server = TestServer::start(move |r| responder.respond(r)).await;
    let s3 = client_for(server.addr(), "us-east-1", &[BUCKET]);
    let listing = s3
        .bucket(BucketName::new(BUCKET).unwrap())
        .list_objects_v2()
        .prefix(PREFIX)
        .max_keys(MaxKeys::new(2).unwrap());

    // A budget of one page: the first page, then the token to continue with.
    let incomplete = listing
        .clone()
        .pages()
        .max_pages(std::num::NonZeroUsize::MIN)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    let Some(Resume::At(token)) = incomplete.error.resume().cloned() else {
        panic!("a bounded inventory says where to resume")
    };
    assert_eq!(token.as_str(), TOKEN);
    // Later, perhaps in another process: continue from the stored token.
    let rest = listing
        .continuation_token(token)
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert!(fake.refusals().is_empty());
}

#[tokio::test]
async fn workflow_b_put_finite_bytes_then_read_them_back() {
    let fake = FakeS3::default();
    let responder = fake.clone();
    let server = TestServer::start(move |r| responder.respond(r)).await;
    let s3 = client_for(server.addr(), "eu-west-3", &[BUCKET]);
    let bucket = s3.bucket(BucketName::new(BUCKET).unwrap());

    let key = ObjectKey::new("uploads/2026/naïve café.bin").unwrap();
    let body: Vec<u8> = (0u8..=255).chain(b"\r\n\0end".iter().copied()).collect();
    let content_type = ContentType::new("application/x-synthetic; version=1").unwrap();
    let metadata = Metadata::new()
        .insert("author", "workflow-b")
        .unwrap()
        .insert("Purpose", "round trip")
        .unwrap();

    // 1. Put: one request, the whole body, created only if absent.
    let put = bucket
        .put_object(key.clone(), body.clone(), content_type.clone())
        .metadata(metadata.clone())
        .cache_control("no-cache")
        .storage_class(StorageClass::StandardIa)
        .if_absent()
        .send()
        .await
        .unwrap();
    let put_etag = put.etag.clone().unwrap();
    assert_eq!(put.meta.request_id(), Some("FAKE-PUT-REQUEST-ID"));

    // The fake stored exactly what was sent.
    let stored = fake.object(BUCKET, "uploads/2026/naïve café.bin").unwrap();
    assert_eq!(stored.bytes, body);
    assert_eq!(stored.content_type, content_type.as_str());
    assert_eq!(stored.etag, put_etag.as_str());
    assert_eq!(stored.storage_class.as_deref(), Some("STANDARD_IA"));
    let stored_meta: Vec<(&str, &str)> = stored
        .metadata
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        stored_meta,
        [("author", "workflow-b"), ("purpose", "round trip")]
    );
    let recorded = &server.requests()[0];
    assert_eq!(recorded.target, "/uploads/2026/na%C3%AFve%20caf%C3%A9.bin");
    assert_eq!(
        recorded.header("content-length"),
        Some(body.len().to_string().as_str())
    );
    assert!(!recorded.chunked);

    // 2. Read it back: bytes, content type, metadata and the entity tag of the write. The tag is
    //    compared as an opaque validator; nothing assumes it is a digest of the bytes.
    let object = bucket.get_object(key.clone()).send().await.unwrap();
    assert_eq!(object.head.etag.as_ref(), Some(&put_etag));
    assert_eq!(
        object.head.content_type.as_deref(),
        Some(content_type.as_str())
    );
    assert_eq!(object.head.metadata, metadata);
    assert_eq!(object.head.storage_class, Some(StorageClass::StandardIa));
    let (head, bytes) = object.bytes(64 * 1024).await.unwrap();
    assert_eq!(&bytes[..], &body[..]);
    assert_eq!(head.content_length, Some(body.len() as u64));

    // 3. Read it chunk by chunk, counting what arrives.
    let mut streamed = bucket.get_object(key.clone()).send().await.unwrap();
    let mut collected = Vec::new();
    while let Some(chunk) = streamed.body.chunk().await.unwrap() {
        collected.extend_from_slice(&chunk);
    }
    assert_eq!(collected, body);
    assert_eq!(streamed.body.received(), body.len() as u64);
    assert!(streamed.body.is_finished());

    // 4. The same create-only write again: the object exists, 412, nothing overwritten.
    let error = bucket
        .put_object(key, b"other".to_vec(), content_type)
        .if_absent()
        .send()
        .await
        .unwrap_err();
    assert!(error.is_precondition_failed());
    assert_eq!(
        fake.object(BUCKET, "uploads/2026/naïve café.bin")
            .unwrap()
            .bytes,
        body
    );

    assert!(fake.refusals().is_empty(), "{:?}", fake.refusals());
    for verified in fake.verified() {
        assert_eq!(verified.authorization.region, "eu-west-3");
        assert_eq!(verified.authorization.service, "s3");
    }
}
