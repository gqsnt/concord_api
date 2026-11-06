//! Offline application-style checks of the public Notion SDK.

use std::num::NonZeroUsize;

use concord_example_testserver::{Recorded, Reply, Script, TestServer};
use concord_sdk_notion::{
    Error, ErrorCode, NotionClient, PageSize, Query, SelectEquals, pagination::PagesError, render,
    reqwest,
};
use futures_util::StreamExt;
use serde_json::{Value, json};

const QUERY_1: &str = include_str!("../fixtures/first-use-query-1.json");
const QUERY_2: &str = include_str!("../fixtures/first-use-query-2.json");
const PAGE: &str = include_str!("../fixtures/first-use-page.json");
const CHILDREN_1: &str = include_str!("../fixtures/first-use-children-1.json");
const CHILDREN_2: &str = include_str!("../fixtures/first-use-children-2.json");
const NESTED: &str = include_str!("../fixtures/first-use-nested.json");
const ERROR: &str = include_str!("../fixtures/first-use-error.json");
const INCOMPLETE: &str = include_str!("../fixtures/first-use-incomplete.json");

async fn server(replies: Vec<Reply>) -> TestServer {
    let script = Script::new(replies);
    TestServer::start(move |_| script.next()).await
}

fn client(server: &TestServer) -> NotionClient {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    NotionClient::new(http, "synthetic-notion-token")
        .with_base_url(&format!("{}/v1", server.url()))
        .unwrap()
}

fn query() -> Query {
    Query {
        filter: Some(SelectEquals::new("Priority", "Done")),
        page_size: PageSize::new(2).unwrap(),
    }
}

fn common(request: &Recorded, method: &str, path: &str) {
    assert_eq!(request.method, method);
    assert_eq!(request.path(), path);
    assert_eq!(
        request.header("authorization"),
        Some("Bearer synthetic-notion-token")
    );
    assert_eq!(request.header("notion-version"), Some("2026-03-11"));
}

fn check_query(request: &Recorded, cursor: Option<&str>) {
    common(request, "POST", "/v1/data_sources/source-synthetic/query");
    assert_eq!(request.header("content-type"), Some("application/json"));
    let actual: Value = serde_json::from_slice(&request.body).unwrap();
    let mut expected =
        json!({"filter":{"property":"Priority","select":{"equals":"Done"}},"page_size":2});
    if let Some(cursor) = cursor {
        expected["start_cursor"] = json!(cursor);
    }
    assert_eq!(actual, expected);
}

fn check_children(request: &Recorded, id: &str, cursor: Option<&str>) {
    common(request, "GET", &format!("/v1/blocks/{id}/children"));
    assert_eq!(request.query_all("page_size"), ["2"]);
    assert_eq!(
        request.query_all("start_cursor"),
        cursor.into_iter().collect::<Vec<_>>()
    );
    assert_eq!(
        request.query_pairs().len(),
        if cursor.is_some() { 2 } else { 1 }
    );
    assert!(request.body.is_empty());
}

