//! Wire-level request shapes, every documented error class, malformed / truncated / oversized /
//! wrong-media bodies, documented-rule violations and redaction, against the local stand-in.

mod common;

use common::*;
use concord_example_testserver::{Body, Reply};
use concord_sdk_backblaze_b2_native::concord::ErrorKind;
use concord_sdk_backblaze_b2_native::concord::bytes::Bytes;
use concord_sdk_backblaze_b2_native::{
    ApplicationKey, BucketId, ChecksumFailure, Client, ContentType, ERROR_BODY_LIMIT, Error,
    ErrorCode, Evidence, FileId, FileName, InputError, JSON_BODY_LIMIT, Session, Sha1Hex,
    UploadFile, UploadPolicy, UploadTarget, Violation,
};
use serde_json::{Value, json};

const MIB: usize = 1024 * 1024;

fn hello_file() -> UploadFile {
    UploadFile::bytes(
        FileName::new("hello-b2.txt").unwrap(),
        ContentType::new("text/plain").unwrap(),
        hello(),
    )
}

fn set_authorize(b2: &FakeB2, reply: impl Fn() -> Reply + Send + 'static) {
    b2.state.lock().unwrap().authorize_reply = Some(Box::new(move |_| reply()));
}

fn concord_kind(error: &Error) -> ErrorKind {
    match error {
        Error::Concord(e) => e.kind(),
        other => panic!("not a Concord error: {other:?}"),
    }
}

async fn stored(b2: &FakeB2, session: &Session) -> FileId {
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let uploaded = session
        .upload_file(&mut target, hello_file(), UploadPolicy::once())
        .await
        .unwrap();
    assert_eq!(
        b2.stored_ids().last().unwrap(),
        uploaded.file.file_id.as_str()
    );
    uploaded.file.file_id
}

// --- Request shapes -------------------------------------------------------------------------

#[tokio::test]
async fn authorize_account_sends_basic_credentials_and_decodes_the_documented_example() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let request = b2.auth.single();
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, "/b2api/v4/b2_authorize_account");
    assert_eq!(
        request.header("authorization"),
        credentials()["authorization"].as_str()
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.body_len, 0);

    let account = session.account();
    assert_eq!(account.account_id(), "ACCOUNT_ID");
    let allowed = account.allowed();
    assert_eq!(allowed.buckets.len(), 3);
    assert_eq!(allowed.buckets[0].id.as_str(), "bucketId1");
    assert_eq!(allowed.buckets[0].name.as_deref(), Some("bucketIdName1"));
    assert_eq!(allowed.buckets[2].name, None);
    assert!(allowed.has_capability("writeFiles") && allowed.has_capability("readFiles"));
    assert_eq!(allowed.name_prefix, None);
    assert_eq!(account.key_expiration_millis(), None);
}

#[tokio::test]
async fn get_upload_url_goes_to_the_issued_api_url_with_the_account_token() {
    let b2 = FakeB2::start().await;
    // The assembled documented answer, verbatim: an https upload URL with a query.
    b2.push_get_upload_url(Reply::json(200, fixture("get_upload_url.json")));
    let session = b2.session().await;
    let target = session.get_upload_url(&bucket()).await.unwrap();
    let request = b2.api.single();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.target,
        format!("/b2api/v4/b2_get_upload_url?bucketId={BUCKET}")
    );
    assert_eq!(request.header("authorization"), Some(ACCOUNT_TOKEN));
    assert_eq!(request.header("accept"), Some("application/json"));
    let documented = fixture_json("get_upload_url.json");
    assert_eq!(
        target.upload_url().map(|u| u.as_str()),
        documented["uploadUrl"].as_str()
    );
    assert_eq!(target.bucket_id().as_str(), BUCKET);
    assert!(b2.auth.requests().len() == 1 && b2.upload.requests().is_empty());
}

#[tokio::test]
async fn upload_file_posts_to_the_issued_url_verbatim_with_length_and_sha1() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let file = hello_file().with_last_modified_millis(1_559_585_435_000);
    session
        .upload_file(&mut target, file, UploadPolicy::once())
        .await
        .unwrap();

    let request = b2.upload.single();
    let facts = hello_facts();
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.target,
        format!("/b2api/v4/b2_upload_file?cvt=c001_v0001005_t0001&bucket={BUCKET}")
    );
    assert_eq!(
        request.header("authorization"),
        Some("2_upload_token_1_upld")
    );
    assert_eq!(request.header("x-bz-file-name"), Some("hello-b2.txt"));
    assert_eq!(request.header("content-type"), Some("text/plain"));
    assert_eq!(request.header("content-length"), Some("63"));
    assert_eq!(request.header("x-bz-content-sha1"), facts["sha1"].as_str());
    assert_eq!(
        request.header("x-bz-info-src_last_modified_millis"),
        Some("1559585435000")
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert!(!request.chunked);
    assert_eq!(request.body, hello());
}

