//! Maintenance regressions using only the public SDK and an owned loopback server.

mod support;

use std::sync::{Arc, Mutex};

use concord_example_testserver::{Reply, TestServer};
use concord_sdk_google_drive::{
    CHUNK_ALIGNMENT, ChangesOptions, NewFile, PageToken, UploadCheckpoint, UploadStatus,
    pagination::PagesError,
};
use serde_json::json;
use support::{RATE_LIMIT, auth, cap, client, fields};

// Deliberately decomposed e + combining acute; normalization would change the name.
const NAME: &str = "cafe\u{301}-東京-🛰-concord-boundary-0.bin";
const METADATA: &[u8] = include_bytes!("../fixtures/maintenance-new-file.json");
const MEDIA: &[u8] = include_bytes!("../fixtures/maintenance-media.bin");
const MULTIPART: &[u8] = include_bytes!("../fixtures/maintenance-multipart.bin");
const SESSION_TARGET: &str = "/upload/drive/v3/files?upload_id=SYNTHETIC%2fONLY%2B%3d&uploadType=resumable&tag=a+b&tag=a%20b&empty=&opaque=%252F";
const START: &str = "start/%2F +東京=";
const NEXT: &str = "next/%2b+ e\u{301}=";
const EXTRA: &str = "extra/%25 +🛰=";
const POLL: &str = "poll/%2F+ é=";
const AFTER_POLL: &str = "poll/%252F +終=";

fn metadata() -> NewFile {
    NewFile {
        name: NAME.into(),
        ..NewFile::default()
    }
}

fn file(total: usize) -> serde_json::Value {
    json!({"id": "maintenance-file", "name": NAME,
           "mimeType": "application/octet-stream", "size": total.to_string()})
}

fn json_reply(value: serde_json::Value) -> Reply {
    Reply::json(200, serde_json::to_vec(&value).unwrap())
}

