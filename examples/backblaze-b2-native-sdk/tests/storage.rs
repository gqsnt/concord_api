//! Regression checks of page boundaries, request shape and uncertain writes.

mod storage_support;

use std::{
    collections::VecDeque,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::Duration,
};

use concord_example_testserver::{Recorded, Reply};
use concord_sdk_backblaze_b2_native::{
    BucketId, Error, ErrorCode, FileId, FileName, FileNameQuery, InputError, Violation,
    concord::{Delivery, ErrorKind, ResponseMeta},
};
use serde_json::json;
use storage_support::*;

fn cap(count: usize) -> NonZeroUsize {
    NonZeroUsize::new(count).unwrap()
}

async fn queued(replies: Vec<Reply>) -> Fixture {
    let replies = Mutex::new(VecDeque::from(replies));
    Fixture::start(move |_: &Recorded| {
        replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected automatic repetition")
    })
    .await
}

fn meta(meta: &ResponseMeta, method: &str, endpoint: &str) {
    let op = meta.operation().unwrap();
    assert_eq!(op.method(), Some(method));
    assert_eq!(op.name(), format!("B2Wire.storage.{endpoint}"));
    assert!(op.is_endpoint());
    assert_eq!(meta.headers()["x-fixture-request-id"], "storage-request");
}

