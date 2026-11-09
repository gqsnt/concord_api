//! Independent application consumer: predictions and evidence are in FIRST_USE.md.

use std::num::NonZeroUsize;
use std::time::Duration;

use concord::ErrorKind;
use concord_example_testserver::{Body, Recorded, Reply, Script, TestServer};
use concord_pagination::{PagesError, Resume};
use concord_sdk_gitlab::{
    ContextError, ErrorBody, GitLab, GitLabError, IssueIid, IssueOrder, IssueState, IssueStatus,
    Labels, PaginationProblem, PerPage, PrivateToken, ProjectRef, SortOrder,
};

const TOKEN: &str = "synthetic-first-use-token";
const PROJECT: &str = "demo/platform/reader";
const COLLECTION: &str = "/api/v4/projects/demo%2Fplatform%2Freader/issues";
const ISSUE_TARGET: &str = "/api/v4/projects/demo%2Fplatform%2Freader/issues/31";
const ISSUE_31: &[u8] = include_bytes!("../fixtures/first-use/issue-31.json");
const ISSUE_32: &[u8] = include_bytes!("../fixtures/first-use/issue-32.json");
const PROJECT_73: &[u8] = include_bytes!("../fixtures/first-use/project-73.json");
const EMPTY: &[u8] = include_bytes!("../fixtures/first-use/empty.json");
const NOT_FOUND: &[u8] = include_bytes!("../fixtures/first-use/not-found.json");
const FORBIDDEN: &[u8] = include_bytes!("../fixtures/first-use/forbidden.json");
const RATE_LIMITED: &[u8] = include_bytes!("../fixtures/first-use/rate-limited.json");
const PROXY_ERROR: &[u8] = include_bytes!("../fixtures/first-use/proxy-error.html");

fn client(server: &TestServer) -> GitLab {
    let http = concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord_sdk_gitlab::redirect_policy())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    concord::BaseUrl::parse((format!("{}/api/v4", server.url())).as_ref())
        .map(|origin| GitLab::at(http, origin, PrivateToken::new(TOKEN).unwrap()))
        .unwrap()
}

async fn fake() -> (TestServer, Script, GitLab) {
    let script = Script::default();
    let replies = script.clone();
    let server = TestServer::start(move |_| replies.next()).await;
    let gitlab = client(&server);
    (server, script, gitlab)
}

fn listing(gitlab: &GitLab) -> concord_sdk_gitlab::api::GitLabProjectIssues {
    gitlab.project(ProjectRef::path(PROJECT).unwrap()).issues()
}

fn one_issue(issue: &[u8]) -> Reply {
    let mut array = Vec::from(b"[".as_slice());
    array.extend_from_slice(issue);
    array.push(b']');
    Reply::json(200, array)
}

fn next(server: &TestServer, target: &str) -> String {
    format!("<{}{target}>; rel=\"next\"", server.url())
}

fn assert_get(request: &Recorded, target: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, target);
    assert_eq!(request.header_all("private-token"), [TOKEN]);
    assert_eq!(request.header_all("accept"), ["application/json"]);
    assert!(request.body.is_empty());
    assert_eq!(request.body_len, 0);
}

fn assert_requests(server: &TestServer, targets: &[&str]) {
    let requests = server.requests();
    assert_eq!(requests.len(), targets.len());
    for (request, target) in requests.iter().zip(targets) {
        assert_get(request, target);
    }
}

