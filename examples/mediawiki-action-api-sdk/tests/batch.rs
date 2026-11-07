//! Batch content fetch: wire shape, continuation merge and fail-closed cases, on loopback only.
//! Sources: API:Query, API:Revisions and API:Continue/en, read 2026-11-07. Fixtures are synthetic.

mod support;

use concord_sdk_mediawiki_action_api::{
    BATCH_BODY_LIMIT, BODY_LIMIT, Error, PageAnswer, PageContent, PageIdLimit, PageIds,
    TooManyPageIds, Unavailable, concord::ErrorKind,
};
use concord_testserver::{Recorded, Reply, TestServer};
use support::*;

const REQUESTED: [u64; 7] = [12, 404, 7, 12, 31, 5, 8];

fn ids(list: &[u64]) -> PageIds {
    PageIds::new(list.iter().copied(), PageIdLimit::Standard).unwrap()
}

fn sorted_pairs(request: &Recorded) -> Vec<(String, String)> {
    let mut pairs = request.query_pairs();
    pairs.sort();
    pairs
}

fn expected_pairs(extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut pairs: Vec<_> = [
        ("action", "query"),
        ("format", "json"),
        ("formatversion", "2"),
        ("pageids", BATCH_WIRE_IDS),
        ("prop", "revisions"),
        ("rvprop", "ids|content"),
        ("rvslots", "main"),
    ]
    .iter()
    .chain(extra)
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    pairs.sort();
    pairs
}

fn content(
    pageid: u64,
    title: &str,
    revision: u64,
    text: &str,
) -> Result<PageContent, Unavailable> {
    Ok(PageContent {
        pageid,
        title: title.into(),
        revision,
        content: text.into(),
    })
}

fn invalid(result: Result<Vec<PageAnswer>, Error>) {
    assert!(
        matches!(result, Err(Error::InvalidResponse(ref meta)) if meta.status() == 200),
        "{result:?}"
    );
}