#[tokio::test]
async fn a_path_form_upload_url_is_followed_verbatim() {
    let b2 = FakeB2::start().await;
    b2.state.lock().unwrap().path_form = true;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    session
        .upload_file(&mut target, hello_file(), UploadPolicy::once())
        .await
        .unwrap();
    assert_eq!(
        b2.upload.single().target,
        format!("/b2api/v4/b2_upload_file/{BUCKET}/c001_v0001005_t0001")
    );
}

#[tokio::test]
async fn file_names_are_percent_encoded_utf8() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let facts = hello_facts();
    let name = FileName::new(facts["encodedNameInput"].as_str().unwrap()).unwrap();
    assert_eq!(
        name.header_value(),
        facts["encodedNameHeader"].as_str().unwrap()
    );
    let file = UploadFile::bytes(name, ContentType::auto(), hello());
    session
        .upload_file(&mut target, file, UploadPolicy::once())
        .await
        .unwrap();
    assert_eq!(
        b2.upload.single().header("x-bz-file-name"),
        facts["encodedNameHeader"].as_str()
    );
}

#[tokio::test]
async fn a_one_shot_stream_is_sent_with_its_declared_content_length() {
    struct Chunks(Vec<Bytes>);
    impl futures_core::Stream for Chunks {
        type Item = Result<Bytes, std::io::Error>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            let this = self.get_mut();
            std::task::Poll::Ready((!this.0.is_empty()).then(|| Ok(this.0.remove(0))))
        }
    }
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let bytes = hello();
    // A source longer than declared: exactly the declared bytes go out (the transport writes at
    // most `Content-Length`), and the source is not polled past them.
    let mut longer = bytes.clone();
    longer.extend_from_slice(b"EXTRA BYTES NOT SENT");
    let chunks: Vec<Bytes> = longer.chunks(10).map(Bytes::copy_from_slice).collect();
    let file = UploadFile::stream(
        FileName::new("hello-b2.txt").unwrap(),
        ContentType::new("text/plain; charset=utf-8").unwrap(),
        63,
        Sha1Hex::new(&hello_sha1()).unwrap(),
        Chunks(chunks),
    )
    .unwrap();
    assert!(!file.is_replayable());
    session
        .upload_file(&mut target, file, UploadPolicy::once())
        .await
        .unwrap();
    let request = b2.upload.single();
    assert_eq!(request.header("content-length"), Some("63"));
    assert!(!request.chunked);
    assert_eq!(request.body, bytes);
    assert_eq!(
        request.header("content-type"),
        Some("text/plain; charset=utf-8")
    );
}

#[tokio::test]
async fn an_empty_file_is_sent_with_content_length_zero() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let file = UploadFile::bytes(
        FileName::new("empty").unwrap(),
        ContentType::new("application/octet-stream").unwrap(),
        Vec::new(),
    );
    assert_eq!(
        file.sha1().as_str(),
        hello_facts()["emptySha1"].as_str().unwrap()
    );
    let uploaded = session
        .upload_file(&mut target, file, UploadPolicy::once())
        .await
        .unwrap();
    let request = b2.upload.single();
    assert_eq!(request.header("content-length"), Some("0"));
    assert_eq!(
        request.header("x-bz-content-sha1"),
        hello_facts()["emptySha1"].as_str()
    );
    assert_eq!(uploaded.file.content_length, 0);
}

#[tokio::test]
async fn download_file_by_id_goes_to_the_issued_download_url_with_the_account_token() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let id = stored(&b2, &session).await;
    session.download_file_by_id(&id, MIB).await.unwrap();
    let request = b2.download.single();
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), "/b2api/v4/b2_download_file_by_id");
    assert_eq!(request.query_all("fileId"), [id.as_str()]);
    assert_eq!(request.header("authorization"), Some(ACCOUNT_TOKEN));
    assert_eq!(request.header("range"), None);
}

