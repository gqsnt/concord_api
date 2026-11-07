//! The two connected workflows end to end against the local stand-in, and the required failure:
//! a new upload URL alone can neither replay a consumed stream nor prove that an earlier
//! ambiguous upload stored nothing.

mod common;

use std::convert::Infallible;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll};

use common::*;
use concord_sdk_backblaze_b2_native::concord::bytes::Bytes;
use concord_sdk_backblaze_b2_native::retry::StopReason;
use concord_sdk_backblaze_b2_native::{
    ChecksumFailure, ContentType, Error, ErrorCode, Evidence, FileName, Sha1Hex, UploadFile,
    UploadPolicy,
};

const MIB: usize = 1024 * 1024;

fn hello_file() -> UploadFile {
    UploadFile::bytes(
        FileName::new("hello-b2.txt").unwrap(),
        ContentType::new("text/plain").unwrap(),
        hello(),
    )
}

/// A one-shot source: `chunks` once, then the end. Not `Clone`.
struct OneShot(std::vec::IntoIter<Bytes>);

impl futures_core::Stream for OneShot {
    type Item = Result<Bytes, Infallible>;
    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.get_mut().0.next().map(Ok))
    }
}

fn hello_stream() -> UploadFile {
    let bytes = hello();
    let (a, b) = bytes.split_at(20);
    let source = OneShot(vec![Bytes::copy_from_slice(a), Bytes::copy_from_slice(b)].into_iter());
    UploadFile::stream(
        FileName::new("hello-b2.txt").unwrap(),
        ContentType::new("text/plain").unwrap(),
        bytes.len() as u64,
        Sha1Hex::new(&hello_sha1()).unwrap(),
        source,
    )
    .unwrap()
}

fn retrying(n: usize) -> UploadPolicy {
    UploadPolicy::retrying(NonZeroUsize::new(n).unwrap())
}

#[tokio::test]
async fn authorize_upload_with_length_and_sha1_then_download_and_verify() {
    let b2 = FakeB2::start().await;
    let session = b2.client().authorize_account(&key()).await.unwrap();
    assert_eq!(
        session.account().api_url().as_str(),
        format!("{}/", b2.api.url())
    );
    assert_eq!(
        session.account().download_url().as_str(),
        format!("{}/", b2.download.url())
    );
    assert!(session.account().allowed().has_capability("writeFiles"));

    // Workflow 1: an upload URL with its own token, then one upload with length and SHA-1.
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let uploaded = session
        .upload_file(&mut target, hello_file(), UploadPolicy::once())
        .await
        .unwrap();
    let facts = hello_facts();
    assert_eq!(
        uploaded.file.content_sha1.as_str(),
        facts["sha1"].as_str().unwrap()
    );
    assert_eq!(
        uploaded.file.content_length,
        facts["length"].as_u64().unwrap()
    );
    assert_eq!(uploaded.file.file_name, "hello-b2.txt");
    assert_eq!(uploaded.file.bucket_id.as_str(), BUCKET);
    assert_eq!(uploaded.possible_duplicates(), 0);
    let [attempt] = uploaded.report.attempts() else {
        panic!()
    };
    assert_eq!(attempt.evidence(), Evidence::Stored);
    assert!(!attempt.fresh_upload_url());
    assert!(target.is_usable(), "a stored upload keeps the URL usable");

    // On the wire: each operation went to the host issued for it, with its own credential.
    let auth = b2.auth.single();
    assert_eq!(
        auth.header("authorization"),
        credentials()["authorization"].as_str()
    );
    let api = b2.api.single();
    assert_eq!(api.header("authorization"), Some(ACCOUNT_TOKEN));
    let upload = b2.upload.single();
    assert_eq!(
        upload.header("authorization"),
        Some("2_upload_token_1_upld")
    );
    assert_eq!(upload.header("x-bz-content-sha1"), facts["sha1"].as_str());
    assert_eq!(upload.header("content-length"), Some("63"));
    assert_eq!(upload.body, hello());

    // Workflow 2: download by ID and verify the returned checksum.
    let download = session
        .download_file_by_id(&uploaded.file.file_id, MIB)
        .await
        .unwrap();
    assert_eq!(&download.bytes[..], &hello()[..]);
    assert_eq!(
        download.file.content_sha1.as_str(),
        facts["sha1"].as_str().unwrap()
    );
    assert_eq!(download.file.file_id, uploaded.file.file_id);
    assert_eq!(download.file.file_name.as_deref(), Some("hello-b2.txt"));
    assert_eq!(download.file.upload_timestamp, Some(1_559_585_435_000));
    let get = b2.download.single();
    assert_eq!(get.header("authorization"), Some(ACCOUNT_TOKEN));
    assert_eq!(get.query_all("fileId"), [uploaded.file.file_id.as_str()]);
}

