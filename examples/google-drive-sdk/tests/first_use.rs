//! Application-style offline first use of the public Google Drive SDK.

use concord_example_testserver::{Body, Reply, Script, TestServer};
use concord_sdk_google_drive::{
    CHUNK_ALIGNMENT, ChangesOptions, ClientConfig, DriveClient, ErrorKind, FileField, FileFields,
    ListOptions, NewFile, UploadCheckpoint, UploadStatus,
    pagination::{PagesError, Resume},
};
use std::num::NonZeroUsize;

const TOKEN: &str = "SYNTHETIC_TEST_BEARER_DO_NOT_USE";
const MEDIA: &[u8] = include_bytes!("../fixtures/first-use-media.bin");

fn client(server: &TestServer) -> DriveClient {
    DriveClient::with_config(
        TOKEN,
        ClientConfig {
            origin: server.url(),
            ..ClientConfig::default()
        },
    )
    .unwrap()
}
fn budget(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}
fn fields() -> FileFields {
    FileFields::only(&[FileField::Name, FileField::MimeType, FileField::Size])
}
fn named(name: &str) -> NewFile {
    NewFile {
        name: name.into(),
        ..NewFile::default()
    }
}
fn authenticated(requests: &[concord_example_testserver::Recorded]) {
    for request in requests {
        assert_eq!(
            request.header("authorization"),
            Some("Bearer SYNTHETIC_TEST_BEARER_DO_NOT_USE")
        );
    }
}

