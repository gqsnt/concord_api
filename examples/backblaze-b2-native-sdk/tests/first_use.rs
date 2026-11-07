//! Independent consumer calls written from README and public rustdoc only.

use concord::{Delivery, ErrorKind, ResponseMeta, bytes::Bytes};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_backblaze_b2_native::{
    ApplicationKey, BucketId, ChecksumFailure, Client, ContentType, Error, ErrorCode, Evidence,
    FileName, Session, Sha1Hex, UploadFile, UploadPolicy, concord, http_client_builder,
    retry::StopReason,
};
use futures_core::Stream;
use serde_json::json;
use std::{
    collections::VecDeque,
    num::NonZeroUsize,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

const KEY_ID: &str = "004f12abc000000000000001";
const KEY: &str = "K004_fixture_application_secret";
const ACCOUNT: &str = "f12abc345678";
const ACCOUNT_TOKEN: &str = "4_fixture_account_authorization_secret";
const BUCKET: &str = "4a48fe8875c6214145260818";
const FILE_ID: &str =
    "4_h4a48fe8875c6214145260818_f000000000000472a_d20261107_m102022_c001_v0000123_t0104";
const NAME: &str = "reports/2026 résumé.txt";
const ENCODED_NAME: &str = "reports/2026%20r%C3%A9sum%C3%A9.txt";
const CONTENT: &[u8] = b"hello\n";
const SHA1: &str = "f572d396fae9206628714fb2ce00f72e94f2258f";
const TIMESTAMP: u64 = 1_793_440_822_000;

type Replies = Arc<Mutex<VecDeque<Reply>>>;

fn enqueue(queue: &Replies, reply: Reply) {
    queue.lock().unwrap().push_back(reply);
}

fn take(queue: &Replies) -> Reply {
    queue
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected fixture request")
}

fn provider(status: u16, code: &str) -> Reply {
    Reply::json(
        status,
        json!({
            "status": status, "code": code, "message": "provider_private_diagnostic"
        })
        .to_string(),
    )
    .header("x-fixture-request-id", "request-44")
}

fn stored() -> Reply {
    Reply::json(
        200,
        json!({
            "accountId": ACCOUNT, "bucketId": BUCKET, "action": "upload",
            "fileId": FILE_ID, "fileName": NAME, "contentLength": CONTENT.len(),
            "contentSha1": SHA1, "contentType": "text/plain", "contentMd5": null,
            "fileInfo": {"src_last_modified_millis": TIMESTAMP.to_string()},
            "uploadTimestamp": TIMESTAMP
        })
        .to_string(),
    )
}

fn download_head(id: &str) -> Reply {
    Reply::new(200)
        .header("x-bz-file-id", id)
        .header("x-bz-file-name", ENCODED_NAME)
        .header("content-type", "text/plain")
        .header("x-bz-upload-timestamp", TIMESTAMP.to_string())
        .header("x-bz-info-src_last_modified_millis", TIMESTAMP.to_string())
        .header("x-bz-info-review-note", "verified%20fixture")
        .header("x-fixture-request-id", "request-44")
}

fn downloaded() -> Reply {
    download_head(FILE_ID)
        .header("x-bz-content-sha1", SHA1)
        .body(CONTENT)
}

fn memory_file() -> UploadFile {
    UploadFile::bytes(
        FileName::new(NAME).unwrap(),
        ContentType::new("text/plain").unwrap(),
        CONTENT.to_vec(),
    )
    .with_last_modified_millis(TIMESTAMP)
}

fn retrying() -> UploadPolicy {
    UploadPolicy::retrying(NonZeroUsize::new(3).unwrap())
}

// A caller-owned one-shot source; no extra stream utility dependency is needed.
struct OnceSource {
    chunk: Option<Bytes>,
    delivered: Arc<AtomicUsize>,
}

impl Stream for OnceSource {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.chunk.take().map(|chunk| {
            self.delivered.fetch_add(chunk.len(), Ordering::SeqCst);
            Ok(chunk)
        }))
    }
}

struct Fixture {
    account: TestServer,
    api: TestServer,
    upload: TestServer,
    download: TestServer,
    uploads: Replies,
    downloads: Replies,
    api_overrides: Replies,
}