#[tokio::test]
async fn a_one_shot_stream_with_info_uploads_and_downloads() {
    let b2 = FakeB2::start().await;
    b2.state.lock().unwrap().path_form = true;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let facts = hello_facts();
    let name = facts["encodedNameInput"].as_str().unwrap();
    let file = UploadFile::stream(
        FileName::new(name).unwrap(),
        ContentType::auto(),
        63,
        Sha1Hex::new(&hello_sha1().to_uppercase()).unwrap(),
        OneShot(vec![Bytes::from(hello())].into_iter()),
    )
    .unwrap()
    .with_last_modified_millis(1_700_000_000_000);
    let uploaded = session
        .upload_file(&mut target, file, UploadPolicy::once())
        .await
        .unwrap();
    assert_eq!(uploaded.file.file_name, name);
    let upload = b2.upload.single();
    assert_eq!(
        upload.header("x-bz-file-name"),
        facts["encodedNameHeader"].as_str()
    );
    assert_eq!(upload.header("content-type"), Some("b2/x-auto"));
    assert!(!upload.chunked);

    let download = session
        .download_file_by_id(&uploaded.file.file_id, MIB)
        .await
        .unwrap();
    assert_eq!(download.file.file_name.as_deref(), Some(name));
    assert_eq!(
        download
            .file
            .info
            .get("src_last_modified_millis")
            .map(String::as_str),
        Some("1700000000000")
    );
}

#[tokio::test]
async fn one_upload_url_serves_consecutive_uploads() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    for _ in 0..3 {
        session
            .upload_file(&mut target, hello_file(), UploadPolicy::once())
            .await
            .unwrap();
    }
    assert_eq!(b2.get_upload_url_requests(), 1);
    assert_eq!(b2.upload_requests().len(), 3);
    assert_eq!(b2.stored_ids().len(), 3, "each upload is its own version");
}

#[tokio::test]
async fn a_download_whose_bytes_do_not_match_the_returned_sha1_is_refused() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let uploaded = session
        .upload_file(&mut target, hello_file(), UploadPolicy::once())
        .await
        .unwrap();
    b2.set_download(DownloadMode::Tampered);
    let error = session
        .download_file_by_id(&uploaded.file.file_id, MIB)
        .await
        .unwrap_err();
    let Error::Checksum(checksum) = error else {
        panic!("{error:?}")
    };
    let facts = hello_facts();
    assert_eq!(
        checksum.failure(),
        &ChecksumFailure::Mismatch {
            expected: facts["sha1"].as_str().unwrap().to_owned(),
            actual: facts["tamperedSha1"].as_str().unwrap().to_owned(),
        }
    );
}

// --- The required failure ---------------------------------------------------------------------

#[tokio::test]
async fn a_new_upload_url_cannot_replay_a_consumed_stream() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    b2.push_upload(UploadMode::Fail(503, "service_unavailable"));

    // The caller permits replay and three attempts; a one-shot stream still gets one.
    let error = session
        .upload_file(&mut target, hello_stream(), retrying(3))
        .await
        .unwrap_err();
    assert_eq!(error.stop(), StopReason::NotReplayable);
    assert!(error.stream_consumed());
    assert!(
        error.may_have_stored(),
        "a 503 does not prove that nothing was stored"
    );
    assert_eq!(error.error().code(), Some(&ErrorCode::ServiceUnavailable));
    let [attempt] = error.report().attempts() else {
        panic!("{error:?}")
    };
    assert_eq!(attempt.evidence(), Evidence::ServerFailed);
    assert!(!target.is_usable(), "the rejected upload URL is retired");
    assert!(error.into_file().is_none(), "the source is spent");
    assert_eq!(b2.upload_requests().len(), 1);
    assert_eq!(
        b2.get_upload_url_requests(),
        1,
        "no new URL was fetched for nothing to send"
    );

    // A new upload URL is available, but it has nothing to send: the caller needs a new source.
    let fresh = session.get_upload_url(&bucket()).await.unwrap();
    assert!(fresh.is_usable());
    assert_eq!(b2.upload_requests().len(), 1);
}