#[tokio::test]
async fn a_pages_use_filters_and_exact_links_until_explicit_completion() {
    let (server, script, gitlab) = fake().await;
    let second = format!("{COLLECTION}?per_page=1&state=opened&labels=bug%2Chelp%20wanted&page=2");
    let third = format!("{COLLECTION}?page=3&per_page=1&search=export%20%2B%20caf%C3%A9");
    script.push(
        one_issue(ISSUE_31)
            .header("link", next(&server, &second))
            .header("x-request-id", "first-use-page-1")
            .header("x-page", "1")
            .header("x-per-page", "2")
            .header("x-next-page", "2"),
    );
    script.push(
        Reply::json(200, EMPTY)
            .header("link", next(&server, &third))
            .header("x-page", "2")
            .header("x-next-page", "3"),
    );
    script.push(
        one_issue(ISSUE_32)
            .header("link", format!("<{}{second}>; rel=\"prev\"", server.url()))
            .header("x-page", "3")
            .header("x-prev-page", "2")
            .header("x-next-page", ""),
    );
    let mut pages = listing(&gitlab)
        .set_state(IssueState::Opened)
        .set_labels(Labels::all_of(["bug", "help wanted"]).unwrap())
        .set_milestone("Reader 1.0")
        .set_search("export + café")
        .set_author_username("fixture-reader")
        .set_order_by(IssueOrder::UpdatedAt)
        .set_sort(SortOrder::Asc)
        .set_per_page(PerPage::new(2).unwrap())
        .pages();
    assert!(server.requests().is_empty(), "construction is lazy");

    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].iid.get(), 31);
    assert_eq!(first.items[0].title, "Keep exports readable — café");
    assert!(matches!(first.items[0].state, IssueStatus::Opened));
    assert_eq!(first.meta.request_id(), Some("first-use-page-1"));
    let meta = first.meta.pagination();
    assert_eq!(meta.page, Some(1));
    assert_eq!(meta.per_page, Some(2));
    assert_eq!(meta.next_page, Some(2));
    assert_eq!(meta.total, None);
    assert_eq!(meta.total_pages, None);
    let middle = pages.next_page().await.unwrap().unwrap();
    assert!(middle.items.is_empty());
    assert_eq!(middle.meta.pagination().next_page, Some(3));
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(last.items[0].iid.get(), 32);
    assert_eq!(last.meta.pagination().prev_page, Some(2));
    assert_eq!(last.meta.pagination().next_page, None);
    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.next_page().await.unwrap().is_none());

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].path(), COLLECTION);
    assert_get(&requests[0], &requests[0].target);
    let mut query = requests[0].query_pairs();
    query.sort();
    let mut expected: Vec<_> = [
        ("state", "opened"),
        ("labels", "bug,help wanted"),
        ("milestone", "Reader 1.0"),
        ("search", "export + café"),
        ("author_username", "fixture-reader"),
        ("order_by", "updated_at"),
        ("sort", "asc"),
        ("per_page", "2"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    expected.sort();
    assert_eq!(query, expected);
    assert_get(&requests[1], &second);
    assert_get(&requests[2], &third);
    assert_eq!(script.remaining(), 0);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_collect_complete_continues_after_an_empty_page() {
    let (server, script, gitlab) = fake().await;
    let second = format!("{COLLECTION}?page=2");
    script.push(Reply::json(200, EMPTY).header("link", next(&server, &second)));
    script.push(one_issue(ISSUE_32));
    let issues = listing(&gitlab).items().collect_complete().await.unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].iid.get(), 32);
    assert_requests(&server, &[COLLECTION, &second]);
    server.shutdown().await;
}

#[tokio::test]
async fn a_failed_page_preserves_prefix_document_metadata_and_retry_position() {
    let (server, script, gitlab) = fake().await;
    let second = format!("{COLLECTION}?page=2&per_page=1");
    script.push(one_issue(ISSUE_31).header("link", next(&server, &second)));
    script.push(
        Reply::json(429, RATE_LIMITED)
            .header("retry-after", "7")
            .header("x-request-id", "first-use-rate-limit"),
    );
    let listing = listing(&gitlab);
    let incomplete = listing
        .clone()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].iid.get(), 31);
    let resume = incomplete.error.resume().unwrap().clone();
    let PagesError::Provider { error, failed, .. } = incomplete.error else {
        panic!("the second page failed");
    };
    let Resume::At(position) = failed else {
        panic!("the failed page is not the start");
    };
    assert_eq!(
        position.raw_url().as_str(),
        format!("{}{second}", server.url())
    );
    let GitLabError::Api(api) = error else {
        panic!("provider status is preserved");
    };
    assert_eq!(api.status.as_u16(), 429);
    assert!(matches!(api.body, ErrorBody::Document(_)));
    assert_eq!(
        api.message().and_then(|m| m.as_text()),
        Some("429 Too Many Requests")
    );
    assert_eq!(api.meta.retry_after(), Some(7));
    assert_eq!(api.meta.request_id(), Some("first-use-rate-limit"));
    assert_eq!(api.operation.as_ref().unwrap().method(), Some("GET"));
    assert_eq!(server.requests().len(), 2, "no automatic retry");

    // Retry deliberately, using the SDK's returned continuation, without replaying page 1.
    script.push(one_issue(ISSUE_32));
    let rest = listing
        .pages()
        .starting_at(resume)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].iid.get(), 32);
    assert_requests(&server, &[COLLECTION, &second, &second]);
    server.shutdown().await;
}

