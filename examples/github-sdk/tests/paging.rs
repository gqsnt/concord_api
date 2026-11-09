//! The Link-header page factory on the wire: following `rel="next"` exactly as sent, empty
//! pages, absent `last`, foreign and malformed links, continuation binding, envelope projection.
//!
//! These tests drive [`LinkPages::fetch`] directly to pin the factory's own contract; the
//! workflows in `concord-example-github-app` drive it through `concord-pagination`. The last
//! tests pin the ordinary traversals (`items()`, `pages()`): typed end, opt-in cap, kept prefix.

mod support;

use std::num::NonZeroUsize;

use concord::reqwest::{self, StatusCode, Url};
use concord_example_github_sdk::ids::{IssueNumber, RunId};
use concord_example_github_sdk::models::IssueComment;
use concord_example_github_sdk::paging::{Binding, LinkPage, NextLink, operation};
use concord_example_github_sdk::{GitHubConfig, GitHubContext, GitHubError, RepoRef};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Budget, Outcome, PagesError, Resume, sequential};
use support::*;

fn ids(page: &LinkPage<IssueComment>) -> Vec<u64> {
    page.items.iter().map(|c| c.id.get()).collect()
}

/// `NextLink::to_stored`'s documented form: `url` stored with `binding`.
fn stored(binding: &Binding, url: impl std::fmt::Display) -> String {
    let parent = binding
        .parent()
        .map_or_else(|| "-".to_owned(), |parent| parent.to_string());
    format!(
        "github-link/1 {} {} {parent} {url}",
        binding.operation(),
        binding.repo()
    )
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
    let restored = pages.restore(&stored(pages.binding(), "http://evil.test/steal?page=2"));
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

/// A persisted continuation is restored against the same factory and fetches that page; the
/// stored form round-trips.
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
    let url = format!(
        "{}/repositories/42/issues/7/comments?page=2",
        api_base(server.addr())
    );
    let next = pages
        .restore(&stored(pages.binding(), Url::parse(&url).unwrap()))
        .unwrap();
    assert_eq!(pages.restore(&next.to_stored()).unwrap(), next);
    let page = pages.fetch(Some(next)).await.unwrap();
    assert_eq!(ids(&page), [9]);
    server.shutdown().await;
}