#[tokio::test]
async fn a_consumed_stream_whose_answer_was_lost_may_have_been_stored() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    // The whole body arrives and is stored; the answer never does.
    b2.push_upload(UploadMode::StoreThenHangUp);

    let error = session
        .upload_file(&mut target, hello_stream(), retrying(3))
        .await
        .unwrap_err();
    let [attempt] = error.report().attempts() else {
        panic!("{error:?}")
    };
    assert_eq!(
        attempt.evidence(),
        Evidence::NoAnswer,
        "not a source failure"
    );
    assert!(error.may_have_stored());
    assert!(error.stream_consumed());
    assert_eq!(error.stop(), StopReason::NotReplayable);
    assert_eq!(b2.stored_ids().len(), 1, "it was in fact stored");
    assert_eq!(b2.get_upload_url_requests(), 1);
}

#[tokio::test]
async fn a_new_upload_url_does_not_prove_an_ambiguous_upload_failed() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    // The first attempt is stored, but the answer never arrives.
    b2.push_upload(UploadMode::StoreThenHangUp);

    let uploaded = session
        .upload_file(&mut target, hello_file(), retrying(2))
        .await
        .unwrap();
    let evidence: Vec<_> = uploaded
        .report
        .attempts()
        .iter()
        .map(|a| a.evidence())
        .collect();
    assert_eq!(evidence, [Evidence::NoAnswer, Evidence::Stored]);
    assert!(uploaded.report.attempts()[1].fresh_upload_url());
    assert_eq!(uploaded.possible_duplicates(), 1);

    // Two versions exist: the "ambiguous" one did succeed, and the SDK never claimed otherwise.
    let stored = b2.stored_ids();
    assert_eq!(stored.len(), 2);
    assert_eq!(uploaded.file.file_id.as_str(), stored[1]);
    let first = concord_sdk_backblaze_b2_native::FileId::new(stored[0].clone()).unwrap();
    let earlier = session.download_file_by_id(&first, MIB).await.unwrap();
    assert_eq!(&earlier.bytes[..], &hello()[..]);

    // Each attempt went to its own upload URL with its own token.
    let uploads = b2.upload_requests();
    assert_eq!(uploads.len(), 2);
    assert_ne!(uploads[0].target, uploads[1].target);
    assert_ne!(
        uploads[0].header("authorization"),
        uploads[1].header("authorization")
    );
}

#[tokio::test]
async fn without_replay_permission_an_ambiguous_upload_is_reported_not_resent() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    b2.push_upload(UploadMode::StoreThenHangUp);

    let error = session
        .upload_file(&mut target, hello_file(), UploadPolicy::once())
        .await
        .unwrap_err();
    assert_eq!(error.stop(), StopReason::NotReplayable);
    assert!(error.may_have_stored());
    assert!(!error.stream_consumed());
    assert!(
        matches!(error.error(), Error::Concord(e) if e.delivery() == concord_sdk_backblaze_b2_native::concord::Delivery::Unknown)
    );
    assert_eq!(b2.upload_requests().len(), 1);
    assert_eq!(
        b2.stored_ids().len(),
        1,
        "it was stored; the SDK says only that it may have been"
    );
    assert!(!target.is_usable());
    assert!(
        error.into_file().is_some(),
        "buffered bytes can still be sent, by the caller's choice"
    );
}

#[tokio::test]
async fn a_503_with_replay_permitted_retries_at_a_new_upload_url() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    b2.push_upload(UploadMode::Fail(503, "service_unavailable"));

    let uploaded = session
        .upload_file(&mut target, hello_file(), retrying(3))
        .await
        .unwrap();
    let evidence: Vec<_> = uploaded
        .report
        .attempts()
        .iter()
        .map(|a| a.evidence())
        .collect();
    assert_eq!(evidence, [Evidence::ServerFailed, Evidence::Stored]);
    assert_eq!(
        uploaded.report.attempts()[0].code(),
        Some(&ErrorCode::ServiceUnavailable)
    );
    assert_eq!(
        uploaded.possible_duplicates(),
        1,
        "the 503 attempt stays unresolved"
    );
    assert_eq!(b2.get_upload_url_requests(), 2);
    assert!(target.is_usable());
}

#[tokio::test]
async fn an_expired_upload_token_is_refused_and_retried_without_duplicates() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    b2.push_upload(UploadMode::Fail(401, "expired_auth_token"));

    let uploaded = session
        .upload_file(&mut target, hello_file(), retrying(2))
        .await
        .unwrap();
    let evidence: Vec<_> = uploaded
        .report
        .attempts()
        .iter()
        .map(|a| a.evidence())
        .collect();
    assert_eq!(evidence, [Evidence::Refused, Evidence::Stored]);
    assert_eq!(uploaded.possible_duplicates(), 0);
}

