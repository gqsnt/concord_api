//! Wire-level checks against hand-authored fixtures; no Slack requests or credentials.

mod support;

use std::time::Duration;

use concord::{ErrorKind, reqwest};
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_slack::{
    Client, ClientConfig, ConversationOptions, EnvelopeProblem, Error, ErrorCode, PageSize,
    RetryAfter, SUCCESS_BODY_LIMIT, UsersOptions,
};
use support::{assert_get, client, fixture};

#[tokio::test]
async fn users_request_encodes_opaque_cursor_exactly_once() {
    let server = TestServer::start(|r| {
        if r.query_all("cursor").is_empty() {
            Reply::json(200, fixture("users-first"))
        } else {
            Reply::json(200, fixture("users-last-empty"))
        }
    })
    .await;
    let slack = client(&server);
    let options = UsersOptions {
        limit: PageSize::new(2).unwrap(),
        include_locale: Some(true),
        team_id: Some("T_SYNTHETIC +&".into()),
    };
    let first = slack.users_page(&options, None).await.unwrap();
    assert_eq!(
        first.value.members[0].extra["profile"]["display_name"],
        "Alice"
    );
    assert_eq!(first.warning.as_deref(), Some("synthetic_warning"));
    let metadata = first.response_metadata.as_ref().unwrap();
    assert_eq!(metadata.warnings, ["synthetic_page_warning"]);
    let cursor = metadata.next_cursor.as_ref().unwrap();
    assert_eq!(cursor.as_str(), "opaque=+/&?%2F# space");
    assert!(!format!("{cursor:?}").contains("opaque"));
    slack.users_page(&options, Some(cursor)).await.unwrap();
    let requests = server.requests();
    assert_get(
        &requests[0],
        "/api/users.list?limit=2&include_locale=true&team_id=T_SYNTHETIC+%2B%26",
    );
    assert_get(
        &requests[1],
        "/api/users.list?limit=2&cursor=opaque%3D%2B%2F%26%3F%252F%23+space&include_locale=true&team_id=T_SYNTHETIC+%2B%26",
    );
    assert_eq!(requests[1].query_all("cursor"), [cursor.as_str()]);
    assert_eq!(requests[1].query_all("limit"), ["2"]);
    server.shutdown().await;
}

#[tokio::test]
async fn construction_defaults_and_auth_are_redacted() {
    let server = TestServer::start(|_| Reply::json(200, fixture("users-last-null"))).await;
    let slack = client(&server);
    assert!(!format!("{slack:?}").contains("synthetic-slack-token"));
    slack
        .users_page(&UsersOptions::default(), None)
        .await
        .unwrap();
    assert_get(&server.single(), "/api/users.list?limit=200");
    assert!(PageSize::new(0).is_none());
    assert!(PageSize::new(1001).is_none());
    assert_eq!(PageSize::new(1).unwrap().get(), 1);
    assert_eq!(PageSize::new(1000).unwrap().get(), 1000);
    server.shutdown().await;
}

#[tokio::test]
async fn conversation_request_and_typed_success() {
    let server = TestServer::start(|_| {
        Reply::json(200, fixture("conversation")).header("x-slack-req-id", "synthetic-request-id")
    })
    .await;
    let response = client(&server)
        .conversation_info(
            "C_SYNTHETIC +&?=",
            ConversationOptions {
                include_locale: Some(false),
                include_num_members: Some(true),
            },
        )
        .await
        .unwrap();
    assert_eq!(response.value.name.as_deref(), Some("general"));
    assert_eq!(response.value.num_members, Some(2));
    assert_eq!(response.value.extra["topic"]["value"], "Fixture discussion");
    assert_eq!(response.meta.request_id(), Some("synthetic-request-id"));
    assert_eq!(
        response.meta.operation.unwrap().to_string(),
        "GET SlackApi.conversations_info"
    );
    assert!(!format!("{:?}", response.meta).contains("synthetic-request-id"));
    assert_get(
        &server.single(),
        "/api/conversations.info?channel=C_SYNTHETIC+%2B%26%3F%3D&include_locale=false&include_num_members=true",
    );
    server.shutdown().await;
}

#[tokio::test]
async fn direct_message_can_omit_channel_name() {
    let server = TestServer::start(|_| Reply::json(200, fixture("conversation-dm"))).await;
    let response = client(&server)
        .conversation_info("D_SYNTHETIC", ConversationOptions::default())
        .await
        .unwrap();
    assert_eq!(response.value.is_im, Some(true));
    assert!(response.value.name.is_none());
    assert_get(
        &server.single(),
        "/api/conversations.info?channel=D_SYNTHETIC",
    );
    server.shutdown().await;
}

