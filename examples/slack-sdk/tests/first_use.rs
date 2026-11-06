//! Application-style first use against a synthetic loopback Slack API.

use std::{num::NonZeroUsize, time::Duration};

use concord::reqwest;
use concord_example_testserver::{Body, Recorded, Reply, Script, TestServer};
use concord_sdk_slack::{
    Client, ClientConfig, ConversationOptions, Error, ErrorCode, PageSize, RetryAfter,
    UsersOptions, pagination::PagesError,
};
use futures_util::StreamExt;

const FIRST: &[u8] = include_bytes!("../fixtures/first-use-users-first.json");
const EMPTY: &[u8] = include_bytes!("../fixtures/first-use-users-empty.json");
const LAST: &[u8] = include_bytes!("../fixtures/first-use-users-last.json");
const INVALID: &[u8] = include_bytes!("../fixtures/first-use-invalid-cursor.json");
const CONVERSATION: &[u8] = include_bytes!("../fixtures/first-use-conversation.json");
const NOT_FOUND: &[u8] = include_bytes!("../fixtures/first-use-channel-not-found.json");

fn client(server: &TestServer) -> Client {
    Client::new(
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
        ClientConfig {
            token: "synthetic-slack-token".into(),
        },
    )
    .with_base_url(format!("{}/api", server.url()))
    .unwrap()
}

async fn scripted(replies: Vec<Reply>) -> (TestServer, Script) {
    let script = Script::new(replies);
    let responder = script.clone();
    (TestServer::start(move |_| responder.next()).await, script)
}

fn check_request(request: &Recorded, path: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), path);
    assert_eq!(
        request.header("authorization"),
        Some("Bearer synthetic-slack-token")
    );
    assert_eq!(request.body_len, 0);
}

#[tokio::test]
async fn pages_continue_through_empty_page_and_exhaust_only_at_empty_cursor() {
    let (server, script) = scripted(vec![
        Reply::json(200, FIRST).header("x-slack-req-id", "synthetic-request-1"),
        Reply::json(200, EMPTY),
        Reply::json(200, LAST),
    ])
    .await;
    let options = UsersOptions {
        limit: PageSize::new(2).unwrap(),
        include_locale: Some(true),
        team_id: Some("T_SYNTHETIC".into()),
    };
    let mut pages = client(&server).users(options);
    assert!(server.requests().is_empty(), "listing should be lazy");
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].name.as_deref(), Some("ada"));
    assert_eq!(first.meta.http.request_id(), Some("synthetic-request-1"));
    assert_eq!(first.meta.warning.as_deref(), Some("synthetic_warning"));
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(last.items[0].id, "U_SYNTHETIC_B");
    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for r in &requests {
        check_request(r, "/api/users.list");
        assert_eq!(r.query_all("limit"), ["2"]);
        assert_eq!(r.query_all("include_locale"), ["true"]);
        assert_eq!(r.query_all("team_id"), ["T_SYNTHETIC"]);
    }
    assert!(requests[0].query_all("cursor").is_empty());
    assert_eq!(requests[1].query_all("cursor"), ["next +/=%"]);
    assert!(requests[1].target.contains("cursor=next+%2B%2F%3D%25"));
    assert_eq!(requests[2].query_all("cursor"), ["next-second"]);
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_page_preserves_collected_users_and_provider_error() {
    let (server, _) = scripted(vec![Reply::json(200, FIRST), Reply::json(200, INVALID)]).await;
    let incomplete = client(&server)
        .users(UsersOptions::default())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].id, "U_SYNTHETIC_A");
    match incomplete.error {
        PagesError::Provider {
            error: Error::Api { error, meta },
            ..
        } => {
            assert_eq!(error.code, ErrorCode::InvalidCursor);
            assert!(meta.status.is_success());
        }
        other => panic!("expected invalid cursor provider error: {other:?}"),
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for r in &requests {
        check_request(r, "/api/users.list");
    }
    assert_eq!(requests[1].query_all("cursor"), ["next +/=%"]);
    server.shutdown().await;
}