// --- Every documented error class ---------------------------------------------------------

#[tokio::test]
async fn every_documented_error_code_is_typed_with_its_status() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let id = stored(&b2, &session).await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let rows = fixture_json("errors.json");
    for row in rows.as_array().unwrap() {
        let operation = row["operation"].as_str().unwrap();
        let status = row["status"].as_u64().unwrap() as u16;
        let code: &'static str =
            Box::leak(row["code"].as_str().unwrap().to_owned().into_boxed_str());
        let error = match operation {
            "authorize_account" => {
                set_authorize(&b2, move || error_reply(status, code));
                b2.client().authorize_account(&key()).await.unwrap_err()
            }
            "get_upload_url" => {
                b2.push_get_upload_url(error_reply(status, code));
                session.get_upload_url(&bucket()).await.unwrap_err()
            }
            "upload_file" => {
                b2.push_upload(UploadMode::Fail(status, code));
                let failure = session
                    .upload_file(&mut target, hello_file(), UploadPolicy::once())
                    .await
                    .unwrap_err();
                let [attempt] = failure.report().attempts() else {
                    panic!("{failure:?}")
                };
                let expected = if status >= 500 {
                    Evidence::ServerFailed
                } else {
                    Evidence::Refused
                };
                assert_eq!(attempt.evidence(), expected, "{row}");
                assert_eq!(failure.may_have_stored(), status >= 500, "{row}");
                assert!(
                    !target.is_usable(),
                    "{row}: a rejection retires the upload URL"
                );
                failure.into_error()
            }
            "download_file_by_id" => {
                b2.set_download(DownloadMode::Reply(Box::new(move |_| {
                    error_reply(status, code)
                })));
                session.download_file_by_id(&id, MIB).await.unwrap_err()
            }
            other => panic!("{other}"),
        };
        let typed = error.code().unwrap_or_else(|| panic!("{row}: {error:?}"));
        assert!(!matches!(typed, ErrorCode::Other(_)), "{row}: modeled code");
        assert_eq!(typed.as_str(), code, "{row}");
        assert_eq!(error.status().map(|s| s.as_u16()), Some(status), "{row}");
        let provider = error.provider().unwrap();
        assert_eq!(
            provider.message(),
            Some(format!("synthetic {code}").as_str())
        );
        assert!(
            !error.to_string().contains("synthetic"),
            "{row}: message not displayed"
        );
        b2.set_download(DownloadMode::Faithful);
    }
}

#[tokio::test]
async fn an_unmodeled_code_is_kept_as_sent() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    // The schema example of the error document: a code none of the four pages lists.
    b2.push_get_upload_url(Reply::json(400, fixture("error_document.json")));
    let error = session.get_upload_url(&bucket()).await.unwrap_err();
    assert_eq!(
        error.code(),
        Some(&ErrorCode::Other("invalid_bucket_name".into()))
    );
    assert_eq!(
        error.provider().unwrap().message(),
        Some("bucket name is too long")
    );
}

#[tokio::test]
async fn an_error_body_that_is_not_b2s_keeps_the_status() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let truncated = Reply::new(400)
        .header("content-type", "application/json")
        .with_body(Body::FailAfter(vec![Bytes::from_static(
            b"{\"status\":400,",
        )]));
    let oversized = Reply::json(
        400,
        format!("{{\"pad\":\"{}\"}}", "x".repeat(ERROR_BODY_LIMIT)),
    );
    let cases: Vec<(Reply, Option<ErrorKind>)> = vec![
        (
            Reply::text(503, "text/html", "<html>proxy</html>"),
            Some(ErrorKind::Metadata),
        ),
        (Reply::new(500), Some(ErrorKind::Metadata)),
        (Reply::json(400, "{\"status\":"), Some(ErrorKind::Decode)),
        (oversized, Some(ErrorKind::Limit)),
        (truncated, Some(ErrorKind::Transport)),
        // A well-formed document that contradicts the HTTP status: `None` below.
        (
            Reply::json(
                400,
                json!({"status": 401, "code": "bad_auth_token"}).to_string(),
            ),
            None,
        ),
    ];
    for (i, (reply, kind)) in cases.into_iter().enumerate() {
        let status = reply.status;
        b2.push_get_upload_url(reply);
        let error = session.get_upload_url(&bucket()).await.unwrap_err();
        let provider = error
            .provider()
            .unwrap_or_else(|| panic!("case {i}: {error:?}"));
        assert_eq!(provider.status().as_u16(), status, "case {i}");
        assert_eq!(provider.code(), None, "case {i}");
        match kind {
            Some(kind) => assert_eq!(
                provider.unreadable().map(|e| e.kind()),
                Some(kind),
                "case {i}"
            ),
            None => assert_eq!(
                provider.inconsistent_document().map(|d| d.status()),
                Some(401)
            ),
        }
    }
}