impl Fixture {
    async fn start() -> Self {
        let uploads = Replies::default();
        let downloads = Replies::default();
        let api_overrides = Replies::default();
        let queue = uploads.clone();
        let upload = TestServer::start(move |_: &Recorded| take(&queue)).await;
        let queue = downloads.clone();
        let download = TestServer::start(move |_: &Recorded| take(&queue)).await;
        let upload_base = upload.url();
        let queue = api_overrides.clone();
        let serial = AtomicUsize::new(0);
        let api = TestServer::start(move |_: &Recorded| {
            let n = serial.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(reply) = queue.lock().unwrap().pop_front() {
                return reply;
            }
            Reply::json(200, json!({
                "bucketId": BUCKET,
                "uploadUrl": format!("{upload_base}/b2api/v4/b2_upload_file/{BUCKET}/slot-{n}?cvt=fixture%2F{n}&bucket={BUCKET}"),
                "authorizationToken": format!("3_upload_slot_{n}_secret")
            }).to_string())
        }).await;
        let api_base = api.url();
        let download_base = download.url();
        let account =
            TestServer::start(move |_: &Recorded| {
                Reply::json(200, json!({
                "accountId": ACCOUNT, "authorizationToken": ACCOUNT_TOKEN,
                "apiInfo": {"storageApi": {
                    "apiUrl": api_base, "downloadUrl": download_base,
                    "allowed": {
                        "buckets": [{"id": BUCKET, "name": "fixture-reports"}],
                        "capabilities": ["readFiles", "writeFiles"], "namePrefix": "reports/"
                    }
                }},
                "applicationKeyExpirationTimestamp": null
            }).to_string())
            })
            .await;
        Self {
            account,
            api,
            upload,
            download,
            uploads,
            downloads,
            api_overrides,
        }
    }

    async fn authorize(&self) -> Session {
        let http = http_client_builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let client = Client::new(http)
            .with_base_url(&self.account.url())
            .unwrap();
        let key = ApplicationKey::new(KEY_ID, KEY).unwrap();
        let session = client.authorize_account(&key).await.unwrap();
        assert_redacted(&format!("{key:?} {session:?}"), &[]);
        assert_eq!(session.account().account_id(), ACCOUNT);
        assert_eq!(
            session.account().api_url().as_str().trim_end_matches('/'),
            self.api.url()
        );
        assert_eq!(
            session
                .account()
                .download_url()
                .as_str()
                .trim_end_matches('/'),
            self.download.url()
        );
        assert!(session.account().allowed().has_capability("writeFiles"));
        assert_eq!(
            session.account().allowed().name_prefix.as_deref(),
            Some("reports/")
        );
        assert_eq!(session.account().key_expiration_millis(), None);
        session
    }

    fn assert_control_wire(&self) {
        let request = self.account.single();
        assert_eq!(request.method, "GET");
        assert_eq!(request.target, "/b2api/v4/b2_authorize_account");
        assert!(request.body.is_empty());
        let basic = request.header("authorization").unwrap();
        assert_eq!(basic, format!("Basic {}", basic_fixture_value()));
        assert_redacted(
            &format!("{:?}", ApplicationKey::new(KEY_ID, KEY).unwrap()),
            &[basic],
        );
        for request in self.api.requests() {
            assert_eq!(request.method, "GET");
            assert_eq!(request.path(), "/b2api/v4/b2_get_upload_url");
            assert_eq!(
                request.query_pairs(),
                vec![("bucketId".into(), BUCKET.into())]
            );
            assert_eq!(request.header_all("authorization"), vec![ACCOUNT_TOKEN]);
            assert!(request.body.is_empty());
        }
    }

    async fn shutdown(self) {
        assert!(self.uploads.lock().unwrap().is_empty());
        assert!(self.downloads.lock().unwrap().is_empty());
        assert!(self.api_overrides.lock().unwrap().is_empty());
        for server in [self.account, self.api, self.upload, self.download] {
            assert_eq!(server.shutdown().await.open_after, 0);
        }
    }
}

// Fixed, independently encoded fixture credential, not an SDK auth helper.
fn basic_fixture_value() -> &'static str {
    "MDA0ZjEyYWJjMDAwMDAwMDAwMDAwMDAxOkswMDRfZml4dHVyZV9hcHBsaWNhdGlvbl9zZWNyZXQ="
}

