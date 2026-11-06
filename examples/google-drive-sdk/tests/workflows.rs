#![allow(missing_docs)]

mod support;

use std::sync::{Arc, Mutex};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_google_drive::{
    CHUNK_ALIGNMENT, ChangesOptions, ListOptions, PageToken, UploadCheckpoint, UploadStatus,
    pagination::{PagesError, Resume},
};
use support::*;

#[derive(Default)]
struct FakeDrive {
    multipart_created: bool,
    session_created: bool,
    accepted: Vec<u8>,
    interrupted: bool,
    complete: bool,
}

impl FakeDrive {
    fn reply(&mut self, request: &Recorded) -> Reply {
        auth(request);
        match (request.method.as_str(), request.path()) {
            ("GET", "/drive/v3/files") => {
                assert!(request.body.is_empty());
                match request.query_all("pageToken").first().map(String::as_str) {
                    None => Reply::json(200, FILES_1),
                    Some("files/+2=é") => Reply::json(200, FILES_2),
                    _ => panic!("unknown files token"),
                }
            }
            ("GET", "/drive/v3/files/file-before") => Reply::json(
                200,
                br#"{"id":"file-before","name":"before.txt","size":"8"}"#,
            ),
            ("POST", "/upload/drive/v3/files") => match request.query_all("uploadType")[0].as_str()
            {
                "multipart" => {
                    assert_eq!(request.body, MULTIPART);
                    self.multipart_created = true;
                    Reply::json(200, CREATED)
                }
                "resumable" => {
                    assert_eq!(request.body, NEW_FILE);
                    assert_eq!(
                        request.header("x-upload-content-length"),
                        Some(source().len().to_string().as_str())
                    );
                    self.session_created = true;
                    initiation(request)
                }
                _ => panic!("unknown upload mode"),
            },
            ("GET", "/drive/v3/files/file-new") => {
                assert!(self.multipart_created);
                Reply::json(200, CREATED)
            }
            ("PUT", "/upload/drive/v3/files") => {
                assert!(self.session_created);
                assert_eq!(request.query_all("upload_id"), ["SYNTHETIC_SESSION_ONLY"]);
                let source = source();
                if request.body.is_empty() {
                    assert_eq!(
                        request.header("content-range"),
                        Some(format!("bytes */{}", source.len()).as_str())
                    );
                    if self.complete {
                        return Reply::json(200, RESUMED);
                    }
                    return Reply::new(308)
                        .header("range", format!("bytes=0-{}", self.accepted.len() - 1));
                }
                let offset = self.accepted.len();
                let end = offset + request.body.len();
                assert_eq!(
                    request.header("content-range"),
                    Some(format!("bytes {offset}-{}/{}", end - 1, source.len()).as_str())
                );
                assert_eq!(request.body, source[offset..end]);
                self.accepted.extend_from_slice(&request.body);
                if end == source.len() {
                    self.complete = true;
                    return Reply::json(201, RESUMED);
                }
                if offset == CHUNK_ALIGNMENT && !self.interrupted {
                    self.interrupted = true;
                    // Server committed the second chunk, but the caller cannot see its response.
                    return Reply::hang_up();
                }
                Reply::new(308).header("range", format!("bytes=0-{}", end - 1))
            }
            ("GET", "/drive/v3/changes/startPageToken") => Reply::json(200, START),
            ("GET", "/drive/v3/changes") => {
                assert!(self.complete);
                match request.query_all("pageToken")[0].as_str() {
                    "change/start+0=" => Reply::json(200, CHANGES_1),
                    "change/next+1=" => Reply::json(200, CHANGES_2),
                    "change/next+2=" => Reply::json(200, CHANGES_3),
                    "change/poll+3=" => Reply::json(
                        200,
                        br#"{"changes":[],"newStartPageToken":"change/poll+4="}"#,
                    ),
                    _ => panic!("unknown changes token"),
                }
            }
            _ => panic!("unexpected operation {} {}", request.method, request.path()),
        }
    }
}

