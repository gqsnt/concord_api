//! Workflows A and B end to end through the public API, against fixtures on a loopback server.

mod support;

use std::num::NonZeroUsize;

use concord_example_testserver::{Reply, TestServer};
use concord_pagination::{PagesError, Resume};
use concord_sdk_gitlab::{
    ContextError, GitLabError, IssueIid, IssueState, IssueStatus, PerPage, ProjectRef,
};
use support::*;

fn widgets() -> ProjectRef {
    ProjectRef::path(PROJECT_PATH).unwrap()
}

/// The two-page script of workflow A: page 1 (two issues) links to page 2 (one issue) in the
/// documented `Link` form, with the documented offset headers.
fn two_pages(server_url: &str) -> Vec<Reply> {
    let at = |query: &str| format!("{server_url}{ISSUES_PATH}?{query}");
    let page_1 = offset_headers(json(200, PAGE_1), 1, 2, Some(3)).header(
        "link",
        link(&[
            (&at("page=2&per_page=2&state=opened"), "next"),
            (&at("page=1&per_page=2&state=opened"), "first"),
            (&at("page=2&per_page=2&state=opened"), "last"),
        ]),
    );
    let page_2 = offset_headers(json(200, PAGE_2), 2, 2, Some(3)).header(
        "link",
        link(&[
            (&at("page=1&per_page=2&state=opened"), "prev"),
            (&at("page=1&per_page=2&state=opened"), "first"),
            (&at("page=2&per_page=2&state=opened"), "last"),
        ]),
    );
    vec![page_1, page_2]
}

/// A server whose replies are built once its URL is known (links are absolute).
async fn serve(build: impl FnOnce(&str) -> Vec<Reply>) -> TestServer {
    let script = concord_example_testserver::Script::new([]);
    let responder = script.clone();
    let server = TestServer::start(move |_| responder.next()).await;
    for reply in build(&server.url()) {
        script.push(reply);
    }
    server
}

#[tokio::test]
async fn a_pages_through_project_issues_by_link() {
    let server = serve(two_pages).await;
    let gitlab = client(&server);

    let mut pages = gitlab
        .project(widgets())
        .issues()
        .set_state(IssueState::Opened)
        .set_per_page(PerPage::new(2).unwrap())
        .pages();
    let first = pages.next_page().await.unwrap().expect("page 1");
    let second = pages.next_page().await.unwrap().expect("page 2");
    assert!(
        pages.next_page().await.unwrap().is_none(),
        "the end: no rel=next"
    );

    let iids: Vec<u64> = first
        .items
        .iter()
        .chain(&second.items)
        .map(|issue| issue.iid.get())
        .collect();
    assert_eq!(iids, [3, 2, 1]);
    assert_eq!(second.items[0].state, IssueStatus::Closed);

    // The retained headers of each page.
    let meta = &first.meta;
    assert_eq!(meta.request_id(), Some("req-fixture-page-1"));
    let pagination = meta.pagination();
    assert_eq!(
        (pagination.page, pagination.per_page, pagination.next_page),
        (Some(1), Some(2), Some(2))
    );
    assert_eq!(
        (pagination.total, pagination.total_pages),
        (Some(3), Some(2))
    );
    assert_eq!(
        pagination.prev_page, None,
        "empty x-prev-page on the first page"
    );
    assert!(second.next.is_none());
    assert_eq!(second.meta.pagination().next_page, None);
    assert!(second.meta.links().unwrap().prev().is_some());

    // Exactly two requests: the declared first page, then the link as sent.
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].target,
        format!("{ISSUES_PATH}?state=opened&per_page=2")
    );
    assert_eq!(
        requests[1].target,
        format!("{ISSUES_PATH}?page=2&per_page=2&state=opened")
    );
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert_eq!(request.header("private-token"), Some(TOKEN));
        assert_eq!(request.header("accept"), Some("application/json"));
        assert!(request.body.is_empty());
    }
}