#[tokio::test]
async fn list_inspect_upload_and_retrieve() {
    let script = Script::new([
        Reply::json(
            200,
            br#"{"files":[{"id":"old","name":"old.txt"}],"nextPageToken":"p+2"}"#,
        ),
        Reply::json(200, br#"{"files":[{"id":"other","name":"other.txt"}]}"#),
        Reply::json(
            200,
            br#"{"id":"old","name":"old.txt","mimeType":"text/plain"}"#,
        ),
        Reply::json(200, r#"{"id":"created","name":"café.bin","size":"29"}"#),
        Reply::json(200, r#"{"id":"created","name":"café.bin","size":"29"}"#),
    ]);
    let replies = script.clone();
    let server = TestServer::start(move |_| replies.next()).await;
    let drive = client(&server);
    let options = ListOptions {
        fields: fields(),
        query: Some("trashed = false".into()),
        ..ListOptions::default()
    };
    let files = drive
        .files(options, Resume::Start, budget(5))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        files.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
        ["old", "other"]
    );
    let first = drive.get_file(&files[0].id, &fields()).await.unwrap();
    assert_eq!(first.value.name.as_deref(), Some("old.txt"));
    let created = drive
        .upload_multipart(
            named("café.bin"),
            "application/octet-stream",
            MEDIA,
            &fields(),
        )
        .await
        .unwrap();
    assert_eq!(created.value.id, "created");
    assert_eq!(created.meta.status().as_u16(), 200);
    let fetched = drive.get_file(&created.value.id, &fields()).await.unwrap();
    assert_eq!(fetched.value.name.as_deref(), Some("café.bin"));
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    authenticated(&requests);
    for r in &requests[..2] {
        assert_eq!((r.method.as_str(), r.path()), ("GET", "/drive/v3/files"));
        assert_eq!(r.query_all("spaces"), ["drive"]);
        assert_eq!(r.query_all("q"), ["trashed = false"]);
        let f = r.query_all("fields");
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("nextPageToken"));
        assert!(f[0].contains("id"));
    }
    assert_eq!(requests[1].query_all("pageToken"), ["p+2"]);
    assert_eq!(
        (requests[2].method.as_str(), requests[2].path()),
        ("GET", "/drive/v3/files/old")
    );
    assert_eq!(
        (requests[4].method.as_str(), requests[4].path()),
        ("GET", "/drive/v3/files/created")
    );
    let upload = &requests[3];
    assert_eq!(
        (upload.method.as_str(), upload.path()),
        ("POST", "/upload/drive/v3/files")
    );
    assert_eq!(upload.query_all("uploadType"), ["multipart"]);
    let ct = upload.header("content-type").unwrap();
    let boundary = ct
        .strip_prefix("multipart/related; boundary=")
        .unwrap()
        .trim_matches('"');
    assert!(
        !MEDIA
            .windows(boundary.len())
            .any(|w| w == boundary.as_bytes())
    );
    let body = &upload.body;
    let separator = format!("\r\n--{boundary}\r\n");
    let split = body
        .windows(separator.len())
        .position(|w| w == separator.as_bytes())
        .unwrap();
    let meta = &body[..split];
    assert!(meta.starts_with(format!("--{boundary}\r\n").as_bytes()));
    assert!(String::from_utf8_lossy(meta).contains("application/json"));
    let metadata_start = meta.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let metadata: serde_json::Value = serde_json::from_slice(&meta[metadata_start..]).unwrap();
    assert_eq!(metadata["name"], "café.bin");
    let part = &body[split + separator.len()..];
    let expected = [
        b"Content-Type: application/octet-stream\r\n\r\n".as_slice(),
        MEDIA,
        format!("\r\n--{boundary}--\r\n").as_bytes(),
    ]
    .concat();
    assert_eq!(part, expected);
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_list_page_keeps_prefix_and_provider_error() {
    let script = Script::new([
        Reply::json(200, br#"{"files":[{"id":"kept"}],"nextPageToken":"next"}"#),
        Reply::json(403, br#"{"error":{"code":403,"message":"Synthetic denial","errors":[{"reason":"rateLimitExceeded","message":"Synthetic denial"}]}}"#),
    ]);
    let replies = script.clone();
    let server = TestServer::start(move |_| replies.next()).await;
    let drive = client(&server);
    let result = drive
        .files(ListOptions::default(), Resume::Start, budget(5))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(result.collected.len(), 1);
    assert_eq!(result.collected[0].id, "kept");
    match result.error {
        PagesError::Provider { error, failed, .. } => {
            assert!(matches!(failed, Resume::At(token) if token.as_str() == "next"));
            let provider = error.provider().unwrap();
            assert_eq!(provider.code, 403);
            assert_eq!(provider.message, "Synthetic denial");
            assert_eq!(provider.errors[0].reason, "rateLimitExceeded");
            assert_eq!(error.meta().unwrap().status().as_u16(), 403);
        }
        other => panic!("unexpected error {other:?}"),
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].query_all("pageToken"), ["next"]);
    authenticated(&requests);
    server.shutdown().await;
}

#[tokio::test]
async fn resume_interrupted_upload_and_poll_changes() {
    use std::sync::{Arc, Mutex};
    let count = Arc::new(Mutex::new(0));
    let state = count.clone();
    let server = TestServer::start(move |r| {
        let mut n = state.lock().unwrap();
        *n += 1;
        match *n {
            1 => Reply::json(200, br#"{"startPageToken":"start"}"#),
            2 => Reply::new(200).header(
                "Location",
                format!(
                    "http://{}/upload/drive/v3/files?upload_id=synthetic",
                    r.host
                ),
            ),
            3 => Reply::new(308).header("Range", "bytes=0-262143"),
            4 => Reply::hang_up(),
            5 => Reply::new(308).header("Range", "bytes=0-524287"),
            6 => Reply::json(201, br#"{"id":"uploaded","name":"resumed.bin"}"#),
            7 => Reply::json(
                200,
                br#"{"changes":[{"fileId":"uploaded","removed":false}],"nextPageToken":"more"}"#,
            ),
            8 => Reply::json(
                200,
                br#"{"changes":[{"fileId":"gone","removed":true}],"newStartPageToken":"later"}"#,
            ),
            _ => panic!("unexpected request"),
        }
    })
    .await;
    let drive = client(&server);
    let start = drive
        .start_page_token()
        .await
        .unwrap()
        .value
        .start_page_token;
    let source = vec![0x5a; CHUNK_ALIGNMENT * 2 + 7];
    let mut session = drive
        .initiate_upload(
            named("resumed.bin"),
            "application/octet-stream",
            source.len() as u64,
            &fields(),
        )
        .await
        .unwrap()
        .value;
    assert!(
        matches!(session.send_chunk(&source[..CHUNK_ALIGNMENT]).await.unwrap(), UploadStatus::Incomplete { acknowledged, .. } if acknowledged == CHUNK_ALIGNMENT as u64)
    );
    assert!(
        session
            .send_chunk(&source[CHUNK_ALIGNMENT..CHUNK_ALIGNMENT * 2])
            .await
            .is_err()
    );
    assert!(session.needs_probe());
    let count_after_error = server.requests().len();
    assert!(
        session
            .send_chunk(&source[CHUNK_ALIGNMENT..])
            .await
            .is_err()
    );
    assert_eq!(server.requests().len(), count_after_error);
    let saved = serde_json::to_vec(&session.checkpoint()).unwrap();
    let checkpoint: UploadCheckpoint = serde_json::from_slice(&saved).unwrap();
    let mut session = drive.restore_upload(checkpoint).unwrap();
    assert!(session.needs_probe());
    assert!(
        matches!(session.probe().await.unwrap(), UploadStatus::Incomplete { acknowledged, .. } if acknowledged == (CHUNK_ALIGNMENT * 2) as u64)
    );
    let offset = session.acknowledged() as usize;
    let done = session.send_chunk(&source[offset..]).await.unwrap();
    match done {
        UploadStatus::Complete(response) => assert_eq!(response.value.id, "uploaded"),
        _ => panic!("expected completion"),
    }
    assert!(session.is_complete());
    let pages = drive
        .changes(start, ChangesOptions::default(), budget(5))
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].items[0].file_id, "uploaded");
    assert!(pages[1].items[0].removed);
    assert_eq!(
        pages[1]
            .meta
            .new_start_page_token
            .as_ref()
            .unwrap()
            .as_str(),
        "later"
    );
    let req = server.requests();
    assert_eq!(req.len(), 8);
    authenticated(&req);
    assert_eq!(
        (req[0].method.as_str(), req[0].path()),
        ("GET", "/drive/v3/changes/startPageToken")
    );
    assert_eq!(
        (req[1].method.as_str(), req[1].path()),
        ("POST", "/upload/drive/v3/files")
    );
    assert_eq!(req[1].query_all("uploadType"), ["resumable"]);
    assert_eq!(
        req[1].header("x-upload-content-length"),
        Some(source.len().to_string().as_str())
    );
    assert_eq!(
        req[1].header("x-upload-content-type"),
        Some("application/octet-stream")
    );
    for r in &req[2..6] {
        assert_eq!(
            (r.method.as_str(), r.path()),
            ("PUT", "/upload/drive/v3/files")
        );
        assert_eq!(r.query_all("upload_id"), ["synthetic"]);
    }
    assert_eq!(req[2].body, source[..CHUNK_ALIGNMENT]);
    assert_eq!(req[3].body, source[CHUNK_ALIGNMENT..CHUNK_ALIGNMENT * 2]);
    assert_eq!(
        req[2].header("content-range"),
        Some(format!("bytes 0-262143/{}", source.len()).as_str())
    );
    assert_eq!(
        req[3].header("content-range"),
        Some(format!("bytes 262144-524287/{}", source.len()).as_str())
    );
    assert_eq!(
        req[4].header("content-range"),
        Some(format!("bytes */{}", source.len()).as_str())
    );
    assert!(req[4].body.is_empty());
    assert_eq!(
        req[5].header("content-range"),
        Some(format!("bytes 524288-524294/{}", source.len()).as_str())
    );
    assert_eq!(req[5].body, source[offset..]);
    assert_eq!(req[6].path(), "/drive/v3/changes");
    assert_eq!(req[6].query_all("pageToken"), ["start"]);
    assert_eq!(req[7].query_all("pageToken"), ["more"]);
    server.shutdown().await;
}

#[tokio::test]
async fn truncated_changes_page_preserves_previous_page_and_never_exhausts() {
    let script = Script::new([
        Reply::json(
            200,
            br#"{"changes":[{"fileId":"kept","removed":false}],"nextPageToken":"second"}"#,
        ),
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![br#"{"changes":["#.as_slice().into()])),
    ]);
    let replies = script.clone();
    let server = TestServer::start(move |_| replies.next()).await;
    let drive = client(&server);
    let start = concord_sdk_google_drive::PageToken::new("start").unwrap();
    let mut pages = drive.changes(start, ChangesOptions::default(), budget(4));
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].file_id, "kept");
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Provider { .. })
    ));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].query_all("pageToken"), ["second"]);
    authenticated(&requests);
    server.shutdown().await;
}

#[tokio::test]
async fn expired_session_is_distinct() {
    let server = TestServer::start(move |r| {
        if r.method == "POST" {
            Reply::new(200).header(
                "Location",
                format!("http://{}/upload/drive/v3/files?upload_id=expired", r.host),
            )
        } else {
            Reply::json(
                404,
                br#"{"error":{"code":404,"message":"Synthetic expired session"}}"#,
            )
        }
    })
    .await;
    let drive = client(&server);
    let mut session = drive
        .initiate_upload(named("x"), "application/octet-stream", 1, &fields())
        .await
        .unwrap()
        .value;
    let error = session.probe().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::SessionExpired);
    assert!(error.meta().is_some());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].header("content-range"), Some("bytes */1"));
    authenticated(&requests);
    server.shutdown().await;
}