#[tokio::test]
async fn a_page_cap_is_incomplete_and_can_resume() {
    let (server, script, gitlab) = fake().await;
    let second = format!("{COLLECTION}?page=2");
    script.push(one_issue(ISSUE_31).header("link", next(&server, &second)));
    let listing = listing(&gitlab);
    let mut pages = listing
        .clone()
        .pages()
        .max_pages(NonZeroUsize::new(1).unwrap());
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 1);
    let Err(PagesError::Bounded { resume, .. }) = pages.next_page().await else {
        panic!("a cap with a next link must not report completion");
    };
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 1);
    script.push(one_issue(ISSUE_32));
    let remaining = listing
        .pages()
        .starting_at(resume)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(remaining[0].iid.get(), 32);
    assert_requests(&server, &[COLLECTION, &second]);
    server.shutdown().await;
}

#[tokio::test]
async fn a_cap_exactly_at_last_page_is_success() {
    let (server, script, gitlab) = fake().await;
    script.push(Reply::json(200, EMPTY));
    let issues = listing(&gitlab)
        .pages()
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert!(issues.is_empty());
    assert_requests(&server, &[COLLECTION]);
    server.shutdown().await;
}

#[tokio::test]
async fn a_failed_item_reader_never_claims_successful_exhaustion() {
    let (server, script, gitlab) = fake().await;
    let second = format!("{COLLECTION}?page=2");
    script.push(one_issue(ISSUE_31).header("link", next(&server, &second)));
    script.push(Reply::json(403, FORBIDDEN));
    let mut items = listing(&gitlab).items();
    assert_eq!(items.next().await.unwrap().unwrap().iid.get(), 31);
    assert_eq!(server.requests().len(), 1, "no read-ahead");
    assert!(matches!(
        items.next().await,
        Err(PagesError::Provider { .. })
    ));
    assert!(matches!(items.next().await, Err(PagesError::Ended { .. })));
    assert_requests(&server, &[COLLECTION, &second]);
    server.shutdown().await;
}

#[tokio::test]
async fn a_truncated_second_page_is_not_a_successful_prefix_or_completion() {
    let (server, script, gitlab) = fake().await;
    let second = format!("{COLLECTION}?page=2");
    script.push(one_issue(ISSUE_31).header("link", next(&server, &second)));
    // Even a complete JSON value cannot make an unfinished HTTP body complete.
    script.push(
        Reply::json(200, EMPTY)
            .header("x-request-id", "first-use-cut-body")
            .with_body(Body::FailAfter(vec![EMPTY.into()])),
    );
    let incomplete = listing(&gitlab)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].iid.get(), 31);
    let PagesError::Provider { error, .. } = incomplete.error else {
        panic!("the unfinished body must fail");
    };
    assert_eq!(error.request_kind(), Some(ErrorKind::Transport));
    assert_eq!(
        error.meta().unwrap().request_id(),
        Some("first-use-cut-body")
    );
    assert_requests(&server, &[COLLECTION, &second]);
    server.shutdown().await;
}