/// A persisted continuation is bound by its stored binding and by what its path addresses
/// (OPUS-1): issue 13's comments link restored into issue 11's factory, another repository's
/// continuation, or another operation's link is refused by `restore` itself, before any request.
/// The link of the factory's own collection is accepted in either repository form (owner and
/// name in any ASCII case), also under a base path, and fetches that page.
#[tokio::test]
async fn persisted_continuation_of_another_collection_is_refused() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query() == Some("page=2") {
            gh_json(200, &comments_json(&[9], 11, "octo-org/hello-world"))
        } else {
            Reply::new(500)
        }
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let issue_11 = repo.issue_comment_pages(IssueNumber(11), |op| op);
    let base = api_base(server.addr());
    let url = |path: &str| Url::parse(&format!("{base}{path}")).unwrap();
    let refused = |path: &str| match issue_11.restore(&stored(issue_11.binding(), url(path))) {
        Err(GitHubError::ContinuationMismatch { expected, found }) => {
            assert_eq!(*expected, *issue_11.binding(), "{path}");
            *found
        }
        other => panic!("{path}: expected a mismatch, got {other:?}"),
    };

    // Another parent of the same operation, in both repository forms: the parent the path names.
    for path in [
        "/repositories/101/issues/13/comments?page=2",
        "/repos/octo-org/hello-world/issues/13/comments?page=2",
    ] {
        let found = refused(path);
        assert_eq!(
            (found.operation(), found.parent()),
            (operation::ISSUE_COMMENTS, Some(13)),
            "{path}"
        );
    }
    // Another repository's continuation, in either URL form: refused by its stored binding.
    let second = Binding::new(
        operation::ISSUE_COMMENTS,
        RepoRef::new("octo-org", "second-repo"),
        Some(11),
    );
    for path in [
        "/repositories/202/issues/11/comments?page=2",
        "/repos/octo-org/second-repo/issues/11/comments?page=2",
    ] {
        match issue_11.restore(&stored(&second, url(path))) {
            Err(GitHubError::ContinuationMismatch { found, .. }) => assert_eq!(*found, second),
            other => panic!("{path}: expected a mismatch, got {other:?}"),
        }
    }
    // Another parent or operation in the stored binding.
    for binding in [
        Binding::new(
            operation::ISSUE_COMMENTS,
            RepoRef::new("octo-org", "hello-world"),
            Some(13),
        ),
        Binding::new(
            operation::PULL_REVIEWS,
            RepoRef::new("octo-org", "hello-world"),
            Some(11),
        ),
    ] {
        let restored = issue_11.restore(&stored(
            &binding,
            url("/repositories/101/issues/11/comments"),
        ));
        match restored {
            Err(GitHubError::ContinuationMismatch { found, .. }) => assert_eq!(*found, binding),
            other => panic!("expected a mismatch, got {other:?}"),
        }
    }
    // Another operation or no collection of this operation.
    for path in [
        "/repos/octo-org/hello-world/pulls/11/reviews?page=2",
        "/repositories/101/issues/comments?page=2",
        "/repositories/101/issues/11?page=2",
        "/repositories/101/issues/11/comments/12?page=2",
        "/repositories/101/issues/011/comments?page=2",
        "/repositories/x101/issues/11/comments?page=2",
        "/orgs/octo-org/issues/11/comments?page=2",
        "/?page=2",
    ] {
        let found = refused(path);
        assert_eq!(found.operation(), operation::UNRECOGNIZED, "{path}");
    }
    // A top-level listing: its own path only.
    let issues = repo.issue_pages(repo.issues(&Default::default()));
    assert!(matches!(
        issues.restore(&stored(
            issues.binding(),
            url("/repositories/101/pulls?page=2")
        )),
        Err(GitHubError::ContinuationMismatch { .. })
    ));
    assert!(
        issues
            .restore(&stored(
                issues.binding(),
                url("/repositories/101/issues?page=2")
            ))
            .is_ok()
    );
    assert_eq!(server.requests().len(), 0, "refused restores send nothing");

    // The factory's own collection: accepted, and the page is fetched where the link points.
    for path in [
        "/repositories/101/issues/11/comments?page=2",
        "/repos/Octo-Org/Hello-World/issues/11/comments?page=2",
    ] {
        let next = issue_11
            .restore(&stored(issue_11.binding(), url(path)))
            .unwrap();
        assert_eq!(next.binding(), issue_11.binding());
        assert_eq!(ids(&issue_11.fetch(Some(next)).await.unwrap()), [9]);
    }
    let targets: Vec<String> = server.requests().iter().map(|r| r.target.clone()).collect();
    assert_eq!(
        targets,
        [
            "/repositories/101/issues/11/comments?page=2",
            "/repos/Octo-Org/Hello-World/issues/11/comments?page=2",
        ]
    );

    // Under a base path, the collection is read below it.
    let enterprise = GitHubContext::new(
        http(server.addr(), None),
        GitHubConfig {
            token: TOKEN_A.into(),
        },
    )
    .with_base_url(format!("{base}/api/v3"))
    .unwrap();
    let repo = enterprise.repo(RepoRef::new("octo-org", "hello-world"));
    let issue_11 = repo.issue_comment_pages(IssueNumber(11), |op| op);
    let restore = |path: &str| issue_11.restore(&stored(issue_11.binding(), url(path)));
    assert!(restore("/api/v3/repositories/101/issues/11/comments?page=2").is_ok());
    assert!(matches!(
        restore("/api/v3/repositories/101/issues/13/comments?page=2"),
        Err(GitHubError::ContinuationMismatch { .. })
    ));
    server.shutdown().await;
}