// --- Malformed, truncated, oversized and wrong-media success bodies -------------------------

#[tokio::test]
async fn malformed_success_bodies_are_concord_failures_with_the_head() {
    let b2 = FakeB2::start().await;
    let missing_api_info = json!({"accountId": "a", "authorizationToken": "t"}).to_string();
    let cases: Vec<(Box<dyn Fn() -> Reply + Send>, ErrorKind)> = vec![
        (Box::new(|| Reply::json(200, "{")), ErrorKind::Decode),
        (
            Box::new(move || Reply::json(200, missing_api_info.clone())),
            ErrorKind::Decode,
        ),
        (
            Box::new(|| Reply::text(200, "text/html", "<html></html>")),
            ErrorKind::Metadata,
        ),
        (
            Box::new(|| Reply::new(200).body(fixture("authorize_account.json"))),
            ErrorKind::Metadata,
        ),
        (
            Box::new(|| {
                Reply::new(200)
                    .header("content-type", "application/json")
                    .with_body(Body::FailAfter(vec![Bytes::from_static(
                        b"{\"accountId\":",
                    )]))
            }),
            ErrorKind::Transport,
        ),
        (
            Box::new(|| Reply::json(200, format!("[{}]", "0,".repeat(JSON_BODY_LIMIT / 2) + "0"))),
            ErrorKind::Limit,
        ),
    ];
    for (i, (reply, kind)) in cases.into_iter().enumerate() {
        set_authorize(&b2, reply);
        let error = b2.client().authorize_account(&key()).await.unwrap_err();
        assert_eq!(concord_kind(&error), kind, "case {i}");
        assert_eq!(
            error.status().map(|s| s.as_u16()),
            Some(200),
            "case {i}: head kept"
        );
    }
}

#[tokio::test]
async fn a_documented_content_length_string_or_a_number_is_accepted_and_nothing_else() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    for (length, evidence) in [
        (json!(63), Evidence::Stored),
        (json!("63"), Evidence::Stored),
        (json!("+63"), Evidence::StoredUnverified),
        (json!("63.0"), Evidence::StoredUnverified),
        (json!(64), Evidence::StoredUnverified),
    ] {
        let value = length.clone();
        b2.push_upload(UploadMode::Reply(Box::new(move |_| {
            let mut doc = fixture_json("upload_file.json");
            doc["contentLength"] = value;
            Reply::json(200, doc.to_string())
        })));
        let outcome = session
            .upload_file(&mut target, hello_file(), UploadPolicy::once())
            .await;
        let attempts = match &outcome {
            Ok(uploaded) => uploaded.report.attempts().to_vec(),
            Err(error) => error.report().attempts().to_vec(),
        };
        assert_eq!(attempts.last().unwrap().evidence(), evidence, "{length}");
    }
}

#[tokio::test]
async fn a_2xx_upload_document_that_disagrees_with_the_request_is_stored_unverified() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let mut target = session.get_upload_url(&bucket()).await.unwrap();
    let patches: Vec<(&str, Value)> = vec![
        ("fileName", json!("another.txt")),
        ("bucketId", json!("another-bucket")),
        ("action", json!("start")),
        ("contentSha1", json!("none")),
        ("fileId", Value::Null),
    ];
    for (field, value) in patches {
        let v = value.clone();
        b2.push_upload(UploadMode::Reply(Box::new(move |_| {
            let mut doc = fixture_json("upload_file.json");
            doc[field] = v;
            Reply::json(200, doc.to_string())
        })));
        let error = session
            .upload_file(
                &mut target,
                hello_file(),
                UploadPolicy::retrying(std::num::NonZeroUsize::new(3).unwrap()),
            )
            .await
            .unwrap_err();
        let [attempt] = error.report().attempts() else {
            panic!("{field}: {error:?}")
        };
        assert_eq!(attempt.evidence(), Evidence::StoredUnverified, "{field}");
        let Error::Contract(contract) = error.error() else {
            panic!("{field}: {error:?}")
        };
        assert_eq!(contract.violation(), Violation::UploadedFileVersion);
    }
    // A 2xx the JSON format refuses (wrong media type) is stored-unverified too.
    b2.push_upload(UploadMode::Reply(Box::new(|_| {
        Reply::text(200, "text/html", "<p>ok</p>")
    })));
    let error = session
        .upload_file(&mut target, hello_file(), UploadPolicy::once())
        .await
        .unwrap_err();
    assert_eq!(
        error.report().attempts()[0].evidence(),
        Evidence::StoredUnverified
    );
    assert_eq!(concord_kind(error.error()), ErrorKind::Metadata);
}

