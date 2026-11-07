//! Wire assertions and response/error contracts, entirely against loopback fixtures.

mod support;

use concord::{ErrorKind, bytes::Bytes, reqwest};
use concord_example_testserver::{Body, Reply, Script, TestServer};
use concord_sdk_dropbox::{
    Client, ClientConfig, Entry, Error, InputError, ListFolderOptions, LookupError, NamespaceId,
    PageSize, PathRoot, ProviderErrorKind, RateLimitReason, RequestContext, RetryAfter,
    TaggedError,
};
use std::time::Duration;
use support::*;

#[tokio::test]
async fn root_listing_and_continuation_preserve_exact_wire_and_metadata() {
    let script = Script::new([
        json(200, A_FIRST, "synthetic-a-first"),
        json(200, A_LAST, "synthetic-a-last"),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let first = api
        .list_folder(context_a(), ListFolderOptions::default())
        .await
        .unwrap();
    assert_eq!(first.meta.status(), reqwest::StatusCode::OK);
    assert_eq!(first.meta.request_id(), Some("synthetic-a-first"));
    assert!(!format!("{:?}", first.meta).contains("synthetic-a-first"));
    assert_eq!(
        first.meta.operation().unwrap().name(),
        "DropboxApi.selected.list_folder"
    );
    assert_eq!(first.meta.operation().unwrap().method(), Some("POST"));
    assert!(first.value.has_more);
    assert_eq!(first.value.cursor.context(), &context_a());
    assert!(!format!("{:?}", first.value.cursor).contains("SYNTHETIC_CURSOR"));
    let Entry::File(file) = &first.value.entries[0] else {
        panic!("expected file")
    };
    assert_eq!(file.id, "id:SYNTHETIC_FILE_A");
    assert_eq!(file.size, 42);
    assert!(file.is_downloadable); // Documented default for an omitted field.

    let last = api.list_folder_continue(&first.value.cursor).await.unwrap();
    assert!(!last.value.has_more);
    assert!(matches!(last.value.entries[0], Entry::Folder(_)));
    assert!(matches!(last.value.entries[1], Entry::Deleted(_)));
    assert_eq!(last.meta.request_id(), Some("synthetic-a-last"));
    assert_eq!(
        last.meta.operation().unwrap().name(),
        "DropboxApi.selected.list_folder_continue"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_request(
        &requests[0],
        MEMBER_A,
        r#"{".tag":"root","root":"100"}"#,
        "/2/files/list_folder",
        include_bytes!("../fixtures/request-list-root.json"),
    );
    assert_request(
        &requests[1],
        MEMBER_A,
        r#"{".tag":"root","root":"100"}"#,
        "/2/files/list_folder/continue",
        include_bytes!("../fixtures/request-continue-a.json"),
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn unicode_paths_are_json_data_and_home_root_is_explicit() {
    let server = TestServer::start(|_| json(200, B_LAST, "synthetic-unicode")).await;
    let context = RequestContext::new(MEMBER_A, PathRoot::Home).unwrap();
    let mut options = ListFolderOptions::new("/Équipe \"été\"/Q%2F");
    options.recursive = true;
    options.include_deleted = true;
    options.limit = Some(PageSize::new(2).unwrap());
    let response = client(&server).list_folder(context, options).await.unwrap();
    assert!(response.value.entries[0].path().path_lower.is_none());
    assert_request(
        &server.single(),
        MEMBER_A,
        r#"{".tag":"home"}"#,
        "/2/files/list_folder",
        include_bytes!("../fixtures/request-list-unicode.json"),
    );
    server.shutdown().await;
}

#[tokio::test]
async fn namespace_ids_follow_current_schema_instead_of_assuming_decimal_numbers() {
    let server = TestServer::start(|_| json(200, B_LAST, "synthetic-opaque-namespace")).await;
    let context =
        RequestContext::new(MEMBER_A, PathRoot::namespace("ns:Team_A-9").unwrap()).unwrap();
    client(&server)
        .list_folder(context, ListFolderOptions::default())
        .await
        .unwrap();
    assert_request(
        &server.single(),
        MEMBER_A,
        r#"{".tag":"namespace_id","namespace_id":"ns:Team_A-9"}"#,
        "/2/files/list_folder",
        include_bytes!("../fixtures/request-list-root.json"),
    );
    server.shutdown().await;
}

#[tokio::test]
async fn namespace_context_and_empty_intermediate_page_have_real_continuation() {
    let script = Script::new([
        json(200, B_FIRST, "synthetic-b-first"),
        json(200, B_LAST, "synthetic-b-last"),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let first = api
        .list_folder(context_b(), ListFolderOptions::default())
        .await
        .unwrap();
    assert!(first.value.entries.is_empty());
    assert!(first.value.has_more);
    let last = api.list_folder_continue(&first.value.cursor).await.unwrap();
    assert_eq!(last.value.entries[0].path().name, "B only");
    let requests = server.requests();
    assert_request(
        &requests[0],
        MEMBER_B,
        r#"{".tag":"namespace_id","namespace_id":"200"}"#,
        "/2/files/list_folder",
        include_bytes!("../fixtures/request-list-root.json"),
    );
    assert_request(
        &requests[1],
        MEMBER_B,
        r#"{".tag":"namespace_id","namespace_id":"200"}"#,
        "/2/files/list_folder/continue",
        include_bytes!("../fixtures/request-continue-b.json"),
    );
    server.shutdown().await;
}

#[tokio::test]
async fn documented_tagged_failures_keep_status_request_id_and_typed_detail() {
    let cases: &[(u16, &[u8], TaggedError)] = &[
        (
            409,
            include_bytes!("../fixtures/path-not-found.json"),
            TaggedError::Path {
                path: LookupError::NotFound,
            },
        ),
        (
            409,
            include_bytes!("../fixtures/path-not-folder.json"),
            TaggedError::Path {
                path: LookupError::NotFolder,
            },
        ),
        (
            401,
            include_bytes!("../fixtures/invalid-user.json"),
            TaggedError::InvalidSelectUser,
        ),
        (
            401,
            include_bytes!("../fixtures/missing-scope.json"),
            TaggedError::MissingScope {
                required_scope: "files.metadata.read".into(),
            },
        ),
        (
            403,
            include_bytes!("../fixtures/forbidden.json"),
            TaggedError::TeamAccessDenied,
        ),
        (
            422,
            include_bytes!("../fixtures/no-permission.json"),
            TaggedError::NoPermission,
        ),
    ];
    for (status, fixture, expected) in cases {
        let script = Script::new([json(*status, fixture, "synthetic-failure-id")]);
        let server = TestServer::start(move |_| script.next()).await;
        let error = client(&server)
            .list_folder(context_a(), ListFolderOptions::default())
            .await
            .unwrap_err();
        assert_eq!(error.meta().unwrap().status().as_u16(), *status);
        assert_eq!(
            error.meta().unwrap().request_id(),
            Some("synthetic-failure-id")
        );
        assert_eq!(
            error.meta().unwrap().operation().unwrap().name(),
            "DropboxApi.selected.list_folder"
        );
        let provider = error.provider().unwrap();
        let ProviderErrorKind::Tagged(kind) = &provider.kind else {
            panic!("expected tagged error")
        };
        assert_eq!(kind, expected);
        if matches!(
            expected,
            TaggedError::Path {
                path: LookupError::NotFound
            }
        ) {
            assert_eq!(
                provider.user_message.as_ref().unwrap().locale.as_deref(),
                Some("en")
            );
            assert!(provider.summary.starts_with("path/not_found/"));
        }
        let Error::Http(http) = &error else {
            panic!("expected HTTP error")
        };
        assert_eq!(http.body.bytes().unwrap(), *fixture);
        assert!(!format!("{error:?} {error}").contains(&provider.summary));
        assert_eq!(server.requests().len(), 1); // No automatic retry.
        server.shutdown().await;
    }
}

#[tokio::test]
async fn stale_root_reports_new_root_without_switching_context() {
    let server = TestServer::start(|_| {
        json(
            422,
            include_bytes!("../fixtures/invalid-root.json"),
            "synthetic-root-error",
        )
    })
    .await;
    let error = client(&server)
        .list_folder(context_a(), ListFolderOptions::default())
        .await
        .unwrap_err();
    let ProviderErrorKind::Tagged(TaggedError::InvalidRoot { invalid_root }) =
        &error.provider().unwrap().kind
    else {
        panic!("expected invalid_root")
    };
    assert_eq!(invalid_root.root_namespace_id, "300");
    assert_eq!(invalid_root.kind, "user");
    assert_eq!(error.meta().unwrap().status().as_u16(), 422);
    assert_request(
        &server.single(),
        MEMBER_A,
        r#"{".tag":"root","root":"100"}"#,
        "/2/files/list_folder",
        include_bytes!("../fixtures/request-list-root.json"),
    );
    server.shutdown().await;
}

#[tokio::test]
async fn expired_continuation_is_a_typed_reset_with_its_own_operation() {
    let script = Script::new([
        json(200, A_FIRST, "synthetic-first"),
        json(
            409,
            include_bytes!("../fixtures/reset.json"),
            "synthetic-reset",
        ),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let first = api
        .list_folder(context_a(), ListFolderOptions::default())
        .await
        .unwrap();
    let error = api
        .list_folder_continue(&first.value.cursor)
        .await
        .unwrap_err();
    assert!(matches!(
        error.provider().unwrap().kind,
        ProviderErrorKind::Tagged(TaggedError::Reset)
    ));
    assert_eq!(
        error.meta().unwrap().operation().unwrap().name(),
        "DropboxApi.selected.list_folder_continue"
    );
    assert_eq!(error.meta().unwrap().request_id(), Some("synthetic-reset"));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn rate_limit_keeps_header_and_body_delays_separate_and_never_retries() {
    let server = TestServer::start(|_| {
        json(
            429,
            include_bytes!("../fixtures/rate-limited.json"),
            "synthetic-rate",
        )
        .header("Retry-After", "3")
    })
    .await;
    let error = client(&server)
        .list_folder(context_a(), ListFolderOptions::default())
        .await
        .unwrap_err();
    assert_eq!(
        error.meta().unwrap().retry_after(),
        RetryAfter::Seconds(Duration::from_secs(3))
    );
    let ProviderErrorKind::RateLimited(rate) = &error.provider().unwrap().kind else {
        panic!("expected rate limit")
    };
    assert_eq!(rate.reason, RateLimitReason::TooManyRequests);
    assert_eq!(rate.retry_after, 7);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn retry_after_distinguishes_missing_invalid_and_zero_on_plaintext_429() {
    for (headers, expected) in [
        (vec![], RetryAfter::Missing),
        (vec!["0"], RetryAfter::Seconds(Duration::ZERO)),
        (vec![" 0 \t"], RetryAfter::Seconds(Duration::ZERO)),
        (vec!["+1"], RetryAfter::Invalid),
        (vec!["-1"], RetryAfter::Invalid),
        (vec!["not-a-number"], RetryAfter::Invalid),
        (vec!["18446744073709551616"], RetryAfter::Invalid),
        (vec!["1", "2"], RetryAfter::Invalid),
    ] {
        let server = TestServer::start(move |_| {
            let mut reply = Reply::text(
                429,
                "text/plain",
                include_bytes!("../fixtures/plain-error.txt"),
            )
            .header("x-dropbox-request-id", "synthetic-plain-rate");
            for value in &headers {
                reply = reply.header("Retry-After", *value);
            }
            reply
        })
        .await;
        let error = client(&server)
            .list_folder(context_a(), ListFolderOptions::default())
            .await
            .unwrap_err();
        assert_eq!(error.meta().unwrap().status().as_u16(), 429);
        assert_eq!(error.meta().unwrap().retry_after(), expected);
        assert!(error.provider().is_none());
        server.shutdown().await;
    }
}

#[tokio::test]
async fn plain_bad_request_html_server_error_and_future_error_are_retained() {
    let replies = [
        Reply::text(
            400,
            "text/plain",
            include_bytes!("../fixtures/plain-error.txt"),
        ),
        Reply::text(
            500,
            "text/html",
            include_bytes!("../fixtures/proxy-error.html"),
        ),
        Reply::json(409, include_bytes!("../fixtures/future-error.json")),
        Reply::json(409, include_bytes!("../fixtures/malformed.json")),
    ];
    for (index, reply) in replies.into_iter().enumerate() {
        let status = reply.status;
        let script = Script::new([reply.header("x-dropbox-request-id", "synthetic-unusual")]);
        let server = TestServer::start(move |_| script.next()).await;
        let error = client(&server)
            .list_folder(context_a(), ListFolderOptions::default())
            .await
            .unwrap_err();
        assert_eq!(error.meta().unwrap().status().as_u16(), status);
        assert_eq!(
            error.meta().unwrap().request_id(),
            Some("synthetic-unusual")
        );
        let Error::Http(http) = &error else {
            panic!("expected HTTP failure")
        };
        assert!(!http.body.bytes().unwrap().is_empty());
        if index == 2 {
            let provider = error
                .provider()
                .expect("future error envelope must be retained");
            assert!(matches!(
                provider.kind,
                ProviderErrorKind::Tagged(TaggedError::Other)
            ));
            assert_eq!(provider.detail[".tag"], "future_context_error");
            assert_eq!(provider.detail["extra"], "synthetic diagnostic");
        } else {
            assert!(error.provider().is_none());
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn invalid_success_documents_are_errors_with_response_metadata() {
    let fixtures: &[&[u8]] = &[
        include_bytes!("../fixtures/malformed.json"),
        include_bytes!("../fixtures/missing-entries.json"),
        include_bytes!("../fixtures/missing-has-more.json"),
        include_bytes!("../fixtures/missing-cursor.json"),
        include_bytes!("../fixtures/unknown-entry.json"),
        include_bytes!("../fixtures/missing-file-id.json"),
    ];
    for fixture in fixtures {
        let script = Script::new([json(200, fixture, "synthetic-decode-error")]);
        let server = TestServer::start(move |_| script.next()).await;
        let error = client(&server)
            .list_folder(context_a(), ListFolderOptions::default())
            .await
            .unwrap_err();
        assert_eq!(
            error.meta().unwrap().request_id(),
            Some("synthetic-decode-error")
        );
        assert!(matches!(error, Error::Concord { error, .. } if error.kind() == ErrorKind::Decode));
        server.shutdown().await;
    }
}

#[tokio::test]
async fn empty_terminal_cursor_cannot_be_mistaken_for_completion() {
    let server = TestServer::start(|_| {
        json(
            200,
            include_bytes!("../fixtures/empty-cursor.json"),
            "synthetic-empty-cursor",
        )
    })
    .await;
    let error = client(&server)
        .list_folder(context_a(), ListFolderOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::EmptyCursor { .. }));
    assert_eq!(
        error.meta().unwrap().request_id(),
        Some("synthetic-empty-cursor")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn non_json_or_missing_content_type_is_not_a_successful_empty_page() {
    for reply in [
        Reply::text(200, "text/html", B_LAST),
        Reply::new(200).body(B_LAST),
    ] {
        let script = Script::new([reply.header("x-dropbox-request-id", "synthetic-content-type")]);
        let server = TestServer::start(move |_| script.next()).await;
        let error = client(&server)
            .list_folder(context_a(), ListFolderOptions::default())
            .await
            .unwrap_err();
        assert_eq!(
            error.meta().unwrap().request_id(),
            Some("synthetic-content-type")
        );
        assert!(
            matches!(error, Error::Concord { error, .. } if error.kind() == ErrorKind::Metadata)
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn oversized_or_interrupted_error_body_never_hides_http_status_and_request_id() {
    for (body, expected) in [
        (
            Body::Full(Bytes::from(vec![
                b'x';
                concord_sdk_dropbox::ERROR_BODY_LIMIT + 1
            ])),
            ErrorKind::Limit,
        ),
        (
            Body::FailAfter(vec![Bytes::from_static(b"synthetic truncated body")]),
            ErrorKind::Transport,
        ),
    ] {
        let script = Script::new([Reply::new(429)
            .with_body(body)
            .header("x-dropbox-request-id", "synthetic-body-failure")]);
        let server = TestServer::start(move |_| script.next()).await;
        let error = client(&server)
            .list_folder(context_a(), ListFolderOptions::default())
            .await
            .unwrap_err();
        assert_eq!(error.meta().unwrap().status().as_u16(), 429);
        assert_eq!(
            error.meta().unwrap().request_id(),
            Some("synthetic-body-failure")
        );
        let Error::Http(http) = error else {
            panic!("expected HTTP failure")
        };
        assert!(http.body.bytes().is_none());
        let read = http.body.read_error().unwrap();
        assert_eq!(read.kind(), expected);
        assert_eq!(
            read.operation().unwrap().name(),
            "DropboxApi.selected.list_folder"
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn success_body_limit_and_truncation_keep_metadata() {
    for (body, expected) in [
        (
            Body::Full(Bytes::from(vec![
                b' ';
                concord_sdk_dropbox::SUCCESS_BODY_LIMIT + 1
            ])),
            ErrorKind::Limit,
        ),
        (
            Body::FailAfter(vec![Bytes::from_static(b"{\"entries\":[]")]),
            ErrorKind::Transport,
        ),
    ] {
        let script = Script::new([Reply::json(200, b"")
            .with_body(body)
            .header("x-dropbox-request-id", "synthetic-success-body")]);
        let server = TestServer::start(move |_| script.next()).await;
        let error = client(&server)
            .list_folder(context_a(), ListFolderOptions::default())
            .await
            .unwrap_err();
        assert_eq!(
            error.meta().unwrap().request_id(),
            Some("synthetic-success-body")
        );
        assert!(matches!(error, Error::Concord { error, .. } if error.kind() == expected));
        server.shutdown().await;
    }
}

#[test]
fn local_context_validation_rejects_header_injection_and_invalid_sizes() {
    for member in ["", "dbmid:bad\r\nx-injected: yes", "member space", "mémber"] {
        assert_eq!(
            RequestContext::new(member, PathRoot::Home).unwrap_err(),
            InputError::MemberId
        );
    }
    for root in ["", "root/id", "1\"}", "1\n2", "１２"] {
        assert_eq!(NamespaceId::new(root).unwrap_err(), InputError::NamespaceId);
    }
    assert_eq!(NamespaceId::new("000123").unwrap().as_str(), "000123");
    assert_eq!(PageSize::new(0).unwrap_err(), InputError::PageSize);
    assert_eq!(PageSize::new(2001).unwrap_err(), InputError::PageSize);
    assert_eq!(PageSize::new(2000).unwrap().get(), 2000);
}

#[tokio::test]
async fn invalid_bearer_fails_before_io_and_debug_is_redacted() {
    let server = TestServer::start(|_| panic!("invalid header must not reach server")).await;
    let api = Client::new(
        reqwest::Client::new(),
        ClientConfig {
            token: "SYNTHETIC_INVALID_TOKEN\r\nx-injected: yes".into(),
        },
    )
    .with_base_url(format!("{}/2", server.url()))
    .unwrap();
    assert!(!format!("{api:?}").contains("SYNTHETIC_INVALID_TOKEN"));
    assert!(!format!("{:?}", context_a()).contains(MEMBER_A));
    let error = api
        .list_folder(context_a(), ListFolderOptions::default())
        .await
        .unwrap_err();
    assert!(error.meta().is_none());
    assert!(!format!("{error:?} {error}").contains("SYNTHETIC_INVALID_TOKEN"));
    assert!(matches!(error, Error::Concord { error, .. } if error.kind() == ErrorKind::Build));
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn transport_failure_has_no_fabricated_metadata() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let error = client(&server)
        .list_folder(context_a(), ListFolderOptions::default())
        .await
        .unwrap_err();
    assert!(error.meta().is_none());
    assert!(matches!(error, Error::Concord { error, .. } if error.kind() == ErrorKind::Transport));
    server.shutdown().await;
}
