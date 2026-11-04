//! The Link-header page factory on the wire: following `rel="next"` exactly as sent, empty
//! pages, absent `last`, foreign and malformed links, continuation binding, envelope projection.
//!
//! These tests drive [`LinkPages::fetch`] directly to pin the factory's own contract; the
//! workflows in `concord-example-github-app` drive it through `concord-pagination`.

mod support;

use concord::reqwest::{self, Url};
use concord_example_github_sdk::ids::{IssueNumber, RunId};
use concord_example_github_sdk::models::IssueComment;
use concord_example_github_sdk::paging::{LinkPage, NextLink, operation};
use concord_example_github_sdk::{GitHubConfig, GitHubContext, GitHubError, RepoRef};
use concord_example_testserver::{Recorded, Reply, TestServer};
use support::*;

fn ids(page: &LinkPage<IssueComment>) -> Vec<u64> {
    page.items.iter().map(|c| c.id.get()).collect()
}

/// Three pages: page 1 links to a repository-id URL with a cursor (and no `last`), page 2 is
/// empty but links on, page 3 has no `next`. Each request goes exactly where the link pointed,
/// with the API headers; no page number is computed.
#[tokio::test]
async fn follows_next_links_as_sent() {
    let server = TestServer::start(move |r: &Recorded| {
        let host = r.host.clone();
        match r.target.as_str() {
            "/repos/octo-org/hello-world/issues/7/comments?per_page=2" => {
                gh_json(200, &comments_json(&[1, 2], 7, "octo-org/hello-world")).header(
                    "link",
                    format!("<http://{host}/repositories/42/issues/7/comments?per_page=2&after=Y3Vyc29yOjI%3D>; rel=\"next\""),
                )
            }
            "/repositories/42/issues/7/comments?per_page=2&after=Y3Vyc29yOjI%3D" => {
                gh_json(200, "[]").header(
                    "link",
                    // A relative reference, resolved against this response's URL.
                    "</repositories/42/issues/7/comments?per_page=2&after=Y3Vyc29yOjI%3D&page=3>; rel=\"next\", </repositories/42/issues/7/comments?per_page=2>; rel=\"first\"",
                )
            }
            "/repositories/42/issues/7/comments?per_page=2&after=Y3Vyc29yOjI%3D&page=3" => {
                gh_json(200, &comments_json(&[3], 7, "octo-org/hello-world")).header(
                    "link",
                    "</repositories/42/issues/7/comments?per_page=2>; rel=\"first\"",
                )
            }
            _ => gh_json(404, r#"{"message":"Not Found"}"#),
        }
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let per_page = concord_example_github_sdk::params::PerPage::new(2).unwrap();
    let pages = repo.issue_comment_pages(IssueNumber(7), |op| op.set_per_page(per_page));

    let first = pages.fetch(None).await.unwrap();
    assert_eq!(ids(&first), [1, 2]);
    assert!(
        first.meta.response.links().unwrap().last().is_none(),
        "absent last"
    );
    let second = pages.fetch(first.next.clone()).await.unwrap();
    assert!(second.items.is_empty(), "an empty page that links on");
    let third = pages.fetch(second.next.clone()).await.unwrap();
    assert_eq!(ids(&third), [3]);
    assert!(third.next.is_none(), "no next: the listing ended");
    assert_eq!(
        third.meta.response.request_id(),
        Some("0000:1111:2222:3333:4444")
    );

    let wire = server.requests();
    assert_eq!(wire.len(), 3);
    for request in &wire {
        assert_api_headers(request, TOKEN_A);
    }
    // Debug of a continuation names its binding, never its URL (it carries the cursor).
    let shown = format!("{:?} {:?}", first.next.as_ref().unwrap(), first);
    assert!(shown.contains("issues/list-comments") && shown.contains("Some(7)"));
    assert!(!shown.contains("Y3Vyc29y"));
    server.shutdown().await;
}

/// A `next` link to another host is not followed: the token would leave the API. The other host
/// receives nothing.
#[tokio::test]
async fn foreign_next_link_is_refused() {
    let elsewhere = TestServer::start(|_: &Recorded| gh_json(200, "[]")).await;
    let port = elsewhere.addr().port();
    let api = TestServer::start(move |_: &Recorded| {
        gh_json(200, &comments_json(&[1], 7, "octo-org/hello-world")).header(
            "link",
            format!("<http://evil.test:{port}/steal?page=2>; rel=\"next\""),
        )
    })
    .await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .resolve(API_HOST, api.addr())
        .resolve("evil.test", elsewhere.addr())
        .build()
        .unwrap();
    let github = GitHubContext::new(
        http,
        GitHubConfig {
            token: TOKEN_A.into(),
        },
    )
    .with_base_url(api_base(api.addr()))
    .unwrap();
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let pages = repo.issue_comment_pages(IssueNumber(7), |op| op);
    let error = pages.fetch(None).await.unwrap_err();
    assert!(
        matches!(error, GitHubError::ForeignLink { meta: Some(_) }),
        "{error:?}"
    );
    assert_hidden(
        &format!("{error} {error:?}"),
        &["evil.test", "steal", TOKEN_A],
    );
    assert_eq!(elsewhere.requests().len(), 0);
    assert_eq!(api.requests().len(), 1);
    // A persisted continuation is checked the same way.
    let restored = pages.restore(Url::parse("http://evil.test/steal?page=2").unwrap());
    assert!(matches!(
        restored,
        Err(GitHubError::ForeignLink { meta: None })
    ));
    elsewhere.shutdown().await;
    api.shutdown().await;
}

/// Under a base path (GitHub Enterprise Server's `/api/v3`), a link on the same host but outside
/// the base path is foreign too.
#[tokio::test]
async fn link_outside_the_base_path_is_refused() {
    let server = TestServer::start(|r: &Recorded| {
        let host = r.host.clone();
        gh_json(200, "[]").header(
            "link",
            format!("<http://{host}/admin/export?page=2>; rel=\"next\""),
        )
    })
    .await;
    let github = GitHubContext::new(
        http(server.addr(), None),
        GitHubConfig {
            token: TOKEN_A.into(),
        },
    )
    .with_base_url(format!("{}/api/v3", api_base(server.addr())))
    .unwrap();
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let error = repo
        .issue_comment_pages(IssueNumber(7), |op| op)
        .fetch(None)
        .await
        .unwrap_err();
    assert!(matches!(error, GitHubError::ForeignLink { .. }));
    assert_eq!(
        server.single().target,
        "/api/v3/repos/octo-org/hello-world/issues/7/comments"
    );
    server.shutdown().await;
}

/// A malformed `Link` header fails the page: it is never read as "no next page".
#[tokio::test]
async fn malformed_link_fails_the_page() {
    let server = TestServer::start(|_: &Recorded| {
        gh_json(200, &comments_json(&[1], 7, "octo-org/hello-world"))
            .header("link", "<https://api.github.test/r?page=2; rel=\"next\"")
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let error = repo
        .issue_comment_pages(IssueNumber(7), |op| op)
        .fetch(None)
        .await
        .unwrap_err();
    match &error {
        GitHubError::Link { meta, .. } => assert_eq!(meta.status().as_u16(), 200),
        other => panic!("expected a Link error, got {other:?}"),
    }
    server.shutdown().await;
}

/// A continuation belongs to its operation, repository and parent: the comments link of issue 7
/// cannot continue issue 8's comments, nor another repository's. Nothing is sent.
#[tokio::test]
async fn continuation_binding_is_checked() {
    let server = TestServer::start(|r: &Recorded| {
        let host = r.host.clone();
        gh_json(200, &comments_json(&[1], 7, "octo-org/hello-world")).header(
            "link",
            format!("<http://{host}/repositories/42/issues/7/comments?page=2>; rel=\"next\""),
        )
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let other_repo = github.repo(RepoRef::new("octo-org", "second-repo"));
    let next: NextLink = repo
        .issue_comment_pages(IssueNumber(7), |op| op)
        .fetch(None)
        .await
        .unwrap()
        .next
        .unwrap();
    assert_eq!(next.binding().operation(), operation::ISSUE_COMMENTS);

    let issue_8 = repo.issue_comment_pages(IssueNumber(8), |op| op);
    let error = issue_8.fetch(Some(next.clone())).await.unwrap_err();
    assert!(matches!(error, GitHubError::ContinuationMismatch { .. }));
    let elsewhere = other_repo.issue_comment_pages(IssueNumber(7), |op| op);
    assert!(matches!(
        elsewhere.fetch(Some(next.clone())).await,
        Err(GitHubError::ContinuationMismatch { .. })
    ));
    let jobs = repo.run_job_pages(RunId(7), |op| op);
    assert!(matches!(
        jobs.fetch(Some(next)).await,
        Err(GitHubError::ContinuationMismatch { .. })
    ));
    assert_eq!(
        server.requests().len(),
        1,
        "mismatched continuations send nothing"
    );
    server.shutdown().await;
}

/// Envelope listings: items are moved out of `jobs`/`artifacts`/`workflow_runs`, `total_count`
/// is reported, and an envelope whose `total_count` is below its own page is refused.
#[tokio::test]
async fn envelopes_are_projected_and_checked() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path().ends_with("/jobs") {
            gh_json(200, official!("job-paginated.json"))
        } else {
            gh_json(
                200,
                &official!("artifact-paginated.json").replacen(
                    "\"total_count\": 2",
                    "\"total_count\": 1",
                    1,
                ),
            )
        }
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let jobs = repo
        .run_job_pages(RunId(30433642), |op| op)
        .fetch(None)
        .await
        .unwrap();
    assert_eq!(jobs.items.len(), 1);
    assert_eq!(jobs.meta.total_count, Some(1));
    assert!(jobs.next.is_none());

    let error = repo
        .run_artifact_pages(RunId(30433642), |op| op)
        .fetch(None)
        .await
        .unwrap_err();
    assert!(matches!(error, GitHubError::Envelope { .. }));
    assert_eq!(
        error.to_string(),
        "GitHub page: total_count 1 is below the 2 items on the page"
    );
    server.shutdown().await;
}

/// A persisted continuation is restored against the same factory and fetches that page.
#[tokio::test]
async fn persisted_continuation_resumes() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query() == Some("page=2") {
            gh_json(200, &comments_json(&[9], 7, "octo-org/hello-world"))
        } else {
            Reply::new(500)
        }
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let pages = repo.issue_comment_pages(IssueNumber(7), |op| op);
    let stored = format!(
        "{}/repositories/42/issues/7/comments?page=2",
        api_base(server.addr())
    );
    let next = pages.restore(Url::parse(&stored).unwrap()).unwrap();
    let page = pages.fetch(Some(next)).await.unwrap();
    assert_eq!(ids(&page), [9]);
    server.shutdown().await;
}
