//! Secrets: the bearer token (`DriveClient::with_config`, `src/client.rs:72`; `#[sensitive]
//! token`, `src/api.rs:10`, sent as `Authorization: Bearer`), the `#[sensitive]` files and
//! changes `pageToken` values (`src/api.rs:20,50`; `PageToken` withholds them,
//! `src/model.rs:121`) and the resumable upload session URL (a "sensitive capability",
//! `src/upload.rs:17`, withheld by `UploadSession`/`UploadCheckpoint`). Each has its own canary.
//! The page tokens travel as query values; the session URL comes from the initiation's
//! `Location` (its `upload_id`) and travels as the chunk's request target. The server echoes the
//! token into headers, the provider error message and reason (withheld by `ProviderError` and
//! `ProviderReason`) and every malformed, wrong-media and truncated body. Read (`get_file`) and
//! write (`create_file`) cover the seven scenarios, with `list_files_page`, a resumed `files`
//! walk, `list_changes_page` and `initiate_upload` beside them; the success case also uploads
//! a chunk and restores a checkpoint. A token refused by `with_config` gets its own canary in a
//! separate test (it never travels).
//! Not covered: native reqwest values, the explicit clear accessors (`PageToken::as_str`,
//! `UploadSession::session_url`), clear decoded models (`File`); the generated `DriveApi` is
//! crate-private (no public endpoint or `Prepared`). No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{net::TcpListener, num::NonZeroUsize, time::Duration};

fn refused() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: Reply, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => success,
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_google_drive::pagination::Resume;
use concord_sdk_google_drive::{
    ChangesOptions, ClientConfig, DriveClient, Error, FileField, FileFields, ListOptions, NewFile,
    PageToken, UploadStatus,
};

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    c.assert_debug_absent("error metadata", &error.meta());
    c.assert_debug_absent("provider error", &error.provider());
    if let Some(provider) = error.provider() {
        c.assert_debug_absent("provider reasons", &provider.errors);
    }
    if let Some(e) = error.concord() {
        concord_error(c, e);
    }
}

fn result<T: std::fmt::Debug>(c: &mut Canaries, label: &str, result: &Result<T, Error>) {
    c.assert_debug_absent(label, result);
    if let Err(e) = result {
        error(c, e);
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("google-drive");
    let token = c.issue();
    let files_token = PageToken::new(c.issue()).unwrap();
    let changes_token = PageToken::new(c.issue()).unwrap();
    let session = c.issue();
    c.assert_debug_absent("files page token", &files_token);
    let fields = FileFields::only(&[FileField::Name, FileField::Size]);
    for case in 0..7 {
        let (echo, session) = (token.clone(), session.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let file = Reply::json(200, r#"{"id":"file-1","name":"a.bin","size":"3"}"#);
            let success = match (r.method.as_str(), r.path()) {
                ("POST", "/upload/drive/v3/files") => Reply::new(200).header(
                    "location",
                    format!(
                        "http://{}/upload/drive/v3/files?uploadType=resumable&upload_id={session}",
                        r.host
                    ),
                ),
                (_, "/drive/v3/files") if r.method == "GET" => {
                    Reply::json(200, r#"{"files":[],"incompleteSearch":false}"#)
                }
                (_, "/drive/v3/changes") => {
                    Reply::json(200, r#"{"changes":[],"newStartPageToken":"start-2"}"#)
                }
                _ => file,
            };
            let problem = format!(
                r#"{{"error":{{"code":401,"message":"{echo}","errors":[{{"domain":"global","reason":"authError","message":"{echo}","location":"Authorization","locationType":"header"}}],"status":"UNAUTHENTICATED"}}}}"#
            );
            reply(case, success, &problem, &echo)
        })
        .await;
        let config = ClientConfig {
            origin: if case == 6 { refused() } else { server.url() },
            timeout: Duration::from_secs(2),
            ..ClientConfig::default()
        };
        c.assert_debug_absent("config", &config);
        let drive = DriveClient::with_config(&token, config).unwrap();
        c.assert_debug_absent("client", &drive);
        let read = drive.get_file("file-1", &fields).await;
        result(&mut c, "read Result", &read);
        assert_eq!(read.is_ok(), case == 0);
        let metadata = NewFile {
            name: "a.bin".into(),
            ..NewFile::default()
        };
        let write = drive.create_file(metadata.clone(), &fields).await;
        result(&mut c, "write Result", &write);
        assert_eq!(write.is_ok(), case == 0);
        let list = drive
            .list_files_page(&ListOptions::default(), Some(&files_token))
            .await;
        result(&mut c, "files page Result", &list);
        let mut walk = drive.files(
            ListOptions::default(),
            Resume::At(files_token.clone()),
            NonZeroUsize::new(2).unwrap(),
        );
        c.assert_debug_absent("files walk", &walk);
        match walk.next_page().await {
            Ok(page) => {
                assert_eq!(case, 0);
                c.assert_debug_absent("files walk page", &page);
            }
            Err(e) => {
                c.assert_debug_absent("PagesError", &e);
                c.assert_display_absent("PagesError", &e);
                c.assert_error_absent("PagesError and sources", &e);
                error(&mut c, e.provider_error().unwrap());
            }
        }
        let changes = drive
            .list_changes_page(&changes_token, &ChangesOptions::default())
            .await;
        result(&mut c, "changes page Result", &changes);
        let initiated = drive
            .initiate_upload(metadata, "application/octet-stream", 3, &fields)
            .await;
        result(&mut c, "upload initiation Result", &initiated);
        if case == 0 {
            let response = initiated.unwrap();
            c.assert_debug_absent("initiation metadata", &response.meta);
            let mut upload = response.value;
            c.assert_debug_absent("upload session", &upload);
            let checkpoint = upload.checkpoint();
            c.assert_debug_absent("upload checkpoint", &checkpoint);
            let status = upload.send_chunk(b"abc").await;
            result(&mut c, "chunk Result", &status);
            assert!(matches!(status, Ok(UploadStatus::Complete(_))));
            let restored = drive.restore_upload(checkpoint);
            result(&mut c, "restored session", &restored);
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 250);
    println!(
        "google-drive: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn refused_token_never_prints() {
    let mut c = Canaries::new("google-drive");
    let token = format!("{} with spaces", c.issue());
    let e = DriveClient::new(token).unwrap_err();
    error(&mut c, &e);
}