/// The stored form keeps the repository a link belongs to (Sol's fix re-check of OPUS-1): a
/// repository-id link returned to repository A, stored and restored through repository B's
/// factory, is refused, although its URL names no repository; through A's factory it resumes.
/// A link naming the repository by a new name (after a rename redirect) restores through the
/// factory it was returned to (SOL-R3). Text that is not the stored form is refused.
#[tokio::test]
async fn stored_continuation_keeps_its_repository() {
    let server = TestServer::start(|r: &Recorded| {
        let host = r.host.clone();
        match r.target.as_str() {
            "/repos/org/repo-a/issues/11/comments" => {
                gh_json(200, &comments_json(&[1], 11, "org/repo-a")).header(
                    "link",
                    format!(
                        "<http://{host}/repositories/101/issues/11/comments?page=2>; rel=\"next\""
                    ),
                )
            }
            "/repos/org/old-name/issues/11/comments" => {
                gh_json(200, &comments_json(&[1], 11, "org/new-name")).header(
                    "link",
                    format!(
                        "<http://{host}/repos/org/new-name/issues/11/comments?page=2>; rel=\"next\""
                    ),
                )
            }
            "/repositories/101/issues/11/comments?page=2"
            | "/repos/org/new-name/issues/11/comments?page=2" => {
                gh_json(200, &comments_json(&[2], 11, "org/repo-a"))
            }
            _ => Reply::new(500),
        }
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo_a = github.repo(RepoRef::new("org", "repo-a"));
    let repo_b = github.repo(RepoRef::new("org", "repo-b"));
    let a = repo_a.issue_comment_pages(IssueNumber(11), |op| op);
    let b = repo_b.issue_comment_pages(IssueNumber(11), |op| op);

    let next = a.fetch(None).await.unwrap().next.unwrap();
    let text = next.to_stored();
    assert!(
        text.starts_with("github-link/1 issues/list-comments org/repo-a 11 http://"),
        "{text}"
    );
    match b.restore(&text) {
        Err(GitHubError::ContinuationMismatch { expected, found }) => {
            assert_eq!(*expected, *b.binding());
            assert_eq!(*found, *a.binding());
        }
        other => panic!("expected a mismatch, got {other:?}"),
    }
    let requests_before = server.requests().len();
    let resumed = a.restore(&text).unwrap();
    assert_eq!(resumed, next);
    assert_eq!(
        server.requests().len(),
        requests_before,
        "restore sends nothing"
    );
    assert_eq!(ids(&a.fetch(Some(resumed)).await.unwrap()), [2]);

    // A renamed repository's link, returned to the old name's factory.
    let old = github
        .repo(RepoRef::new("org", "old-name"))
        .issue_comment_pages(IssueNumber(11), |op| op);
    let renamed = old.fetch(None).await.unwrap().next.unwrap();
    let resumed = old.restore(&renamed.to_stored()).unwrap();
    assert_eq!(ids(&old.fetch(Some(resumed)).await.unwrap()), [2]);

    for text in [
        "",
        "http://example.test/repositories/101/issues/11/comments?page=2",
        "github-link/2 issues/list-comments org/repo-a 11 http://example.test/x",
        "github-link/1 issues/list-comments org/repo-a 011 http://example.test/x",
        "github-link/1 issues/list-comments org-repo-a 11 http://example.test/x",
        "github-link/1 issues/list-comments org/repo-a 11 not-a-url",
        "github-link/1 issues/list-comments org/repo-a 11 http://example.test/x extra",
    ] {
        assert!(
            matches!(a.restore(text), Err(GitHubError::MalformedStoredLink)),
            "{text:?}"
        );
    }
    let unknown = a.restore("github-link/1 gists/list org/repo-a 11 http://example.test/x");
    match unknown {
        Err(GitHubError::ContinuationMismatch { found, .. }) => {
            assert_eq!(found.operation(), operation::UNRECOGNIZED)
        }
        other => panic!("expected a mismatch, got {other:?}"),
    }
    server.shutdown().await;
}

/// A fragment is never sent (SOL-R1): links that differ only by fragment request the same page,
/// so a `next` repeating the requested URL with another fragment is refused on the first repeat,
/// and a kept link carries no fragment.
#[tokio::test]
async fn next_link_differing_only_by_fragment_does_not_advance() {
    let server = TestServer::start(|r: &Recorded| {
        let host = r.host.clone();
        // Page 1 links to page 2 with `#v1`; page 2 links to itself with `#v2`.
        let n = if r.target.ends_with("page=2") { 2 } else { 1 };
        gh_json(200, &comments_json(&[1], 7, "octo-org/hello-world")).header(
            "link",
            format!("<http://{host}/repositories/42/issues/7/comments?page=2#v{n}>; rel=\"next\""),
        )
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let pages = repo.issue_comment_pages(IssueNumber(7), |op| op);
    let first = pages.fetch(None).await.unwrap();
    let next = first.next.unwrap();
    assert_eq!(next.raw_url().fragment(), None);
    assert_eq!(next.raw_url().query(), Some("page=2"));
    assert!(matches!(
        pages.fetch(Some(next)).await,
        Err(GitHubError::LinkDidNotAdvance { .. })
    ));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

/// A `rel="next"` equal to the URL just requested (OPUS-2) is refused on the first repeat, also
/// when it repeats the first page's URL; the chain ends there instead of at the budget.
#[tokio::test]
async fn next_link_equal_to_the_request_is_refused() {
    let server = TestServer::start(|r: &Recorded| {
        let host = r.host.clone();
        // Every page, page 2 included, says: next is page 2.
        gh_json(200, &comments_json(&[1], 7, "octo-org/hello-world")).header(
            "link",
            format!("<http://{host}/repositories/42/issues/7/comments?page=2>; rel=\"next\""),
        )
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let pages = repo.issue_comment_pages(IssueNumber(7), |op| op);
    let budget = Budget::new(std::num::NonZeroUsize::new(50).unwrap());
    let mut chain = sequential(Resume::Start, budget, |key| pages.fetch(key));
    let mut items = 0;
    let mut errors = Vec::new();
    while let Some(page) = chain.next().await {
        match page {
            Ok(page) => items += page.items.len(),
            Err(error) => errors.push(error),
        }
    }
    let report = chain.finish();
    assert_eq!(
        items, 1,
        "page 1 only: the repeated page's items are not returned"
    );
    assert_eq!(errors.len(), 1);
    match &errors[0] {
        GitHubError::LinkDidNotAdvance { meta } => assert_eq!(meta.status().as_u16(), 200),
        other => panic!("expected LinkDidNotAdvance, got {other:?}"),
    }
    assert_eq!(
        errors[0].to_string(),
        "GitHub page: next link is the URL just requested; not followed"
    );
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(report.progress.admitted, 2);
    assert_eq!(server.requests().len(), 2);

    // The first page linking to itself is the same violation.
    let server_2 = TestServer::start(|r: &Recorded| {
        let host = r.host.clone();
        let target = r.target.clone();
        gh_json(200, &comments_json(&[1], 7, "octo-org/hello-world"))
            .header("link", format!("<http://{host}{target}>; rel=\"next\""))
    })
    .await;
    let github = context(server_2.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    assert!(matches!(
        repo.issue_comment_pages(IssueNumber(7), |op| op)
            .fetch(None)
            .await,
        Err(GitHubError::LinkDidNotAdvance { .. })
    ));
    assert_eq!(server_2.requests().len(), 1);
    server_2.shutdown().await;
    server.shutdown().await;
}

const COMMENTS: &str = "/repos/octo-org/hello-world/issues/7/comments";

/// Issue 7's comments on three pages linked by `page=N` (1, 2 / 3, 4 / 5); `page2` answers the
/// second page instead, when given.
fn three_pages(r: &Recorded, page2: Option<&dyn Fn(&str) -> Reply>) -> Reply {
    let host = r.host.clone();
    let next = |n: u32| format!("<http://{host}{COMMENTS}?page={n}>; rel=\"next\"");
    let repo = "octo-org/hello-world";
    match r.target.strip_prefix(COMMENTS) {
        Some("") => gh_json(200, &comments_json(&[1, 2], 7, repo)).header("link", next(2)),
        Some("?page=2") => match page2 {
            Some(page2) => page2(&host),
            None => gh_json(200, &comments_json(&[3, 4], 7, repo)).header("link", next(3)),
        },
        Some("?page=3") => gh_json(200, &comments_json(&[5], 7, repo)),
        _ => gh_json(404, r#"{"message":"Not Found"}"#),
    }
}

fn comment_ids(comments: &[IssueComment]) -> Vec<u64> {
    comments.iter().map(|c| c.id.get()).collect()
}

/// The ordinary path: `items()` follows every `rel="next"` and ends only after the page without
/// one; `pages()` keeps each page's metadata.
#[tokio::test]
async fn ordinary_items_follow_every_link_to_the_last_page() {
    let server = TestServer::start(|r: &Recorded| three_pages(r, None)).await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let comments = repo.issue_comment_pages(IssueNumber(7), |op| op);

    let all = comments.items().collect_complete().await.unwrap();
    assert_eq!(comment_ids(&all), [1, 2, 3, 4, 5]);

    let mut pages = comments.pages();
    let mut sizes = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        assert_eq!(
            page.meta.response.request_id(),
            Some("0000:1111:2222:3333:4444")
        );
        sizes.push(page.items.len());
    }
    assert_eq!(sizes, [2, 2, 1]);
    assert!(
        pages.next_page().await.unwrap().is_none(),
        "the end repeats"
    );
    let wire = server.requests();
    assert_eq!(wire.len(), 6);
    for request in &wire {
        assert_api_headers(request, TOKEN_A);
    }
    server.shutdown().await;
}

/// A cap reached while GitHub links on is an error carrying the next link, never a silent
/// prefix; continuing from it reads the rest. A cap reached exactly on the last page is the end.
#[tokio::test]
async fn a_cap_with_more_pages_is_an_error_and_on_the_last_page_is_the_end() {
    let server = TestServer::start(|r: &Recorded| three_pages(r, None)).await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let comments = repo.issue_comment_pages(IssueNumber(7), |op| op);

    let incomplete = comments
        .items()
        .max_pages(NonZeroUsize::new(2).unwrap())
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(comment_ids(&incomplete.collected), [1, 2, 3, 4]);
    let PagesError::Bounded { resume, .. } = incomplete.error else {
        panic!("a cap with more pages is `Bounded`: {:?}", incomplete.error)
    };
    let Resume::At(link) = &resume else {
        panic!("the resume point is the next link")
    };
    assert_eq!(link.raw_url().query(), Some("page=3"));
    assert_eq!(link.binding(), comments.binding());
    assert_eq!(server.requests().len(), 2, "the cap is not exceeded");

    let rest = comments
        .pages()
        .starting_at(resume)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(comment_ids(&rest), [5]);

    let exact = comments
        .pages()
        .max_pages(NonZeroUsize::new(3).unwrap())
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        exact.len(),
        3,
        "a cap exactly on the last page is exhaustion"
    );
    server.shutdown().await;
}

/// A failed second page ends the traversal with the SDK's typed error (its metadata kept) and
/// the failed link; `collect_complete` keeps the first page's items. Nothing is retried, and the
/// traversal never reads as ended afterwards.
#[tokio::test]
async fn a_failed_second_page_keeps_the_first_pages_items() {
    let server = TestServer::start(|r: &Recorded| {
        three_pages(
            r,
            Some(&|_| {
                Reply::json(502, r#"{"message":"Server Error"}"#)
                    .header("x-github-request-id", "PAGE:2")
            }),
        )
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let comments = repo.issue_comment_pages(IssueNumber(7), |op| op);

    let incomplete = comments.items().collect_complete().await.unwrap_err();
    assert_eq!(comment_ids(&incomplete.collected), [1, 2]);
    let PagesError::Provider { error, failed, .. } = incomplete.error else {
        panic!("a failed page is `Provider`: {:?}", incomplete.error)
    };
    assert_eq!(error.status(), Some(StatusCode::BAD_GATEWAY));
    assert_eq!(error.meta().unwrap().request_id(), Some("PAGE:2"));
    let Resume::At(link) = failed else {
        panic!("the failed page is the second one")
    };
    assert_eq!(link.raw_url().query(), Some("page=2"));
    assert_eq!(server.requests().len(), 2, "nothing is retried");

    let mut pages = comments.pages();
    assert!(pages.next_page().await.unwrap().is_some());
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Provider { .. })
    ));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended {
            outcome: Outcome::Failed,
            ..
        })
    ));
    server.shutdown().await;
}

/// The SDK's link policy holds on the ordinary path: a `next` link off the API origin is the
/// page's error (its items are not returned), the other host receives nothing, and the
/// traversal does not end silently.
#[tokio::test]
async fn a_foreign_next_link_is_an_error_not_an_end() {
    let elsewhere = TestServer::start(|_: &Recorded| gh_json(200, "[]")).await;
    let port = elsewhere.addr().port();
    let api = TestServer::start(move |r: &Recorded| {
        three_pages(
            r,
            Some(&|_| {
                gh_json(200, &comments_json(&[3, 4], 7, "octo-org/hello-world")).header(
                    "link",
                    format!("<http://evil.test:{port}{COMMENTS}?page=3>; rel=\"next\""),
                )
            }),
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
    let incomplete = repo
        .issue_comment_pages(IssueNumber(7), |op| op)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(comment_ids(&incomplete.collected), [1, 2]);
    assert!(
        matches!(
            incomplete.error,
            PagesError::Provider {
                error: GitHubError::ForeignLink { meta: Some(_) },
                ..
            }
        ),
        "{:?}",
        incomplete.error
    );
    assert_eq!(
        elsewhere.requests().len(),
        0,
        "the token never left the API"
    );
    elsewhere.shutdown().await;
    api.shutdown().await;
}

#[tokio::test]
async fn a_guarded_link_walk_refuses_a_longer_cycle_with_its_binding_and_prefix() {
    for fragments in [false, true] {
        let server = TestServer::start(move |r: &Recorded| {
            let (id, next) = if r.target.contains("page=A") {
                (2, "B")
            } else if r.target.contains("page=B") {
                (3, "A")
            } else {
                (1, "A")
            };
            let fragment = if fragments { if id == 3 { "#second" } else { "#first" } } else { "" };
            gh_json(200, &comments_json(&[id], 7, "octo-org/hello-world")).header(
                "link",
                format!("<http://{}/repositories/42/issues/7/comments?page={next}&cursor=F1_GITHUB_CANARY{fragment}>; rel=\"next\"", r.host),
            )
        }).await;
        let github = context(server.addr(), TOKEN_A);
        let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
        let listing = repo.issue_comment_pages(IssueNumber(7), |op| op);
        let partial = listing
            .pages()
            .refuse_repeated_keys()
            .collect_complete()
            .await
            .unwrap_err();
        assert_eq!(
            partial
                .collected
                .iter()
                .flat_map(ids)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(matches!(partial.error, PagesError::Repeated { .. }));
        let progress = partial.error.progress();
        assert_eq!(
            (progress.admitted, progress.succeeded, progress.failed),
            (3, 3, 0)
        );
        let key = partial.error.resume().unwrap().key().unwrap();
        assert_eq!(key.binding(), listing.binding());
        assert_eq!(key.raw_url().fragment(), None);
        assert!(!format!("{partial} {partial:?}").contains("F1_GITHUB_CANARY"));
        for page in &partial.collected {
            assert_eq!(
                page.meta.response.request_id(),
                Some("0000:1111:2222:3333:4444")
            );
        }
        assert_eq!(server.requests().len(), 3);
        for request in server.requests() {
            assert_api_headers(&request, TOKEN_A);
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn starting_at_a_restored_link_preserves_headers_metadata_and_binding_refusal() {
    let server = TestServer::start(|_: &Recorded| {
        gh_json(200, &comments_json(&[7], 7, "octo-org/hello-world"))
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let listing = repo.issue_comment_pages(IssueNumber(7), |op| op);
    let restored = listing
        .restore(&stored(
            listing.binding(),
            format!(
                "{}/repositories/42/issues/7/comments?page=3",
                api_base(server.addr())
            ),
        ))
        .unwrap();
    let mut pages = listing.pages().starting_at(Resume::At(restored.clone()));
    assert_eq!(server.requests().len(), 0);
    let page = pages.next_page().await.unwrap().unwrap();
    assert_eq!(ids(&page), [7]);
    assert_eq!(
        page.meta.response.request_id(),
        Some("0000:1111:2222:3333:4444")
    );
    assert_api_headers(&server.requests()[0], TOKEN_A);
    assert!(pages.next_page().await.unwrap().is_none());
    let other = repo.issue_comment_pages(IssueNumber(8), |op| op);
    let mut foreign = other.pages().starting_at(Resume::At(restored));
    assert!(matches!(
        foreign.next_page().await,
        Err(PagesError::Provider {
            error: GitHubError::ContinuationMismatch { .. },
            ..
        })
    ));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}
