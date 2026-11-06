//! End-to-end issue workflows.
use concord::reqwest;
use concord_example_testserver::{Reply, Script, TestServer};
use concord_pagination::{PagesError, Resume};
use concord_sdk_linear::Linear;
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
