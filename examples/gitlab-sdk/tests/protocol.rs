//! Wire-level checks: request method, target, headers and body; the documented success and
//! failure shapes; the negative cases of the brief (encoded project path, private token, `Link`
//! pagination and retained headers).

mod support;

use concord::ErrorKind;
use concord::reqwest::Url;
use concord_example_testserver::{Reply, TestServer};
use concord_pagination::{PagesError, Resume};
use concord_sdk_gitlab::link::{self, LinkError};
use concord_sdk_gitlab::{
    ErrorBody, ErrorMessage, GitLab, GitLabError, InputError, IssueIid, IssueOrder, IssueState,
    Labels, PaginationProblem, PerPage, PrivateToken, ProjectRef, SortOrder,
};
use support::*;

fn widgets() -> ProjectRef {
    ProjectRef::path(PROJECT_PATH).unwrap()
}

fn offline() -> GitLab {
    GitLab::with_token(http(), PrivateToken::new(TOKEN).unwrap())
}

// --- Requests -------------------------------------------------------------------------------

#[tokio::test]
async fn the_issue_listing_on_the_wire() {
    let (server, gitlab) = scripted([offset_headers(json(200, PAGE_1), 1, 50, Some(2))]).await;
    let page = gitlab
        .project(widgets())
        .issues()
        .set_state(IssueState::Opened)
        .set_labels(Labels::all_of(["bug", "ui"]).unwrap())
        .set_milestone("v2.0")
        .set_search("café & ü")
        .set_author_username("fixture-author")
        .set_order_by(IssueOrder::UpdatedAt)
        .set_sort(SortOrder::Asc)
        .set_per_page(PerPage::new(50).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(page.value.len(), 2);
    assert_eq!(page.meta.pagination().total, Some(2));
    assert!(page.meta.links().unwrap().next().is_none());

    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.target,
        format!(
            "{ISSUES_PATH}?state=opened&labels=bug%2Cui&milestone=v2.0&search=caf%C3%A9+%26+%C3%BC\
             &author_username=fixture-author&order_by=updated_at&sort=asc&per_page=50"
        )
    );
    assert_eq!(request.header("private-token"), Some(TOKEN));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("authorization"), None);
    assert_eq!(request.header("content-type"), None);
    assert!(request.body.is_empty() && !request.chunked);
}

#[test]
fn the_project_path_is_one_encoded_segment() {
    let gitlab = offline();
    let url = |project: ProjectRef| {
        gitlab
            .project(project)
            .issue(IssueIid::new(7).unwrap())
            .prepare()
            .unwrap()
            .request()
            .url()
            .to_string()
    };
    assert_eq!(
        url(widgets()),
        "https://gitlab.com/api/v4/projects/acme%2Ftools%2Fwidgets/issues/7"
    );
    assert_eq!(
        url(ProjectRef::path("my.group/my-project_1").unwrap()),
        "https://gitlab.com/api/v4/projects/my.group%2Fmy-project_1/issues/7"
    );
    assert_eq!(
        url(ProjectRef::id(42).unwrap()),
        "https://gitlab.com/api/v4/projects/42/issues/7"
    );
}

#[test]
fn project_paths_that_would_be_wrong_on_the_wire_are_refused() {
    let cases = [
        ("", InputError::ProjectPathNotNamespaced),
        ("widgets", InputError::ProjectPathNotNamespaced),
        ("4021", InputError::ProjectPathNotNamespaced),
        ("/acme/widgets", InputError::ProjectPathSegment),
        ("acme/widgets/", InputError::ProjectPathSegment),
        ("acme//widgets", InputError::ProjectPathSegment),
        ("acme/../widgets", InputError::ProjectPathSegment),
        ("acme%2Fwidgets", InputError::ProjectPathPreEncoded),
        ("acme/wid gets", InputError::ProjectPathCharacter),
        ("acme/widgets\n", InputError::ProjectPathCharacter),
    ];
    for (path, expected) in cases {
        assert_eq!(ProjectRef::path(path), Err(expected), "{path:?}");
    }
    assert_eq!(ProjectRef::id(0), Err(InputError::ZeroId));
    assert_eq!(IssueIid::new(0), Err(InputError::ZeroId));
    assert_eq!(PerPage::new(0), Err(InputError::PerPage));
    assert_eq!(PerPage::new(101), Err(InputError::PerPage));
    assert_eq!(PerPage::new(100), Ok(PerPage::MAX));
    assert_eq!(Labels::all_of(["a,b"]), Err(InputError::Label));
    assert_eq!(Labels::all_of(Vec::<String>::new()), Err(InputError::Label));
    assert_eq!(Labels::any().to_string(), "Any");
    assert_eq!(Labels::none().to_string(), "None");
}