#[tokio::test]
async fn a_refusal_that_is_not_retryable_stops_at_once() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    b2.push_upload(UploadMode::Fail(401, "unauthorized"));

    let error = session
        .upload_file(&mut target, hello_file(), retrying(3))
        .await
        .unwrap_err();
    assert_eq!(error.stop(), StopReason::ClassifierStop);
    assert!(!error.may_have_stored());
    assert_eq!(b2.upload_requests().len(), 1);
}

#[tokio::test]
async fn a_stored_but_unverified_answer_is_never_retried() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    // A 200 whose document names another SHA-1 than the one sent.
    b2.push_upload(UploadMode::Reply(Box::new(|_| {
        let mut doc = fixture_json("upload_file.json");
        doc["contentSha1"] =
            serde_json::Value::String(hello_facts()["tamperedSha1"].as_str().unwrap().into());
        concord_example_testserver::Reply::json(200, doc.to_string())
    })));

    let error = session
        .upload_file(&mut target, hello_file(), retrying(3))
        .await
        .unwrap_err();
    let [attempt] = error.report().attempts() else {
        panic!("{error:?}")
    };
    assert_eq!(attempt.evidence(), Evidence::StoredUnverified);
    assert!(error.may_have_stored());
    assert_eq!(error.stop(), StopReason::ClassifierStop);
    assert_eq!(b2.upload_requests().len(), 1);
}

#[tokio::test]
async fn a_stream_never_handed_to_the_transport_comes_back_and_can_be_sent() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    // Retire the target with a refused upload.
    b2.push_upload(UploadMode::Fail(400, "bad_request"));
    session
        .upload_file(&mut target, hello_file(), UploadPolicy::once())
        .await
        .unwrap_err();
    assert!(!target.is_usable());

    // The next upload first needs a new URL, and b2_get_upload_url is unavailable.
    b2.push_get_upload_url(error_reply(503, "service_unavailable"));
    let error = session
        .upload_file(&mut target, hello_stream(), UploadPolicy::once())
        .await
        .unwrap_err();
    let [attempt] = error.report().attempts() else {
        panic!("{error:?}")
    };
    assert_eq!(attempt.evidence(), Evidence::NoUploadUrl);
    assert!(!error.stream_consumed());
    assert!(!error.may_have_stored());
    let file = error.into_file().expect("the untouched stream is returned");

    let uploaded = session
        .upload_file(&mut target, file, UploadPolicy::once())
        .await
        .unwrap();
    assert!(uploaded.report.attempts()[0].fresh_upload_url());
    assert_eq!(b2.upload_requests().len(), 2);
}

#[tokio::test]
async fn a_source_shorter_than_declared_fails_before_the_body_completes() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let file = UploadFile::stream(
        FileName::new("hello-b2.txt").unwrap(),
        ContentType::new("text/plain").unwrap(),
        63,
        Sha1Hex::new(&hello_sha1()).unwrap(),
        OneShot(vec![Bytes::from_static(b"only ten b")].into_iter()),
    )
    .unwrap();
    let error = session
        .upload_file(&mut target, file, retrying(3))
        .await
        .unwrap_err();
    let [attempt] = error.report().attempts() else {
        panic!("{error:?}")
    };
    assert_eq!(attempt.evidence(), Evidence::SourceFailed);
    assert!(
        !error.may_have_stored(),
        "the declared Content-Length was never reached"
    );
    assert!(error.stream_consumed());
    assert!(b2.stored_ids().is_empty());
}

#[tokio::test]
async fn an_attempt_abandoned_at_the_deadline_may_have_stored() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    // The upload host has the whole request but answers too late.
    b2.push_upload(UploadMode::Reply(Box::new(|_| {
        concord_example_testserver::Reply::json(200, fixture("upload_file.json"))
            .delay(std::time::Duration::from_secs(5))
    })));
    let mut policy = retrying(3);
    policy.retry.timeout = Some(std::time::Duration::from_millis(200));

    let error = session
        .upload_file(&mut target, hello_file(), policy)
        .await
        .unwrap_err();
    assert!(matches!(error.error(), Error::Deadline), "{error:?}");
    let [attempt] = error.report().attempts() else {
        panic!("{error:?}")
    };
    assert_eq!(attempt.evidence(), Evidence::Abandoned);
    assert_eq!(error.stop(), StopReason::Deadline);
    assert!(error.may_have_stored());
    assert!(
        !target.is_usable(),
        "an abandoned attempt's URL is not reused"
    );
    assert!(
        b2.upload
            .wait_until(std::time::Duration::from_secs(2), |s| s.requests().len()
                == 1)
            .await,
        "the request reached the upload host before it was abandoned"
    );
}
