//! Public developer paths against loopback fixtures, including partial outcomes.

mod support;

use std::{
    future::{Future, poll_fn},
    num::NonZeroUsize,
    task::Poll,
    time::Duration,
};

use concord_example_testserver::{Reply, TestServer};
use concord_sdk_slack::{
    ConversationOptions, Error, ErrorCode, PageSize, RetryAfter, UsersOptions,
    pagination::{Outcome, PagesError},
};
use futures_util::StreamExt;
use support::{assert_get, client, fixture};

const FIRST: &str = "/api/users.list?limit=2";
const SECOND: &str = "/api/users.list?limit=2&cursor=opaque%3D%2B%2F%26%3F%252F%23+space";
const THIRD: &str = "/api/users.list?limit=2&cursor=second%3Dcursor";

fn options() -> UsersOptions {
    UsersOptions {
        limit: PageSize::new(2).unwrap(),
        ..UsersOptions::default()
    }
}

#[tokio::test]
async fn workflow_a_pages_users_through_short_and_empty_continuing_pages() {
    let server = TestServer::start(|request| {
        let name = match request.target.as_str() {
            FIRST => "users-first",
            SECOND => "users-empty-continuing",
            THIRD => "users-last-empty",
            other => panic!("unexpected target {other}"),
        };
        Reply::json(200, fixture(name)).header("x-slack-req-id", "synthetic-page-id")
    })
    .await;
    let mut pages = client(&server).users(options());
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 1); // Less than limit still continues.
    assert_eq!(first.meta.http.request_id(), Some("synthetic-page-id"));
    assert_eq!(first.meta.warning.as_deref(), Some("synthetic_warning"));
    assert_eq!(first.meta.cache_ts, Some(1234567890));
    assert!(first.next.is_some());
    let empty = pages.next_page().await.unwrap().unwrap();
    assert!(empty.items.is_empty() && empty.next.is_some());
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(last.items[0].name.as_deref(), Some("bob"));
    assert_eq!(last.items[0].deleted, Some(true));
    assert!(last.next.is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    for (request, expected) in server.requests().iter().zip([FIRST, SECOND, THIRD]) {
        assert_get(request, expected);
    }
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_a_ends_without_an_extra_request_for_every_terminal_shape() {
    for name in [
        "users-last-empty",
        "users-last-null",
        "users-last-missing-cursor",
        "users-last-missing-metadata",
        "users-last-null-metadata",
    ] {
        let server = TestServer::start(move |_| Reply::json(200, fixture(name))).await;
        let mut pages = client(&server)
            .users(options())
            .max_pages(NonZeroUsize::new(1).unwrap());
        assert!(pages.next_page().await.unwrap().is_some());
        // A cap on exactly the final page is exhaustion, not a bounded prefix.
        assert!(pages.next_page().await.unwrap().is_none(), "{name}");
        assert_eq!(pages.finish().outcome, Outcome::Exhausted);
        assert_get(&server.single(), FIRST);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn workflow_a_complete_collection_and_standard_item_stream() {
    let server = TestServer::start(|request| {
        Reply::json(
            200,
            fixture(if request.query_all("cursor").is_empty() {
                "users-first"
            } else {
                "users-last-empty"
            }),
        )
    })
    .await;
    let slack = client(&server);
    let users = slack
        .users(options())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        users.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(),
        ["U_SYNTHETIC_ALICE", "U_SYNTHETIC_BOB"]
    );
    let mut stream = slack.users(options()).items().into_stream();
    assert_eq!(
        stream.next().await.unwrap().unwrap().name.as_deref(),
        Some("alice")
    );
    assert_eq!(
        stream.next().await.unwrap().unwrap().name.as_deref(),
        Some("bob")
    );
    assert!(stream.next().await.is_none());
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_a_limit_is_an_error_that_keeps_the_prefix() {
    let server = TestServer::start(|_| Reply::json(200, fixture("users-first"))).await;
    let failure = client(&server)
        .users(options())
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(failure.collected.len(), 1);
    assert_eq!(failure.collected[0].name.as_deref(), Some("alice"));
    match failure.error {
        PagesError::Bounded { resume, .. } => {
            assert!(!format!("{resume:?}").contains("opaque"));
        }
        other => panic!("expected bounded prefix: {other:?}"),
    }
    assert_get(&server.single(), FIRST);
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_a_failed_second_page_keeps_prefix_and_typed_provider_error() {
    let server = TestServer::start(|request| {
        Reply::json(
            200,
            fixture(if request.query_all("cursor").is_empty() {
                "users-first"
            } else {
                "invalid-cursor"
            }),
        )
        .header("x-slack-req-id", "synthetic-invalid-cursor-id")
    })
    .await;
    let failure = client(&server)
        .users(options())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(failure.collected[0].id, "U_SYNTHETIC_ALICE");
    match failure.error {
        PagesError::Provider {
            error: Error::Api { error, meta },
            ..
        } => {
            assert_eq!(error.code, ErrorCode::InvalidCursor);
            assert_eq!(meta.request_id(), Some("synthetic-invalid-cursor-id"));
            assert_eq!(meta.status.as_u16(), 200);
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(server.requests().len(), 2);
    assert_get(&server.requests()[1], SECOND);
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_a_stream_yields_terminal_error_before_ending() {
    let server = TestServer::start(|request| {
        if request.query_all("cursor").is_empty() {
            Reply::json(200, fixture("users-first"))
        } else {
            Reply::json(429, fixture("rate-limited")).header("retry-after", "30")
        }
    })
    .await;
    let mut stream = client(&server).users(options()).items().into_stream();
    assert_eq!(
        stream.next().await.unwrap().unwrap().name.as_deref(),
        Some("alice")
    );
    assert!(
        matches!(stream.next().await.unwrap(), Err(PagesError::Provider {
        error: Error::RateLimited { retry_after: RetryAfter::Seconds(delay), .. }, ..
    }) if delay == Duration::from_secs(30))
    );
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_a_failure_does_not_turn_into_exhaustion_on_next_call() {
    let server = TestServer::start(|_| Reply::json(200, fixture("invalid-cursor"))).await;
    let mut pages = client(&server).users(options());
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Provider { .. })
    ));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(pages.finish().outcome, Outcome::Failed);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_b_retrieves_conversation_then_surfaces_application_failure() {
    let server = TestServer::start(|request| match request.target.as_str() {
        "/api/conversations.info?channel=C_SYNTHETIC_GENERAL&include_num_members=true" => {
            Reply::json(200, fixture("conversation"))
        }
        "/api/conversations.info?channel=C_SYNTHETIC_MISSING" => {
            Reply::json(200, fixture("channel-not-found"))
        }
        other => panic!("unexpected target {other}"),
    })
    .await;
    let slack = client(&server);
    let conversation = slack
        .conversation_info(
            "C_SYNTHETIC_GENERAL",
            ConversationOptions {
                include_num_members: Some(true),
                ..ConversationOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(conversation.value.id, "C_SYNTHETIC_GENERAL");
    assert_eq!(conversation.value.num_members, Some(2));
    let failure = slack
        .conversation_info("C_SYNTHETIC_MISSING", ConversationOptions::default())
        .await
        .unwrap_err();
    assert!(
        matches!(failure, Error::Api { error, meta } if error.code == ErrorCode::ChannelNotFound && meta.status.as_u16() == 200)
    );
    assert_eq!(server.requests().len(), 2);
    for request in server.requests() {
        assert_get(&request, &request.target);
    }
    server.shutdown().await;
}

#[tokio::test]
async fn rate_limit_wait_is_caller_controlled_with_virtual_time_and_no_automatic_retry() {
    let server = TestServer::start(|_| {
        Reply::json(429, fixture("rate-limited")).header("retry-after", "30")
    })
    .await;
    let slack = client(&server);
    let failure = slack
        .conversation_info("C_SYNTHETIC_GENERAL", ConversationOptions::default())
        .await
        .unwrap_err();
    let delay = match failure {
        Error::RateLimited {
            retry_after: RetryAfter::Seconds(delay),
            ..
        } => delay,
        other => panic!("unexpected failure {other:?}"),
    };
    tokio::time::pause();
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);
    assert!(poll_fn(|cx| Poll::Ready(sleep.as_mut().poll(cx).is_pending())).await);
    tokio::time::advance(Duration::from_secs(29)).await;
    assert!(poll_fn(|cx| Poll::Ready(sleep.as_mut().poll(cx).is_pending())).await);
    assert_eq!(server.requests().len(), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    sleep.await;
    assert_eq!(server.requests().len(), 1); // Waiting alone never re-executes the SDK request.
    tokio::time::resume();
    let _explicit_retry = slack
        .conversation_info("C_SYNTHETIC_GENERAL", ConversationOptions::default())
        .await
        .unwrap_err();
    assert_eq!(server.requests().len(), 2);
    for request in server.requests() {
        assert_get(
            &request,
            "/api/conversations.info?channel=C_SYNTHETIC_GENERAL",
        );
    }
    server.shutdown().await;
}