#[tokio::test]
async fn a_bad_next_links_fail_instead_of_silently_ending_or_looping() {
    for case in ["missing", "malformed", "not-advancing", "other-collection"] {
        let (server, script, gitlab) = fake().await;
        let reply = one_issue(ISSUE_31).header("x-request-id", "first-use-bad-link");
        let reply = match case {
            "missing" => reply.header("x-next-page", "2"),
            "malformed" => reply.header("link", "not-a-link"),
            "not-advancing" => reply.header("link", next(&server, COLLECTION)),
            "other-collection" => reply.header("link", next(&server, "/api/v4/users?page=2")),
            _ => unreachable!(),
        };
        script.push(reply);
        let mut pages = listing(&gitlab).pages();
        let Err(PagesError::Provider {
            error: GitLabError::Pagination { problem, meta },
            ..
        }) = pages.next_page().await
        else {
            panic!("{case} is an explicit pagination failure");
        };
        assert!(match case {
            "missing" => matches!(problem, PaginationProblem::MissingNextLink),
            "malformed" => matches!(problem, PaginationProblem::MalformedLink(_)),
            "not-advancing" => matches!(problem, PaginationProblem::NotAdvancing),
            "other-collection" => matches!(problem, PaginationProblem::OtherCollection),
            _ => false,
        });
        assert_eq!(meta.unwrap().request_id(), Some("first-use-bad-link"));
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended { .. })
        ));
        assert_requests(&server, &[COLLECTION]);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn a_foreign_link_is_not_requested() {
    let foreign = TestServer::start(|_| Reply::json(200, EMPTY)).await;
    let (server, script, gitlab) = fake().await;
    script.push(one_issue(ISSUE_31).header("link", next(&foreign, COLLECTION)));
    let error = listing(&gitlab)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(matches!(
        error.error,
        PagesError::Provider {
            error: GitLabError::Pagination {
                problem: PaginationProblem::ForeignLink,
                ..
            },
            ..
        }
    ));
    assert_requests(&server, &[COLLECTION]);
    assert!(foreign.requests().is_empty());
    server.shutdown().await;
    foreign.shutdown().await;
}

#[tokio::test]
async fn a_continuation_from_another_project_is_refused_without_io() {
    let (server, script, gitlab) = fake().await;
    let second = format!("{COLLECTION}?page=2");
    script.push(one_issue(ISSUE_31).header("link", next(&server, &second)));
    let mut pages = listing(&gitlab)
        .pages()
        .max_pages(NonZeroUsize::new(1).unwrap());
    pages.next_page().await.unwrap().unwrap();
    let error = pages.next_page().await.unwrap_err();
    let resume = error.resume().unwrap().clone();
    let incomplete = gitlab
        .project(ProjectRef::id(74).unwrap())
        .issues()
        .pages()
        .starting_at(resume)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(matches!(
        incomplete.error,
        PagesError::Provider {
            error: GitLabError::Pagination {
                problem: PaginationProblem::ContinuationMismatch,
                meta: None,
            },
            ..
        }
    ));
    assert_requests(&server, &[COLLECTION]);
    server.shutdown().await;
}