#[tokio::test]
async fn one_issue_and_one_project_on_the_wire() {
    let (server, gitlab) = scripted([json(200, ISSUE), json(200, PROJECT)]).await;
    let issue = gitlab
        .project(widgets())
        .issue(IssueIid::new(2).unwrap())
        .send()
        .await
        .unwrap()
        .into_value();
    let project = gitlab
        .project(ProjectRef::id(PROJECT_ID).unwrap())
        .get()
        .send()
        .await
        .unwrap()
        .into_value();

    assert_eq!((issue.id.get(), issue.iid.get()), (9002, 2));
    assert_eq!(issue.labels, ["bug"]);
    assert_eq!(issue.author.username, "fixture-dev");
    assert!(issue.milestone.is_none() && issue.closed_at.is_none());
    assert_eq!(
        issue.links.as_ref().and_then(|l| l.project.as_deref()),
        Some("https://gitlab.example.com/api/v4/projects/4021")
    );
    assert_eq!(project.name_with_namespace, "Acme / Tools / Widgets");
    assert_eq!(project.default_branch.as_deref(), Some("main"));
    assert_eq!(project.open_issues_count, Some(2));

    let requests = server.requests();
    assert_eq!(
        [requests[0].target.as_str(), requests[1].target.as_str()],
        [
            "/api/v4/projects/acme%2Ftools%2Fwidgets/issues/2",
            "/api/v4/projects/4021"
        ]
    );
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert_eq!(request.header("private-token"), Some(TOKEN));
        assert!(request.body.is_empty());
    }
}

#[test]
fn the_token_is_validated_flagged_and_never_printed() {
    for bad in [
        "",
        " leading",
        "trailing\n",
        "inner space",
        "tab\tinside",
        "café",
    ] {
        assert_eq!(
            PrivateToken::new(bad).unwrap_err(),
            InputError::Token,
            "{bad:?}"
        );
    }
    let token = PrivateToken::new(TOKEN).unwrap();
    assert_eq!(format!("{token:?}"), "PrivateToken(<redacted>)");

    let gitlab = offline();
    assert!(!format!("{gitlab:?}").contains(TOKEN));
    let prepared = gitlab.project(widgets()).issues().prepare().unwrap();
    assert!(prepared.request().headers()["private-token"].is_sensitive());
    let shown = format!("{prepared:?}");
    assert!(shown.contains("private-token: <redacted>") && !shown.contains(TOKEN));
    GitLab::validate_header_names().unwrap();
}

// --- Documented failure shapes --------------------------------------------------------------

#[tokio::test]
async fn documented_error_documents_are_kept_with_their_status() {
    let cases: [(u16, &[u8]); 3] = [(401, UNAUTHORIZED), (403, FORBIDDEN), (400, VALIDATION)];
    for (status, body) in cases {
        let (_server, gitlab) =
            scripted([json(status, body).header("x-request-id", "req-fixture-error")]).await;
        let error = gitlab
            .project(widgets())
            .issue(IssueIid::new(2).unwrap())
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status().map(|s| s.as_u16()), Some(status));
        assert_eq!(
            error.meta().and_then(|m| m.request_id()),
            Some("req-fixture-error")
        );
        let GitLabError::Api(api) = &error else {
            panic!("an API error");
        };
        let message = api.message().expect("a GitLab error document");
        match status {
            401 => assert_eq!(message.as_text(), Some("401 Unauthorized")),
            403 => assert_eq!(message.as_text(), Some("403 Forbidden")),
            _ => {
                let ErrorMessage::Fields(fields) = message else {
                    panic!("a validation object");
                };
                assert_eq!(fields["bio"][0], "is too long (maximum is 255 characters)");
            }
        }
        let shown = format!("{error} {error:?}");
        assert!(
            !shown.contains(TOKEN) && !shown.contains("127.0.0.1"),
            "{shown}"
        );
    }
}

#[tokio::test]
async fn the_error_display_names_the_operation_and_status_only() {
    let (_server, gitlab) = scripted([json(401, UNAUTHORIZED)]).await;
    let error = gitlab
        .project(widgets())
        .issue(IssueIid::new(2).unwrap())
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "GET GitLab.project.issue: GitLab answered HTTP 401 Unauthorized with an error document"
    );
}