fn assert_redacted(rendered: &str, additional: &[&str]) {
    for secret in [
        KEY_ID,
        KEY,
        ACCOUNT,
        ACCOUNT_TOKEN,
        "3_upload_slot_",
        "hello",
        "provider_private_diagnostic",
    ]
    .into_iter()
    .chain(additional.iter().copied())
    {
        assert!(
            !rendered.contains(secret),
            "SDK rendering exposed a fixture secret"
        );
    }
}

fn assert_meta(meta: &ResponseMeta, status: u16, endpoint: &str) {
    assert_eq!(meta.status().as_u16(), status);
    let operation = meta.operation().unwrap();
    let caller_name = endpoint.strip_prefix("b2_").unwrap_or(endpoint);
    assert!(
        operation.name().ends_with(&format!(".{caller_name}")),
        "observed operation: {:?}",
        operation
    );
    assert!(operation.is_endpoint());
    assert_eq!(
        operation.method(),
        Some(if caller_name == "upload_file" {
            "POST"
        } else {
            "GET"
        })
    );
    assert_eq!(meta.headers()["x-fixture-request-id"], "request-44");
}

fn assert_upload_wire(request: &Recorded, slot: usize) {
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.target,
        format!(
            "/b2api/v4/b2_upload_file/{BUCKET}/slot-{slot}?cvt=fixture%2F{slot}&bucket={BUCKET}"
        )
    );
    assert_eq!(
        request.header_all("authorization"),
        vec![format!("3_upload_slot_{slot}_secret")]
    );
    assert_eq!(request.header("content-length"), Some("6"));
    assert_eq!(request.header("content-type"), Some("text/plain"));
    assert_eq!(request.header("x-bz-file-name"), Some(ENCODED_NAME));
    assert_eq!(request.header("x-bz-content-sha1"), Some(SHA1));
    assert_eq!(request.body, CONTENT);
    assert_eq!(request.body_len, CONTENT.len() as u64);
    assert!(!request.chunked);
    assert!(request.header("transfer-encoding").is_none());
}

