//! End-to-end issue workflows.
use concord::reqwest;
use concord_example_testserver::{Reply, Script, TestServer};
use concord_pagination::{PagesError, Resume};
use concord_sdk_linear::Linear;
use serde_json::{Value, json};
use std::num::NonZeroUsize;

async fn setup(replies: Vec<Reply>) -> (TestServer, Linear) {
    let script = Script::new(replies);
    let server = TestServer::start(move |_| script.next()).await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .build()
        .unwrap();
    let client = Linear::api_key(http, "synthetic-value")
        .unwrap()
        .with_base_url(&server.url())
        .unwrap();
    (server, client)
}
#[tokio::test]
async fn page_issues_and_retain_partial_diagnostics() {
    let (server, client) = setup(vec![
        Reply::json(200, include_bytes!("../fixtures/page-one.json")),
        Reply::json(200, include_bytes!("../fixtures/page-partial.json")),
    ])
    .await;
    let mut pages = client.issues(1, Resume::Start);
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].id, "issue-one");
    assert!(first.meta.errors.is_empty());
    let second = pages.next_page().await.unwrap().unwrap();
    assert_eq!(second.items[0].id, "issue-two");
    assert_eq!(second.items[0].description, None);
    assert_eq!(
        second.meta.errors[0].path.as_ref().unwrap()[3],
        "description"
    );
    assert_eq!(second.meta.extensions.unwrap()["trace"], "synthetic");
    assert!(pages.next_page().await.unwrap().is_none());
    let requests = server.requests();
    assert!(
        std::str::from_utf8(&requests[0].body)
            .unwrap()
            .contains("\"after\":null")
    );
    assert!(
        std::str::from_utf8(&requests[1].body)
            .unwrap()
            .contains("\"after\":\"cursor-one\"")
    );
    server.shutdown().await;
}
#[tokio::test]
async fn fetch_issue_selected_fields() {
    let (server, client) = setup(vec![Reply::json(
        200,
        include_bytes!("../fixtures/issue.json"),
    )])
    .await;
    let answer = client.issue("ABC-1").await.unwrap();
    assert!(answer.is_success());
    let issue = answer.envelope.data.unwrap().issue.unwrap();
    assert_eq!(
        (
            issue.id.as_str(),
            issue.title.as_str(),
            issue.description.as_deref()
        ),
        ("issue-one", "First issue", Some("First description"))
    );
    assert_eq!(issue.due_date.as_deref(), Some("2026-12-01"));
    server.shutdown().await;
}
#[tokio::test]
async fn bounded_traversal_is_not_exhaustion() {
    let (server, client) = setup(vec![Reply::json(
        200,
        include_bytes!("../fixtures/page-one.json"),
    )])
    .await;
    let mut pages = client
        .issues(1, Resume::Start)
        .max_pages(NonZeroUsize::new(1).unwrap());
    assert!(pages.next_page().await.unwrap().is_some());
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Bounded { .. })
    ));
    server.shutdown().await;
}
#[tokio::test]
async fn failed_second_page_keeps_prefix_and_resume_cursor() {
    let (server, client) = setup(vec![
        Reply::json(200, include_bytes!("../fixtures/page-one.json")),
        Reply::json(200, include_bytes!("../fixtures/errors.json")),
    ])
    .await;
    let incomplete = client
        .issues(1, Resume::Start)
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].items[0].id, "issue-one");
    match incomplete.error {
        PagesError::Provider {
            error: concord_sdk_linear::Error::Page { response, .. },
            failed: Resume::At(cursor),
            ..
        } => {
            assert_eq!(cursor, "cursor-one");
            assert_eq!(
                response.envelope.errors[0].message,
                "Synthetic authorization failure"
            );
        }
        other => panic!("{other:?}"),
    }
    server.shutdown().await;
}
#[tokio::test]
async fn due_date_field_error_is_attributed_by_path_and_traversal_continues() {
    let (server, client) = setup(vec![
        Reply::json(
            200,
            include_bytes!("../fixtures/page-due-date-partial.json"),
        ),
        Reply::json(200, include_bytes!("../fixtures/page-partial.json")),
    ])
    .await;
    let pages = client
        .issues(3, Resume::Start)
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(pages.len(), 2);
    let first = &pages[0];
    let due: Vec<_> = first.items.iter().map(|i| i.due_date.as_deref()).collect();
    assert_eq!(due, [Some("2026-12-24"), None, None]);
    // A partial page is still a page: its opaque cursor is followed unchanged.
    assert_eq!(first.next.as_deref(), Some("due/cursor+=?&"));
    assert_eq!(first.meta.errors.len(), 1);
    let error = &first.meta.errors[0];
    assert_eq!(error.message, "Synthetic due date unavailable");
    assert_eq!(
        error.path,
        Some(vec![
            json!("issues"),
            json!("nodes"),
            json!(2),
            json!("dueDate")
        ])
    );
    assert_eq!(error.locations, Some(vec![json!({"line":1,"column":114})]));
    assert_eq!(
        error.extensions.as_ref().unwrap()["code"],
        "SYNTHETIC_DUE_DATE_FAILURE"
    );
    assert_eq!(error.extra["syntheticDetail"], "retained");
    assert_eq!(
        first.meta.extensions.as_ref().unwrap()["trace"],
        "synthetic-due-date"
    );
    // Two due dates are null; only the retained path says which one failed.
    let failed: Vec<_> = first
        .meta
        .errors
        .iter()
        .filter_map(|e| match e.path.as_deref()? {
            [_, _, Value::Number(index), field] if field == "dueDate" => index.as_u64(),
            _ => None,
        })
        .collect();
    assert_eq!(failed, [2]);
    // The next page keeps its own diagnostics; nothing is merged or dropped.
    let second = &pages[1];
    assert_eq!(second.items[0].due_date.as_deref(), Some("2027-01-15"));
    assert_eq!(second.meta.errors.len(), 1);
    assert_eq!(
        second.meta.errors[0].path.as_ref().unwrap()[3],
        "description"
    );
    assert_eq!(second.next, None);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let bodies: Vec<Value> = requests
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect();
    assert_eq!(bodies[0]["variables"], json!({"first":3,"after":null}));
    assert_eq!(
        bodies[1]["variables"],
        json!({"first":3,"after":"due/cursor+=?&"})
    );
    // The fixture's location (line 1, column 114) points at `dueDate` in the sent document.
    for body in &bodies {
        assert_eq!(&body["query"].as_str().unwrap()[113..120], "dueDate");
    }
    server.shutdown().await;
}