#[tokio::test]
async fn a_rate_limit_keeps_retry_after_and_the_unrecognized_body() {
    let (_server, gitlab) =
        scripted([Reply::text(429, "text/plain", RATE_LIMITED).header("retry-after", "30")]).await;
    let error = gitlab.project(widgets()).issues().send().await.unwrap_err();
    assert_eq!(error.meta().and_then(|m| m.retry_after()), Some(30));
    let GitLabError::Api(api) = error else {
        panic!("an API error");
    };
    let ErrorBody::Unrecognized(bytes) = &api.body else {
        panic!("not a GitLab document");
    };
    assert_eq!(bytes.as_ref(), RATE_LIMITED);
}

#[tokio::test]
async fn a_proxy_page_and_an_oversized_body_are_not_error_documents() {
    let (_server, gitlab) = scripted([
        Reply::text(502, "text/html", PROXY_ERROR),
        json(500, &vec![b' '; 64 * 1024 + 1]),
    ])
    .await;
    let proxy = gitlab.project(widgets()).issues().send().await.unwrap_err();
    let GitLabError::Api(api) = proxy else {
        panic!("an API error");
    };
    assert_eq!(api.status.as_u16(), 502);
    assert!(matches!(api.body, ErrorBody::Unrecognized(_)) && api.message().is_none());

    let large = gitlab.project(widgets()).issues().send().await.unwrap_err();
    let GitLabError::Api(api) = large else {
        panic!("an API error");
    };
    assert!(matches!(api.body, ErrorBody::TooLarge { limit: 65536 }));
}

#[tokio::test]
async fn a_2xx_that_is_not_json_is_refused_before_its_body_is_read() {
    let (_server, gitlab) = scripted([Reply::text(200, "text/html", PROXY_ERROR)]).await;
    let error = gitlab.project(widgets()).issues().send().await.unwrap_err();
    assert_eq!(error.request_kind(), Some(ErrorKind::Metadata));
    assert_eq!(error.meta().map(|m| m.status().as_u16()), Some(200));
}

#[tokio::test]
async fn a_malformed_2xx_body_is_a_decode_error_with_its_metadata() {
    let (_server, gitlab) =
        scripted([json(200, MALFORMED).header("x-request-id", "req-fixture-malformed")]).await;
    let error = gitlab.project(widgets()).issues().send().await.unwrap_err();
    assert_eq!(error.request_kind(), Some(ErrorKind::Decode));
    assert_eq!(
        error.meta().and_then(|m| m.request_id()),
        Some("req-fixture-malformed")
    );
}

#[tokio::test]
async fn a_transport_failure_has_no_metadata_and_no_url() {
    let (_server, gitlab) = scripted([Reply::hang_up()]).await;
    let error = gitlab.project(widgets()).issues().send().await.unwrap_err();
    assert_eq!(error.request_kind(), Some(ErrorKind::Transport));
    assert!(error.meta().is_none());
    let shown = format!("{error} {error:?}");
    assert!(
        !shown.contains("127.0.0.1") && !shown.contains(TOKEN),
        "{shown}"
    );
}

// --- Pagination contract ----------------------------------------------------------------------

/// The first page of a listing whose reply carries `link` (and `x-next-page` when given), and
/// what the traversal returns for it.
async fn first_page_with(
    build: impl FnOnce(&str) -> Reply,
) -> (
    TestServer,
    Result<(), PagesError<GitLabError, concord_sdk_gitlab::NextPage>>,
) {
    let script = concord_example_testserver::Script::new([]);
    let responder = script.clone();
    let server = TestServer::start(move |_| responder.next()).await;
    script.push(build(&server.url()));
    let mut pages = client(&server).project(widgets()).issues().pages();
    let result = pages.next_page().await.map(|_| ());
    (server, result)
}

