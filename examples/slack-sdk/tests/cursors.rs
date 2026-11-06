//! Maintenance regression: opaque cursor bytes across the public paging APIs and real HTTP.

mod support;

use std::num::NonZeroUsize;

use concord_example_testserver::{Recorded, Reply, Script, TestServer};
use concord_sdk_slack::{
    Error, ErrorCode, PageSize, UsersOptions,
    pagination::{Outcome, PagesError, Resume},
};
use support::{assert_get, client, fixture};

struct CursorCase {
    body: &'static [u8],
    raw: &'static str,
    encoded: &'static str,
}

// Wire expectations are hand-written, not produced by the encoder under test. Percent-looking
// text stays literal (including lowercase `f`); `&cursor=` must not inject another parameter.
const CASES: &[CursorCase] = &[
    CursorCase {
        body: include_bytes!("../fixtures/users-first.json"),
        raw: "opaque=+/&?%2F# space",
        encoded: "opaque%3D%2B%2F%26%3F%252F%23+space",
    },
    CursorCase {
        body: include_bytes!("../fixtures/users-cursor-padding.json"),
        raw: "==",
        encoded: "%3D%3D",
    },
    CursorCase {
        body: include_bytes!("../fixtures/users-whitespace-cursor.json"),
        raw: "  =  ",
        encoded: "++%3D++",
    },
    CursorCase {
        body: include_bytes!("../fixtures/users-cursor-percent-text.json"),
        raw: "%3D%2f%2B%2526&cursor=shadow#tail +/=",
        encoded: "%253D%252f%252B%252526%26cursor%3Dshadow%23tail+%2B%2F%3D",
    },
];

fn options() -> UsersOptions {
    UsersOptions {
        limit: PageSize::new(2).unwrap(),
        ..UsersOptions::default()
    }
}

async fn scripted(replies: Vec<Reply>) -> (TestServer, Script) {
    let script = Script::new(replies);
    let responder = script.clone();
    (TestServer::start(move |_| responder.next()).await, script)
}

fn replies(last: &'static [u8]) -> Vec<Reply> {
    CASES
        .iter()
        .map(|case| Reply::json(200, case.body))
        .chain([Reply::json(200, last)])
        .collect()
}

fn assert_cursor_request(request: &Recorded, case: &CursorCase) {
    assert_get(
        request,
        &format!("/api/users.list?limit=2&cursor={}", case.encoded),
    );
    assert_eq!(request.query_all("cursor"), [case.raw]);
    assert_eq!(request.query_all("limit"), ["2"]);
    assert_eq!(request.query_pairs().len(), 2);
}

fn assert_chain(server: &TestServer) {
    let requests = server.requests();
    assert_eq!(requests.len(), CASES.len() + 1);
    assert_get(&requests[0], "/api/users.list?limit=2");
    assert!(requests[0].query_all("cursor").is_empty());
    for (request, case) in requests[1..].iter().zip(CASES) {
        assert_cursor_request(request, case);
    }
}

#[tokio::test]
async fn manual_paging_round_trips_reserved_cursors_once() {
    for case in CASES {
        let (server, script) = scripted(vec![
            Reply::json(200, case.body),
            Reply::json(200, fixture("users-last-empty")),
        ])
        .await;
        let slack = client(&server);
        let options = options();
        let first = slack.users_page(&options, None).await.unwrap();
        let cursor = first.response_metadata.unwrap().next_cursor.unwrap();
        assert_eq!(cursor.as_str(), case.raw);
        let last = slack.users_page(&options, Some(&cursor)).await.unwrap();
        assert!(last.response_metadata.unwrap().next_cursor.is_none());
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert_get(&requests[0], "/api/users.list?limit=2");
        assert_cursor_request(&requests[1], case);
        assert_eq!(script.remaining(), 0);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn lazy_chain_preserves_cursors_and_stops_at_each_terminal_shape() {
    for terminal in [
        "users-last-empty",
        "users-last-null",
        "users-last-missing-cursor",
        "users-last-missing-metadata",
        "users-last-null-metadata", // Existing defensive tolerance, not a documented Slack shape.
    ] {
        let (server, script) = scripted(replies(fixture(terminal))).await;
        let mut pages = client(&server)
            .users(options())
            .max_pages(NonZeroUsize::new(CASES.len() + 1).unwrap());
        assert!(server.requests().is_empty());
        for (index, case) in CASES.iter().enumerate() {
            let page = pages.next_page().await.unwrap().unwrap();
            assert_eq!(page.next.as_ref().unwrap().as_str(), case.raw);
            assert_eq!(page.meta.response_metadata.unwrap().next_cursor, page.next);
            if index > 0 {
                assert!(page.items.is_empty(), "empty pages must still continue");
            }
        }
        assert!(pages.next_page().await.unwrap().unwrap().next.is_none());
        // A cap exactly at the final page is exhaustion; no probe or restart request is sent.
        assert!(pages.next_page().await.unwrap().is_none());
        assert!(pages.next_page().await.unwrap().is_none());
        assert_eq!(pages.finish().outcome, Outcome::Exhausted);
        assert_chain(&server);
        assert_eq!(script.remaining(), 0);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn cap_keeps_raw_cursor_for_an_explicit_manual_continuation() {
    let (server, script) = scripted(replies(fixture("users-last-empty"))).await;
    let slack = client(&server);
    let options = options();
    let mut pages = slack
        .users(options.clone())
        .max_pages(NonZeroUsize::new(CASES.len()).unwrap());
    for _ in CASES {
        assert!(pages.next_page().await.unwrap().is_some());
    }
    let cursor = match pages.next_page().await.unwrap_err() {
        PagesError::Bounded {
            resume: Resume::At(cursor),
            ..
        } => cursor,
        other => panic!("expected a resumable cap: {other:?}"),
    };
    assert_eq!(cursor.as_str(), CASES.last().unwrap().raw);
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended {
            outcome: Outcome::Bounded,
            ..
        })
    ));
    assert_eq!(pages.finish().outcome, Outcome::Bounded);
    assert_eq!(server.requests().len(), CASES.len());
    assert_eq!(script.remaining(), 1);
    let last = slack.users_page(&options, Some(&cursor)).await.unwrap();
    assert!(last.response_metadata.unwrap().next_cursor.is_none());
    assert_chain(&server);
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_cursor_keeps_the_prefix_and_exact_failed_cursor() {
    let (server, script) = scripted(replies(fixture("invalid-cursor"))).await;
    let incomplete = client(&server)
        .users(options())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].id, "U_SYNTHETIC_ALICE");
    match incomplete.error {
        PagesError::Provider {
            failed: Resume::At(cursor),
            error: Error::Api { error, meta },
            ..
        } => {
            assert_eq!(cursor.as_str(), CASES.last().unwrap().raw);
            assert_eq!(error.code, ErrorCode::InvalidCursor);
            assert!(meta.status.is_success());
        }
        other => panic!("expected typed invalid_cursor failure: {other:?}"),
    }
    assert_chain(&server);
    assert_eq!(script.remaining(), 0);
    server.shutdown().await;
}
