//! The S3 example's two paginated lists, read from XML as the service writes it: what each page
//! holds, and what the next request asks for.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord::sign::Credentials;
use concord::Base;
use concord_testkit::Script;
// Compile the page's sources in this test crate, where cfg(test) selects wire.rs.
#[path = "../src/s3/mod.rs"]
mod s3;
use s3::ObjectStore;

fn store(script: &Script) -> (ObjectStore, Base) {
    let bucket: Base = "https://bucket.s3.eu-west-1.amazonaws.com".parse().unwrap();
    let store = ObjectStore::new(
        "eu-west-1",
        Credentials::new("ACCESS", "secret"),
        vec![bucket.clone()],
    )
    .with_transport(script.clone());
    (store, bucket)
}

#[tokio::test(start_paused = true)]
async fn s3_objects_are_read_from_the_service_xml_and_the_token_moves() {
    let script = Script::new();
    script.answer(200).body(
        "<ListBucketResult><IsTruncated>true</IsTruncated>\
         <NextContinuationToken>t2</NextContinuationToken>\
         <Contents><Key>a</Key><Size>1</Size></Contents>\
         <CommonPrefixes><Prefix>d/</Prefix></CommonPrefixes></ListBucketResult>",
    );
    script.answer(200).body(
        "<ListBucketResult><IsTruncated>false</IsTruncated>\
         <Contents><Key>b</Key><Size>2</Size></Contents></ListBucketResult>",
    );
    let (store, bucket) = store(&script);
    let mut pages = store.bucket(bucket).list_objects().pages();
    let (mut keys, mut prefixes) = (Vec::new(), Vec::new());
    while let Some(page) = pages.try_next().await.unwrap() {
        keys.extend(
            page.objects
                .into_iter()
                .map(|object| (object.key, object.size)),
        );
        prefixes.extend(page.common_prefixes.into_iter().map(|prefix| prefix.prefix));
    }
    assert_eq!(keys, [("a".to_owned(), 1), ("b".to_owned(), 2)]);
    assert_eq!(prefixes, ["d/"]);
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].url().as_str().contains("continuation-token=t2"));
}

#[tokio::test(start_paused = true)]
async fn s3_parts_are_read_from_the_service_xml_and_the_marker_moves() {
    let script = Script::new();
    script.answer(200).body(
        "<ListPartsResult><IsTruncated>true</IsTruncated>\
         <NextPartNumberMarker>1</NextPartNumberMarker>\
         <Part><PartNumber>1</PartNumber><ETag>\"e1\"</ETag><Size>5</Size></Part></ListPartsResult>",
    );
    script.answer(200).body(
        "<ListPartsResult><IsTruncated>false</IsTruncated>\
         <Part><PartNumber>2</PartNumber><ETag>\"e2\"</ETag><Size>6</Size></Part></ListPartsResult>",
    );
    let (store, bucket) = store(&script);
    let mut pages = store
        .bucket(bucket)
        .object("o")
        .upload("up")
        .list_parts()
        .pages();
    let mut parts = Vec::new();
    while let Some(page) = pages.try_next().await.unwrap() {
        parts.extend(
            page.parts
                .into_iter()
                .map(|part| (part.part_number, part.etag, part.size)),
        );
    }
    assert_eq!(
        parts,
        [
            (Some(1), Some("\"e1\"".to_owned()), Some(5)),
            (Some(2), Some("\"e2\"".to_owned()), Some(6))
        ]
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].url().as_str().contains("part-number-marker=1"));
}

/// The listings ask for `encoding-type=url`, so the service writes each key, prefix and key marker
/// percent-encoded; the types decode each once. A listed key given to `object(…)` addresses that
/// object, and a key marker goes back as the key it names.
#[tokio::test(start_paused = true)]
async fn s3_listed_keys_and_prefixes_are_decoded_once() {
    let script = Script::new();
    script.answer(200).body(
        "<ListBucketResult><IsTruncated>false</IsTruncated>\
         <Contents><Key>a%2Fb%2Bc%2520+d</Key><Size>1</Size></Contents>\
         <CommonPrefixes><Prefix>a%2F</Prefix></CommonPrefixes></ListBucketResult>",
    );
    script.answer(200);
    let (objects, bucket) = store(&script);
    let bucket = objects.bucket(bucket);
    let page = bucket
        .list_objects()
        .pages()
        .try_next()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(page.objects[0].key, "a/b+c%20 d");
    assert_eq!(page.common_prefixes[0].prefix, "a/");
    let key = page.objects[0].key.clone();
    let _ = bucket.object(key).get().await.unwrap();
    assert_eq!(
        script.requests()[1].url().path(),
        "/a/b%2Bc%2520%20d",
        "the object that was listed, encoded once"
    );

    let script = Script::new();
    script.answer(200).body(
        "<ListMultipartUploadsResult><IsTruncated>true</IsTruncated>\
         <NextKeyMarker>k%2F1</NextKeyMarker><NextUploadIdMarker>u</NextUploadIdMarker>\
         </ListMultipartUploadsResult>",
    );
    script.answer(200).body(
        "<ListMultipartUploadsResult><IsTruncated>false</IsTruncated></ListMultipartUploadsResult>",
    );
    let (uploads, bucket) = store(&script);
    let mut pages = uploads.bucket(bucket).list_multipart_uploads().pages();
    while pages.try_next().await.unwrap().is_some() {}
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].url().query(),
        Some("uploads=&encoding-type=url&key-marker=k%2F1&upload-id-marker=u")
    );
}