// --- Documented rules on issued URLs, tokens and downloads ---------------------------------

#[tokio::test]
async fn unusable_issued_urls_and_tokens_are_refused_before_any_use() {
    let b2 = FakeB2::start().await;
    let api = b2.api.url();
    let with_creds = api.replace("http://", "http://user:pw@");
    for (patch, violation) in [
        (json!({ "apiUrl": with_creds }), Violation::IssuedUrl),
        (
            json!({ "apiUrl": format!("{api}?x=1") }),
            Violation::IssuedUrl,
        ),
        (
            json!({ "downloadUrl": "ftp://f001.backblazeb2.com" }),
            Violation::IssuedUrl,
        ),
        (
            json!({ "allowed": { "buckets": [{"id": "", "name": null}], "capabilities": [] } }),
            Violation::AllowedBucket,
        ),
    ] {
        b2.state.lock().unwrap().storage_api_patch = Some(patch.clone());
        let error = b2.client().authorize_account(&key()).await.unwrap_err();
        let Error::Contract(contract) = &error else {
            panic!("{patch}: {error:?}")
        };
        assert_eq!(contract.violation(), violation, "{patch}");
    }
    b2.state.lock().unwrap().storage_api_patch = None;
    set_authorize(&b2, || {
        let mut doc = fixture_json("authorize_account.json");
        doc["authorizationToken"] = json!("line\nbreak");
        Reply::json(200, doc.to_string())
    });
    let error = b2.client().authorize_account(&key()).await.unwrap_err();
    assert!(matches!(&error, Error::Contract(c) if c.violation() == Violation::IssuedToken));
    assert!(b2.api.requests().is_empty() && b2.download.requests().is_empty());

    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let upload = b2.upload.url();
    for (doc, violation) in [
        (
            json!({"bucketId": BUCKET, "uploadUrl": format!("{upload}/u#f"), "authorizationToken": "t"}),
            Violation::IssuedUrl,
        ),
        (
            json!({"bucketId": BUCKET, "uploadUrl": "mailto:a@b", "authorizationToken": "t"}),
            Violation::IssuedUrl,
        ),
        (
            json!({"bucketId": "other", "uploadUrl": format!("{upload}/u"), "authorizationToken": "t"}),
            Violation::UploadUrlBucket,
        ),
        (
            json!({"bucketId": BUCKET, "uploadUrl": format!("{upload}/u"), "authorizationToken": ""}),
            Violation::IssuedToken,
        ),
    ] {
        b2.push_get_upload_url(Reply::json(200, doc.to_string()));
        let error = session.get_upload_url(&bucket()).await.unwrap_err();
        assert!(
            matches!(&error, Error::Contract(c) if c.violation() == violation),
            "{doc}: {error:?}"
        );
    }
    assert!(b2.upload.requests().is_empty());
}