#[tokio::test]
async fn b_issue_context_uses_iid_then_the_reported_project_id() {
    let (server, script, gitlab) = fake().await;
    script.push(Reply::json(200, ISSUE_31).header("x-request-id", "first-use-issue"));
    script.push(Reply::json(200, PROJECT_73).header("x-request-id", "first-use-project"));
    let context = gitlab
        .issue_context(
            ProjectRef::path(PROJECT).unwrap(),
            IssueIid::new(31).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(context.issue.value.id.get(), 900031);
    assert_eq!(context.issue.value.iid.get(), 31);
    assert_eq!(context.issue.value.project_id.get(), 73);
    assert_eq!(context.issue.value.title, "Keep exports readable — café");
    assert_eq!(context.issue.value.description, None);
    assert_eq!(context.issue.value.closed_at, None);
    assert_eq!(context.issue.value.author.username, "fixture-reader");
    assert_eq!(context.issue.value.labels, ["bug", "help wanted"]);
    assert_eq!(context.issue.meta.request_id(), Some("first-use-issue"));
    assert_eq!(context.project.value.id, context.issue.value.project_id);
    assert_eq!(context.project.value.path_with_namespace, PROJECT);
    assert_eq!(context.project.value.default_branch, None);
    assert_eq!(context.project.value.namespace.full_path, "demo/platform");
    assert_eq!(context.project.meta.request_id(), Some("first-use-project"));
    assert_requests(&server, &[ISSUE_TARGET, "/api/v4/projects/73"]);
    server.shutdown().await;
}

#[tokio::test]
async fn b_missing_issue_keeps_provider_document_and_skips_project_request() {
    let (server, script, gitlab) = fake().await;
    script.push(Reply::json(404, NOT_FOUND).header("x-request-id", "first-use-missing"));
    let Err(ContextError::Issue(error)) = gitlab
        .issue_context(
            ProjectRef::path(PROJECT).unwrap(),
            IssueIid::new(31).unwrap(),
        )
        .await
    else {
        panic!("a missing issue cannot have project context");
    };
    assert!(error.is_not_found());
    let GitLabError::Api(api) = error else {
        panic!("expected a provider error document");
    };
    assert_eq!(api.status.as_u16(), 404);
    assert_eq!(
        api.message().and_then(|m| m.as_text()),
        Some("404 Not Found")
    );
    assert_eq!(api.meta.request_id(), Some("first-use-missing"));
    assert_requests(&server, &[ISSUE_TARGET]);
    server.shutdown().await;
}

#[tokio::test]
async fn b_project_failure_preserves_the_retrieved_issue() {
    let (server, script, gitlab) = fake().await;
    script.push(Reply::json(200, ISSUE_31).header("x-request-id", "first-use-saved-issue"));
    script.push(Reply::json(403, FORBIDDEN).header("x-request-id", "first-use-forbidden-project"));
    let Err(ContextError::Project { issue, error }) = gitlab
        .issue_context(
            ProjectRef::path(PROJECT).unwrap(),
            IssueIid::new(31).unwrap(),
        )
        .await
    else {
        panic!("project failure must preserve the issue");
    };
    assert_eq!(issue.value.iid.get(), 31);
    assert_eq!(issue.meta.request_id(), Some("first-use-saved-issue"));
    let GitLabError::Api(api) = error else {
        panic!("expected a provider error document");
    };
    assert_eq!(api.status.as_u16(), 403);
    assert_eq!(
        api.message().and_then(|m| m.as_text()),
        Some("403 Forbidden")
    );
    assert_eq!(api.meta.request_id(), Some("first-use-forbidden-project"));
    assert_requests(&server, &[ISSUE_TARGET, "/api/v4/projects/73"]);
    server.shutdown().await;
}

#[tokio::test]
async fn b_project_mismatch_keeps_both_responses_for_diagnosis() {
    let (server, script, gitlab) = fake().await;
    script.push(Reply::json(200, ISSUE_31));
    let mut wrong_project: serde_json::Value = serde_json::from_slice(PROJECT_73).unwrap();
    wrong_project["id"] = 74.into();
    script.push(Reply::json(
        200,
        serde_json::to_vec(&wrong_project).unwrap(),
    ));
    let Err(ContextError::Mismatch { issue, project }) = gitlab
        .issue_context(
            ProjectRef::path(PROJECT).unwrap(),
            IssueIid::new(31).unwrap(),
        )
        .await
    else {
        panic!("another project is not valid context");
    };
    assert_eq!(issue.value.iid.get(), 31);
    assert_eq!(issue.value.project_id.get(), 73);
    assert_eq!(project.value.id.get(), 74);
    assert_requests(&server, &[ISSUE_TARGET, "/api/v4/projects/73"]);
    server.shutdown().await;
}

#[tokio::test]
async fn b_single_calls_send_the_sdk_error_document_and_metadata() {
    // Between waves 2-3 (coordinator integration): the client declares `send SendPolicy;`, so
    // the generated `send()` is the SDK path (formerly `Call::call()`). The Concord-only path
    // this test compared it with is now `prepare()?.send()`; expected values are unchanged.
    let (server, script, gitlab) = fake().await;
    script.push(Reply::json(404, NOT_FOUND).header("x-request-id", "first-use-send"));
    script.push(Reply::json(404, NOT_FOUND).header("x-request-id", "first-use-call"));
    let issue = gitlab
        .project(ProjectRef::path(PROJECT).unwrap())
        .issue(IssueIid::new(31).unwrap());

    let error: concord::Error<concord_sdk_gitlab::ErrorDocument> =
        issue.clone().prepare().unwrap().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status().unwrap().as_u16(), 404);
    let GitLabError::Api(api) = issue.send().await.unwrap_err() else {
        panic!("send() adds the SDK's provider error document and metadata");
    };
    assert_eq!(api.status.as_u16(), 404);
    assert_eq!(
        api.message().and_then(|m| m.as_text()),
        Some("404 Not Found")
    );
    assert_eq!(api.meta.request_id(), Some("first-use-call"));
    assert_requests(&server, &[ISSUE_TARGET, ISSUE_TARGET]);
    server.shutdown().await;
}