#[tokio::test]
async fn batch_sends_distinct_ids_without_rvlimit_and_replays_the_whole_continuation() {
    let server = TestServer::start(batch_service).await;
    api(&server.url())
        .query()
        .fetch_pages(&ids(&REQUESTED))
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/w/api.php");
        assert_eq!(request.header("user-agent"), Some(UA));
        // API:Revisions: rvlimit may only be used with a single page.
        assert!(request.query_all("rvlimit").is_empty());
    }
    assert_eq!(sorted_pairs(&requests[0]), expected_pairs(&[]));
    assert_eq!(
        sorted_pairs(&requests[1]),
        expected_pairs(&[("rvcontinue", "7|9001"), ("continue", "||")])
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_page_listed_without_revision_in_a_continued_response_is_answered_later() {
    // Precondition of the regression: response 1 continues and lists page 7 without revisions.
    let first: serde_json::Value = serde_json::from_str(BATCH_FIRST).unwrap();
    assert!(first["continue"].is_object());
    assert!(first["query"]["pages"][2]["revisions"].is_null());
    assert_eq!(first["query"]["pages"][2]["pageid"], 7);

    let server = TestServer::start(batch_service).await;
    let answers = api(&server.url())
        .query()
        .fetch_pages(&ids(&REQUESTED))
        .await
        .unwrap();
    let alpha = content(12, "Batch alpha", 120, "Alpha synthetic text.\n");
    assert_eq!(
        answers,
        [
            PageAnswer {
                pageid: 12,
                content: alpha.clone()
            },
            PageAnswer {
                pageid: 404,
                content: Err(Unavailable::Missing)
            },
            PageAnswer {
                pageid: 7,
                content: content(
                    7,
                    "Batch beta",
                    70,
                    "Beta synthetic text, café & |pipes|.\n"
                ),
            },
            PageAnswer {
                pageid: 12,
                content: alpha
            },
            PageAnswer {
                pageid: 31,
                content: Err(Unavailable::NoContent)
            },
            PageAnswer {
                pageid: 5,
                content: Err(Unavailable::Invalid)
            },
            PageAnswer {
                pageid: 8,
                content: Err(Unavailable::NoContent)
            },
        ]
    );
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn replayed_or_stalled_continuation_fails_closed_after_one_follow_up() {
    // A server that ignores the continuation replays page 12's revision: not progress.
    let replay = TestServer::start(|_: &Recorded| Reply::json(200, BATCH_FIRST)).await;
    invalid(
        api(&replay.url())
            .query()
            .fetch_pages(&ids(&REQUESTED))
            .await,
    );
    assert_eq!(replay.requests().len(), 2);
    replay.shutdown().await;

    // A second revision for an answered page is refused even when the response also answers
    // the pending pages (API:Continue: the next response carries the *next* properties).
    let again = TestServer::start(|request: &Recorded| {
        if param(request, "rvcontinue").is_none() {
            return Reply::json(200, BATCH_FIRST);
        }
        let alpha = r#"{"pageid": 12, "ns": 0, "title": "Batch alpha"}"#;
        let first: serde_json::Value = serde_json::from_str(BATCH_FIRST).unwrap();
        let replayed = first["query"]["pages"][1].to_string();
        Reply::json(200, BATCH_LAST.replace(alpha, &replayed))
    })
    .await;
    invalid(
        api(&again.url())
            .query()
            .fetch_pages(&ids(&REQUESTED))
            .await,
    );
    assert_eq!(again.requests().len(), 2);
    again.shutdown().await;

    // A continued response that answers no pending page would otherwise loop forever; a
    // third request would receive an API error instead of the expected InvalidResponse.
    let stalled =
        TestServer::start(
            |request: &Recorded| match param(request, "rvcontinue").as_deref() {
                None => Reply::json(200, BATCH_FIRST),
                Some("7|9001") => Reply::json(
                    200,
                    BATCH_LAST
                        .replace(
                            r#""batchcomplete": true,"#,
                            r#""continue": {"rvcontinue": "7|9002", "continue": "||"},"#,
                        )
                        .replace(r#""revisions""#, r#""unrelated""#),
                ),
                Some(_) => Reply::json(200, API_ERROR),
            },
        )
        .await;
    invalid(
        api(&stalled.url())
            .query()
            .fetch_pages(&ids(&REQUESTED))
            .await,
    );
    assert_eq!(stalled.requests().len(), 2);
    stalled.shutdown().await;
}

#[tokio::test]
async fn rows_that_cannot_belong_to_the_batch_are_invalid_responses() {
    let revision = r#""revisions":[{"revid":1,"slots":{"main":{"content":"x"}}}]"#;
    let page_1 = format!(r#"{{"pageid":1,"title":"One",{revision}}}"#);
    let page_2 = r#"{"pageid":2,"missing":true}"#;
    for rows in [
        format!(r#"{page_1},{page_2},{{"pageid":3,"missing":true}}"#), // not requested
        format!(r#"{page_1},{page_2},{{"title":"No id","invalid":true}}"#), // unattributable
        format!(r#"{page_1},{page_2},{page_2}"#),                      // duplicate row
        page_1.clone(),                                                // ID 2 never answered
        format!(
            r#"{{"pageid":1,"title":"One","revisions":[{{"revid":1}},{{"revid":2}}]}},{page_2}"#
        ), // two latest revisions
        format!(r#"{{"pageid":1,{revision}}},{page_2}"#),              // content without a title
    ] {
        let body = format!(r#"{{"batchcomplete":true,"query":{{"pages":[{rows}]}}}}"#);
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, &body)).await;
        invalid(api(&server.url()).query().fetch_pages(&ids(&[1, 2])).await);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn whole_response_failures_fail_the_batch_and_warnings_are_not_followed() {
    for mode in 0..3 {
        let server = TestServer::start(move |request: &Recorded| {
            if param(request, "continue").is_none() {
                return match mode {
                    // The SDK refuses every warnings envelope, also on a continued response.
                    0 => Reply::json(
                        200,
                        BATCH_FIRST.replacen(
                            '{',
                            r#"{"warnings":{"result":{"warnings":"synthetic"}},"#,
                            1,
                        ),
                    ),
                    _ => Reply::json(200, BATCH_FIRST),
                };
            }
            match mode {
                1 => Reply::json(200, API_ERROR),
                _ => Reply::json(200, r#"{"batchcomplete":true}"#),
            }
        })
        .await;
        let result = api(&server.url())
            .query()
            .fetch_pages(&ids(&REQUESTED))
            .await;
        match mode {
            0 => assert!(matches!(result, Err(Error::Warnings(_))), "{result:?}"),
            1 => assert!(matches!(result, Err(Error::Api { .. })), "{result:?}"),
            _ => invalid(result),
        }
        assert_eq!(server.requests().len(), if mode == 0 { 1 } else { 2 });
        server.shutdown().await;
    }
}

#[tokio::test]
async fn page_id_limits_are_checked_before_anything_is_sent() {
    // API:Query: at most 50 page IDs, 500 for clients allowed higher limits.
    assert_eq!(PageIdLimit::default(), PageIdLimit::Standard);
    assert!(PageIds::new(1..=50, PageIdLimit::Standard).is_ok());
    assert_eq!(
        PageIds::new(1..=51, PageIdLimit::Standard),
        Err(TooManyPageIds {
            count: 51,
            limit: 50
        })
    );
    assert_eq!(
        PageIds::new(vec![9; 51], PageIdLimit::Standard),
        Err(TooManyPageIds {
            count: 51,
            limit: 50
        })
    );
    assert!(PageIds::new(1..=500, PageIdLimit::HighLimits).is_ok());
    assert_eq!(
        PageIds::new(1..=501, PageIdLimit::HighLimits)
            .unwrap_err()
            .to_string(),
        "501 page IDs exceed the limit of 500"
    );
    assert_eq!(ids(&[3, 1, 3, 2]).to_string(), "3|1|2");
    assert_eq!(ids(&[3, 1, 3, 2]).requested(), [3, 1, 3, 2]);

    let server = TestServer::start(|_: &Recorded| panic!("an empty batch sends nothing")).await;
    let answers = api(&server.url())
        .query()
        .fetch_pages(&ids(&[]))
        .await
        .unwrap();
    assert!(answers.is_empty());
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn batch_bodies_have_their_own_bound_and_single_page_bodies_keep_theirs() {
    let query = api("http://127.0.0.1:9").query();
    let (_, _, decoder) = query.contents(ids(&[1])).prepare().unwrap().into_parts();
    assert_eq!(decoder.limit(), Some(BATCH_BODY_LIMIT));
    let (_, _, decoder) = query.content(1).prepare().unwrap().into_parts();
    assert_eq!(decoder.limit(), Some(BODY_LIMIT));

    // 2 MiB of content: an ordinary batch result, above the single-page bound.
    let text = "a".repeat(2 * 1024 * 1024);
    let body = format!(
        r#"{{"batchcomplete":true,"query":{{"pages":[{{"pageid":1,"title":"Big","revisions":[{{"revid":3,"slots":{{"main":{{"content":"{text}"}}}}}}]}}]}}}}"#
    );
    let server = TestServer::start(move |_: &Recorded| Reply::json(200, &body)).await;
    let query = api(&server.url()).query();
    let answers = query.fetch_pages(&ids(&[1])).await.unwrap();
    assert_eq!(
        answers[0].content.as_ref().unwrap().content.len(),
        text.len()
    );
    let Err(Error::Concord(error)) = query.fetch_page(1).await else {
        panic!("single-page bound unchanged")
    };
    assert_eq!(error.kind(), ErrorKind::Limit);
    server.shutdown().await;
}

#[tokio::test]
async fn a_continuation_cannot_overwrite_the_batch_inputs() {
    for key in ["pageids", "prop", "rvprop", "rvslots", "rvlimit"] {
        let body =
            format!(r#"{{"continue":{{"continue":"||","{key}":"9"}},"query":{{"pages":[]}}}}"#);
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, &body)).await;
        let result = api(&server.url()).query().fetch_pages(&ids(&[1])).await;
        assert!(
            matches!(&result, Err(Error::Concord(e)) if e.kind() == ErrorKind::Decode),
            "{key}: {result:?}"
        );
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn single_page_fetch_shares_the_row_rule() {
    // A page ID the wiki does not know is listed without `ns`/`title` (FIRST_USE F1).
    for (body, unavailable) in [
        (r#"{"query":{"pages":[{"pageid":2,"missing":true}]}}"#, true),
        (
            r#"{"query":{"pages":[{"pageid":2,"revisions":[{"revid":4,"slots":{"main":{"content":"x"}}}]}]}}"#,
            false,
        ),
    ] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, body)).await;
        let result = api(&server.url()).query().fetch_page(2).await;
        if unavailable {
            assert!(
                matches!(result, Err(Error::ContentUnavailable)),
                "{result:?}"
            );
        } else {
            assert!(
                matches!(result, Err(Error::InvalidResponse(_))),
                "{result:?}"
            );
        }
        server.shutdown().await;
    }
}
