//! End-to-end public SDK workflows and explicit partial outcomes.
use concord::reqwest;
use concord_example_testserver::{Reply, Script, TestServer};
use concord_sdk_notion::models::{BlockValue, QueryResult};
use concord_sdk_notion::pagination::{PagesError, Resume};
use concord_sdk_notion::{
    EnvelopeProblem, Error, ErrorCode, NotionClient, PageSize, Query, SelectEquals, render,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::num::NonZeroUsize;

const QUERY1: &[u8] = include_bytes!("../fixtures/query-1.json");
const QUERY2: &[u8] = include_bytes!("../fixtures/query-2.json");
const PAGE: &[u8] = include_bytes!("../fixtures/page.json");
const CHILDREN1: &[u8] = include_bytes!("../fixtures/children-1.json");
const CHILDREN2: &[u8] = include_bytes!("../fixtures/children-2.json");
const LAST: &[u8] = include_bytes!("../fixtures/empty-last.json");

fn reply(body: &[u8]) -> Reply {
    Reply::new(200)
        .header("Content-Type", "application/json")
        .body(body)
}
fn client(server: &TestServer) -> NotionClient {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    NotionClient::new(http, "synthetic-workflow-token")
        .with_base_url(&format!("{}/v1", server.url()))
        .unwrap()
}
fn query() -> Query {
    Query {
        filter: Some(SelectEquals::new("Priority", "Done")),
        page_size: PageSize::new(2).unwrap(),
    }
}
fn cap() -> NonZeroUsize {
    NonZeroUsize::new(20).unwrap()
}

#[tokio::test]
async fn workflow_a_queries_every_page_then_retrieves_selected_page() {
    let script = Script::new([reply(QUERY1), reply(QUERY2), reply(PAGE)]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let results = api
        .query_pages("data-source", query())
        .max_pages(cap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(results.len(), 3);
    let pages: Vec<_> = results.iter().filter_map(QueryResult::as_page).collect();
    assert_eq!(pages.len(), 2);
    let QueryResult::Other { object, raw } = &results[2] else {
        panic!("wiki data source result lost")
    };
    assert_eq!(object, "data_source");
    assert_eq!(raw["synthetic_wiki_child"], true);
    let selected = pages
        .iter()
        .find(|page| render::property(&page.properties["Name"]) == "Plan café 🚀")
        .unwrap();
    let page = api.retrieve_page(selected.id.clone()).await.unwrap();
    assert_eq!(render::property(&page.value.properties["Priority"]), "Done");
    assert_eq!(
        render::property(&page.value.properties["Future"]),
        "[unsupported property: future_metric]"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[0].body,
        include_bytes!("../fixtures/query-request-1.json")
    );
    assert_eq!(
        requests[1].body,
        include_bytes!("../fixtures/query-request-2.json")
    );
    assert_eq!(
        requests[2].target,
        "/v1/pages/22222222-2222-4222-8222-222222222222"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_b_reads_second_children_page_and_expands_known_and_unknown_parents() {
    let server = TestServer::start(|request| match request.path() {
        "/v1/pages/page" => reply(PAGE),
        "/v1/blocks/page/children" if request.query_all("start_cursor").is_empty() => {
            reply(CHILDREN1)
        }
        "/v1/blocks/page/children" => {
            assert_eq!(request.query_all("start_cursor"), ["children/cursor+two"]);
            reply(CHILDREN2)
        }
        "/v1/blocks/paragraph-with-children/children" => {
            reply(include_bytes!("../fixtures/nested-children.json"))
        }
        "/v1/blocks/unknown-with-children/children" => {
            reply(include_bytes!("../fixtures/unknown-children.json"))
        }
        _ => panic!("unexpected workflow request"),
    })
    .await;
    let api = client(&server);
    let page = api.retrieve_page("page").await.unwrap().value;
    assert_eq!(render::property(&page.properties["Name"]), "Plan café 🚀");
    let blocks = api
        .children("page", PageSize::new(2).unwrap())
        .max_pages(cap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(blocks.len(), 6);
    assert!(
        matches!(&blocks[2].value, BlockValue::Unknown { kind, .. } if kind == "future_callout")
    );
    let mut rendered = Vec::new();
    for block in blocks {
        rendered.push(render::block(&block));
        if block.has_children {
            let children = api
                .children(block.id, PageSize::default())
                .max_pages(cap())
                .items()
                .collect_complete()
                .await
                .unwrap();
            assert!(children.iter().all(|child| !child.has_children)); // fixture depth is two
            rendered.extend(
                children
                    .iter()
                    .map(|child| format!("  {}", render::block(child))),
            );
        }
    }
    assert_eq!(
        rendered,
        [
            "## Plan",
            "Hello café 🚀",
            "  Nested detail.",
            "[unsupported block: future_callout]",
            "  Inside future block.",
            "- [x] Ship",
            "- Review",
            "---"
        ]
    );
    assert_eq!(server.requests().len(), 5); // page metadata, 2 root pages, 2 child parents
    server.shutdown().await;
}

#[tokio::test]
async fn a_failed_second_page_keeps_the_prefix_and_typed_provider_error() {
    let script = Script::new([
        reply(QUERY1),
        Reply::new(429)
            .header("Content-Type", "application/json")
            .header("Retry-After", "5")
            .body(include_bytes!("../fixtures/error-429.json")),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let partial = client(&server)
        .query_pages("source", query())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(partial.collected.len(), 1);
    let PagesError::Provider { error, failed, .. } = partial.error else {
        panic!("expected provider failure")
    };
    assert_eq!(failed, Resume::At("query/cursor+two".into()));
    let Error::Provider { error, meta } = error else {
        panic!("expected typed rate limit")
    };
    assert_eq!(error.code, ErrorCode::RateLimited);
    assert_eq!(meta.headers()["retry-after"], "5");
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn cap_is_distinct_from_exhaustion_and_preserves_a_resume_key() {
    let script = Script::new([reply(CHILDREN1), reply(CHILDREN2)]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let mut pages = api
        .children("page", PageSize::default())
        .max_pages(NonZeroUsize::new(1).unwrap());
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.meta.operation().unwrap().method(), Some("GET"));
    let bounded = pages.next_page().await.unwrap_err();
    assert!(
        matches!(&bounded, PagesError::Bounded { resume: Resume::At(cursor), .. } if cursor == "children/cursor+two")
    );
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    let remaining = api
        .children("page", PageSize::default())
        .starting_at(bounded.resume().unwrap().clone())
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(remaining.len(), 4);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].query_all("start_cursor"),
        ["children/cursor+two"]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn unfamiliar_query_status_is_retained_and_never_declared_complete() {
    let mut value: Value = serde_json::from_slice(QUERY2).unwrap();
    value["request_status"]["type"] = json!("future_pending_state");
    let bytes = serde_json::to_vec(&value).unwrap();
    let server = TestServer::start(move |_| reply(&bytes)).await;
    let failure = client(&server)
        .query_pages("source", query())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(failure.collected.is_empty());
    let PagesError::Provider {
        error: Error::IncompleteQuery { response },
        ..
    } = failure.error
    else {
        panic!("unknown status must not imply completion")
    };
    assert_eq!(
        response.value.request_status.unwrap().kind,
        "future_pending_state"
    );
    assert_eq!(response.value.results.len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn empty_page_with_cursor_continues_and_a_final_page_at_cap_exhausts() {
    let script = Script::new([
        reply(include_bytes!("../fixtures/empty-more.json")),
        reply(LAST),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let mut pages = client(&server)
        .children("page", PageSize::default())
        .max_pages(NonZeroUsize::new(2).unwrap());
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    assert!(pages.next_page().await.unwrap().is_none());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].query_all("start_cursor"), ["after-empty"]);
    server.shutdown().await;
}

#[tokio::test]
async fn provider_query_cap_does_not_masquerade_as_exhaustion() {
    let script = Script::new([
        reply(QUERY1),
        reply(include_bytes!("../fixtures/query-incomplete.json")),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let partial = client(&server)
        .query_pages("source", query())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(partial.collected.len(), 1);
    let PagesError::Provider {
        error: Error::IncompleteQuery { response },
        ..
    } = partial.error
    else {
        panic!("expected incomplete query")
    };
    assert_eq!(
        response.value.results[0].as_page().unwrap().id,
        "capped-page"
    );
    let status = response.value.request_status.unwrap();
    assert_eq!(status.kind, "incomplete");
    assert_eq!(
        status.incomplete_reason.as_deref(),
        Some("query_result_limit_reached")
    );
    assert_eq!(response.meta.status().as_u16(), 200);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn contradictory_cursors_are_errors_and_never_silently_end() {
    let mut one: Value = serde_json::from_slice(LAST).unwrap();
    one["has_more"] = json!(true);
    let mut two = one.clone();
    two["next_cursor"] = json!("");
    let mut three = one.clone();
    three["has_more"] = json!(false);
    three["next_cursor"] = json!("unexpected");
    let script = Script::new(
        [one, two, three]
            .iter()
            .map(|v| reply(&serde_json::to_vec(v).unwrap())),
    );
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    for expected in [
        EnvelopeProblem::CursorMismatch,
        EnvelopeProblem::EmptyCursor,
        EnvelopeProblem::CursorMismatch,
    ] {
        let mut pages = api.children("page", PageSize::default());
        let error = pages.next_page().await.unwrap_err();
        let PagesError::Provider {
            error: Error::Envelope { problem, meta },
            ..
        } = error
        else {
            panic!("expected pagination failure")
        };
        assert_eq!(problem, expected);
        assert_eq!(meta.status().as_u16(), 200);
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended { .. })
        ));
    }
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn repeated_and_cyclic_cursors_stop_without_duplicate_requests() {
    // A single-page self-loop is rejected with response metadata.
    let server = TestServer::start(|_| reply(include_bytes!("../fixtures/empty-more.json"))).await;
    let partial = client(&server)
        .children("page", PageSize::default())
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(partial.collected.len(), 1);
    assert!(matches!(
        partial.error,
        PagesError::Provider {
            error: Error::Envelope {
                problem: EnvelopeProblem::CursorDidNotAdvance,
                ..
            },
            ..
        }
    ));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
    // An A -> B -> A cycle is caught before requesting A a second time.
    let script = Script::new(["A", "B", "A"].map(|cursor| {
        reply(
            &serde_json::to_vec(&json!({
                "object":"list", "results":[], "has_more":true, "next_cursor":cursor
            }))
            .unwrap(),
        )
    }));
    let server = TestServer::start(move |_| script.next()).await;
    let partial = client(&server)
        .children("page", PageSize::default())
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(partial.collected.len(), 3);
    assert_eq!(
        (
            partial.error.progress().admitted,
            partial.error.progress().failed
        ),
        (3, 0)
    );
    assert!(matches!(partial.error, PagesError::Repeated { .. }));
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn item_stream_surfaces_failure_before_ending() {
    let script = Script::new([
        reply(CHILDREN1),
        Reply::new(404)
            .header("Content-Type", "application/json")
            .body(include_bytes!("../fixtures/error-404.json")),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let mut stream = client(&server)
        .children("page", PageSize::default())
        .items()
        .into_stream();
    assert_eq!(stream.next().await.unwrap().unwrap().id, "heading");
    assert_eq!(
        stream.next().await.unwrap().unwrap().id,
        "paragraph-with-children"
    );
    assert!(matches!(
        stream.next().await,
        Some(Err(PagesError::Provider {
            error: Error::Provider { .. },
            ..
        }))
    ));
    assert!(stream.next().await.is_none());
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
