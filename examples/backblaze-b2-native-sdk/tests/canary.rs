//! Secret inputs, each with its own canary:
//! - key ID and application key (`ApplicationKey::new`, src/model.rs:25): the Basic credential
//!   of `b2_authorize_account` (`#[sensitive]` account scope, src/wire.rs:25); checked decoded.
//! - account authorization token (response-only: `authorizationToken` of the authorize fixture;
//!   `#[sensitive]` storage/download scopes, src/wire.rs:31,51): sent on list, delete, download
//!   and `b2_get_upload_url`.
//! - upload token and issued upload URL (response-only: `b2_get_upload_url` fixture; upload scope
//!   src/wire.rs:60; `UploadTarget` withholds both, src/model.rs:363): canary in the URL's path and
//!   `cvt` query, sent as the upload request target with the token in `Authorization`.
//! - account ID (response-only, withheld by `AccountInfo`/`FileVersion` Debug, src/model.rs:241,280).
//! - file content (errors never contain it, src/error.rs:1-4; `Download` Debug prints its length):
//!   the upload body and the download fixture body.
//!
//! Provider error `message` (withheld, src/error.rs:219), extra response headers, malformed/HTML/
//! truncated bodies and an issued URL's userinfo echo every canary. Read: name listing and
//! download; write: upload and version delete; authorize too: 2xx, 400 provider document, 503,
//! malformed, wrong Content-Type, truncated, refused (issued bases/upload URL point at a closed
//! port). Wire exemption: the account ID only — it is response-only and B2 never takes it back.
//! Not covered: native reqwest values; `Prepared`/scope handles, `Storage` and
//! `FileNameTraversal` (private or no `Debug`); explicit clear accessors (`account_id()`,
//! `upload_url()`, `message()`, `Download::bytes`), which the test reads to prove each fixture
//! reached the decoder.