/// The continuation token is opaque: it goes back exactly as the service wrote it, never decoded,
/// so `t+%2B` is sent as `t%2B%252B`.
#[tokio::test(start_paused = true)]
async fn s3_the_continuation_token_is_never_decoded() {
    let script = Script::new();
    script.answer(200).body(
        "<ListBucketResult><IsTruncated>true</IsTruncated>\
         <NextContinuationToken>t+%2B</NextContinuationToken>\
         <Contents><Key>a</Key><Size>1</Size></Contents></ListBucketResult>",
    );
    script
        .answer(200)
        .body("<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>");
    let (objects, bucket) = store(&script);
    let mut pages = objects.bucket(bucket).list_objects().pages();
    while pages.try_next().await.unwrap().is_some() {}
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].url().query(),
        Some("list-type=2&encoding-type=url&continuation-token=t%2B%252B")
    );
}

/// Every member of a part is optional in the service's reference: a part without an `ETag` is
/// read, with `None`.
#[tokio::test(start_paused = true)]
async fn s3_a_part_without_an_etag_is_read() {
    let script = Script::new();
    script.answer(200).body(
        "<ListPartsResult><IsTruncated>false</IsTruncated>\
         <Part><PartNumber>1</PartNumber><Size>5</Size></Part></ListPartsResult>",
    );
    let (store, bucket) = store(&script);
    let mut pages = store
        .bucket(bucket)
        .object("o")
        .upload("up")
        .list_parts()
        .pages();
    let page = pages.try_next().await.unwrap().unwrap();
    assert_eq!(page.parts.len(), 1);
    assert_eq!(
        (
            page.parts[0].part_number,
            page.parts[0].etag.clone(),
            page.parts[0].size
        ),
        (Some(1), None, Some(5))
    );
}

/// No member of a part is required: a page whose part lacks its `Size`, or its `PartNumber`, is
/// read, and a truncated page with a marker still leads to the next.
#[tokio::test(start_paused = true)]
async fn s3_a_part_without_a_size_or_a_number_is_read() {
    for (part, number, size) in [
        (
            "<PartNumber>1</PartNumber><ETag>\"e1\"</ETag>",
            Some(1),
            None,
        ),
        ("<ETag>\"e1\"</ETag><Size>5</Size>", None, Some(5)),
    ] {
        let script = Script::new();
        script.answer(200).body(format!(
            "<ListPartsResult><IsTruncated>true</IsTruncated>\
             <NextPartNumberMarker>1</NextPartNumberMarker><Part>{part}</Part></ListPartsResult>"
        ));
        script
            .answer(200)
            .body("<ListPartsResult><IsTruncated>false</IsTruncated></ListPartsResult>");
        let (store, bucket) = store(&script);
        let mut pages = store
            .bucket(bucket)
            .object("o")
            .upload("up")
            .list_parts()
            .pages();
        let first = pages.try_next().await.unwrap().unwrap();
        assert_eq!(
            (first.parts[0].part_number, first.parts[0].size),
            (number, size),
            "{part}"
        );
        assert!(pages.try_next().await.unwrap().is_some(), "{part}");
        assert!(pages.try_next().await.unwrap().is_none(), "{part}");
        assert_eq!(script.requests().len(), 2, "{part}");
    }
}

#[tokio::test(start_paused = true)]
async fn s3_uploads_send_an_empty_secondary_marker_and_require_the_key_marker() {
    let script = Script::new();
    script.answer(200).body(
        "<ListMultipartUploadsResult><IsTruncated>true</IsTruncated>         <NextKeyMarker>folder%2F</NextKeyMarker>         <CommonPrefixes><Prefix>folder%2F</Prefix></CommonPrefixes>         </ListMultipartUploadsResult>",
    );
    script.answer(200).body(
        "<ListMultipartUploadsResult><IsTruncated>true</IsTruncated>\
         <NextKeyMarker>next%2F</NextKeyMarker>\
         <Upload><Key>folder%2Fx</Key><UploadId>u</UploadId></Upload>\
         <CommonPrefixes><Prefix>next%2F</Prefix></CommonPrefixes>\
         </ListMultipartUploadsResult>",
    );
    script.answer(200).body(
        "<ListMultipartUploadsResult><IsTruncated>true</IsTruncated></ListMultipartUploadsResult>",
    );
    let (uploads, bucket) = store(&script);
    let mut pages = uploads
        .bucket(bucket)
        .list_multipart_uploads()
        .upload_id_marker("old")
        .pages();
    assert!(pages.try_next().await.unwrap().is_some());
    assert!(pages.try_next().await.unwrap().is_some());
    assert!(pages.try_next().await.unwrap().is_some());
    let error = pages.try_next().await.unwrap_err();
    assert_eq!(error.reason(), Some("a truncated page with no next marker"));
    assert!(pages.try_next().await.unwrap().is_none());
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1].url().query(),
        Some("uploads=&encoding-type=url&key-marker=folder%2F&upload-id-marker=")
    );
    assert_eq!(
        requests[2].url().query(),
        Some("uploads=&encoding-type=url&key-marker=next%2F&upload-id-marker=")
    );
}