fn problem(
    result: Result<(), PagesError<GitLabError, concord_sdk_gitlab::NextPage>>,
) -> PaginationProblem {
    match result {
        Err(PagesError::Provider {
            error: GitLabError::Pagination { problem, .. },
            ..
        }) => problem,
        other => panic!("a pagination error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_malformed_link_is_an_error_not_the_last_page() {
    let (server, result) =
        first_page_with(|_| json(200, PAGE_1).header("link", "not-a-link")).await;
    assert_eq!(
        problem(result),
        PaginationProblem::MalformedLink(LinkError::ExpectedTarget { at: 0 })
    );
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_link_to_another_origin_is_never_requested() {
    let elsewhere = TestServer::start(|_| json(200, PAGE_2)).await;
    let foreign = format!("{}{ISSUES_PATH}?page=2", elsewhere.url());
    let (_server, result) =
        first_page_with(|_| json(200, PAGE_1).header("link", link(&[(&foreign, "next")]))).await;
    assert_eq!(problem(result), PaginationProblem::ForeignLink);
    assert!(
        elsewhere.requests().is_empty(),
        "the token never left the API origin"
    );
}

#[tokio::test]
async fn a_link_to_another_collection_is_refused() {
    for path in [
        format!("/api/v4/projects/{PROJECT_ID}/merge_requests?page=2"),
        format!("/api/v4/projects/{PROJECT_ID}/issues/2/notes?page=2"),
        "/elsewhere/projects/4021/issues?page=2".to_owned(),
        "/api/v4/groups/acme/issues?page=2".to_owned(),
    ] {
        let (server, result) = first_page_with(|url| {
            json(200, PAGE_1).header("link", link(&[(&format!("{url}{path}"), "next")]))
        })
        .await;
        assert_eq!(
            problem(result),
            PaginationProblem::OtherCollection,
            "{path}"
        );
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn a_link_that_does_not_advance_is_refused() {
    let (_server, result) = first_page_with(|url| {
        json(200, PAGE_1).header("link", link(&[(&format!("{url}{ISSUES_PATH}"), "next")]))
    })
    .await;
    assert_eq!(problem(result), PaginationProblem::NotAdvancing);
}

#[tokio::test]
async fn x_next_page_without_a_next_link_is_refused() {
    let (_server, result) =
        first_page_with(|_| offset_headers(json(200, PAGE_1), 1, 2, Some(3))).await;
    assert_eq!(problem(result), PaginationProblem::MissingNextLink);
}

/// `Link` is authoritative: an `x-next-page` that disagrees with it is not used, and a listing
/// over 10,000 issues (no `x-total`, `x-total-pages` or `rel="last"`) pages on.
#[tokio::test]
async fn the_link_wins_over_x_next_page_and_missing_totals() {
    let script = concord_example_testserver::Script::new([]);
    let responder = script.clone();
    let server = TestServer::start(move |_| responder.next()).await;
    let next = format!("{}{ISSUES_PATH}?page=2&per_page=2", server.url());
    script.push(
        // Written by hand: `offset_headers` would add its own (empty) `x-next-page` line.
        json(200, PAGE_1)
            .header("x-page", "1")
            .header("x-per-page", "2")
            .header("x-next-page", "5")
            .header("x-prev-page", "")
            .header("link", link(&[(&next, "next")])),
    );
    script.push(offset_headers(json(200, PAGE_2), 2, 2, None));
    let mut pages = client(&server).project(widgets()).issues().pages();
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(
        first.meta.pagination().next_page,
        Some(5),
        "kept, but not followed"
    );
    assert_eq!(first.meta.pagination().total, None);
    assert!(first.meta.links().unwrap().last().is_none());
    pages.next_page().await.unwrap().unwrap();
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(
        server.requests()[1].target,
        format!("{ISSUES_PATH}?page=2&per_page=2")
    );
}

#[tokio::test]
async fn a_continuation_of_another_listing_is_refused_before_sending() {
    let script = concord_example_testserver::Script::new([]);
    let responder = script.clone();
    let server = TestServer::start(move |_| responder.next()).await;
    let next = format!("{}{ISSUES_PATH}?page=2", server.url());
    script.push(json(200, PAGE_1).header("link", link(&[(&next, "next")])));
    let gitlab = client(&server);
    let page = gitlab
        .project(widgets())
        .issues()
        .pages()
        .next_page()
        .await
        .unwrap()
        .unwrap();
    let key = page.next.expect("a next page");

    let other = gitlab
        .project(ProjectRef::path("acme/gadgets").unwrap())
        .issues();
    let result = other
        .pages_from(Resume::At(key))
        .next_page()
        .await
        .map(|_| ());
    assert_eq!(problem(result), PaginationProblem::ContinuationMismatch);
    assert_eq!(
        server.requests().len(),
        1,
        "nothing sent for the refused continuation"
    );
}

#[tokio::test]
async fn page_metadata_never_prints_a_url() {
    let (_server, gitlab) = scripted([json(200, EMPTY).header(
        "link",
        "<http://127.0.0.1:1/api/v4/projects/1/issues?search=private-term&page=1>; rel=\"first\"",
    )])
    .await;
    let page = gitlab.project(widgets()).issues().send().await.unwrap();
    let shown = format!("{:?}", page.meta);
    assert!(
        !shown.contains("private-term") && !shown.contains("127.0.0.1"),
        "{shown}"
    );
    assert!(shown.contains("first: true"));
}

// --- The `Link` parser ------------------------------------------------------------------------

#[test]
fn link_values_are_read_by_rfc_8288() {
    let base = Url::parse("https://gitlab.example.com/api/v4/projects/8/issues?page=1").unwrap();
    // The documented form, plus a relative target, a quoted comma, several relation types,
    // case-insensitive names and an unknown relation.
    let links = link::parse_str(
        "<https://gitlab.example.com/api/v4/projects/8/issues?page=1&per_page=3>; rel=\"prev\", \
         </api/v4/projects/8/issues?page=3&per_page=3>; title=\"a, b\"; REL=\"Next Last\", \
         <https://gitlab.example.com/api/v4/projects/8/issues?page=1&per_page=3>; rel=first, \
         <https://elsewhere.example/>; rel=\"license\"",
        &base,
    )
    .unwrap();
    let page_3 = "https://gitlab.example.com/api/v4/projects/8/issues?page=3&per_page=3";
    assert_eq!(links.next().map(Url::as_str), Some(page_3));
    assert_eq!(links.last().map(Url::as_str), Some(page_3));
    assert!(links.prev().is_some() && links.first().is_some());

    assert!(link::parse_str("", &base).unwrap().next().is_none());
    let malformed = [
        ("https://x/; rel=next", LinkError::ExpectedTarget { at: 0 }),
        ("<https://x/; rel=next", LinkError::Unterminated { at: 0 }),
        (
            "<https://x/>; rel=\"next",
            LinkError::Unterminated { at: 18 },
        ),
        ("<https://x/> rel=next", LinkError::Parameter { at: 13 }),
        ("<https://x/>; =next", LinkError::Parameter { at: 14 }),
        (
            "<https://x/a>; rel=next, <https://x/b>; rel=next",
            LinkError::Conflict { rel: "next" },
        ),
        ("<http://[::1/>; rel=next", LinkError::Url),
    ];
    for (text, expected) in malformed {
        assert_eq!(
            link::parse_str(text, &base).unwrap_err(),
            expected,
            "{text}"
        );
    }
}

// --- Redirects and the token -----------------------------------------------------------------

#[tokio::test]
async fn a_same_origin_redirect_is_followed_and_a_foreign_one_is_not() {
    let elsewhere = TestServer::start(|_| json(200, ISSUE)).await;
    let foreign = format!("{}/api/v4/projects/4021/issues/2", elsewhere.url());
    let script = concord_example_testserver::Script::new([]);
    let responder = script.clone();
    let server = TestServer::start(move |_| responder.next()).await;
    // A moved project: GitLab redirects `GET` on the old path.
    let moved = format!("{}/api/v4/projects/4021/issues/2", server.url());
    script.push(Reply::new(301).header("location", moved));
    script.push(json(200, ISSUE));
    script.push(Reply::new(302).header("location", foreign));
    let gitlab = client(&server);
    let old = ProjectRef::path("acme/old-widgets").unwrap();

    let issue = gitlab
        .project(old.clone())
        .issue(IssueIid::new(2).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(issue.value.iid.get(), 2);
    let requests = server.requests();
    assert_eq!(requests[1].target, "/api/v4/projects/4021/issues/2");
    assert_eq!(requests[1].header("private-token"), Some(TOKEN));

    let error = gitlab
        .project(old)
        .issue(IssueIid::new(2).unwrap())
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(302));
    assert!(
        elsewhere.requests().is_empty(),
        "the token never followed the redirect"
    );
}

/// The control for [`concord_sdk_gitlab::redirect_policy`]: with reqwest's default policy a
/// cross-origin redirect is followed and `PRIVATE-TOKEN` goes with it.
#[tokio::test]
async fn control_the_default_redirect_policy_forwards_the_token() {
    let elsewhere = TestServer::start(|_| json(200, ISSUE)).await;
    let foreign = format!("{}/api/v4/projects/4021/issues/2", elsewhere.url());
    let server =
        TestServer::start(move |_| Reply::new(302).header("location", foreign.clone())).await;
    let http = concord::reqwest::Client::builder()
        .no_proxy()
        .retry(concord::reqwest::retry::never())
        .build()
        .unwrap();
    let gitlab = GitLab::with_token(http, PrivateToken::new(TOKEN).unwrap())
        .with_base_url(base(&server))
        .unwrap();
    gitlab
        .project(widgets())
        .issue(IssueIid::new(2).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(elsewhere.single().header("private-token"), Some(TOKEN));
}