#[tokio::test]
async fn a_download_is_verified_or_refused() {
    let b2 = FakeB2::start().await;
    let session = b2.session().await;
    let id = stored(&b2, &session).await;
    let sha1 = hello_sha1();
    let base = move |id: String, sha1: Option<&str>| {
        let mut reply = Reply::new(200)
            .header("content-type", "text/plain")
            .header("x-bz-file-id", id)
            .header("x-bz-file-name", "hello-b2.txt");
        if let Some(sha1) = sha1 {
            reply = reply.header("x-bz-content-sha1", sha1);
        }
        reply.body(hello())
    };
    let id_text = id.as_str().to_owned();
    type Check = Box<dyn Fn(&Error) -> bool>;
    let cases: Vec<(Responder, Check)> = vec![
        {
            let i = id_text.clone();
            (
                Box::new(move |_| base(i.clone(), None)),
                Box::new(
                    |e| matches!(e, Error::Checksum(c) if *c.failure() == ChecksumFailure::Missing),
                ),
            )
        },
        {
            let i = id_text.clone();
            (
                Box::new(move |_| base(i.clone(), Some("none"))),
                Box::new(
                    |e| matches!(e, Error::Checksum(c) if *c.failure() == ChecksumFailure::Unavailable),
                ),
            )
        },
        {
            let i = id_text.clone();
            (
                Box::new(move |_| base(i.clone(), Some("unverified:abc"))),
                Box::new(
                    |e| matches!(e, Error::Checksum(c) if *c.failure() == ChecksumFailure::Malformed),
                ),
            )
        },
        {
            let (i, s) = (id_text.clone(), sha1.clone());
            (
                Box::new(move |_| base(i.clone(), Some(&s)).header("x-bz-content-sha1", s.clone())),
                Box::new(
                    |e| matches!(e, Error::Checksum(c) if *c.failure() == ChecksumFailure::Missing),
                ),
            )
        },
        {
            let s = sha1.clone();
            (
                Box::new(move |_| base("4_other".into(), Some(&s))),
                Box::new(
                    |e| matches!(e, Error::Contract(c) if c.violation() == Violation::DownloadFileId),
                ),
            )
        },
        {
            let (i, s) = (id_text.clone(), sha1.clone());
            (
                Box::new(move |_| {
                    let mut r = base(i.clone(), Some(&s));
                    r.status = 206;
                    r
                }),
                Box::new(
                    |e| matches!(e, Error::Contract(c) if c.violation() == Violation::DownloadStatus),
                ),
            )
        },
        {
            let (i, s) = (id_text.clone(), sha1.clone());
            (
                Box::new(move |_| {
                    base(i.clone(), Some(&s))
                        .with_body(Body::FailAfter(vec![Bytes::from_static(b"Hello")]))
                }),
                Box::new(
                    |e| matches!(e, Error::Concord(c) if c.kind() == ErrorKind::Transport && c.head().is_some()),
                ),
            )
        },
    ];
    for (i, (reply, check)) in cases.into_iter().enumerate() {
        b2.set_download(DownloadMode::Reply(reply));
        let error = session.download_file_by_id(&id, MIB).await.unwrap_err();
        assert!(check(&error), "case {i}: {error:?}");
    }
    // Oversized for the caller's bound.
    b2.set_download(DownloadMode::Faithful);
    let error = session.download_file_by_id(&id, 10).await.unwrap_err();
    assert_eq!(concord_kind(&error), ErrorKind::Limit);
    // The same bytes verify within the bound, and an uppercase digest is the same digest.
    let upper = sha1.to_uppercase();
    let i = id_text.clone();
    b2.set_download(DownloadMode::Reply(Box::new(move |_| {
        base(i.clone(), Some(&upper))
    })));
    let download = session.download_file_by_id(&id, 63).await.unwrap();
    assert_eq!(download.file.content_sha1.as_str(), hello_sha1());
}

/// Every operation's future can be spawned on a multi-threaded runtime (checked by the compiler).
#[allow(dead_code)]
fn operations_are_send(client: &Client, session: &Session, target: &mut UploadTarget) {
    fn send<T: Send>(_: T) {}
    send(client.authorize_account(&key()));
    send(session.get_upload_url(&bucket()));
    send(session.upload_file(target, hello_file(), UploadPolicy::once()));
    send(session.download_file_by_id(&FileId::new("x").unwrap(), 1));
}

// --- Local refusals -------------------------------------------------------------------------