#[tokio::test]
async fn b_non_gitlab_proxy_failure_keeps_exact_error_bytes() {
    let (server, script, gitlab) = fake().await;
    script.push(Reply::text(502, "text/html", PROXY_ERROR));
    let error = gitlab
        .project(ProjectRef::path(PROJECT).unwrap())
        .issue(IssueIid::new(31).unwrap())
        .send()
        .await
        .unwrap_err();
    let GitLabError::Api(api) = error else {
        panic!("proxy status must be kept");
    };
    assert_eq!(api.status.as_u16(), 502);
    assert!(api.message().is_none());
    let ErrorBody::Unrecognized(bytes) = api.body else {
        panic!("HTML is not a GitLab error document");
    };
    assert_eq!(bytes.as_ref(), PROXY_ERROR);
    assert_requests(&server, &[ISSUE_TARGET]);
    server.shutdown().await;
}

#[tokio::test]
async fn b_bad_success_representation_has_typed_stage_and_response_metadata() {
    for stage in [ErrorKind::Metadata, ErrorKind::Decode] {
        let (server, script, gitlab) = fake().await;
        let reply = if stage == ErrorKind::Metadata {
            Reply::text(200, "text/html", ISSUE_31)
        } else {
            let mut missing_title: serde_json::Value = serde_json::from_slice(ISSUE_31).unwrap();
            missing_title.as_object_mut().unwrap().remove("title");
            Reply::json(200, serde_json::to_vec(&missing_title).unwrap())
        };
        script.push(reply.header("x-request-id", "first-use-bad-success"));
        let Err(ContextError::Issue(error)) = gitlab
            .issue_context(
                ProjectRef::path(PROJECT).unwrap(),
                IssueIid::new(31).unwrap(),
            )
            .await
        else {
            panic!("an invalid issue representation cannot yield context");
        };
        assert_eq!(error.request_kind(), Some(stage));
        assert_eq!(
            error.meta().unwrap().request_id(),
            Some("first-use-bad-success")
        );
        assert_requests(&server, &[ISSUE_TARGET]);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn b_documented_redirect_policy_does_not_forward_token_to_another_origin() {
    let foreign = TestServer::start(|_| Reply::json(200, ISSUE_31)).await;
    let (server, script, gitlab) = fake().await;
    script.push(Reply::new(302).header("location", format!("{}{ISSUE_TARGET}", foreign.url())));
    let Err(ContextError::Issue(error)) = gitlab
        .issue_context(
            ProjectRef::path(PROJECT).unwrap(),
            IssueIid::new(31).unwrap(),
        )
        .await
    else {
        panic!("a foreign redirect must stop the workflow");
    };
    assert_eq!(error.status().unwrap().as_u16(), 302);
    assert_requests(&server, &[ISSUE_TARGET]);
    assert!(foreign.requests().is_empty());
    server.shutdown().await;
    foreign.shutdown().await;
}
