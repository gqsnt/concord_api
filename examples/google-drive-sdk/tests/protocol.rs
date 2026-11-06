#![allow(missing_docs)]

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_example_testserver::{Body, Reply, Script, TestServer};
use concord_sdk_google_drive::{
    CHUNK_ALIGNMENT, ChangesOptions, ClientConfig, DocumentProblem, DriveClient, ErrorKind,
    FileFields, ListOptions, PageToken, UploadCheckpoint, UploadStatus,
    pagination::{PagesError, Resume},
};
use futures_util::StreamExt;
use support::*;

#[tokio::test]
async fn list_get_and_metadata_create_have_exact_wire_shapes() {
    let replies = Script::new([
        Reply::json(200, FILES_1),
        Reply::json(200, b"{\"id\":\"file/id\"}"),
        Reply::json(200, CREATED),
    ]);
    let server = TestServer::start(move |_| replies.next()).await;
    let drive = client(&server);
    let options = ListOptions {
        page_size: 7,
        query: Some("trashed = false and name contains 'café'".into()),
        order_by: Some("name desc".into()),
        fields: fields(),
    };
    let token = PageToken::new("opaque/+cursor=é").unwrap();
    let listed = drive.list_files_page(&options, Some(&token)).await.unwrap();
    assert_eq!(listed.value.next_page_token.unwrap().as_str(), "files/+2=é");
    let file = drive
        .get_file("file/id", &FileFields::only(&[]))
        .await
        .unwrap();
    assert_eq!(file.value.id, "file/id");
    assert!(file.value.name.is_none());
    drive.create_file(metadata(), &fields()).await.unwrap();
    let requests = server.requests();
    let list = &requests[0];
    assert_eq!(list.method, "GET");
    assert_eq!(
        list.target,
        "/drive/v3/files?pageSize=7&fields=nextPageToken%2CincompleteSearch%2Cfiles%28id%2Cname%2CmimeType%2Csize%29&pageToken=opaque%2F%2Bcursor%3D%C3%A9&q=trashed+%3D+false+and+name+contains+%27caf%C3%A9%27&orderBy=name+desc&spaces=drive"
    );
    assert_eq!(list.header("accept"), Some("application/json"));
    assert!(list.body.is_empty());
    assert_eq!(requests[1].target, "/drive/v3/files/file%2Fid?fields=id");
    assert_eq!(requests[1].method, "GET");
    assert!(requests[1].body.is_empty());
    assert_eq!(requests[2].method, "POST");
    assert_eq!(
        requests[2].target,
        "/drive/v3/files?fields=id%2Cname%2CmimeType%2Csize"
    );
    assert_eq!(requests[2].header("content-type"), Some("application/json"));
    assert_eq!(requests[2].body, NEW_FILE);
    for request in &requests {
        auth(request);
    }
    assert!(!format!("{drive:?}").contains(TOKEN));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn multipart_related_is_byte_exact_and_collision_free() {
    let server = TestServer::start(|_| Reply::json(200, CREATED)).await;
    let response = client(&server)
        .upload_multipart(metadata(), "application/octet-stream", SMALL, &fields())
        .await
        .unwrap();
    assert_eq!(response.value.name.as_deref(), Some(NAME));
    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.target,
        "/upload/drive/v3/files?fields=id%2Cname%2CmimeType%2Csize&uploadType=multipart"
    );
    auth(&request);
    assert_eq!(
        request.header("content-type"),
        Some("multipart/related; boundary=concord_drive_1")
    );
    assert_eq!(
        request.header("content-length"),
        Some(MULTIPART.len().to_string().as_str())
    );
    assert!(!request.chunked);
    assert_eq!(request.body, MULTIPART);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn multipart_scans_metadata_too_and_invalid_media_never_sends() {
    let server = TestServer::start(|_| Reply::json(200, CREATED)).await;
    let drive = client(&server);
    let mut metadata = metadata();
    metadata.name = "concord_drive_0.bin".into();
    drive
        .upload_multipart(metadata, "application/octet-stream", b"", &fields())
        .await
        .unwrap();
    assert_eq!(
        server.single().header("content-type"),
        Some("multipart/related; boundary=concord_drive_1")
    );
    for media in [
        "text/plain\r\nx-injected: yes",
        "*/*",
        "not a mime",
        "text/*",
    ] {
        assert!(matches!(
            drive
                .upload_multipart(support::metadata(), media, SMALL, &fields())
                .await
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidInput(_)
        ));
    }
    assert!(
        drive
            .upload_multipart(
                support::metadata(),
                "application/octet-stream",
                &vec![0; 5 * 1024 * 1024 + 1],
                &fields()
            )
            .await
            .is_err()
    );
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn resumable_initiation_chunks_probe_and_completion_preserve_wire() {
    let index = Arc::new(Mutex::new(0));
    let server = TestServer::start(move |request| {
        let mut index = index.lock().unwrap();
        let reply = match *index {
            0 => initiation(request),
            1 => Reply::new(308).header("range", "bytes=0-262143"),
            2 => Reply::new(308).header("range", "bytes=0-262143"),
            3 => Reply::json(201, br#"{"id":"file-resumed"}"#),
            _ => panic!("unexpected request"),
        };
        *index += 1;
        reply
    })
    .await;
    let total = CHUNK_ALIGNMENT + 37;
    let drive = client(&server);
    let started = drive
        .initiate_upload(
            metadata(),
            "application/octet-stream",
            total as u64,
            &fields(),
        )
        .await
        .unwrap();
    let mut session = started.value;
    assert_eq!(
        session.session_url().as_str(),
        started.meta.headers["location"]
    );
    assert!(matches!(
        session
            .send_chunk(&vec![0xff; CHUNK_ALIGNMENT])
            .await
            .unwrap(),
        UploadStatus::Incomplete {
            acknowledged: 262144,
            ..
        }
    ));
    session.probe().await.unwrap();
    assert!(matches!(
        session
            .send_chunk(
                &[0, 255, 128]
                    .repeat(12)
                    .into_iter()
                    .chain([0])
                    .collect::<Vec<_>>()
            )
            .await
            .unwrap(),
        UploadStatus::Complete(_)
    ));
    assert!(session.is_complete());
    assert_eq!(session.acknowledged(), total as u64);
    assert!(session.probe().await.is_err());
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    let init = &requests[0];
    assert_eq!(init.method, "POST");
    assert_eq!(
        init.target,
        "/upload/drive/v3/files?fields=id%2Cname%2CmimeType%2Csize&uploadType=resumable"
    );
    assert_eq!(init.header("content-type"), Some("application/json"));
    assert_eq!(
        init.header("x-upload-content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(
        init.header("x-upload-content-length"),
        Some(total.to_string().as_str())
    );
    assert_eq!(init.body, NEW_FILE);
    for request in &requests[1..] {
        assert_eq!(request.method, "PUT");
        assert_eq!(
            request.target,
            "/upload/drive/v3/files?uploadType=resumable&upload_id=SYNTHETIC_SESSION_ONLY"
        );
    }
    assert_eq!(
        requests[1].header("content-range"),
        Some(format!("bytes 0-262143/{total}").as_str())
    );
    assert_eq!(requests[1].header("content-length"), Some("262144"));
    assert_eq!(
        requests[1].header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(requests[1].body, vec![0xff; CHUNK_ALIGNMENT]);
    assert_eq!(
        requests[2].header("content-range"),
        Some(format!("bytes */{total}").as_str())
    );
    // The status probe is an empty PUT: Concord sends `Content-Length: 0` for a declared empty
    // body (between waves 3-4; this SDK's FRICTION.md entry 7 recorded reqwest omitting it, which
    // differs from Google's documented probe).
    assert_eq!(requests[2].header_all("content-length"), ["0"]);
    assert!(!requests[2].chunked);
    assert!(requests[2].body.is_empty());
    assert_eq!(
        requests[3].header("content-range"),
        Some(format!("bytes 262144-262180/{total}").as_str())
    );
    assert_eq!(
        requests[3].body,
        [0, 255, 128]
            .repeat(12)
            .into_iter()
            .chain([0])
            .collect::<Vec<_>>()
    );
    for request in &requests {
        auth(request);
    }
    assert!(!format!("{session:?}").contains("SYNTHETIC_SESSION_ONLY"));
    server.shutdown().await;
}

#[tokio::test]
async fn location_policy_rejects_cross_origin_credentials_fragments_paths_and_duplicates() {
    let sink = TestServer::start(|_| panic!("credentials must not arrive")).await;
    let locations = Arc::new(Mutex::new(
        vec![
            format!("{}/upload/drive/v3/files?upload_id=SYNTHETIC", sink.url()),
            "https://other.invalid/upload/drive/v3/files".into(),
            "/upload/drive/v3/files?upload_id=SYNTHETIC".into(),
            "not a URL".into(),
        ]
        .into_iter(),
    ));
    let server = TestServer::start(move |request| {
        if let Some(location) = locations.lock().unwrap().next() {
            return Reply::new(200).header("location", &location);
        }
        let current = request.query_all("fields").join("");
        match current.as_str() {
            "id,name" => Reply::new(200).header(
                "location",
                format!(
                    "http://synthetic-user@{}/upload/drive/v3/files",
                    request.host
                ),
            ),
            "id,size" => Reply::new(200).header(
                "location",
                format!("http://{}/upload/drive/v3/files#secret", request.host),
            ),
            "id,mimeType" => Reply::new(200).header(
                "location",
                format!("http://{}/drive/v3/files", request.host),
            ),
            "id,parents" => initiation(request).header("location", "https://other.invalid/"),
            _ => Reply::new(200),
        }
    })
    .await;
    let drive = client(&server);
    for selection in [
        fields(),
        fields(),
        fields(),
        fields(),
        concord_sdk_google_drive::FileFields::only(&[concord_sdk_google_drive::FileField::Name]),
        concord_sdk_google_drive::FileFields::only(&[concord_sdk_google_drive::FileField::Size]),
        concord_sdk_google_drive::FileFields::only(&[
            concord_sdk_google_drive::FileField::MimeType,
        ]),
        concord_sdk_google_drive::FileFields::only(&[concord_sdk_google_drive::FileField::Parents]),
        FileFields::only(&[]),
    ] {
        let error = drive
            .initiate_upload(metadata(), "application/octet-stream", 1, &selection)
            .await
            .unwrap_err();
        assert!(matches!(error.kind(), ErrorKind::Protocol(_)));
        assert_eq!(error.meta().unwrap().status.as_u16(), 200);
        assert!(error.operation().unwrap().name().ends_with("initiate"));
    }
    assert!(sink.requests().is_empty());
    sink.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn redirects_are_not_followed_even_within_origin() {
    let server = TestServer::start(|request| {
        Reply::new(307).header(
            "location",
            format!("http://{}/redirect-target", request.host),
        )
    })
    .await;
    let error = client(&server).get_file("id", &fields()).await.unwrap_err();
    assert_eq!(error.meta().unwrap().status.as_u16(), 307);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn acknowledged_prefix_is_authoritative_and_chunk_validation_is_local() {
    let server = TestServer::start(|request| match request.method.as_str() {
        "POST" => initiation(request),
        "PUT" => Reply::new(308).header("range", "bytes=0-131071"),
        _ => panic!("unexpected method"),
    })
    .await;
    let mut session = client(&server)
        .initiate_upload(
            metadata(),
            "application/octet-stream",
            2 * CHUNK_ALIGNMENT as u64,
            &fields(),
        )
        .await
        .unwrap()
        .value;
    for bad in [&[][..], &[0][..], &vec![0; 2 * CHUNK_ALIGNMENT + 1][..]] {
        assert!(session.send_chunk(bad).await.is_err());
    }
    assert_eq!(server.requests().len(), 1);
    session.send_chunk(&vec![9; CHUNK_ALIGNMENT]).await.unwrap();
    assert_eq!(session.acknowledged(), 131072);
    assert!(!session.needs_probe());
    // The next request begins at the server-confirmed offset, not at 262144.
    session.send_chunk(&vec![8; CHUNK_ALIGNMENT]).await.unwrap();
    assert_eq!(
        server.requests()[2].header("content-range"),
        Some("bytes 131072-393215/524288")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_or_impossible_ranges_leave_a_probe_required() {
    for range in [
        "bytes=1-42",
        "bytes=0-abc",
        "bytes=0-18446744073709551615",
        "bytes=0-262144",
        "bytes=0-524287",
        "bytes=0-42,100-200",
        "bytes=0-+42",
    ] {
        let range = range.to_owned();
        let server = TestServer::start(move |request| {
            if request.method == "POST" {
                initiation(request)
            } else {
                Reply::new(308).header("range", &range)
            }
        })
        .await;
        let mut session = client(&server)
            .initiate_upload(metadata(), "application/octet-stream", 524288, &fields())
            .await
            .unwrap()
            .value;
        let error = session
            .send_chunk(&vec![0; CHUNK_ALIGNMENT])
            .await
            .unwrap_err();
        assert!(matches!(error.kind(), ErrorKind::Protocol(_)));
        assert_eq!(error.meta().unwrap().status.as_u16(), 308);
        assert!(session.needs_probe());
        assert_eq!(session.acknowledged(), 0);
        assert!(session.send_chunk(&vec![0; CHUNK_ALIGNMENT]).await.is_err());
        assert_eq!(server.requests().len(), 2);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn missing_range_means_zero_and_repeated_or_regressed_range_is_rejected() {
    let counter = Arc::new(Mutex::new(0));
    let server = TestServer::start(move |request| {
        if request.method == "POST" {
            return initiation(request);
        }
        let mut n = counter.lock().unwrap();
        *n += 1;
        match *n {
            1 => Reply::new(308),
            2 => Reply::new(308).header("range", "bytes=0-262143"),
            3 => Reply::new(308),
            _ => Reply::new(308)
                .header("range", "bytes=0-262143")
                .header("range", "bytes=0-262143"),
        }
    })
    .await;
    let mut session = client(&server)
        .initiate_upload(metadata(), "application/octet-stream", 524288, &fields())
        .await
        .unwrap()
        .value;
    session.probe().await.unwrap();
    assert_eq!(session.acknowledged(), 0);
    session.send_chunk(&vec![0; CHUNK_ALIGNMENT]).await.unwrap();
    assert!(session.probe().await.is_err());
    assert_eq!(session.acknowledged(), 262144);
    assert!(session.probe().await.is_err());
    assert!(session.needs_probe());
    server.shutdown().await;
}

#[tokio::test]
async fn lost_final_response_is_resolved_by_probe_without_replaying_bytes() {
    let n = Arc::new(Mutex::new(0));
    let server = TestServer::start(move |request| {
        let mut n = n.lock().unwrap();
        *n += 1;
        match *n {
            1 => initiation(request),
            2 => Reply::hang_up(),
            3 => Reply::json(200, CREATED),
            _ => panic!("unexpected replay"),
        }
    })
    .await;
    let mut session = client(&server)
        .initiate_upload(
            metadata(),
            "application/octet-stream",
            SMALL.len() as u64,
            &fields(),
        )
        .await
        .unwrap()
        .value;
    let error = session.send_chunk(SMALL).await.unwrap_err();
    assert_eq!(
        error.concord().unwrap().kind(),
        concord::ErrorKind::Transport,
        "{:?}",
        error.concord()
    );
    assert!(error.meta().is_none());
    assert!(error.operation().unwrap().name().ends_with("transfer"));
    assert!(session.needs_probe());
    assert!(session.send_chunk(SMALL).await.is_err());
    assert!(matches!(
        session.probe().await.unwrap(),
        UploadStatus::Complete(_)
    ));
    assert_eq!(server.requests().len(), 3);
    assert!(server.requests()[2].body.is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn cancellation_blocks_writes_until_probe() {
    let server = TestServer::start(|request| {
        if request.method == "POST" {
            initiation(request)
        } else if request.body.is_empty() {
            Reply::new(308)
        } else {
            Reply {
                delay: Some(Duration::from_secs(10)),
                ..Reply::new(308)
            }
        }
    })
    .await;
    let mut session = client(&server)
        .initiate_upload(metadata(), "application/octet-stream", 524288, &fields())
        .await
        .unwrap()
        .value;
    {
        let bytes = vec![0; CHUNK_ALIGNMENT];
        let sending = session.send_chunk(&bytes);
        tokio::pin!(sending);
        tokio::select! {
            result = &mut sending => panic!("expected pending send: {result:?}"),
            observed = server.wait_until(Duration::from_secs(2), |server| server.requests().len() == 2) => assert!(observed),
        }
        // Drop the polled future only after the loopback server observed the request.
    }
    assert!(session.needs_probe());
    assert!(session.send_chunk(&vec![0; CHUNK_ALIGNMENT]).await.is_err());
    session.probe().await.unwrap();
    assert!(!session.needs_probe());
    assert_eq!(server.requests().len(), 3);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn expired_session_and_503_keep_metadata_and_never_retry() {
    let replies = Script::new([
        Reply::json(503, RATE_LIMIT).header("retry-after", "7"),
        Reply::json(404, EXPIRED),
    ]);
    let server = TestServer::start(move |request| {
        if request.method == "POST" {
            initiation(request)
        } else {
            replies.next()
        }
    })
    .await;
    let mut session = client(&server)
        .initiate_upload(metadata(), "application/octet-stream", 1, &fields())
        .await
        .unwrap()
        .value;
    let error = session.send_chunk(&[0]).await.unwrap_err();
    assert_eq!(error.meta().unwrap().status.as_u16(), 503);
    assert_eq!(error.meta().unwrap().headers["retry-after"], "7");
    assert_eq!(
        error.provider().unwrap().errors[0].reason,
        "rateLimitExceeded"
    );
    assert!(session.needs_probe());
    assert_eq!(server.requests().len(), 2);
    assert_eq!(
        session.probe().await.unwrap_err().kind(),
        ErrorKind::SessionExpired
    );
    assert!(session.needs_probe());
    server.shutdown().await;
}

#[tokio::test]
async fn premature_completion_and_unexpected_status_are_protocol_errors() {
    for status in [200, 201, 204] {
        let server = TestServer::start(move |request| {
            if request.method == "POST" {
                initiation(request)
            } else {
                Reply::json(status, CREATED)
            }
        })
        .await;
        let mut session = client(&server)
            .initiate_upload(metadata(), "application/octet-stream", 524288, &fields())
            .await
            .unwrap()
            .value;
        assert!(matches!(
            session
                .send_chunk(&vec![0; CHUNK_ALIGNMENT])
                .await
                .unwrap_err()
                .kind(),
            ErrorKind::Protocol(_)
        ));
        assert!(session.needs_probe());
        assert!(!session.is_complete());
        server.shutdown().await;
    }
}

#[tokio::test]
async fn checkpoint_restore_is_origin_bound_redacted_and_requires_probe() {
    let server = TestServer::start(|request| {
        if request.method == "POST" {
            initiation(request)
        } else {
            Reply::new(308)
        }
    })
    .await;
    let drive = client(&server);
    let session = drive
        .initiate_upload(metadata(), "application/octet-stream", 1, &fields())
        .await
        .unwrap()
        .value;
    let checkpoint = session.checkpoint();
    assert!(!format!("{checkpoint:?}").contains("SYNTHETIC_SESSION_ONLY"));
    let encoded = serde_json::to_vec(&checkpoint).unwrap();
    assert!(!String::from_utf8_lossy(&encoded).contains(TOKEN));
    let checkpoint: UploadCheckpoint = serde_json::from_slice(&encoded).unwrap();
    let mut restored = drive.restore_upload(checkpoint).unwrap();
    assert!(restored.send_chunk(&[0]).await.is_err());
    restored.probe().await.unwrap();
    assert_eq!(server.requests().len(), 2);
    let other = TestServer::start(|_| panic!("must not receive session credentials")).await;
    let checkpoint = serde_json::from_slice(&encoded).unwrap();
    assert!(client(&other).restore_upload(checkpoint).is_err());
    assert!(other.requests().is_empty());
    other.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn errors_keep_typed_documents_headers_and_safe_diagnostics() {
    let replies = Script::new([
        Reply::json(403, RATE_LIMIT)
            .header("x-guploader-uploadid", "SYNTHETIC_REQUEST_ID")
            .header("retry-after", "3"),
        Reply::json(400, INVALID_FIELDS),
        Reply::json(
            403,
            format!(
                "{{\"error\":{{\"code\":403,\"message\":\"{TOKEN}\",\"errors\":[{{\"reason\":\"{TOKEN}\"}}]}}}}"
            ),
        ),
    ]);
    let server = TestServer::start(move |_| replies.next()).await;
    let drive = client(&server);
    let error = drive.get_file("id", &fields()).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Provider);
    assert_eq!(error.provider().unwrap().code, 403);
    assert_eq!(
        error.provider().unwrap().errors[0].reason,
        "rateLimitExceeded"
    );
    assert_eq!(error.meta().unwrap().headers["retry-after"], "3");
    assert_eq!(
        error.meta().unwrap().headers["x-guploader-uploadid"],
        "SYNTHETIC_REQUEST_ID"
    );
    let error = drive.get_file("id", &fields()).await.unwrap_err();
    assert_eq!(
        error.provider().unwrap().errors[0].location.as_deref(),
        Some("fields")
    );
    let error = drive.get_file("id", &fields()).await.unwrap_err();
    assert_eq!(error.provider().unwrap().message, TOKEN);
    assert!(!format!("{error:?} {error} {:?}", error.provider()).contains(TOKEN));
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_oversized_or_truncated_error_bodies_keep_the_head() {
    let replies = Script::new([
        Reply::text(502, "text/html", b"<html>synthetic proxy failure</html>"),
        Reply::json(403, vec![b'x'; 65]),
        Reply {
            body: Body::FailAfter(vec![concord::bytes::Bytes::from_static(b"{\"error\":")]),
            ..Reply::json(503, b"")
        },
    ]);
    let server = TestServer::start(move |_| replies.next().header("retry-after", "9")).await;
    let drive = DriveClient::with_config(
        TOKEN,
        ClientConfig {
            origin: server.url(),
            max_error_bytes: 64,
            ..ClientConfig::default()
        },
    )
    .unwrap();
    for problem in [
        DocumentProblem::Malformed,
        DocumentProblem::TooLarge,
        DocumentProblem::Read,
    ] {
        let error = drive.get_file("id", &fields()).await.unwrap_err();
        assert_eq!(error.document_problem(), Some(problem));
        assert_eq!(error.meta().unwrap().headers["retry-after"], "9");
        assert!(error.provider().is_none());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn successful_metadata_failures_keep_response_information() {
    let replies = Script::new([
        Reply::text(200, "text/html", CREATED),
        Reply::json(200, b"{not-json"),
        Reply::json(200, vec![b' '; 129]),
        Reply {
            body: Body::FailAfter(vec![concord::bytes::Bytes::from_static(b"{\"id\":")]),
            ..Reply::json(200, b"")
        },
    ]);
    let server =
        TestServer::start(move |_| replies.next().header("x-request-id", "SYNTHETIC_ID")).await;
    let drive = DriveClient::with_config(
        TOKEN,
        ClientConfig {
            origin: server.url(),
            max_response_bytes: 128,
            ..ClientConfig::default()
        },
    )
    .unwrap();
    for kind in [
        concord::ErrorKind::Metadata,
        concord::ErrorKind::Decode,
        concord::ErrorKind::Limit,
        concord::ErrorKind::Transport,
    ] {
        let error = drive.get_file("id", &fields()).await.unwrap_err();
        assert_eq!(error.concord().unwrap().kind(), kind);
        assert_eq!(error.meta().unwrap().status.as_u16(), 200);
        assert_eq!(
            error.meta().unwrap().headers["x-request-id"],
            "SYNTHETIC_ID"
        );
    }
    server.shutdown().await;
}

#[tokio::test]
async fn changes_queries_and_token_boundaries_are_exact() {
    let replies = Script::new([
        Reply::json(200, START),
        Reply::json(200, CHANGES_1),
        Reply::json(200, CHANGES_2),
        Reply::json(200, CHANGES_3),
    ]);
    let server = TestServer::start(move |_| replies.next()).await;
    let drive = client(&server);
    let start = drive
        .start_page_token()
        .await
        .unwrap()
        .value
        .start_page_token;
    let mut pages = drive.changes(
        start,
        ChangesOptions {
            page_size: 9,
            fields: fields(),
        },
        cap(3),
    );
    assert_eq!(
        pages.next_page().await.unwrap().unwrap().items[0].file_id,
        "file-new"
    );
    assert!(pages.next_page().await.unwrap().unwrap().items[0].removed);
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(
        last.meta.new_start_page_token.unwrap().as_str(),
        "change/poll+3="
    );
    assert!(pages.next_page().await.unwrap().is_none());
    let requests = server.requests();
    assert_eq!(
        requests[0].target,
        "/drive/v3/changes/startPageToken?fields=startPageToken"
    );
    assert_eq!(
        requests[1].target,
        "/drive/v3/changes?pageToken=change%2Fstart%2B0%3D&pageSize=9&fields=nextPageToken%2CnewStartPageToken%2Cchanges%28fileId%2Cremoved%2Ctime%2Cfile%28id%2Cname%2CmimeType%2Csize%29%29&spaces=drive&includeRemoved=true"
    );
    assert_eq!(requests[2].query_all("pageToken"), ["change/next+1="]);
    assert_eq!(requests[3].query_all("pageToken"), ["change/next+2="]);
    for request in &requests {
        auth(request);
        assert_eq!(request.method, "GET");
        assert!(request.body.is_empty());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn changes_reject_ambiguous_missing_repeated_or_empty_tokens() {
    for body in [
        r#"{"changes":[],"nextPageToken":"next","newStartPageToken":"new"}"#,
        r#"{"changes":[]}"#,
        r#"{"changes":[],"nextPageToken":"current"}"#,
        r#"{"changes":[],"nextPageToken":""}"#,
    ] {
        let server = TestServer::start(move |_| Reply::json(200, body)).await;
        let error = client(&server)
            .list_changes_page(
                &PageToken::new("current").unwrap(),
                &ChangesOptions::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error.kind(),
            ErrorKind::Protocol(_) | ErrorKind::Concord
        ));
        assert_eq!(error.meta().unwrap().status.as_u16(), 200);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn page_bounds_failures_empty_pages_and_item_streams_are_explicit() {
    let replies = Script::new([
        Reply::json(200, br#"{"files":[],"nextPageToken":"empty-next"}"#),
        Reply::json(200, FILES_2),
        Reply::json(200, FILES_1),
        Reply::json(200, FILES_1),
        Reply::json(403, RATE_LIMIT),
    ]);
    let server = TestServer::start(move |_| replies.next()).await;
    let drive = client(&server);
    let mut stream = drive
        .files(ListOptions::default(), Resume::Start, cap(2))
        .items()
        .into_stream();
    assert_eq!(stream.next().await.unwrap().unwrap().id, "file-other");
    assert!(stream.next().await.is_none());
    let incomplete = drive
        .files(ListOptions::default(), Resume::Start, cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    assert_eq!(
        incomplete.error.resume().unwrap().key().unwrap().as_str(),
        "files/+2=é"
    );
    let incomplete = drive
        .files(ListOptions::default(), Resume::Start, cap(3))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert!(matches!(incomplete.error, PagesError::Provider { .. }));
    server.shutdown().await;
}

#[tokio::test]
async fn incomplete_search_and_nonadvancing_file_tokens_do_not_claim_exhaustion() {
    let replies = Script::new([
        Reply::json(200, br#"{"files":[],"incompleteSearch":true}"#),
        Reply::json(200, br#"{"files":[],"nextPageToken":"same"}"#),
    ]);
    let server = TestServer::start(move |_| replies.next()).await;
    let drive = client(&server);
    let mut pages = drive.files(ListOptions::default(), Resume::Start, cap(2));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Provider { .. })
    ));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    let error = drive
        .list_files_page(
            &ListOptions::default(),
            Some(&PageToken::new("same").unwrap()),
        )
        .await
        .unwrap_err();
    assert!(matches!(error.kind(), ErrorKind::Protocol(_)));
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_configuration_and_options_are_rejected_without_io() {
    for origin in [
        "http://googleapis.com",
        "https://synthetic-user@example.invalid",
        "https://example.invalid/path",
        "https://example.invalid?secret=1",
        "https://example.invalid#frag",
        "file:///tmp/",
    ] {
        assert!(
            DriveClient::with_config(
                TOKEN,
                ClientConfig {
                    origin: origin.into(),
                    ..ClientConfig::default()
                }
            )
            .is_err()
        );
    }
    for token in ["", "Bearer has whitespace", "synthetic\r\nx-injected: yes"] {
        assert!(DriveClient::new(token).is_err());
    }
    let server = TestServer::start(|_| panic!("invalid options must not send")).await;
    let drive = client(&server);
    assert!(
        drive
            .list_files_page(
                &ListOptions {
                    page_size: 0,
                    ..ListOptions::default()
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        drive
            .list_changes_page(
                &PageToken::new("t").unwrap(),
                &ChangesOptions {
                    page_size: 1001,
                    ..ChangesOptions::default()
                }
            )
            .await
            .is_err()
    );
    assert!(
        drive
            .initiate_upload(metadata(), "application/octet-stream", 0, &fields())
            .await
            .is_err()
    );
    for id in ["", ".", ".."] {
        assert!(drive.get_file(id, &fields()).await.is_err());
    }
    assert!(PageToken::new("").is_err());
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn configured_chunk_bound_and_failed_308_body_preserve_confirmed_state() {
    let server = TestServer::start(|request| {
        if request.method == "POST" {
            initiation(request)
        } else {
            Reply::new(308)
                .header("range", "bytes=0-262143")
                .body(vec![b'x'; 65])
        }
    })
    .await;
    let drive = DriveClient::with_config(
        TOKEN,
        ClientConfig {
            origin: server.url(),
            max_chunk_bytes: CHUNK_ALIGNMENT,
            max_response_bytes: 64,
            ..ClientConfig::default()
        },
    )
    .unwrap();
    let mut session = drive
        .initiate_upload(
            metadata(),
            "application/octet-stream",
            3 * CHUNK_ALIGNMENT as u64,
            &fields(),
        )
        .await
        .unwrap()
        .value;
    assert!(
        session
            .send_chunk(&vec![0; 2 * CHUNK_ALIGNMENT])
            .await
            .is_err()
    );
    assert_eq!(server.requests().len(), 1);
    assert!(!session.needs_probe());
    let error = session
        .send_chunk(&vec![0; CHUNK_ALIGNMENT])
        .await
        .unwrap_err();
    assert_eq!(error.concord().unwrap().kind(), concord::ErrorKind::Limit);
    assert_eq!(error.meta().unwrap().status.as_u16(), 308);
    assert_eq!(session.acknowledged(), 0);
    assert!(session.needs_probe());
    server.shutdown().await;
}

#[tokio::test]
async fn failed_final_metadata_requires_probe_and_never_repeats_final_bytes() {
    let replies = Script::new([Reply::json(201, b"{not-json"), Reply::json(200, CREATED)]);
    let server = TestServer::start(move |request| {
        if request.method == "POST" {
            initiation(request)
        } else {
            replies.next()
        }
    })
    .await;
    let mut session = client(&server)
        .initiate_upload(
            metadata(),
            "application/octet-stream",
            SMALL.len() as u64,
            &fields(),
        )
        .await
        .unwrap()
        .value;
    let error = session.send_chunk(SMALL).await.unwrap_err();
    assert_eq!(error.concord().unwrap().kind(), concord::ErrorKind::Decode);
    assert_eq!(error.meta().unwrap().status.as_u16(), 201);
    assert!(session.needs_probe());
    assert!(!session.is_complete());
    assert!(session.send_chunk(SMALL).await.is_err());
    assert!(matches!(
        session.probe().await.unwrap(),
        UploadStatus::Complete(_)
    ));
    assert_eq!(server.requests().len(), 3);
    assert!(server.requests()[2].body.is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn initiation_requires_documented_status_and_empty_body() {
    let server = TestServer::start(|request| {
        let reply = initiation(request);
        match request.header("x-upload-content-length") {
            Some("1") => Reply {
                status: 201,
                ..reply
            },
            Some("2") => reply.body(b"unexpected file resource"),
            _ => panic!("unexpected initiation"),
        }
    })
    .await;
    let drive = client(&server);
    let error = drive
        .initiate_upload(metadata(), "application/octet-stream", 1, &fields())
        .await
        .unwrap_err();
    assert!(matches!(error.kind(), ErrorKind::Protocol(_)));
    assert_eq!(error.meta().unwrap().status.as_u16(), 201);
    let error = drive
        .initiate_upload(metadata(), "application/octet-stream", 2, &fields())
        .await
        .unwrap_err();
    assert_eq!(error.concord().unwrap().kind(), concord::ErrorKind::Limit);
    assert_eq!(error.meta().unwrap().status.as_u16(), 200);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