#[tokio::test]
async fn a_collects_every_issue_and_stops_at_the_end() {
    let server = serve(two_pages).await;
    let gitlab = client(&server);
    let issues = gitlab
        .project(widgets())
        .issues()
        .set_state(IssueState::Opened)
        .set_per_page(PerPage::new(2).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(issues.len(), 3);
    assert_eq!(issues[1].title, "Crash when the name contains café & 🌍");
    assert_eq!(server.requests().len(), 2);
}

/// Perturbation: another first-page size, and a `next` link in another representation (the
/// project by ID, a cursor instead of a page number, another page size, extra and reordered
/// parameters, no `x-next-page`). The SDK requests exactly that link and builds nothing itself.
#[tokio::test]
async fn a_follows_the_returned_link_whatever_its_representation() {
    let server = serve(|url| {
        let next = format!(
            "{url}/api/v4/projects/{PROJECT_ID}/issues?cursor=synthetic-opaque-cursor&order_by=created_at&per_page=2&sort=desc&state=opened"
        );
        let page_1 = json(200, PAGE_2)
            .header("x-per-page", "1")
            .header("link", link(&[(&next, "next")]));
        let page_2 = json(200, PAGE_1).header("x-per-page", "2");
        vec![page_1, page_2]
    })
    .await;
    let gitlab = client(&server);

    let issues = gitlab
        .project(widgets())
        .issues()
        .set_state(IssueState::Opened)
        .set_per_page(PerPage::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(issues.len(), 3);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].target,
        format!("{ISSUES_PATH}?state=opened&per_page=1")
    );
    assert_eq!(
        requests[1].target,
        format!(
            "/api/v4/projects/{PROJECT_ID}/issues?cursor=synthetic-opaque-cursor&order_by=created_at&per_page=2&sort=desc&state=opened"
        )
    );
    assert_eq!(requests[1].header("private-token"), Some(TOKEN));
}

/// Partial outcome: page 2 fails. The collected prefix and the page to retry come back; resuming
/// from it requests that link again and completes the listing.
#[tokio::test]
async fn a_keeps_the_prefix_and_resumes_after_a_failed_page() {
    let server = serve(|url| {
        let mut replies = two_pages(url);
        let page_2 = replies.pop().unwrap();
        let failure = json(500, br#"{"message":"500 Internal Server Error"}"#)
            .header("x-request-id", "req-fixture-failure");
        vec![replies.pop().unwrap(), failure, page_2]
    })
    .await;
    let gitlab = client(&server);
    let listing = gitlab
        .project(widgets())
        .issues()
        .set_state(IssueState::Opened)
        .set_per_page(PerPage::new(2).unwrap());

    let incomplete = listing
        .clone()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2, "page 1 is kept");
    let PagesError::Provider { error, failed, .. } = incomplete.error else {
        panic!("a provider failure");
    };
    assert_eq!(error.status().map(|s| s.as_u16()), Some(500));
    assert_eq!(
        error.meta().and_then(|m| m.request_id()),
        Some("req-fixture-failure")
    );
    let Resume::At(retry) = failed else {
        panic!("the failed page is a linked page");
    };
    assert_eq!(
        retry.raw_url().as_str(),
        format!(
            "{}{ISSUES_PATH}?page=2&per_page=2&state=opened",
            server.url()
        )
    );

    let rest = listing
        .pages_from(Resume::At(retry))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].iid.get(), 1);
    let targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets[1], targets[2],
        "the retry requests the failed link again"
    );
}

/// A cap reached while GitLab links on is an error with the next page, not the end.
#[tokio::test]
async fn a_cap_is_not_the_end() {
    let server = serve(two_pages).await;
    let gitlab = client(&server);
    let mut pages = gitlab
        .project(widgets())
        .issues()
        .set_state(IssueState::Opened)
        .set_per_page(PerPage::new(2).unwrap())
        .pages()
        .max_pages(NonZeroUsize::new(1).unwrap());
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 2);
    let error = pages.next_page().await.unwrap_err();
    assert!(matches!(error, PagesError::Bounded { .. }));
    assert!(matches!(error.resume(), Some(Resume::At(_))));
    assert_eq!(server.requests().len(), 1);
}