#[tokio::test]
async fn multipart_skips_multiple_collisions_without_normalizing_name_or_media() {
    let server = TestServer::start(|_| json_reply(file(MEDIA.len()))).await;
    let created = client(&server)
        .upload_multipart(metadata(), "application/octet-stream", MEDIA, &fields())
        .await
        .unwrap();
    assert_eq!(created.value.name.as_deref(), Some(NAME));
    let request = server.single();
    auth(&request);
    assert_eq!(request.method, "POST");
    assert_eq!(request.query_all("uploadType"), ["multipart"]);
    assert_eq!(
        request.header("content-type"),
        Some("multipart/related; boundary=concord-boundary-3")
    );
    assert_eq!(
        request.header("content-length"),
        Some(MULTIPART.len().to_string().as_str())
    );
    assert_eq!(request.body, MULTIPART);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[derive(Default)]
struct State {
    accepted: Vec<u8>,
    interrupted: bool,
    failed_changes: bool,
}

async fn restart_and_poll(partially_accept_next_chunk: bool) {
    let source: Vec<_> = MEDIA
        .iter()
        .copied()
        .cycle()
        .take(2 * CHUNK_ALIGNMENT + 37)
        .collect();
    let expected = source.clone();
    let state = Arc::new(Mutex::new(State::default()));
    let responder = Arc::clone(&state);
    let server = TestServer::start(move |request| {
        auth(request);
        let mut state = responder.lock().unwrap();
        match (request.method.as_str(), request.path()) {
            ("GET", "/drive/v3/changes/startPageToken") => {
                json_reply(json!({"startPageToken": START}))
            }
            ("POST", "/upload/drive/v3/files") => {
                assert_eq!(request.query_all("uploadType"), ["resumable"]);
                assert_eq!(request.body, METADATA);
                assert_eq!(
                    request.header("x-upload-content-length"),
                    Some(expected.len().to_string().as_str())
                );
                assert_eq!(
                    request.header("x-upload-content-type"),
                    Some("application/octet-stream")
                );
                Reply::new(200).header(
                    "location",
                    format!("http://{}{SESSION_TARGET}", request.host),
                )
            }
            ("PUT", "/upload/drive/v3/files") => {
                // Compare raw target, including escape case, ordering, duplicate and empty pairs.
                assert_eq!(request.target, SESSION_TARGET);
                let offset = state.accepted.len();
                if request.body.is_empty() {
                    assert_eq!(
                        request.header("content-range"),
                        Some(format!("bytes */{}", expected.len()).as_str())
                    );
                    return Reply::new(308).header("range", format!("bytes=0-{}", offset - 1));
                }
                let end = offset + request.body.len();
                assert_eq!(
                    request.header("content-range"),
                    Some(format!("bytes {offset}-{}/{}", end - 1, expected.len()).as_str())
                );
                assert_eq!(
                    request.header("content-type"),
                    Some("application/octet-stream")
                );
                assert_eq!(
                    request.header("content-length"),
                    Some(request.body.len().to_string().as_str())
                );
                assert_eq!(request.body, expected[offset..end]);
                if partially_accept_next_chunk && offset == CHUNK_ALIGNMENT && !state.interrupted {
                    // The body arrived, but only this prefix was committed before the response vanished.
                    state.accepted.extend_from_slice(&request.body[..17]);
                    state.interrupted = true;
                    return Reply::hang_up();
                }
                state.accepted.extend_from_slice(&request.body);
                if end == expected.len() {
                    json_reply(file(expected.len()))
                } else {
                    Reply::new(308).header("range", format!("bytes=0-{}", end - 1))
                }
            }
            ("GET", "/drive/v3/changes") => {
                assert_eq!(state.accepted, expected);
                assert!(request.body.is_empty());
                match request.query_all("pageToken").as_slice() {
                    [token] if token == START => json_reply(json!({
                        "changes": [{"fileId": "before", "removed": true}], "nextPageToken": NEXT
                    })),
                    [token] if token == NEXT => json_reply(json!({
                        "changes": [], "nextPageToken": EXTRA
                    })),
                    [token] if token == EXTRA && !state.failed_changes => {
                        state.failed_changes = true;
                        Reply::json(403, RATE_LIMIT)
                    }
                    [token] if token == EXTRA => json_reply(json!({
                        "changes": [{"fileId": "maintenance-file", "file": file(expected.len())}],
                        "newStartPageToken": POLL
                    })),
                    [token] if token == POLL => json_reply(json!({
                        "changes": [], "newStartPageToken": AFTER_POLL
                    })),
                    tokens => panic!("unexpected changes tokens: {tokens:?}"),
                }
            }
            _ => panic!("unexpected request {} {}", request.method, request.target),
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
    assert_eq!(start.as_str(), START);
    let initiated = drive
        .initiate_upload(
            metadata(),
            "application/octet-stream",
            source.len() as u64,
            &fields(),
        )
        .await
        .unwrap();
    let location = initiated.meta.headers()["location"]
        .to_str()
        .unwrap()
        .to_owned();
    let mut upload = initiated.value;
    assert_eq!(upload.session_url().as_str(), location);
    let first = upload.send_chunk(&source[..CHUNK_ALIGNMENT]).await.unwrap();
    assert!(
        matches!(first, UploadStatus::Incomplete { acknowledged, .. } if acknowledged == CHUNK_ALIGNMENT as u64)
    );
    assert!(!upload.needs_probe());
    // Save both independent positions before the interruption; the next process has only these.
    let saved_upload = serde_json::to_vec(&upload.checkpoint()).unwrap();
    let saved_start = serde_json::to_vec(&start).unwrap();
    if partially_accept_next_chunk {
        let error = upload
            .send_chunk(&source[CHUNK_ALIGNMENT..2 * CHUNK_ALIGNMENT])
            .await
            .unwrap_err();
        assert_eq!(
            error.concord().unwrap().kind(),
            concord::ErrorKind::Transport
        );
        assert!(upload.needs_probe());
        assert_eq!(upload.acknowledged(), CHUNK_ALIGNMENT as u64);
    }
    drop(upload);
    drop(drive);

    let drive = client(&server);
    let checkpoint: UploadCheckpoint = serde_json::from_slice(&saved_upload).unwrap();
    let mut upload = drive.restore_upload(checkpoint).unwrap();
    assert_eq!(upload.session_url().as_str(), location);
    assert_eq!(upload.acknowledged(), CHUNK_ALIGNMENT as u64);
    assert!(upload.needs_probe());
    let before = server.requests().len();
    assert!(
        upload
            .send_chunk(&source[CHUNK_ALIGNMENT..2 * CHUNK_ALIGNMENT])
            .await
            .is_err()
    );
    assert_eq!(server.requests().len(), before);
    let prefix = CHUNK_ALIGNMENT + if partially_accept_next_chunk { 17 } else { 0 };
    assert!(
        matches!(upload.probe().await.unwrap(), UploadStatus::Incomplete { acknowledged, .. } if acknowledged == prefix as u64)
    );
    assert_eq!(upload.acknowledged(), prefix as u64);
    assert!(!upload.needs_probe());
    // The resumed offset may be unaligned; non-final *lengths* must still be aligned.
    assert!(matches!(
        upload
            .send_chunk(&source[prefix..prefix + CHUNK_ALIGNMENT])
            .await
            .unwrap(),
        UploadStatus::Incomplete { .. }
    ));
    let tail = prefix + CHUNK_ALIGNMENT;
    let UploadStatus::Complete(completed) = upload.send_chunk(&source[tail..]).await.unwrap()
    else {
        panic!("missing completed metadata")
    };
    assert!(upload.is_complete());
    assert_eq!(upload.acknowledged(), source.len() as u64);
    assert_eq!(completed.value.name.as_deref(), Some(NAME));
    assert_eq!(
        completed.value.size.as_deref(),
        Some(source.len().to_string().as_str())
    );
    assert_eq!(state.lock().unwrap().accepted, source);

    let start: PageToken = serde_json::from_slice(&saved_start).unwrap();
    let options = ChangesOptions {
        fields: fields(),
        ..ChangesOptions::default()
    };
    let stopped = drive
        .changes(start, options.clone(), cap(2))
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(stopped.collected.len(), 2);
    assert_eq!(stopped.collected[0].items[0].file_id, "before");
    assert!(stopped.collected[1].items.is_empty());
    assert!(
        stopped
            .collected
            .iter()
            .all(|page| page.meta.new_start_page_token.is_none())
    );
    let PagesError::Bounded { resume, .. } = stopped.error else {
        panic!("expected a bound")
    };
    let token = resume.key().unwrap();
    assert_eq!(token.as_str(), EXTRA);
    let saved_extra = serde_json::to_vec(token).unwrap();
    let mut pages = drive.changes(
        serde_json::from_slice(&saved_extra).unwrap(),
        options.clone(),
        cap(1),
    );
    let failure = pages.next_page().await.unwrap_err();
    let retry = failure.resume().unwrap().key().unwrap().clone();
    assert_eq!(retry.as_str(), EXTRA);
    let PagesError::Provider { error, .. } = failure else {
        panic!("expected a provider error")
    };
    assert_eq!(error.meta().unwrap().status().as_u16(), 403);
    assert_eq!(
        error.provider().unwrap().errors[0].reason,
        "rateLimitExceeded"
    );
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    let mut pages = drive.changes(retry, options.clone(), cap(1));
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(last.items[0].file.as_ref().unwrap(), &completed.value);
    let poll = last.meta.new_start_page_token.unwrap();
    assert_eq!(poll.as_str(), POLL);
    assert!(pages.next_page().await.unwrap().is_none());
    let saved_poll = serde_json::to_vec(&poll).unwrap();
    let next = drive
        .list_changes_page(&serde_json::from_slice(&saved_poll).unwrap(), &options)
        .await
        .unwrap();
    assert!(next.value.changes.is_empty());
    assert_eq!(
        next.value.new_start_page_token.unwrap().as_str(),
        AFTER_POLL
    );

    let requests = server.requests();
    let transfers: Vec<_> = requests
        .iter()
        .filter(|request| request.method == "PUT")
        .collect();
    assert_eq!(
        transfers.len(),
        if partially_accept_next_chunk { 5 } else { 4 }
    );
    let probe_index = if partially_accept_next_chunk { 2 } else { 1 };
    assert!(transfers[probe_index].body.is_empty());
    assert_eq!(transfers[probe_index + 1].body, source[prefix..tail]);
    assert_eq!(transfers[probe_index + 2].body, source[tail..]);
    let tokens: Vec<_> = requests
        .iter()
        .filter(|request| request.path() == "/drive/v3/changes")
        .flat_map(|request| request.query_all("pageToken"))
        .collect();
    assert_eq!(tokens, [START, NEXT, EXTRA, EXTRA, POLL]);
    assert_eq!(requests.len(), transfers.len() + 7); // start token, initiate, five changes calls
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn restart_immediately_after_ack_preserves_bytes_link_and_changes_tokens() {
    restart_and_poll(false).await;
}

#[tokio::test]
async fn restart_after_partial_lost_chunk_preserves_bytes_link_and_changes_tokens() {
    restart_and_poll(true).await;
}