#[tokio::test]
async fn query_all_then_retrieve_selected_page() {
    let server = server(vec![
        Reply::json(200, QUERY_1),
        Reply::json(200, QUERY_2),
        Reply::json(200, PAGE).header("x-request-id", "synthetic-response"),
    ])
    .await;
    let notion = client(&server);
    let results = notion
        .query_pages("source-synthetic", query())
        .items()
        .collect_complete()
        .await
        .unwrap();
    let ids: Vec<_> = results
        .iter()
        .map(|r| r.as_page().unwrap().id.as_str())
        .collect();
    assert_eq!(ids, ["page-alpha", "page-beta"]);
    let selected = notion.retrieve_page(ids[1]).await.unwrap();
    assert_eq!(selected.value.id, "page-beta");
    assert_eq!(
        render::property(&selected.value.properties["Name"]),
        "Launch plan"
    );
    assert_eq!(
        render::property(&selected.value.properties["Priority"]),
        "Done"
    );
    assert_eq!(
        selected.meta.headers().get("x-request-id").unwrap(),
        "synthetic-response"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    check_query(&requests[0], None);
    check_query(&requests[1], Some("query-next"));
    common(&requests[2], "GET", "/v1/pages/page-beta");
    assert!(requests[2].query_pairs().is_empty());
    assert!(requests[2].body.is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn failed_query_page_preserves_prefix_and_provider_error() {
    let server = server(vec![
        Reply::json(200, QUERY_1),
        Reply::json(429, ERROR).header("retry-after", "3"),
    ])
    .await;
    let incomplete = client(&server)
        .query_pages("source-synthetic", query())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].as_page().unwrap().id, "page-alpha");
    match incomplete.error {
        PagesError::Provider {
            error: Error::Provider { error, meta },
            ..
        } => {
            assert_eq!(error.code, ErrorCode::RateLimited);
            assert_eq!(error.message, "Slow down");
            assert_eq!(error.request_id.as_deref(), Some("synthetic-request"));
            assert_eq!(meta.status().as_u16(), 429);
            assert_eq!(meta.headers().get("retry-after").unwrap(), "3");
        }
        other => panic!("unexpected failure: {other:?}"),
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    check_query(&requests[0], None);
    check_query(&requests[1], Some("query-next"));
    server.shutdown().await;
}

#[tokio::test]
async fn capped_provider_query_is_not_success_even_at_last_page() {
    let server = server(vec![
        Reply::json(200, QUERY_1),
        Reply::json(200, INCOMPLETE),
    ])
    .await;
    let incomplete = client(&server)
        .query_pages("source-synthetic", query())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].as_page().unwrap().id, "page-alpha");
    match incomplete.error {
        PagesError::Provider {
            error: Error::IncompleteQuery { response },
            ..
        } => {
            assert_eq!(
                response.value.results[0].as_page().unwrap().id,
                "page-unemitted"
            );
        }
        other => panic!("unexpected failure: {other:?}"),
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    check_query(&requests[0], None);
    check_query(&requests[1], Some("query-next"));
    server.shutdown().await;
}

#[tokio::test]
async fn render_paginated_children_and_descendants() {
    let server = server(vec![
        Reply::json(200, CHILDREN_1),
        Reply::json(200, CHILDREN_2),
        Reply::json(200, NESTED),
    ])
    .await;
    let notion = client(&server);
    let blocks = notion
        .children("page-beta", PageSize::new(2).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(blocks.len(), 2);
    assert_eq!(render::block(&blocks[0]), "# Overview");
    assert_eq!(render::block(&blocks[1]), "Read the plan");
    assert!(blocks[1].has_children);
    let nested = notion
        .children(blocks[1].id.clone(), PageSize::new(2).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(nested.len(), 1);
    assert_eq!(render::block(&nested[0]), "- [x] Ship");
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    check_children(&requests[0], "page-beta", None);
    check_children(&requests[1], "page-beta", Some("children-next"));
    check_children(&requests[2], "paragraph", None);
    server.shutdown().await;
}

#[tokio::test]
async fn children_page_budget_reports_partial_work() {
    let server = server(vec![Reply::json(200, CHILDREN_1)]).await;
    let incomplete = client(&server)
        .children("page-beta", PageSize::new(2).unwrap())
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(render::block(&incomplete.collected[0]), "# Overview");
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    check_children(&requests[0], "page-beta", None);
    server.shutdown().await;
}

#[tokio::test]
async fn stream_emits_failure_before_ending() {
    let server = server(vec![Reply::json(200, CHILDREN_1), Reply::json(429, ERROR)]).await;
    let mut stream = client(&server)
        .children("page-beta", PageSize::new(2).unwrap())
        .items()
        .into_stream();
    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(render::block(&first), "# Overview");
    assert!(matches!(
        stream.next().await.unwrap(),
        Err(PagesError::Provider {
            error: Error::Provider { .. },
            ..
        })
    ));
    assert!(stream.next().await.is_none());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    check_children(&requests[0], "page-beta", None);
    check_children(&requests[1], "page-beta", Some("children-next"));
    server.shutdown().await;
}