/// An empty project: one empty page without `rel="next"` is a complete, empty listing.
#[tokio::test]
async fn a_an_empty_listing_is_complete() {
    let (server, gitlab) = scripted([offset_headers(json(200, EMPTY), 1, 20, Some(0))]).await;
    let issues = gitlab
        .project(widgets())
        .issues()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert!(issues.is_empty());
    assert_eq!(server.single().target, ISSUES_PATH);
}

#[tokio::test]
async fn b_retrieves_an_issue_and_its_project() {
    let (server, gitlab) = scripted([
        json(200, ISSUE).header("x-request-id", "req-fixture-issue"),
        json(200, PROJECT).header("x-request-id", "req-fixture-project"),
    ])
    .await;

    let context = gitlab
        .issue_context(widgets(), IssueIid::new(2).unwrap())
        .await
        .unwrap();
    assert_eq!(context.issue.value.iid.get(), 2);
    assert_eq!(context.issue.value.project_id.get(), PROJECT_ID);
    assert_eq!(
        context.issue.value.references.as_ref().unwrap().full,
        "acme/tools/widgets#2"
    );
    assert_eq!(context.project.value.id.get(), PROJECT_ID);
    assert_eq!(context.project.value.path_with_namespace, PROJECT_PATH);
    assert_eq!(context.project.value.namespace.full_path, "acme/tools");
    assert_eq!(context.issue.meta.request_id(), Some("req-fixture-issue"));
    assert_eq!(
        context.project.meta.request_id(),
        Some("req-fixture-project")
    );

    // The issue by encoded path and IID, then the project by the ID the issue reported.
    let targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets,
        [
            "/api/v4/projects/acme%2Ftools%2Fwidgets/issues/2".to_owned(),
            format!("/api/v4/projects/{PROJECT_ID}"),
        ]
    );
    for request in server.requests() {
        assert_eq!(request.header("private-token"), Some(TOKEN));
    }
}

#[tokio::test]
async fn b_a_missing_issue_requests_nothing_else() {
    let (server, gitlab) = scripted([json(404, ISSUE_NOT_FOUND)]).await;
    let error = gitlab
        .issue_context(widgets(), IssueIid::new(99).unwrap())
        .await
        .unwrap_err();
    let ContextError::Issue(error) = error else {
        panic!("the issue failed: {error:?}");
    };
    assert!(error.is_not_found());
    let GitLabError::Api(api) = &error else {
        panic!("an API error");
    };
    assert_eq!(
        api.message().and_then(|m| m.as_text()),
        Some("404 Issue Not Found")
    );
    assert_eq!(server.requests().len(), 1);
}

/// Partial outcome: the issue was retrieved and is kept when its project is not.
#[tokio::test]
async fn b_keeps_the_issue_when_the_project_fails() {
    let (_server, gitlab) = scripted([json(200, ISSUE), json(404, PROJECT_NOT_FOUND)]).await;
    let error = gitlab
        .issue_context(widgets(), IssueIid::new(2).unwrap())
        .await
        .unwrap_err();
    assert_eq!(error.issue().map(|issue| issue.value.iid.get()), Some(2));
    let ContextError::Project { error, .. } = error else {
        panic!("the project failed");
    };
    assert!(error.is_not_found());
}

/// GitLab answering with another project than the issue's is refused, with both kept.
#[tokio::test]
async fn b_refuses_another_project() {
    let (_server, gitlab) = scripted([json(200, ISSUE), json(200, PROJECT_MISMATCH)]).await;
    let error = gitlab
        .issue_context(widgets(), IssueIid::new(2).unwrap())
        .await
        .unwrap_err();
    let ContextError::Mismatch { issue, project } = &error else {
        panic!("a mismatch: {error:?}");
    };
    assert_eq!(issue.value.project_id.get(), PROJECT_ID);
    assert_eq!(project.value.id.get(), 4022);
    assert_eq!(
        error.to_string(),
        "issue retrieved, but GitLab returned project 4022 for project_id 4021"
    );
}