#[tokio::test]
async fn caller_authorizes_uploads_and_keeps_ambiguity_after_refresh() {
    let fixture = Fixture::start().await;
    assert!(ApplicationKey::new("invalid:id", KEY).is_err());
    assert!(BucketId::new("").is_err());
    assert!(FileName::new("").is_err());
    assert!(ContentType::new("not-a-media-type").is_err());
    assert!(Sha1Hex::new("none").is_err());
    assert!(fixture.account.requests().is_empty());

    let session = fixture.authorize().await;
    let bucket = BucketId::new(BUCKET).unwrap();
    let mut target = session.get_upload_url(&bucket).await.unwrap();
    assert!(target.is_usable());
    assert_eq!(target.bucket_id(), &bucket);
    let issued = target.upload_url().unwrap().as_str().to_owned();
    assert_redacted(&format!("{target:?}"), &[&issued]);
    let file = memory_file();
    assert_eq!(file.length(), 6);
    assert_eq!(file.sha1().as_str(), SHA1);
    enqueue(&fixture.uploads, stored());
    let uploaded = session
        .upload_file(&mut target, file, UploadPolicy::once())
        .await
        .unwrap();
    assert_eq!(uploaded.file.file_id.as_str(), FILE_ID);
    assert_eq!(uploaded.file.file_name, NAME);
    assert_eq!(uploaded.file.content_length, 6);
    assert_eq!(uploaded.file.content_sha1.as_str(), SHA1);
    assert_eq!(
        uploaded.file.file_info["src_last_modified_millis"],
        TIMESTAMP.to_string()
    );
    assert_eq!(uploaded.report.attempts().len(), 1);
    assert_eq!(uploaded.report.attempts()[0].evidence(), Evidence::Stored);
    assert_eq!(uploaded.report.retry().stop, StopReason::Done);
    assert_eq!(uploaded.possible_duplicates(), 0);
    assert!(target.is_usable());
    assert_redacted(&format!("{uploaded:?}"), &[&issued]);
    assert_upload_wire(&fixture.upload.single(), 1);
    assert_eq!(
        fixture
            .upload
            .single()
            .header("x-bz-info-src_last_modified_millis"),
        Some(TIMESTAMP.to_string().as_str())
    );

    let delivered = Arc::new(AtomicUsize::new(0));
    let stream = OnceSource {
        chunk: Some(Bytes::from_static(CONTENT)),
        delivered: delivered.clone(),
    };
    let file = UploadFile::stream(
        FileName::new(NAME).unwrap(),
        ContentType::new("text/plain").unwrap(),
        6,
        Sha1Hex::new(SHA1).unwrap(),
        stream,
    )
    .unwrap();
    assert!(!file.is_replayable());
    enqueue(&fixture.uploads, provider(503, "service_unavailable"));
    let failure = session
        .upload_file(&mut target, file, retrying())
        .await
        .unwrap_err();
    assert_eq!(delivered.load(Ordering::SeqCst), 6);
    assert!(failure.stream_consumed());
    assert!(failure.may_have_stored());
    assert_eq!(failure.stop(), StopReason::NotReplayable);
    assert_eq!(failure.report().attempts().len(), 1);
    assert_eq!(
        failure.report().attempts()[0].evidence(),
        Evidence::ServerFailed
    );
    assert_eq!(failure.report().attempts()[0].status(), Some(503));
    assert_eq!(failure.error().code(), Some(&ErrorCode::ServiceUnavailable));
    assert_meta(
        failure.error().provider().unwrap().meta(),
        503,
        "b2_upload_file",
    );
    assert!(!target.is_usable());
    assert!(target.upload_url().is_none());
    assert_redacted(&format!("{failure:?} {failure}"), &[&issued]);
    assert_eq!(
        fixture.api.requests().len(),
        1,
        "consumed stream must not trigger a futile refresh"
    );
    assert_eq!(fixture.upload.requests().len(), 2);
    assert_upload_wire(&fixture.upload.requests()[1], 1);

    let mut fresh = session.get_upload_url(&bucket).await.unwrap();
    assert_ne!(fresh.upload_url().unwrap().as_str(), issued);
    assert_eq!(
        fixture.upload.requests().len(),
        2,
        "a fresh URL sends no file"
    );
    assert!(
        failure.may_have_stored(),
        "refresh cannot resolve earlier storage"
    );
    assert!(failure.stream_consumed());
    assert!(
        failure.into_file().is_none(),
        "refresh cannot restore the consumed source"
    );

    enqueue(&fixture.uploads, provider(503, "service_unavailable"));
    enqueue(&fixture.uploads, stored());
    let retried = session
        .upload_file(&mut fresh, memory_file(), retrying())
        .await
        .unwrap();
    assert_eq!(retried.report.upload_requests(), 2);
    assert_eq!(
        retried
            .report
            .attempts()
            .iter()
            .map(|a| a.evidence())
            .collect::<Vec<_>>(),
        vec![Evidence::ServerFailed, Evidence::Stored]
    );
    assert!(retried.report.attempts()[1].fresh_upload_url());
    assert_eq!(retried.possible_duplicates(), 1);
    assert_eq!(retried.report.retry().stop, StopReason::Done);
    assert_upload_wire(&fixture.upload.requests()[2], 2);
    assert_upload_wire(&fixture.upload.requests()[3], 3);

    // A refresh failure is the last error, but the earlier write remains uncertain.
    enqueue(&fixture.uploads, provider(503, "service_unavailable"));
    enqueue(&fixture.api_overrides, provider(401, "expired_auth_token"));
    let partial = session
        .upload_file(&mut fresh, memory_file(), retrying())
        .await
        .unwrap_err();
    assert_eq!(partial.error().code(), Some(&ErrorCode::ExpiredAuthToken));
    assert_eq!(partial.error().status().unwrap().as_u16(), 401);
    assert_eq!(
        partial
            .report()
            .attempts()
            .iter()
            .map(|a| a.evidence())
            .collect::<Vec<_>>(),
        vec![Evidence::ServerFailed, Evidence::NoUploadUrl]
    );
    assert_eq!(partial.report().unresolved_earlier_attempts(), 1);
    assert_eq!(partial.report().upload_requests(), 1);
    assert!(partial.may_have_stored());
    assert!(!partial.stream_consumed());
    assert_meta(
        partial.error().provider().unwrap().meta(),
        401,
        "b2_get_upload_url",
    );
    assert!(partial.into_file().unwrap().is_replayable());
    assert_eq!(
        fixture.account.requests().len(),
        1,
        "no automatic reauthorization"
    );

    // A successful status with an unreadable result is still possibly stored.
    enqueue(
        &fixture.uploads,
        Reply::json(200, b"{broken").header("x-fixture-request-id", "request-44"),
    );
    let malformed = session
        .upload_file(&mut fresh, memory_file(), retrying())
        .await
        .unwrap_err();
    assert!(
        matches!(malformed.error(), Error::Concord(e) if e.kind() == ErrorKind::Decode && e.delivery() == Delivery::Answered)
    );
    assert_eq!(malformed.report().attempts().len(), 1);
    assert_eq!(
        malformed.report().attempts()[0].evidence(),
        Evidence::StoredUnverified
    );
    assert!(malformed.may_have_stored());
    assert_eq!(malformed.stop(), StopReason::ClassifierStop);
    assert!(malformed.into_file().is_some());
    assert!(!fresh.is_usable());

    for (request, slot) in fixture.upload.requests().iter().zip([1, 1, 2, 3, 3, 5]) {
        assert_upload_wire(request, slot);
    }
    assert_eq!(fixture.upload.requests().len(), 6);
    fixture.assert_control_wire();
    fixture.shutdown().await;
}