use concord_check_canary::{Canaries, wire_text};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_backblaze_b2_native::concord::{self, bytes::Bytes};
use concord_sdk_backblaze_b2_native::{
    ApplicationKey, BucketId, Client, ContentType, Error, ErrorCode, Evidence, FileId, FileName,
    FileNameQuery, Session, Sha1Hex, UploadFile, UploadPolicy, UploadTarget, Violation,
    http_client_builder,
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{num::NonZeroUsize, time::Duration};

const BUCKET: &str = "bucket-1";
const NAME: &str = "reports/a.txt";
const FILE_ID: &str = "4_version_1";
const TAMPERED: &str = "4_version_tampered";

fn http() -> concord::reqwest::Client {
    http_client_builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_example_testserver::refused_url()
}

fn reply(case: usize, success: Reply, echo: &str) -> Reply {
    let problem = |status: u16, code: &str| {
        let doc = json!({"status": status, "code": code, "message": echo});
        Reply::json(status, doc.to_string())
    };
    let response = match case {
        0 | 6 => success,
        1 => problem(400, "bad_request"),
        2 => problem(503, "service_unavailable"),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "").with_body(Body::FailAfter(vec![Bytes::from(format!(
            "{{\"incomplete\":\"{echo}"
        ))])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn authorize_doc(base: &str, account_id: &str, token: &str) -> String {
    json!({
        "accountId": account_id, "authorizationToken": token,
        "apiInfo": {"storageApi": {"apiUrl": base, "downloadUrl": base, "allowed": {
            "buckets": [{"id": BUCKET, "name": "reports"}],
            "capabilities": ["listFiles", "readFiles", "writeFiles", "deleteFiles"],
            "namePrefix": null
        }}},
        "applicationKeyExpirationTimestamp": null
    })
    .to_string()
}

/// One B2 stand-in for the account, API, upload and download hosts. Authorize and
/// `b2_get_upload_url` always answer; the operations under test get the case's reply. In the
/// refused case the second authorization issues closed-port bases and every upload URL is there.
struct Fixture {
    case: usize,
    account_id: String,
    account_token: String,
    upload_token: String,
    upload_url: String,
    content: String,
    echo: String,
    refused: String,
    authorizations: AtomicUsize,
}

impl Fixture {
    fn answer(&self, r: &Recorded) -> Reply {
        let own = format!("http://{}", r.host);
        let path = r.path();
        if path.ends_with("/b2_authorize_account") {
            let n = self.authorizations.fetch_add(1, Ordering::SeqCst);
            let base = if self.case == 6 && n > 0 {
                &self.refused
            } else {
                &own
            };
            let doc = authorize_doc(base, &self.account_id, &self.account_token);
            return Reply::json(200, doc);
        }
        if path.ends_with("/b2_get_upload_url") {
            let base = if self.case == 6 { &self.refused } else { &own };
            let u = &self.upload_url;
            let doc = json!({"bucketId": BUCKET, "authorizationToken": self.upload_token,
                "uploadUrl": format!("{base}/b2api/v4/b2_upload_file/{BUCKET}/{u}?cvt={u}&bucket={BUCKET}")});
            return Reply::json(200, doc.to_string());
        }
        let sha1 = Sha1Hex::of(self.content.as_bytes()).to_string();
        let version = json!({
            "accountId": self.account_id, "bucketId": BUCKET, "action": "upload",
            "fileId": FILE_ID, "fileName": NAME, "contentLength": self.content.len(),
            "contentSha1": sha1, "contentType": "text/plain", "fileInfo": {}, "uploadTimestamp": 1
        });
        let success = match path.rsplit('/').find(|s| s.starts_with("b2_")) {
            Some("b2_list_file_names") => Reply::json(
                200,
                json!({"files": [version], "nextFileName": null}).to_string(),
            ),
            Some("b2_delete_file_version") => Reply::json(
                200,
                json!({"fileName": NAME, "fileId": FILE_ID}).to_string(),
            ),
            Some("b2_upload_file") => Reply::json(200, version.to_string()),
            Some("b2_download_file_by_id") => {
                let tampered = r.query_all("fileId") == [TAMPERED];
                let digest = if tampered {
                    Sha1Hex::of(b"other").to_string()
                } else {
                    sha1
                };
                let id = if tampered { TAMPERED } else { FILE_ID };
                Reply::new(200)
                    .header("content-type", "text/plain")
                    .header("x-bz-file-id", id)
                    .header("x-bz-content-sha1", digest)
                    .body(&self.content)
            }
            _ => panic!("unexpected request {}", r.target),
        };
        reply(self.case, success, &self.echo)
    }
}

fn concord_error(c: &mut Canaries, e: &concord::Error) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error", e);
    c.assert_debug_absent("error status", &e.status());
    c.assert_debug_absent("error code", &e.code());
    match e {
        Error::Concord(e) => concord_error(c, e),
        Error::Provider(p) => {
            c.assert_debug_absent("ProviderError", p);
            c.assert_debug_absent("provider metadata", p.meta());
            c.assert_debug_absent("provider document", &p.document());
            c.assert_debug_absent("inconsistent document", &p.inconsistent_document());
            if let Some(code) = p.code() {
                c.assert_display_absent("provider code", code);
            }
            if let Some(e) = p.unreadable() {
                concord_error(c, e);
            }
        }
        Error::Contract(x) => {
            c.assert_debug_absent("ContractError", x);
            c.assert_debug_absent("contract metadata", &x.meta());
        }
        Error::Checksum(x) => {
            c.assert_debug_absent("ChecksumError", x);
            c.assert_debug_absent("checksum failure", x.failure());
            c.assert_debug_absent("checksum metadata", x.meta());
        }
        Error::Input(i) => c.assert_error_absent("InputError", i),
        _ => {}
    }
}

/// Uploads the canary content once; returns the last attempt's evidence and the stored outcome.
async fn upload(
    c: &mut Canaries,
    session: &Session,
    target: &mut UploadTarget,
    content: &str,
    account_id: &str,
) -> (Evidence, bool) {
    let file = UploadFile::bytes(
        FileName::new(NAME).unwrap(),
        ContentType::new("text/plain").unwrap(),
        content.as_bytes().to_vec(),
    );
    c.assert_debug_absent("upload file", &file);
    let policy = UploadPolicy::once();
    c.assert_debug_absent("upload policy", &policy);
    let result = session.upload_file(target, file, policy).await;
    c.assert_debug_absent("upload Result", &result);
    c.assert_debug_absent("upload target after upload", target);
    match result {
        Ok(u) => {
            c.assert_debug_absent("stored file version", &u.file);
            c.assert_debug_absent("upload report", &u.report);
            c.assert_debug_absent("upload attempts", u.report.attempts());
            assert_eq!(u.file.account_id, account_id); // the response-only value was decoded
            (u.report.attempts()[0].evidence(), true)
        }
        Err(e) => {
            c.assert_error_absent("UploadError", &e);
            c.assert_debug_absent("upload failure report", e.report());
            c.assert_debug_absent("upload retry report", e.report().retry());
            c.assert_debug_absent("upload stop reason", &e.stop());
            error(c, e.error());
            let last = e.report().attempts().last().unwrap().clone();
            if last.status() == Some(400) {
                // Safe diagnosis: status, provider code and evidence survive withholding.
                assert_eq!(last.evidence(), Evidence::Refused);
                assert_eq!(last.code(), Some(&ErrorCode::BadRequest));
                assert!(e.to_string().contains("HTTP status 400 (bad_request)"));
            }
            (last.evidence(), false)
        }
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("backblaze-b2-native");
    let key_id = c.issue();
    let application_key = c.issue();
    let account_token = c.issue();
    let upload_token = c.issue();
    let upload_url = c.issue();
    let account_id = c.issue();
    let content = c.issue();
    let echo = c.issued().join("-");

    let key = ApplicationKey::new(&key_id, &application_key).unwrap();
    c.assert_debug_absent("application key", &key);
    let refusal = ApplicationKey::new(&format!("{key_id}:"), &application_key).unwrap_err();
    c.assert_debug_absent("application key refusal", &refusal);
    c.assert_error_absent("application key refusal", &refusal);
    let base = format!("http://{key_id}:{application_key}@127.0.0.1:1");
    let refusal = Client::new(http()).with_base_url(&base).unwrap_err();
    error(&mut c, &refusal);
    assert_eq!(refusal.to_string(), "invalid B2 input: base URL");

    for case in 0..7 {
        let fixture = Fixture {
            case,
            account_id: account_id.clone(),
            account_token: account_token.clone(),
            upload_token: upload_token.clone(),
            upload_url: upload_url.clone(),
            content: content.clone(),
            echo: echo.clone(),
            refused: refused(),
            authorizations: AtomicUsize::new(0),
        };
        let server = TestServer::start(move |r: &Recorded| fixture.answer(r)).await;
        let client = Client::new(http()).with_base_url(&server.url()).unwrap();
        c.assert_debug_absent("client", &client);

        // Authorize itself under the case: the application key's operation.
        let (aid, token, e) = (account_id.clone(), account_token.clone(), echo.clone());
        let account = TestServer::start(move |r: &Recorded| {
            let doc = authorize_doc(&format!("http://{}", r.host), &aid, &token);
            reply(case, Reply::json(200, doc), &e)
        })
        .await;
        let direct = Client::new(http())
            .with_base_url(&if case == 6 { refused() } else { account.url() })
            .unwrap();
        let result = direct.authorize_account(&key).await;
        c.assert_debug_absent("authorize Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match &result {
            Ok(session) => c.assert_debug_absent("direct session", session),
            Err(e) => {
                error(&mut c, e);
                if case == 1 {
                    assert_eq!(e.code(), Some(&ErrorCode::BadRequest));
                    assert_eq!(e.provider().unwrap().message(), Some(echo.as_str()));
                    let shown = e.to_string();
                    assert!(shown.contains("authorize_account"), "{shown}");
                    assert!(shown.contains("HTTP status 400 (bad_request)"), "{shown}");
                }
                if case == 6 {
                    assert_eq!(e.status(), None);
                }
            }
        }

        // Setup: always-answering authorizations; the second one is the scenario session.
        let live = client.authorize_account(&key).await.unwrap();
        let session = client.authorize_account(&key).await.unwrap();
        assert_eq!(session.account().account_id(), account_id);
        c.assert_debug_absent("session", &session);
        c.assert_debug_absent("account info", session.account());
        c.assert_debug_absent("allowed", session.account().allowed());
        let bucket = BucketId::new(BUCKET).unwrap();
        let mut target = live.get_upload_url(&bucket).await.unwrap();
        c.assert_debug_absent("upload target", &target);
        assert!(target.upload_url().unwrap().as_str().contains(&upload_url));

        // Read: one name-listing page.
        let query = FileNameQuery {
            prefix: Some("reports/".into()),
            ..FileNameQuery::new(bucket.clone())
        };
        c.assert_debug_absent("file-name query", &query);
        let mut walk = session
            .storage()
            .list_file_names(query, NonZeroUsize::new(2).unwrap());
        let page = walk.next_page().await;
        c.assert_debug_absent("list Result", &page);
        assert_eq!(page.is_ok(), case == 0);
        match &page {
            Ok(page) => {
                let page = page.as_ref().unwrap();
                c.assert_debug_absent("listed files", &page.files);
                c.assert_debug_absent("list metadata", &page.meta);
                assert_eq!(page.files[0].file_name, NAME);
            }
            Err(e) => {
                error(&mut c, e);
                if case == 1 {
                    // Safe diagnosis: operation, status and code stay in Display/accessors.
                    assert_eq!(e.status().map(|s| s.as_u16()), Some(400));
                    assert_eq!(e.code(), Some(&ErrorCode::BadRequest));
                    let shown = e.to_string();
                    assert!(shown.contains("list_file_names"), "{shown}");
                    assert!(shown.contains("HTTP status 400 (bad_request)"), "{shown}");
                }
            }
        }
        c.assert_debug_absent("resume point", &walk.resume_point());

        // Write: one upload at the held upload URL, then one that needs a new URL if retired.
        let (evidence, stored) = upload(&mut c, &session, &mut target, &content, &account_id).await;
        assert_eq!(stored, case == 0);
        if case == 6 {
            // The closed port's refusal is classified by Concord's delivery (not asserted here).
            assert!(matches!(evidence, Evidence::NotSent | Evidence::NoAnswer));
        }
        let (evidence, stored) = upload(&mut c, &session, &mut target, &content, &account_id).await;
        assert_eq!(stored, case == 0);
        if case == 6 {
            assert_eq!(evidence, Evidence::NoUploadUrl);
        }

        // Write: one version delete.
        let (name, id) = (FileName::new(NAME).unwrap(), FileId::new(FILE_ID).unwrap());
        let deleted = session.storage().delete_file_version(&name, &id).await;
        c.assert_debug_absent("delete Result", &deleted);
        assert_eq!(deleted.is_ok(), case == 0);
        match &deleted {
            Ok(d) => c.assert_debug_absent("delete metadata", &d.meta),
            Err(e) => {
                c.assert_error_absent("DeleteError", e);
                error(&mut c, e.error());
                if case == 1 {
                    assert!(!e.may_have_deleted());
                    assert!(e.to_string().contains("HTTP status 400"));
                }
            }
        }

        // Read: one verified download, and in the success case a tampered one.
        let downloaded = session.download_file_by_id(&id, 1024).await;
        c.assert_debug_absent("download Result", &downloaded);
        assert_eq!(downloaded.is_ok(), case == 0);
        match &downloaded {
            Ok(d) => {
                c.assert_debug_absent("downloaded file", &d.file);
                c.assert_debug_absent("download metadata", &d.meta);
                assert_eq!(d.bytes, content.as_bytes()); // returned in clear by design
            }
            Err(e) => error(&mut c, e),
        }

        if case == 0 {
            let tampered = FileId::new(TAMPERED).unwrap();
            let e = session
                .download_file_by_id(&tampered, 1024)
                .await
                .unwrap_err();
            error(&mut c, &e);
            assert!(matches!(e, Error::Checksum(_)));
            assert_eq!(
                e.to_string(),
                "downloaded bytes do not match X-Bz-Content-Sha1"
            );

            // An issued base carrying every canary as userinfo is refused without echoing it.
            let doc = authorize_doc(
                &format!("http://u:{echo}@127.0.0.1:1"),
                &account_id,
                &account_token,
            );
            let bad = TestServer::start(move |_: &Recorded| Reply::json(200, doc.clone())).await;
            let e = Client::new(http())
                .with_base_url(&bad.url())
                .unwrap()
                .authorize_account(&key)
                .await
                .unwrap_err();
            error(&mut c, &e);
            assert!(matches!(&e, Error::Contract(x) if x.violation() == Violation::IssuedUrl));
            bad.shutdown().await;

            // Positive control: every canary but the response-only account ID was sent.
            let mut requests = account.requests();
            requests.extend(server.requests());
            c.assert_on_wire_except(&requests, &[&account_id]);
            let basic = format!("{key_id}:{application_key}");
            for r in &requests {
                let wire = wire_text(r);
                assert!(!wire.contains(&account_id));
                if r.path().ends_with("/b2_authorize_account") {
                    assert!(r.header("authorization").unwrap().starts_with("Basic "));
                    assert!(wire.contains(&basic));
                } else if r.path().contains("/b2_upload_file/") {
                    assert_eq!(r.header_all("authorization"), [upload_token.as_str()]);
                    assert!(r.target.contains(&format!("cvt={upload_url}")));
                    assert_eq!(r.body, content.as_bytes());
                } else {
                    assert_eq!(r.header_all("authorization"), [account_token.as_str()]);
                }
            }
        }
        if case == 6 {
            // Refused: nothing but the setup calls reached the stand-in.
            for r in server.requests() {
                let p = r.path();
                assert!(p.ends_with("/b2_authorize_account") || p.ends_with("/b2_get_upload_url"));
            }
        }
        account.shutdown().await;
        server.shutdown().await;
    }
    assert!(c.checked() >= 300);
    println!(
        "backblaze-b2-native: {} renderings; positive control passed (account ID exempt: response-only)",
        c.checked()
    );
}