#[tokio::test]
async fn documented_200_failure_keeps_status_request_id_and_operation() {
    let server = TestServer::start(|_| {
        Reply::json(200, fixture("channel-not-found"))
            .header("x-slack-req-id", "synthetic-failure-id")
    })
    .await;
    let failure = client(&server)
        .conversation_info("C_SYNTHETIC_MISSING", ConversationOptions::default())
        .await
        .unwrap_err();
    assert_eq!(failure.meta().unwrap().status, reqwest::StatusCode::OK);
    assert_eq!(
        failure.meta().unwrap().request_id(),
        Some("synthetic-failure-id")
    );
    match failure {
        Error::Api { error, meta } => {
            assert_eq!(error.code, ErrorCode::ChannelNotFound);
            assert_eq!(
                meta.operation.unwrap().to_string(),
                "GET SlackApi.conversations_info"
            );
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_get(
        &server.single(),
        "/api/conversations.info?channel=C_SYNTHETIC_MISSING",
    );
    server.shutdown().await;
}

#[tokio::test]
async fn known_unknown_and_scope_errors_preserve_details() {
    for (name, code) in [
        ("invalid-cursor", "invalid_cursor"),
        ("missing-scope", "missing_scope"),
        ("unknown-error", "synthetic_future_error"),
    ] {
        let server = TestServer::start(move |_| Reply::json(200, fixture(name))).await;
        let failure = client(&server)
            .users_page(&UsersOptions::default(), None)
            .await
            .unwrap_err();
        match failure {
            Error::Api { error, .. } => {
                assert_eq!(error.code.as_str(), code);
                if name == "missing-scope" {
                    assert_eq!(error.needed.as_deref(), Some("channels:read"));
                    assert_eq!(error.provided.as_deref(), Some("users:read"));
                    assert_eq!(
                        error.response_metadata.unwrap().messages,
                        ["Synthetic missing permission"]
                    );
                } else if name == "unknown-error" {
                    assert!(matches!(error.code, ErrorCode::Other(_)));
                    assert_eq!(error.warning.as_deref(), Some("synthetic_warning"));
                }
            }
            other => panic!("unexpected failure {other:?}"),
        }
        assert_get(&server.single(), "/api/users.list?limit=200");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn all_documented_terminal_cursor_shapes_are_accepted() {
    for name in [
        "users-last-empty",
        "users-last-null",
        "users-last-missing-cursor",
        "users-last-missing-metadata",
        "users-last-null-metadata",
    ] {
        let server = TestServer::start(move |_| Reply::json(200, fixture(name))).await;
        let response = client(&server)
            .users_page(&UsersOptions::default(), None)
            .await
            .unwrap();
        assert!(
            response
                .response_metadata
                .and_then(|m| m.next_cursor)
                .is_none(),
            "{name}"
        );
        assert_get(&server.single(), "/api/users.list?limit=200");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn whitespace_cursor_is_nonempty_and_preserved() {
    let server = TestServer::start(|_| Reply::json(200, fixture("users-whitespace-cursor"))).await;
    let slack = client(&server);
    let response = slack
        .users_page(&UsersOptions::default(), None)
        .await
        .unwrap();
    let cursor = response.response_metadata.unwrap().next_cursor.unwrap();
    assert_eq!(cursor.as_str(), "  =  ");
    slack
        .users_page(&UsersOptions::default(), Some(&cursor))
        .await
        .unwrap();
    assert_get(
        &server.requests()[1],
        "/api/users.list?limit=200&cursor=++%3D++",
    );
    server.shutdown().await;
}

#[tokio::test]
async fn incomplete_or_contradictory_envelopes_are_not_success() {
    for (name, expected) in [
        ("missing-members", EnvelopeProblem::MissingMembers),
        ("missing-error", EnvelopeProblem::MissingError),
        ("conflicting-error", EnvelopeProblem::ConflictingError),
    ] {
        let server = TestServer::start(move |_| Reply::json(200, fixture(name))).await;
        let failure = client(&server)
            .users_page(&UsersOptions::default(), None)
            .await
            .unwrap_err();
        assert!(matches!(failure, Error::Envelope { problem, .. } if problem == expected));
        assert_eq!(failure.meta().unwrap().status.as_u16(), 200);
        server.shutdown().await;
    }
    let server = TestServer::start(|_| Reply::json(200, fixture("missing-channel"))).await;
    let failure = client(&server)
        .conversation_info("C_SYNTHETIC", ConversationOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(
        failure,
        Error::Envelope {
            problem: EnvelopeProblem::MissingChannel,
            ..
        }
    ));
    server.shutdown().await;
}

#[tokio::test]
async fn json_and_content_type_failures_keep_metadata() {
    for (name, content_type, expected) in [
        ("malformed", "application/json", ErrorKind::Decode),
        ("missing-ok", "application/json", ErrorKind::Decode),
        ("invalid-cursor-type", "application/json", ErrorKind::Decode),
        ("proxy-error", "text/html", ErrorKind::Metadata),
    ] {
        let server = TestServer::start(move |_| {
            Reply::text(200, content_type, fixture(name))
                .header("x-slack-req-id", "synthetic-bad-body-id")
        })
        .await;
        let failure = client(&server)
            .users_page(&UsersOptions::default(), None)
            .await
            .unwrap_err();
        assert_eq!(
            failure.meta().unwrap().request_id(),
            Some("synthetic-bad-body-id")
        );
        match failure {
            Error::Concord {
                error,
                meta: Some(_),
            } => {
                assert_eq!(error.kind(), expected);
                assert_eq!(
                    error.operation().unwrap().to_string(),
                    "GET SlackApi.users_list"
                );
            }
            other => panic!("unexpected failure {other:?}"),
        }
        assert_get(&server.single(), "/api/users.list?limit=200");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn interrupted_success_body_is_not_a_completed_page() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/json")
            .header("x-slack-req-id", "synthetic-truncated-id")
            .with_body(Body::FailAfter(vec![fixture("malformed").into()]))
    })
    .await;
    let failure = client(&server)
        .users_page(&UsersOptions::default(), None)
        .await
        .unwrap_err();
    assert_eq!(
        failure.meta().unwrap().request_id(),
        Some("synthetic-truncated-id")
    );
    assert!(
        matches!(failure, Error::Concord { error, .. } if error.kind() == ErrorKind::Transport)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn rate_limit_survives_json_malformed_html_and_empty_bodies() {
    for name in ["rate-limited", "malformed", "proxy-error", "empty"] {
        let server = TestServer::start(move |_| {
            Reply::new(429)
                .header("retry-after", "30")
                .header("x-slack-req-id", "synthetic-rate-id")
                .body(if name == "empty" { b"" } else { fixture(name) })
        })
        .await;
        let failure = client(&server)
            .conversation_info("C_SYNTHETIC", ConversationOptions::default())
            .await
            .unwrap_err();
        match failure {
            Error::RateLimited {
                retry_after,
                meta,
                body,
            } => {
                assert_eq!(retry_after, RetryAfter::Seconds(Duration::from_secs(30)));
                assert_eq!(meta.status.as_u16(), 429);
                assert_eq!(meta.request_id(), Some("synthetic-rate-id"));
                assert_eq!(meta.headers["retry-after"], "30");
                assert_eq!(
                    body.bytes().unwrap(),
                    if name == "empty" { b"" } else { fixture(name) }
                );
                assert_eq!(
                    body.provider_error().map(|p| p.code.as_str()),
                    (name == "rate-limited").then_some("ratelimited")
                );
            }
            other => panic!("unexpected failure {other:?}"),
        }
        assert_get(
            &server.single(),
            "/api/conversations.info?channel=C_SYNTHETIC",
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn retry_after_diagnostics_do_not_guess_a_delay() {
    let cases = [
        (vec![], RetryAfter::Missing),
        (vec!["0"], RetryAfter::Seconds(Duration::ZERO)),
        (vec!["-1"], RetryAfter::Invalid),
        (vec!["+30"], RetryAfter::Invalid),
        (vec!["30.0"], RetryAfter::Invalid),
        (vec!["tomorrow"], RetryAfter::Invalid),
        (vec!["Wed, 21 Oct 2015 07:28:00 GMT"], RetryAfter::Invalid),
        (vec!["18446744073709551616"], RetryAfter::Invalid),
        (vec!["30", "30"], RetryAfter::Invalid),
    ];
    for (values, expected) in cases {
        let copies = values.clone();
        let server = TestServer::start(move |_| {
            let mut reply = Reply::json(429, fixture("rate-limited"));
            for value in &copies {
                reply = reply.header("retry-after", *value);
            }
            reply
        })
        .await;
        let failure = client(&server)
            .users_page(&UsersOptions::default(), None)
            .await
            .unwrap_err();
        match failure {
            Error::RateLimited {
                retry_after, meta, ..
            } => {
                assert_eq!(retry_after, expected, "{values:?}");
                assert_eq!(
                    meta.headers.get_all("retry-after").iter().count(),
                    values.len()
                );
            }
            other => panic!("unexpected failure {other:?}"),
        }
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn oversized_or_interrupted_error_body_keeps_429_headers() {
    for too_large in [true, false] {
        let server = TestServer::start(move |_| {
            let reply = Reply::new(429).header("retry-after", "30");
            if too_large {
                reply.body(vec![b'x'; 64 * 1024 + 1])
            } else {
                reply.with_body(Body::FailAfter(vec![fixture("malformed").into()]))
            }
        })
        .await;
        let failure = client(&server)
            .users_page(&UsersOptions::default(), None)
            .await
            .unwrap_err();
        match failure {
            Error::RateLimited {
                meta,
                body,
                retry_after,
            } => {
                assert_eq!(retry_after, RetryAfter::Seconds(Duration::from_secs(30)));
                assert_eq!(meta.status.as_u16(), 429);
                assert!(body.bytes().is_none());
                let error = body.read_error().unwrap();
                assert_eq!(
                    error.kind(),
                    if too_large {
                        ErrorKind::Limit
                    } else {
                        ErrorKind::Transport
                    }
                );
                assert_eq!(
                    error.operation().unwrap().to_string(),
                    "GET SlackApi.users_list"
                );
            }
            other => panic!("unexpected failure {other:?}"),
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn other_http_errors_retain_bounded_body_and_provider_error() {
    let server = TestServer::start(|_| Reply::json(403, fixture("missing-scope"))).await;
    let failure = client(&server)
        .conversation_info("C_SYNTHETIC", ConversationOptions::default())
        .await
        .unwrap_err();
    match failure {
        Error::Http { meta, body } => {
            assert_eq!(meta.status.as_u16(), 403);
            assert_eq!(body.bytes().unwrap(), fixture("missing-scope"));
            assert_eq!(body.provider_error().unwrap().code, ErrorCode::MissingScope);
        }
        other => panic!("unexpected failure {other:?}"),
    }
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_auth_header_fails_before_io_and_keeps_secret_out_of_diagnostics() {
    let server = TestServer::start(|_| panic!("invalid header must not be transmitted")).await;
    let slack = Client::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        ClientConfig {
            token: "synthetic-secret\r\ninvalid".into(),
        },
    )
    .with_base_url(format!("{}/api", server.url()))
    .unwrap();
    let failure = slack
        .users_page(&UsersOptions::default(), None)
        .await
        .unwrap_err();
    assert!(failure.meta().is_none());
    assert!(!format!("{failure:?} {failure}").contains("synthetic-secret"));
    assert!(matches!(failure, Error::Concord { error, .. } if error.kind() == ErrorKind::Build));
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

/// A synthetic `ok: true` users page of exactly `len` bytes (padding in an ignored field).
fn users_page_of(len: usize) -> Vec<u8> {
    let (prefix, suffix) = (r#"{"ok":true,"members":[],"pad":""#, r#""}"#);
    let mut body = prefix.as_bytes().to_vec();
    body.resize(len - suffix.len(), b'x');
    body.extend_from_slice(suffix.as_bytes());
    body
}

#[tokio::test]
async fn success_body_is_decoded_up_to_the_limit_and_refused_beyond() {
    // Between waves 2-3 (coordinator integration): `Received::decode_limited` bounds 2xx bodies.
    for len in [SUCCESS_BODY_LIMIT, SUCCESS_BODY_LIMIT + 1] {
        let body = users_page_of(len);
        assert_eq!(body.len(), len);
        let server = TestServer::start(move |_| {
            Reply::json(200, body.clone()).header("x-slack-req-id", "synthetic-large-id")
        })
        .await;
        let result = client(&server)
            .users_page(&UsersOptions::default(), None)
            .await;
        if len == SUCCESS_BODY_LIMIT {
            assert!(
                result.unwrap().value.members.is_empty(),
                "exactly at the limit"
            );
        } else {
            match result.unwrap_err() {
                Error::Concord {
                    error,
                    meta: Some(meta),
                } => {
                    assert_eq!(error.kind(), ErrorKind::Limit);
                    assert_eq!(
                        error.operation().unwrap().to_string(),
                        "GET SlackApi.users_list"
                    );
                    assert_eq!(meta.status.as_u16(), 200);
                    assert_eq!(meta.request_id(), Some("synthetic-large-id"));
                }
                other => panic!("unexpected failure {other:?}"),
            }
        }
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
