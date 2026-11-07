//! Connected workflows and continuation-loss regression, exclusively on loopback.

mod support;

use std::num::NonZeroUsize;

use concord_sdk_mediawiki_action_api::{
    Error,
    pagination_engine::{PagesError, Resume},
};
use concord_testserver::{Recorded, Reply, TestServer};
use support::*;

fn cap(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

#[tokio::test]
async fn search_choose_second_hit_and_fetch_its_raw_content() {
    let server = TestServer::start(|request: &Recorded| {
        assert_eq!(request.path(), "/w/api.php");
        match param(request, "list").as_deref() {
            Some("search") => Reply::json(200, SEARCH),
            _ => {
                assert_eq!(param(request, "pageids").as_deref(), Some("2"));
                Reply::json(200, CONTENT)
            }
        }
    })
    .await;
    let query = api(&server.url()).query();
    let found = query.search("Rust & café/?").send().await.unwrap();
    assert_eq!(found.data.search.len(), 2);
    let selected = &found.data.search[1];
    assert_eq!(selected.title, "Rust language");
    let content = query.fetch_page(selected.pageid).await.unwrap();
    assert_eq!(content.pageid, 2);
    assert_eq!(content.revision, 42);
    assert_eq!(
        content.content,
        "== Rust ==\nA synthetic [[programming language]].\n"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        param(&requests[0], "srsearch").as_deref(),
        Some("Rust & café/?")
    );
    assert!(param(&requests[1], "srsearch").is_none());
    server.shutdown().await;
}

#[tokio::test]
async fn search_can_continue_with_a_numeric_offset_and_generic_field() {
    let server = TestServer::start(|request: &Recorded| {
        if param(request, "sroffset").is_some() {
            assert_eq!(param(request, "sroffset").as_deref(), Some("2"));
            assert_eq!(param(request, "continue").as_deref(), Some("-||"));
            Reply::json(200, SEARCH_LAST)
        } else {
            Reply::json(200, SEARCH)
        }
    })
    .await;
    let query = api(&server.url()).query();
    let first = query.search("Rust").set_srlimit(2).send().await.unwrap();
    let last = query
        .search("Rust")
        .set_srlimit(2)
        .set_continuation(first.continuation.unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(last.data.search[0].pageid, 3);
    assert!(last.continuation.is_none());
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn category_traversal_follows_all_fields_through_an_empty_page() {
    let server = TestServer::start(category_service).await;
    let all = api(&server.url())
        .query()
        .category_pages("Category:Rust & café", 1, None, cap(3))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        all.iter().map(|page| page.pageid).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert!(param(&requests[0], "continue").is_none());
    for request in &requests {
        assert_eq!(
            param(request, "cmtitle").as_deref(),
            Some("Category:Rust & café")
        );
        assert_eq!(param(request, "cmlimit").as_deref(), Some("1"));
    }
    assert_eq!(
        param(&requests[1], "cmcontinue").as_deref(),
        Some("page|café & /+|2")
    );
    assert_eq!(param(&requests[1], "continue").as_deref(), Some("-||"));
    assert_eq!(param(&requests[1], "futureoffset").as_deref(), Some("7"));
    assert_eq!(param(&requests[2], "continue").as_deref(), Some("||"));
    assert_eq!(param(&requests[2], "futureoffset").as_deref(), Some("8"));
    server.shutdown().await;
}

#[tokio::test]
async fn negative_control_dropping_generic_continue_repeats_a_page() {
    let server = TestServer::start(category_service).await;
    let query = api(&server.url()).query();
    let first = query
        .category_members("Category:Rust")
        .send()
        .await
        .unwrap();
    let fields = first.continuation.unwrap();
    // Deliberate test-only mutation reproduces a broken consumer. Preserve every other
    // continuation field so this isolates loss of the generic `continue` parameter.
    let (http, mut request, decoder) = query
        .category_members("Category:Rust")
        .prepare()
        .unwrap()
        .into_parts();
    for (key, value) in fields.fields() {
        if key != "continue" {
            request.url_mut().query_pairs_mut().append_pair(key, value);
        }
    }
    let bytes = decoder
        .execute(&http, request)
        .await
        .unwrap()
        .bytes_limited(concord_sdk_mediawiki_action_api::BODY_LIMIT)
        .await
        .unwrap();
    let seen = server.requests();
    assert!(param(&seen[1], "continue").is_none());
    assert_eq!(param(&seen[1], "futureoffset").as_deref(), Some("7"));
    let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let repeated_id = wire["query"]["categorymembers"][0]["pageid"]
        .as_u64()
        .unwrap();
    assert_eq!([first.data.categorymembers[0].pageid, repeated_id], [1, 1]);
    server.shutdown().await;
}

#[tokio::test]
async fn bounded_prefix_can_resume_with_the_whole_token() {
    let server = TestServer::start(category_service).await;
    let query = api(&server.url()).query();
    let incomplete = query
        .category_pages("Category:Rust", 1, None, cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected[0].pageid, 1);
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    let Resume::At(next) = incomplete.error.resume().unwrap().clone() else {
        panic!("resume key")
    };
    assert_eq!(next.fields().len(), 3);
    let tail = query
        .category_pages("Category:Rust", 1, Some(next), cap(2))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        tail.iter().map(|page| page.pageid).collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_page_preserves_prefix_and_does_not_report_exhaustion() {
    let server = TestServer::start(|request: &Recorded| {
        if param(request, "continue").is_none() {
            Reply::json(200, CATEGORY_FIRST)
        } else {
            Reply::json(200, API_ERROR)
        }
    })
    .await;
    let mut pages = api(&server.url())
        .query()
        .category_pages("Category:Rust", 1, None, cap(3));
    assert_eq!(pages.next_page().await.unwrap().unwrap().items[0].pageid, 1);
    let failure = pages.next_page().await.unwrap_err();
    assert!(matches!(failure.provider_error(), Some(Error::Api { .. })));
    assert_eq!(failure.resume().unwrap().key().unwrap().fields().len(), 3);
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
