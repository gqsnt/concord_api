//! Maintenance regressions and consumer workflows; all responses are local synthetic fixtures.

mod support;

use std::num::NonZeroU32;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_open_library::{Work, WorkSort};

#[test]
fn description_text_accepts_both_shapes_and_preserves_absence_and_empty_text() {
    for (body, expected) in [
        (r#"{}"#, None),
        (r#"{"description":null}"#, None),
        (r#"{"description":""}"#, Some("")),
        (
            r#"{"description":{"type":"/type/text","value":""}}"#,
            Some(""),
        ),
        (
            r#"{"description":"  Café\n*notes*  "}"#,
            Some("  Café\n*notes*  "),
        ),
        (
            r#"{"description":{"type":"/type/text","value":"  Café\n*notes*  ","extra":true}}"#,
            Some("  Café\n*notes*  "),
        ),
    ] {
        let work: Work = serde_json::from_str(body).unwrap();
        assert_eq!(work.description_text(), expected, "fixture: {body}");
    }
}

#[test]
fn unrecognized_descriptions_remain_raw_without_fabricating_text() {
    for description in [
        serde_json::json!(42),
        serde_json::json!(["text"]),
        serde_json::json!({"value": "text"}),
        serde_json::json!({"type": "/type/other", "value": "text"}),
        serde_json::json!({"type": "/type/text"}),
        serde_json::json!({"type": "/type/text", "value": null}),
        serde_json::json!({"type": "/type/text", "value": 42}),
    ] {
        let work: Work = serde_json::from_value(serde_json::json!({
            "description": description.clone(),
        }))
        .unwrap();
        assert_eq!(work.description_text(), None);
        assert_eq!(work.description.as_ref(), Some(&description));
    }
}

#[tokio::test]
async fn consumer_sorts_search_then_reads_selected_work_text() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/search.json" => Reply::json(200, r#"{"docs":[{"key":"/works/OL27448W"}]}"#),
        "/works/OL27448W.json" => Reply::json(
            200,
            r#"{"description":{"type":"/type/text","value":"A local description."}}"#,
        ),
        _ => Reply::new(500),
    })
    .await;
    let api = support::client(&server);
    for (sort, wire) in [
        (WorkSort::New, "new"),
        (WorkSort::Old, "old"),
        (WorkSort::Random, "random"),
        (WorkSort::Key, "key"),
    ] {
        let page = api
            .search()
            .works("local notes")
            .set_page(NonZeroU32::new(2).unwrap())
            .set_limit(NonZeroU32::new(3).unwrap())
            .set_sort(sort)
            .send()
            .await
            .unwrap();
        let work = api
            .works()
            .get(page.docs[0].id().unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(work.description_text(), Some("A local description."));
        assert!(work.title.is_none() && work.subjects.is_none());
        let requests = server.requests();
        assert_eq!(
            requests[requests.len() - 2].target,
            format!("/search.json?q=local+notes&page=2&limit=3&sort={wire}")
        );
        assert_eq!(requests.last().unwrap().target, "/works/OL27448W.json");
    }

    api.search()
        .works("local notes")
        .set_sort(WorkSort::Old)
        .maybe_sort(None)
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 9);
    assert_eq!(
        requests[8].target,
        "/search.json?q=local+notes&page=1&limit=20"
    );
    for request in requests {
        assert_eq!(request.method, "GET");
        assert_eq!(request.host, server.addr().to_string());
        assert_eq!(request.header("accept"), Some("application/json"));
        assert_eq!(request.body_len, 0);
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn consumer_reads_both_text_shapes_in_an_authors_work_batch() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/search/authors.json" => Reply::json(200, r#"{"docs":[{"key":"OL23919A"}]}"#),
        "/authors/OL23919A/works.json" => Reply::json(
            200,
            r#"{"entries":[
            {"description":"A local description."},
            {"description":{"type":"/type/text","value":"A local description."}},
            {}, {"description":null}, {"description":""},
            {"description":{"type":"/type/text","value":""}}
        ]}"#,
        ),
        _ => Reply::new(500),
    })
    .await;
    let api = support::client(&server);
    let authors = api.search().authors("local author").send().await.unwrap();
    let works = api
        .author(authors.docs[0].id().unwrap())
        .works()
        .send()
        .await
        .unwrap();
    let texts: Vec<Option<&str>> = works.entries.iter().map(Work::description_text).collect();
    assert_eq!(
        texts,
        [
            Some("A local description."),
            Some("A local description."),
            None,
            None,
            Some(""),
            Some("")
        ]
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].target, "/search/authors.json?q=local+author");
    assert_eq!(requests[1].target, "/authors/OL23919A/works.json");
    assert_eq!(server.shutdown().await.open_after, 0);
}