#[tokio::test]
async fn caller_downloads_the_uploaded_id_and_withholds_unverified_bytes() {
    let fixture = Fixture::start().await;
    let session = fixture.authorize().await;
    let mut target = session
        .get_upload_url(&BucketId::new(BUCKET).unwrap())
        .await
        .unwrap();
    enqueue(&fixture.uploads, stored());
    let uploaded = session
        .upload_file(&mut target, memory_file(), UploadPolicy::once())
        .await
        .unwrap();
    let id = &uploaded.file.file_id;

    enqueue(&fixture.downloads, downloaded());
    let verified = session.download_file_by_id(id, 1024).await.unwrap();
    assert_eq!(&verified.bytes[..], CONTENT);
    assert_eq!(verified.file.file_id, *id);
    assert_eq!(verified.file.content_sha1.as_str(), SHA1);
    assert_eq!(Sha1Hex::of(&verified.bytes).as_str(), SHA1);
    assert_eq!(verified.file.content_length, 6);
    assert_eq!(verified.file.file_name.as_deref(), Some(NAME));
    assert_eq!(verified.file.content_type.as_deref(), Some("text/plain"));
    assert_eq!(verified.file.upload_timestamp, Some(TIMESTAMP));
    assert_eq!(verified.file.info["review-note"], "verified fixture");
    assert_eq!(
        verified.file.info["src_last_modified_millis"],
        TIMESTAMP.to_string()
    );
    assert_meta(&verified.meta, 200, "b2_download_file_by_id");
    assert_redacted(&format!("{verified:?}"), &[]);

    enqueue(
        &fixture.downloads,
        download_head(FILE_ID)
            .header("x-bz-content-sha1", SHA1)
            .body(b"jello\n"),
    );
    let corrupted = session.download_file_by_id(id, 1024).await.unwrap_err();
    match &corrupted {
        Error::Checksum(e) => {
            assert!(
                matches!(e.failure(), ChecksumFailure::Mismatch { expected, actual } if expected == SHA1 && actual != SHA1)
            );
            assert_meta(e.meta(), 200, "b2_download_file_by_id");
        }
        other => panic!("expected checksum error, got {other:?}"),
    }
    assert_redacted(&format!("{corrupted:?} {corrupted}"), &["jello"]);

    for (header, expected) in [
        (None, ChecksumFailure::Missing),
        (Some("none"), ChecksumFailure::Unavailable),
        (Some("not-a-digest"), ChecksumFailure::Malformed),
    ] {
        let mut reply = download_head(FILE_ID).body(CONTENT);
        if let Some(header) = header {
            reply = reply.header("x-bz-content-sha1", header);
        }
        enqueue(&fixture.downloads, reply);
        let failure = session.download_file_by_id(id, 1024).await.unwrap_err();
        assert!(matches!(failure, Error::Checksum(ref e) if e.failure() == &expected));
    }
    enqueue(
        &fixture.downloads,
        downloaded().header("x-bz-content-sha1", SHA1),
    );
    let duplicate = session.download_file_by_id(id, 1024).await.unwrap_err();
    assert!(
        matches!(duplicate, Error::Checksum(ref e) if e.failure() == &ChecksumFailure::Missing)
    );

    enqueue(
        &fixture.downloads,
        download_head("4_another_version")
            .header("x-bz-content-sha1", SHA1)
            .body(CONTENT),
    );
    assert!(matches!(
        session.download_file_by_id(id, 1024).await.unwrap_err(),
        Error::Contract(_)
    ));
    let mut partial_status = downloaded();
    partial_status.status = 206;
    enqueue(&fixture.downloads, partial_status);
    assert!(matches!(
        session.download_file_by_id(id, 1024).await.unwrap_err(),
        Error::Contract(_)
    ));

    enqueue(&fixture.downloads, downloaded());
    let too_large = session.download_file_by_id(id, 5).await.unwrap_err();
    match too_large {
        Error::Concord(e) => {
            assert_eq!(e.kind(), ErrorKind::Limit);
            assert_eq!(e.delivery(), Delivery::Answered);
            assert_meta(&e.head_meta().unwrap(), 200, "b2_download_file_by_id");
        }
        other => panic!("expected bounded read error, got {other:?}"),
    }
    enqueue(
        &fixture.downloads,
        downloaded().with_body(Body::FailAfter(vec![Bytes::from_static(b"hel")])),
    );
    let interrupted = session.download_file_by_id(id, 1024).await.unwrap_err();
    assert!(
        matches!(interrupted, Error::Concord(ref e) if e.kind() == ErrorKind::Transport && e.head().is_some())
    );

    enqueue(&fixture.downloads, provider(404, "not_found"));
    let missing = session.download_file_by_id(id, 1024).await.unwrap_err();
    assert_eq!(missing.code(), Some(&ErrorCode::NotFound));
    assert_meta(
        missing.provider().unwrap().meta(),
        404,
        "b2_download_file_by_id",
    );
    assert_eq!(
        missing.provider().unwrap().message(),
        Some("provider_private_diagnostic")
    );
    assert_redacted(&format!("{missing:?} {missing}"), &[]);

    enqueue(&fixture.downloads, provider(403, "future_provider_code"));
    let unknown = session.download_file_by_id(id, 1024).await.unwrap_err();
    assert_eq!(
        unknown.code(),
        Some(&ErrorCode::Other("future_provider_code".into()))
    );
    enqueue(
        &fixture.downloads,
        Reply::text(503, "text/html", "<html>provider_private_diagnostic</html>")
            .header("x-fixture-request-id", "request-44"),
    );
    let proxy = session.download_file_by_id(id, 1024).await.unwrap_err();
    assert_eq!(proxy.status().unwrap().as_u16(), 503);
    assert!(proxy.code().is_none());
    assert!(proxy.provider().unwrap().unreadable().is_some());
    assert_meta(
        proxy.provider().unwrap().meta(),
        503,
        "b2_download_file_by_id",
    );
    assert_redacted(&format!("{proxy:?} {proxy}"), &[]);
    enqueue(&fixture.downloads, Reply::json(404, json!({
        "status": 401, "code": "expired_auth_token", "message": "provider_private_diagnostic"
    }).to_string()));
    let inconsistent = session.download_file_by_id(id, 1024).await.unwrap_err();
    assert_eq!(inconsistent.status().unwrap().as_u16(), 404);
    assert!(inconsistent.code().is_none());
    assert!(
        inconsistent
            .provider()
            .unwrap()
            .inconsistent_document()
            .is_some()
    );

    // Earlier verified data survives, and a subsequent complete download still works.
    assert_eq!(&verified.bytes[..], CONTENT);
    enqueue(&fixture.downloads, downloaded());
    assert_eq!(
        &session
            .download_file_by_id(id, CONTENT.len())
            .await
            .unwrap()
            .bytes[..],
        CONTENT
    );
    assert_eq!(
        fixture.download.requests().len(),
        15,
        "no hidden retry or pagination"
    );
    for request in fixture.download.requests() {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/b2api/v4/b2_download_file_by_id");
        assert_eq!(
            request.query_pairs(),
            vec![("fileId".into(), FILE_ID.into())]
        );
        assert_eq!(request.header_all("authorization"), vec![ACCOUNT_TOKEN]);
        assert!(request.header("range").is_none());
        assert!(request.body.is_empty());
    }
    assert_upload_wire(&fixture.upload.single(), 1);
    fixture.assert_control_wire();
    fixture.shutdown().await;
}