#[test]
fn inputs_are_refused_locally() {
    assert_eq!(
        ApplicationKey::new("", "k").unwrap_err(),
        InputError::ApplicationKey
    );
    assert_eq!(
        ApplicationKey::new("id", "").unwrap_err(),
        InputError::ApplicationKey
    );
    assert_eq!(
        ApplicationKey::new("i:d", "k").unwrap_err(),
        InputError::ApplicationKey
    );
    assert_eq!(
        ApplicationKey::new("id", "k\n").unwrap_err(),
        InputError::ApplicationKey
    );
    assert_eq!(BucketId::new("").unwrap_err(), InputError::BucketId);
    assert_eq!(BucketId::new("a\0").unwrap_err(), InputError::BucketId);
    assert_eq!(FileId::new("").unwrap_err(), InputError::FileId);
    assert_eq!(FileName::new("").unwrap_err(), InputError::FileName);
    for bad in ["", "text", "text/", "/plain", "text/plain\n", "tëxt/plain"] {
        assert_eq!(
            ContentType::new(bad).unwrap_err(),
            InputError::ContentType,
            "{bad:?}"
        );
    }
    assert!(ContentType::new("text/plain; charset=utf-8").is_ok());
    for bad in ["", "abc", &"g".repeat(40), &"a".repeat(41)] {
        assert_eq!(Sha1Hex::new(bad).unwrap_err(), InputError::Sha1, "{bad:?}");
    }
    let empty = futures_core_empty();
    let error = UploadFile::stream(
        FileName::new("x").unwrap(),
        ContentType::auto(),
        0,
        Sha1Hex::new(hello_facts()["emptySha1"].as_str().unwrap()).unwrap(),
        empty,
    )
    .unwrap_err();
    assert_eq!(error, InputError::EmptyStream);
    for bad in [
        "ftp://x",
        "http://u:p@x",
        "http://x/?q",
        "http://x/#f",
        "nope",
    ] {
        let error = Client::new(http()).with_base_url(bad).unwrap_err();
        assert!(matches!(error, Error::Input(InputError::BaseUrl)), "{bad}");
    }
}

fn futures_core_empty() -> impl futures_core::Stream<Item = Result<Bytes, std::io::Error>> + Send {
    struct Empty;
    impl futures_core::Stream for Empty {
        type Item = Result<Bytes, std::io::Error>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Ready(None)
        }
    }
    Empty
}

// --- Redaction ------------------------------------------------------------------------------

#[tokio::test]
async fn secrets_and_issued_urls_never_reach_debug_or_display() {
    let b2 = FakeB2::start().await;
    let c = credentials();
    let secrets = [
        c["keyId"].as_str().unwrap().to_owned(),
        c["applicationKey"].as_str().unwrap().to_owned(),
        c["authorization"]
            .as_str()
            .unwrap()
            .trim_start_matches("Basic ")
            .to_owned(),
        ACCOUNT_TOKEN.to_owned(),
        "ACCOUNT_ID".to_owned(),
        "2_upload_token_".to_owned(),
        "c001_v0001005_t".to_owned(),
        "synthetic ".to_owned(),
        // `accountId` of the assembled upload document.
        "12f634bf3cbz".to_owned(),
    ];
    let mut shown = Vec::new();
    let key = key();
    shown.push(format!("{key:?}"));
    let client = b2.client();
    shown.push(format!("{client:?}"));
    let session = client.authorize_account(&key).await.unwrap();
    shown.push(format!("{session:?} {:?}", session.account()));
    let mut target: UploadTarget = session.get_upload_url(&bucket()).await.unwrap();
    shown.push(format!("{target:?}"));
    shown.push(format!("{:?}", hello_file()));

    b2.push_upload(UploadMode::Fail(503, "service_unavailable"));
    let failure = session
        .upload_file(&mut target, hello_file(), UploadPolicy::once())
        .await
        .unwrap_err();
    shown.push(format!("{failure:?} {failure}"));
    b2.push_upload(UploadMode::HangUp);
    let failure = session
        .upload_file(&mut target, hello_file(), UploadPolicy::once())
        .await
        .unwrap_err();
    shown.push(format!("{failure:?} {failure}"));
    b2.push_get_upload_url(error_reply(401, "expired_auth_token"));
    let error = session.get_upload_url(&bucket()).await.unwrap_err();
    shown.push(format!("{error:?} {error}"));

    let uploaded = session
        .upload_file(&mut target, hello_file(), UploadPolicy::once())
        .await
        .unwrap();
    shown.push(format!("{uploaded:?}"));
    let download = session
        .download_file_by_id(&uploaded.file.file_id, MIB)
        .await
        .unwrap();
    shown.push(format!("{download:?}"));
    b2.set_download(DownloadMode::Tampered);
    let error = session
        .download_file_by_id(&uploaded.file.file_id, MIB)
        .await
        .unwrap_err();
    shown.push(format!("{error:?} {error}"));

    for text in &shown {
        for secret in &secrets {
            assert!(!text.contains(secret.as_str()), "{secret:?} in {text}");
        }
        assert!(
            !text.contains(&hello_facts()["tamperedBody"].as_str().unwrap().to_string()),
            "{text}"
        );
    }
    assert!(shown[0].contains("<redacted>"));
}