async fn fake() -> (TestServer, Arc<Mutex<FakeDrive>>) {
    let state = Arc::new(Mutex::new(FakeDrive::default()));
    let responder = Arc::clone(&state);
    let server = TestServer::start(move |request| responder.lock().unwrap().reply(request)).await;
    (server, state)
}

#[tokio::test]
async fn workflow_a_lists_inspects_uploads_unicode_binary_and_retrieves_metadata() {
    let (server, state) = fake().await;
    let drive = client(&server);
    let files = drive
        .files(
            ListOptions {
                fields: fields(),
                ..ListOptions::default()
            },
            Resume::Start,
            cap(8),
        )
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        files
            .iter()
            .map(|file| file.id.as_str())
            .collect::<Vec<_>>(),
        ["file-before", "file-other"]
    );
    let before = drive.get_file(&files[0].id, &fields()).await.unwrap().value;
    assert_eq!(before.name.as_deref(), Some("before.txt"));
    let created = drive
        .upload_multipart(metadata(), "application/octet-stream", SMALL, &fields())
        .await
        .unwrap()
        .value;
    let fetched = drive.get_file(&created.id, &fields()).await.unwrap().value;
    assert_eq!(created, fetched);
    assert_eq!(fetched.name.as_deref(), Some(NAME));
    assert_eq!(fetched.size.unwrap().parse::<usize>().unwrap(), SMALL.len());
    assert!(state.lock().unwrap().multipart_created);
    assert_eq!(server.requests().len(), 5);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn workflow_b_restores_uncertain_session_reconstructs_exact_bytes_and_retains_changes_boundaries()
 {
    let (server, state) = fake().await;
    let drive = client(&server);
    let initial_token = drive
        .start_page_token()
        .await
        .unwrap()
        .value
        .start_page_token;
    let source = source();
    let initiated = drive
        .initiate_upload(
            metadata(),
            "application/octet-stream",
            source.len() as u64,
            &fields(),
        )
        .await
        .unwrap();
    let mut upload = initiated.value;
    let location = upload.session_url().as_str().to_owned();
    assert_eq!(initiated.meta.headers()["location"], location);
    upload.send_chunk(&source[..CHUNK_ALIGNMENT]).await.unwrap();
    assert_eq!(upload.acknowledged(), CHUNK_ALIGNMENT as u64);
    let uncertain = upload
        .send_chunk(&source[CHUNK_ALIGNMENT..2 * CHUNK_ALIGNMENT])
        .await
        .unwrap_err();
    assert_eq!(
        uncertain.concord().unwrap().kind(),
        concord::ErrorKind::Transport
    );
    assert!(upload.needs_probe());
    assert_eq!(upload.acknowledged(), CHUNK_ALIGNMENT as u64);
    let request_count = server.requests().len();
    assert!(
        upload
            .send_chunk(&source[CHUNK_ALIGNMENT..2 * CHUNK_ALIGNMENT])
            .await
            .is_err()
    );
    assert_eq!(server.requests().len(), request_count);

    // Simulate process restart: persist only checkpoint data and inject auth into a new client.
    let protected_state = serde_json::to_vec(&upload.checkpoint()).unwrap();
    drop(upload);
    let checkpoint: UploadCheckpoint = serde_json::from_slice(&protected_state).unwrap();
    let mut upload = client(&server).restore_upload(checkpoint).unwrap();
    assert_eq!(upload.session_url().as_str(), location);
    assert!(upload.needs_probe());
    assert!(matches!(
        upload.probe().await.unwrap(),
        UploadStatus::Incomplete {
            acknowledged: 524288,
            ..
        }
    ));
    let offset = upload.acknowledged() as usize;
    let UploadStatus::Complete(completed) = upload.send_chunk(&source[offset..]).await.unwrap()
    else {
        panic!("expected final metadata")
    };
    assert_eq!(completed.value.id, "file-resumed");
    assert_eq!(
        completed.value.size.unwrap().parse::<usize>().unwrap(),
        source.len()
    );
    assert!(upload.is_complete());
    assert_eq!(state.lock().unwrap().accepted, source);

    // Stop at a budget with a retained continuation, then consume the additional changes page.
    let options = ChangesOptions {
        fields: fields(),
        ..ChangesOptions::default()
    };
    let mut pages = drive.changes(initial_token, options.clone(), cap(2));
    let first = pages.next_page().await.unwrap().unwrap();
    let second = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].file_id, "file-new");
    assert!(second.items[0].removed);
    assert!(second.items[0].file.is_none());
    assert!(second.meta.new_start_page_token.is_none());
    let PagesError::Bounded { resume, .. } = pages.next_page().await.unwrap_err() else {
        panic!("expected page bound")
    };
    assert_eq!(resume.key().unwrap().as_str(), "change/next+2=");
    let mut resumed = drive.changes(resume.key().unwrap().clone(), options.clone(), cap(2));
    let last = resumed.next_page().await.unwrap().unwrap();
    assert_eq!(last.items[0].file_id, completed.value.id);
    let next_poll = last.meta.new_start_page_token.unwrap();
    assert_eq!(next_poll.as_str(), "change/poll+3=");
    assert!(resumed.next_page().await.unwrap().is_none());
    let poll = drive.list_changes_page(&next_poll, &options).await.unwrap();
    assert!(poll.value.changes.is_empty());
    assert_eq!(
        poll.value.new_start_page_token.unwrap().as_str(),
        "change/poll+4="
    );

    let requests = server.requests();
    let transfers: Vec<_> = requests
        .iter()
        .filter(|request| request.method == "PUT")
        .collect();
    assert_eq!(transfers.len(), 4); // first chunk, uncertain second, status probe, final suffix
    assert_eq!(
        transfers
            .iter()
            .filter(|request| !request.body.is_empty())
            .map(|request| request.body.len())
            .sum::<usize>(),
        source.len()
    );
    assert_eq!(
        transfers[2].header("content-range"),
        Some(format!("bytes */{}", source.len()).as_str())
    );
    assert_eq!(transfers[3].body, source[2 * CHUNK_ALIGNMENT..]);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn workflow_a_page_budget_preserves_prefix_and_can_resume_with_same_options() {
    let (server, _) = fake().await;
    let drive = client(&server);
    let options = ListOptions {
        fields: fields(),
        ..ListOptions::default()
    };
    let incomplete = drive
        .files(options.clone(), Resume::Start, cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected[0].id, "file-before");
    let resume = incomplete.error.resume().unwrap().clone();
    let rest = drive
        .files(options, resume, cap(2))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest[0].id, "file-other");
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_b_failed_changes_page_keeps_prior_changes_and_failed_token() {
    let calls = Arc::new(Mutex::new(0));
    let server = TestServer::start(move |request| {
        auth(request);
        let mut calls = calls.lock().unwrap();
        *calls += 1;
        if *calls == 1 {
            Reply::json(200, CHANGES_1)
        } else {
            Reply::json(403, RATE_LIMIT)
        }
    })
    .await;
    let drive = client(&server);
    let mut pages = drive.changes(
        PageToken::new("change/start+0=").unwrap(),
        ChangesOptions::default(),
        cap(8),
    );
    assert_eq!(
        pages.next_page().await.unwrap().unwrap().items[0].file_id,
        "file-new"
    );
    let error = pages.next_page().await.unwrap_err();
    assert_eq!(
        error.resume().unwrap().key().unwrap().as_str(),
        "change/next+1="
    );
    let PagesError::Provider { error, .. } = error else {
        panic!("expected provider error")
    };
    assert_eq!(
        error.provider().unwrap().errors[0].reason,
        "rateLimitExceeded"
    );
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