#[tokio::test]
async fn page_cap_is_not_exhaustion_even_after_empty_page() {
    let (server, _) = scripted(vec![Reply::json(200, EMPTY)]).await;
    let mut pages = client(&server)
        .users(UsersOptions::default())
        .max_pages(NonZeroUsize::new(1).unwrap());
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Bounded { .. })
    ));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    check_request(&server.single(), "/api/users.list");
    server.shutdown().await;
}

#[tokio::test]
async fn stream_emits_terminal_error_when_later_body_is_cut_off() {
    let truncated = Reply::json(200, b"")
        .header("x-slack-req-id", "synthetic-truncated")
        .with_body(Body::FailAfter(vec![
            b"{\"ok\":true,\"members\":[".as_slice().into(),
        ]));
    let (server, _) = scripted(vec![Reply::json(200, FIRST), truncated]).await;
    let mut stream = client(&server)
        .users(UsersOptions::default())
        .items()
        .into_stream();
    assert_eq!(stream.next().await.unwrap().unwrap().id, "U_SYNTHETIC_A");
    match stream.next().await.unwrap().unwrap_err() {
        PagesError::Provider {
            error: Error::Concord {
                meta: Some(meta), ..
            },
            ..
        } => {
            assert_eq!(meta.request_id(), Some("synthetic-truncated"));
        }
        other => panic!("expected truncated page error: {other:?}"),
    }
    assert!(stream.next().await.is_none());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for r in &requests {
        check_request(r, "/api/users.list");
    }
    assert_eq!(requests[1].query_all("cursor"), ["next +/=%"]);
    server.shutdown().await;
}

#[tokio::test]
async fn conversation_success_and_application_failure() {
    let (server, _) = scripted(vec![
        Reply::json(200, CONVERSATION).header("x-slack-req-id", "synthetic-conversation"),
        Reply::json(200, NOT_FOUND).header("x-slack-req-id", "synthetic-not-found"),
    ])
    .await;
    let slack = client(&server);
    let options = ConversationOptions {
        include_num_members: Some(true),
        ..Default::default()
    };
    let result = slack
        .conversation_info("C_SYNTHETIC_GENERAL", options)
        .await
        .unwrap();
    assert_eq!(result.value.id, "C_SYNTHETIC_GENERAL");
    assert_eq!(result.value.name.as_deref(), Some("general"));
    assert_eq!(result.value.num_members, Some(3));
    assert_eq!(result.meta.request_id(), Some("synthetic-conversation"));
    match slack
        .conversation_info("C_SYNTHETIC_MISSING", options)
        .await
        .unwrap_err()
    {
        Error::Api { error, meta } => {
            assert_eq!(error.code, ErrorCode::ChannelNotFound);
            assert!(meta.status.is_success());
            assert_eq!(meta.request_id(), Some("synthetic-not-found"));
        }
        other => panic!("expected application error: {other:?}"),
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for r in &requests {
        check_request(r, "/api/conversations.info");
        assert_eq!(r.query_all("include_num_members"), ["true"]);
    }
    assert_eq!(requests[0].query_all("channel"), ["C_SYNTHETIC_GENERAL"]);
    assert_eq!(requests[1].query_all("channel"), ["C_SYNTHETIC_MISSING"]);
    server.shutdown().await;
}

#[tokio::test]
async fn rate_limit_reports_delay_without_retrying() {
    let (server, _) = scripted(vec![
        Reply::json(429, b"not json").header("retry-after", "7"),
    ])
    .await;
    let error = client(&server)
        .conversation_info("C_SYNTHETIC_GENERAL", Default::default())
        .await
        .unwrap_err();
    assert!(
        matches!(error, Error::RateLimited { retry_after: RetryAfter::Seconds(d), .. } if d == Duration::from_secs(7))
    );
    let request = server.single();
    check_request(&request, "/api/conversations.info");
    assert_eq!(request.query_all("channel"), ["C_SYNTHETIC_GENERAL"]);
    server.shutdown().await;
}