#[tokio::test]
async fn traversal_keeps_filters_across_empty_pages_cap_and_resume_until_null() {
    // Both cursors are opaque and need not name an actual file. The first page is short.
    let first_cursor = "reports/résumé +&?#%/";
    let second_cursor = "reports/z/end";
    let fixture = queued(vec![
        page(vec![folder()], Some(first_cursor)),
        page(vec![], Some(second_cursor)),
        page(vec![file()], None),
    ])
    .await;
    let session = fixture.session(Duration::from_secs(3)).await;
    let mut traversal = session.storage().list_file_names(query(), cap(2));
    let first = traversal.next_page().await.unwrap().unwrap();
    assert_eq!(first.files[0].action, "folder");
    assert_eq!(first.files[0].file_id, None);
    assert_eq!(first.files[0].content_sha1, None);
    meta(&first.meta, "GET", "list_file_names");
    assert!(!traversal.is_complete());
    assert!(
        traversal
            .next_page()
            .await
            .unwrap()
            .unwrap()
            .files
            .is_empty()
    );
    assert_eq!(traversal.pages_remaining(), 0);
    assert!(traversal.next_page().await.unwrap().is_none());
    assert_eq!(fixture.api.requests().len(), 2);
    assert!(!traversal.is_complete(), "cap is not completion");
    let resume = traversal.resume_point().unwrap();
    assert_eq!(
        resume,
        FileNameQuery {
            start_file_name: Some(second_cursor.into()),
            ..query()
        }
    );
    let mut traversal = session.storage().list_file_names(resume, cap(4));
    let final_page = traversal.next_page().await.unwrap().unwrap();
    assert_eq!(final_page.files[0].file_id.as_ref().unwrap().as_str(), ID);
    assert_eq!(final_page.files[0].file_name, NAME);
    assert!(final_page.next_file_name.is_none());
    assert!(traversal.is_complete());
    assert!(traversal.resume_point().is_none());
    assert!(traversal.next_page().await.unwrap().is_none());
    assert_eq!(fixture.api.requests().len(), 3, "no request after null");
    for (request, cursor) in
        fixture
            .api
            .requests()
            .iter()
            .zip([None, Some(first_cursor), Some(second_cursor)])
    {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/issued/api/b2api/v4/b2_list_file_names");
        assert_eq!(request.query_all("bucketId"), [BUCKET]);
        assert_eq!(request.query_all("prefix"), ["reports/"]);
        assert_eq!(request.query_all("delimiter"), ["/"]);
        assert_eq!(request.query_all("maxFileCount"), ["2"]);
        assert_eq!(
            request.query_all("startFileName"),
            cursor.into_iter().collect::<Vec<_>>()
        );
        assert_eq!(
            request.query_pairs().len(),
            if cursor.is_some() { 5 } else { 4 }
        );
        assert!(request.body.is_empty());
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn default_and_zero_counts_are_sent_as_documented_and_large_counts_refused_locally() {
    let fixture = queued(vec![
        page(vec![], None),
        page(vec![], None),
        page(vec![], None),
    ])
    .await;
    let session = fixture.session(Duration::from_secs(3)).await;
    let defaults = FileNameQuery::new(BucketId::new(BUCKET).unwrap());
    for count in [None, Some(0), Some(10_000)] {
        let mut traversal = session.storage().list_file_names(
            FileNameQuery {
                max_file_count: count,
                ..defaults.clone()
            },
            cap(1),
        );
        assert!(
            traversal
                .next_page()
                .await
                .unwrap()
                .unwrap()
                .next_file_name
                .is_none()
        );
    }
    let mut invalid = session.storage().list_file_names(
        FileNameQuery {
            max_file_count: Some(10_001),
            ..defaults
        },
        cap(1),
    );
    assert!(matches!(
        invalid.next_page().await.unwrap_err(),
        Error::Input(InputError::MaxFileCount)
    ));
    assert_eq!(invalid.pages_remaining(), 1);
    assert_eq!(fixture.api.requests().len(), 3);
    for (request, expected) in fixture
        .api
        .requests()
        .iter()
        .zip([vec![], vec!["0"], vec!["10000"]])
    {
        assert_eq!(request.query_all("maxFileCount"), expected);
        for omitted in ["prefix", "delimiter", "startFileName"] {
            assert!(request.query_all(omitted).is_empty());
        }
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn failed_pages_keep_the_resume_query_and_consume_the_request_budget() {
    let mut other_bucket = file();
    other_bucket["bucketId"] = json!("different-bucket");
    let fixture = queued(vec![
        provider(503, "service_unavailable"),
        Reply::json(200, "{broken"),
        Reply::json(200, r#"{"files":[]}"#), // a missing cursor is not null
        page(vec![other_bucket], None),
        page(vec![], None),
    ])
    .await;
    let session = fixture.session(Duration::from_secs(3)).await;
    let original = FileNameQuery {
        start_file_name: Some("reports/initial".into()),
        ..query()
    };
    let mut traversal = session.storage().list_file_names(original.clone(), cap(4));
    let error = traversal.next_page().await.unwrap_err();
    assert_eq!(error.code(), Some(&ErrorCode::ServiceUnavailable));
    meta(error.provider().unwrap().meta(), "GET", "list_file_names");
    for _ in 0..2 {
        assert!(
            matches!(traversal.next_page().await.unwrap_err(), Error::Concord(e) if e.kind() == ErrorKind::Decode)
        );
    }
    assert!(
        matches!(traversal.next_page().await.unwrap_err(), Error::Contract(e) if e.violation() == Violation::ListedFile)
    );
    assert!(!traversal.is_complete());
    assert_eq!(traversal.resume_point(), Some(original.clone()));
    assert!(traversal.next_page().await.unwrap().is_none());
    assert_eq!(fixture.api.requests().len(), 4);
    let mut resumed = session
        .storage()
        .list_file_names(traversal.resume_point().unwrap(), cap(1));
    resumed.next_page().await.unwrap();
    assert!(resumed.is_complete());
    for request in fixture.api.requests() {
        assert_eq!(request.query_all("startFileName"), ["reports/initial"]);
        assert_eq!(request.query_all("prefix"), ["reports/"]);
        assert_eq!(request.query_all("delimiter"), ["/"]);
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn newly_documented_listing_errors_remain_typed_without_automatic_retry() {
    let cases = [
        (400, ErrorCode::InvalidBucketId),
        (400, ErrorCode::OutOfRange),
        (429, ErrorCode::TooManyRequests),
        (401, ErrorCode::ExpiredAuthToken),
    ];
    let fixture = queued(
        cases
            .iter()
            .map(|(status, code)| provider(*status, code.as_str()))
            .collect(),
    )
    .await;
    let session = fixture.session(Duration::from_secs(3)).await;
    for (status, code) in &cases {
        let mut traversal = session.storage().list_file_names(query(), cap(3));
        let error = traversal.next_page().await.unwrap_err();
        assert_eq!(error.code(), Some(code));
        assert_eq!(error.status().unwrap().as_u16(), *status);
        assert_eq!(
            error.provider().unwrap().code().unwrap().as_str(),
            code.as_str()
        );
        assert_eq!(traversal.resume_point(), Some(query()));
    }
    assert_eq!(fixture.api.requests().len(), cases.len());
    fixture.shutdown().await;
}

#[tokio::test]
async fn delete_sends_exact_json_name_and_id_and_types_a_mismatch_as_provider_refusal() {
    let fixture = queued(vec![
        deleted(),
        provider(400, "file_not_present"),
        provider(401, "access_denied"),
    ])
    .await;
    let session = fixture.session(Duration::from_secs(3)).await;
    let name = FileName::new(NAME).unwrap();
    let id = FileId::new(ID).unwrap();
    let success = session
        .storage()
        .delete_file_version(&name, &id)
        .await
        .unwrap();
    assert_eq!(success.file_name, name);
    assert_eq!(success.file_id, id);
    meta(&success.meta, "POST", "delete_file_version");
    let error = session
        .storage()
        .delete_file_version(&name, &FileId::new("wrong-id").unwrap())
        .await
        .unwrap_err();
    assert_eq!(error.error().code(), Some(&ErrorCode::FileNotPresent));
    assert_eq!(error.error().status().unwrap().as_u16(), 400);
    assert!(!error.may_have_deleted());
    meta(
        error.error().provider().unwrap().meta(),
        "POST",
        "delete_file_version",
    );
    let error = session
        .storage()
        .delete_file_version(&name, &id)
        .await
        .unwrap_err();
    assert_eq!(error.error().code(), Some(&ErrorCode::AccessDenied));
    assert!(!error.may_have_deleted());
    let rendered = format!("{error:?} {error}");
    assert!(!rendered.contains(TOKEN));
    assert!(!rendered.contains("private provider message"));
    assert_eq!(fixture.api.requests().len(), 3);
    for (request, expected_id) in fixture.api.requests().iter().zip([ID, "wrong-id", ID]) {
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.target,
            "/issued/api/b2api/v4/b2_delete_file_version"
        );
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&request.body).unwrap(),
            json!({"fileName": NAME, "fileId": expected_id})
        );
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn a_timed_out_delete_can_have_committed_or_not_and_is_never_resent() {
    for commits in [true, false] {
        let exists = Arc::new(Mutex::new(true));
        let state = exists.clone();
        let fixture = Fixture::start(move |request: &Recorded| {
            if request.path().ends_with("b2_delete_file_version") {
                if commits {
                    *state.lock().unwrap() = false;
                }
                deleted().delay(Duration::from_secs(2))
            } else {
                assert!(request.path().ends_with("b2_list_file_names"));
                page(
                    if *state.lock().unwrap() {
                        vec![file()]
                    } else {
                        vec![]
                    },
                    None,
                )
            }
        })
        .await;
        let session = fixture.session(Duration::from_millis(200)).await;
        let error = session
            .storage()
            .delete_file_version(&FileName::new(NAME).unwrap(), &FileId::new(ID).unwrap())
            .await
            .unwrap_err();
        assert!(error.may_have_deleted());
        assert!(
            matches!(error.error(), Error::Concord(e) if e.kind() == ErrorKind::Transport && e.delivery() == Delivery::Unknown)
        );
        assert_eq!(*exists.lock().unwrap(), !commits);
        let mut check = session.storage().list_file_names(query(), cap(1));
        let found = check.next_page().await.unwrap().unwrap();
        assert_eq!(found.files.is_empty(), commits);
        assert!(check.is_complete());
        assert_eq!(
            fixture.api.requests().len(),
            2,
            "one delete, then caller's explicit list"
        );
        assert_eq!(
            fixture
                .api
                .requests()
                .iter()
                .filter(|r| r.method == "POST")
                .count(),
            1
        );
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn unconfirmed_deletes_preserve_uncertainty_and_do_not_repeat() {
    let fixture = queued(vec![
        Reply::hang_up(),
        provider(503, "service_unavailable"),
        provider(408, "request_timeout"),
        Reply::json(200, "{broken"),
        Reply::json(
            200,
            json!({"fileName": NAME, "fileId": "another-id"}).to_string(),
        ),
    ])
    .await;
    let session = fixture.session(Duration::from_secs(3)).await;
    for n in 1..=5 {
        let error = session
            .storage()
            .delete_file_version(&FileName::new(NAME).unwrap(), &FileId::new(ID).unwrap())
            .await
            .unwrap_err();
        assert!(error.may_have_deleted());
        assert_eq!(fixture.api.requests().len(), n);
        if n == 5 {
            assert!(
                matches!(error.error(), Error::Contract(e) if e.violation() == Violation::DeletedFileVersion)
            );
        }
    }
    fixture.shutdown().await;
}
