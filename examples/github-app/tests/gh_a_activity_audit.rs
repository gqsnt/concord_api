//! GH-A end to end against a loopback GitHub stand-in serving the synthetic fixtures of
//! `tests/fixtures/MANIFEST.json`.
//!
//! Repository A (`octo-org/hello-world`, id 101) lists, with `state=all&labels=bug&since=..&per_page=2`:
//! page 1 = issue #11 and pull request #12 (`Link: next` to a `/repositories/101/...` URL, no
//! `last`), page 2 = empty with `next`, `prev`, `first` (no `last`), page 3 = issue #13 with
//! `prev`/`first` only. Issue #11 has two comment pages, #12 two review pages and its details,
//! #13 two comment pages. Repository B (`octo-org/second-repo`, id 202) has one issue on one page
//! without any `Link` header. Faults are switched on per test.
//!
//! Run: `cargo test -p concord-example-github-app --test gh_a_activity_audit`.

mod support;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::reqwest::StatusCode;
use concord_example_github_app::audit::{
    ActivityEntry, ActivityReport, AuditBudget, AuditContinuation, AuditError, AuditInput,
    ChildCompletion, ChildKind, ChildResume, Completion, Consistency, FailedAt, RepositoryState,
    activity_audit,
};
use concord_example_github_app::requests::last_rate;
use concord_example_github_sdk::filters::IssueFilter;
use concord_example_github_sdk::ids::{CommentId, IssueNumber, PullNumber, PullRequestId};
use concord_example_github_sdk::models::{ReviewState, State};
use concord_example_github_sdk::params::{ETag, Labels, PerPage, StateFilter, Timestamp};
use concord_example_github_sdk::{GitHubError, RepoRef};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::Resume;
use support::*;

const ISSUES_QUERY: &str = "state=all&labels=bug&since=2026-10-01T00%3A00%3A00Z&per_page=2";

/// Faults a test switches on: how many times a given page fails with `500`.
#[derive(Default)]
struct Faults {
    issues_page_2: AtomicUsize,
    comments_13_page_2: AtomicUsize,
    served: AtomicUsize,
}

fn fail_once(counter: &AtomicUsize) -> bool {
    counter
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
        .is_ok()
}

fn stand_in(faults: Arc<Faults>) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    move |r: &Recorded| {
        let n = faults.served.fetch_add(1, Ordering::SeqCst);
        let o = origin(r);
        let issues = format!("/repositories/101/issues?{ISSUES_QUERY}");
        match (r.path(), r.query().unwrap_or("")) {
            ("/repos/octo-org/hello-world", "") => match r.header("if-none-match") {
                Some("\"repo-a-v1\"") => Reply::new(304)
                    .header("etag", "\"repo-a-v1\"")
                    .header("x-github-request-id", format!("REQ:{n:04}"))
                    .header("x-ratelimit-remaining", (4999 - n).to_string()),
                _ => gh_json(fixture!("repository-a.json"), n).header("etag", "\"repo-a-v1\""),
            },
            ("/repos/octo-org/second-repo", "") => {
                gh_json(fixture!("repository-b.json"), n).header("etag", "\"repo-b-v1\"")
            }
            ("/repos/octo-org/hello-world/issues", q) if q == ISSUES_QUERY => {
                gh_json(fixture!("issues-a-1.json"), n)
                    .header("link", format!("<{o}{issues}&page=2>; rel=\"next\""))
            }
            ("/repositories/101/issues", q) if q == format!("{ISSUES_QUERY}&page=2") => {
                if fail_once(&faults.issues_page_2) {
                    return gh_error(500, fixture!("error-500.json"));
                }
                gh_json(fixture!("issues-a-2.json"), n).header(
                    "link",
                    format!(
                        "<{o}{issues}&page=3>; rel=\"next\", <{o}{issues}&page=1>; rel=\"prev\", <{o}{issues}&page=1>; rel=\"first\""
                    ),
                )
            }
            ("/repositories/101/issues", q) if q == format!("{ISSUES_QUERY}&page=3") => {
                gh_json(fixture!("issues-a-3.json"), n).header(
                    "link",
                    format!(
                        "<{o}{issues}&page=2>; rel=\"prev\", <{o}{issues}&page=1>; rel=\"first\""
                    ),
                )
            }
            ("/repos/octo-org/hello-world/issues/11/comments", "") => {
                gh_json(fixture!("comments-a-11-1.json"), n).header(
                    "link",
                    format!("<{o}/repositories/101/issues/11/comments?page=2>; rel=\"next\""),
                )
            }
            ("/repositories/101/issues/11/comments", "page=2") => {
                gh_json(fixture!("comments-a-11-2.json"), n)
            }
            ("/repos/octo-org/hello-world/pulls/12", "") => gh_json(fixture!("pull-a-12.json"), n),
            ("/repos/octo-org/hello-world/pulls/12/reviews", "") => {
                gh_json(fixture!("reviews-a-12-1.json"), n).header(
                    "link",
                    format!("<{o}/repositories/101/pulls/12/reviews?page=2>; rel=\"next\""),
                )
            }
            ("/repositories/101/pulls/12/reviews", "page=2") => {
                gh_json(fixture!("reviews-a-12-2.json"), n)
            }
            ("/repos/octo-org/hello-world/issues/13/comments", "") => {
                gh_json(fixture!("comments-a-13-1.json"), n).header(
                    "link",
                    format!("<{o}/repositories/101/issues/13/comments?page=2>; rel=\"next\""),
                )
            }
            ("/repositories/101/issues/13/comments", "page=2") => {
                if fail_once(&faults.comments_13_page_2) {
                    return gh_error(500, fixture!("error-500.json"));
                }
                gh_json(fixture!("comments-a-13-2.json"), n)
            }
            ("/repos/octo-org/second-repo/issues", q) if q == ISSUES_QUERY => {
                gh_json(fixture!("issues-b-1.json"), n)
            }
            ("/repos/octo-org/second-repo/issues/21/comments", "") => {
                gh_json(fixture!("comments-b-21-1.json"), n)
            }
            _ => gh_error(404, r#"{"message":"Not Found","status":"404"}"#),
        }
    }
}

fn filter() -> IssueFilter {
    IssueFilter {
        state: Some(StateFilter::All),
        labels: Some(Labels::new(["bug"]).unwrap()),
        since: Some(Timestamp::parse("2026-10-01T00:00:00Z").unwrap()),
        per_page: Some(PerPage::new(2).unwrap()),
        ..IssueFilter::default()
    }
}

fn budget(issue_pages: usize, child_pages: usize) -> AuditBudget {
    AuditBudget {
        issue_pages: NonZeroUsize::new(issue_pages).unwrap(),
        child_pages: NonZeroUsize::new(child_pages).unwrap(),
    }
}

fn input(budget: AuditBudget) -> AuditInput {
    AuditInput {
        filter: filter(),
        budget,
        repository_etag: None,
        from: None,
    }
}

fn comment_ids(entry: &ActivityEntry) -> Vec<u64> {
    match entry {
        ActivityEntry::Issue { comments, .. } => {
            comments.items.iter().map(|c| c.id.get()).collect()
        }
        ActivityEntry::PullRequest { .. } => panic!("not an issue"),
    }
}

fn numbers(report: &ActivityReport) -> Vec<u64> {
    report.entries.iter().map(|e| e.number().get()).collect()
}

/// The whole listing: three parent pages (the second empty), absent `last`, nested child
/// collections over several pages, issue versus pull-request representations, request metadata.
#[tokio::test]
async fn complete_audit_over_multiple_pages() {
    let server = TestServer::start(stand_in(Arc::default())).await;
    let github = github(server.addr(), None, TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let report = activity_audit(&repo, input(budget(5, 5))).await;
    assert!(
        matches!(report.completion, Completion::Complete),
        "{:?}",
        report.completion
    );
    assert_eq!(report.consistency, Consistency::LiveBestEffort);
    match &report.repository {
        Some(RepositoryState::Fetched { summary, etag }) => {
            assert_eq!(summary.full_name, "octo-org/hello-world");
            assert_eq!(etag.as_ref(), Some(&ETag::new("\"repo-a-v1\"")));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(numbers(&report), [11, 12, 13]);
    assert_eq!(report.issues().count(), 2);
    assert_eq!(report.pull_requests().count(), 1);

    // Issue #11: comments over two pages.
    let ActivityEntry::Issue {
        comments,
        state,
        labels,
        ..
    } = &report.entries[0]
    else {
        panic!("#11 is an issue")
    };
    assert_eq!(
        comments.items.iter().map(|c| c.id).collect::<Vec<_>>(),
        [CommentId(1101), CommentId(1102), CommentId(1103)]
    );
    assert_eq!(
        (comments.pages, &comments.completion),
        (2, &ChildCompletion::Complete)
    );
    assert_eq!(
        (state, labels.as_slice()),
        (&State::Open, ["bug".to_owned()].as_slice())
    );

    // #12 is a pull request in the Issues API: details from the Pulls API (its own id), reviews.
    let ActivityEntry::PullRequest {
        number,
        details,
        reviews,
        ..
    } = &report.entries[1]
    else {
        panic!("#12 is a pull request")
    };
    assert_eq!(*number, PullNumber(12));
    let details = details.as_ref().unwrap();
    assert_eq!(details.id, PullRequestId(7012));
    assert_eq!(
        (details.merged, details.commits, details.head.as_str()),
        (false, 2, "fix-empty")
    );
    assert_eq!(
        reviews
            .items
            .iter()
            .map(|r| r.state.clone())
            .collect::<Vec<_>>(),
        [ReviewState::ChangesRequested, ReviewState::Approved]
    );
    assert_eq!(reviews.pages, 2);
    assert_eq!(comment_ids(&report.entries[2]), [1301, 1302]);

    // The wire: filters on the first page, then the Link URLs exactly as sent (the empty page 2
    // continues to page 3; no page number was computed), every request with token A.
    let wire = server.requests();
    let targets: Vec<&str> = wire.iter().map(|r| r.target.as_str()).collect();
    assert_eq!(
        targets,
        [
            "/repos/octo-org/hello-world".to_owned(),
            format!("/repos/octo-org/hello-world/issues?{ISSUES_QUERY}"),
            "/repos/octo-org/hello-world/issues/11/comments".to_owned(),
            "/repositories/101/issues/11/comments?page=2".to_owned(),
            "/repos/octo-org/hello-world/pulls/12".to_owned(),
            "/repos/octo-org/hello-world/pulls/12/reviews".to_owned(),
            "/repositories/101/pulls/12/reviews?page=2".to_owned(),
            format!("/repositories/101/issues?{ISSUES_QUERY}&page=2"),
            format!("/repositories/101/issues?{ISSUES_QUERY}&page=3"),
            "/repos/octo-org/hello-world/issues/13/comments".to_owned(),
            "/repositories/101/issues/13/comments?page=2".to_owned(),
        ]
    );
    for request in &wire {
        assert_api_request(request, TOKEN_A);
    }

    // Request metadata: one record per request, request ids and rate limits kept.
    assert_eq!(report.requests.len(), wire.len());
    assert_eq!(report.requests[1].operation, "issues/list-for-repo");
    assert_eq!(report.requests[1].request_id.as_deref(), Some("REQ:0001"));
    assert_eq!(
        last_rate(&report.requests).unwrap().remaining,
        Some(4999 - 10)
    );
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

/// A second audit refreshes the repository with its `ETag`: `304`, reported as not modified.
#[tokio::test]
async fn conditional_refresh_with_etag() {
    let server = TestServer::start(stand_in(Arc::default())).await;
    let github = github(server.addr(), None, TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let first = activity_audit(&repo, input(budget(1, 1))).await;
    let Some(RepositoryState::Fetched {
        etag: Some(etag), ..
    }) = first.repository
    else {
        panic!("first audit fetches the repository")
    };
    let second = activity_audit(
        &repo,
        AuditInput {
            repository_etag: Some(etag.clone()),
            ..input(budget(1, 1))
        },
    )
    .await;
    assert_eq!(
        second.repository,
        Some(RepositoryState::NotModified { etag })
    );
    assert_eq!(second.requests[0].status, Some(304));
    let wire = server.requests();
    let refreshes: Vec<Option<&str>> = wire
        .iter()
        .filter(|r| r.path() == "/repos/octo-org/hello-world")
        .map(|r| r.header("if-none-match"))
        .collect();
    assert_eq!(refreshes, [None, Some("\"repo-a-v1\"")]);
    server.shutdown().await;
}

/// A child page fails after earlier successes (issue #13's second comment page answers `500`,
/// after #11, #12 and #13's first page succeeded). The report keeps what was collected, names the
/// failed page apart from the restart point, and the continuation resumes there: the parent
/// position taken just before #13 was emitted is replayed (#13 re-emitted, no earlier item), and
/// #13's comments continue at the failed page.
///
/// Observed detail: that position is the *empty* page 2, not page 3. `Items::next` crossed the
/// empty page and fetched page 3 within the call that returned #13, so the checkpoint taken before
/// the call still names page 2; resuming costs one extra (empty) page request. See FRICTION.md.
#[tokio::test]
async fn child_failure_after_successes_then_resume() {
    let faults = Arc::new(Faults::default());
    faults.comments_13_page_2.store(1, Ordering::SeqCst);
    let server = TestServer::start(stand_in(faults.clone())).await;
    let github = github(server.addr(), None, TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let first = activity_audit(&repo, input(budget(5, 5))).await;
    assert_eq!(numbers(&first), [11, 12, 13]);
    assert_eq!(
        comment_ids(&first.entries[2]),
        [1301],
        "the prefix before the failure is kept"
    );
    let Completion::Interrupted {
        error,
        failed,
        continuation,
    } = &first.completion
    else {
        panic!("{:?}", first.completion)
    };
    let AuditError::GitHub(github_error) = error else {
        panic!("{error:?}")
    };
    assert_eq!(
        github_error.status(),
        Some(StatusCode::INTERNAL_SERVER_ERROR)
    );
    assert_eq!(github_error.meta().unwrap().request_id(), Some("REQ:FAIL"));
    let FailedAt::Child {
        number,
        kind,
        page: Some(Resume::At(failed_link)),
    } = failed
    else {
        panic!("{failed:?}")
    };
    assert_eq!((*number, *kind), (IssueNumber(13), ChildKind::Comments));
    assert_eq!(
        failed_link.raw_url().path(),
        "/repositories/101/issues/13/comments"
    );
    // Restart identity: the parent position before #13 (the empty page 2, nothing skipped) and
    // the child's failed page.
    let ChildResume { number, kind, page } = continuation.child.clone().unwrap();
    assert_eq!((number, kind), (IssueNumber(13), ChildKind::Comments));
    assert_eq!(page, Resume::At(failed_link.clone()));
    assert_eq!(continuation.parent.skip, 0);
    assert_eq!(
        continuation.parent.page.key().unwrap().raw_url().query(),
        Some(format!("{ISSUES_QUERY}&page=2").as_str())
    );
    let before_resume = server.requests().len();

    let resumed = activity_audit(
        &repo,
        AuditInput {
            from: Some(continuation.clone()),
            ..input(budget(5, 5))
        },
    )
    .await;
    assert!(
        matches!(resumed.completion, Completion::Complete),
        "{:?}",
        resumed.completion
    );
    assert_eq!(numbers(&resumed), [13]);
    assert_eq!(comment_ids(&resumed.entries[0]), [1302]);
    let resumed_targets: Vec<String> = server.requests()[before_resume..]
        .iter()
        .map(|r| r.target.clone())
        .collect();
    assert_eq!(
        resumed_targets,
        [
            "/repos/octo-org/hello-world".to_owned(),
            format!("/repositories/101/issues?{ISSUES_QUERY}&page=2"),
            format!("/repositories/101/issues?{ISSUES_QUERY}&page=3"),
            "/repositories/101/issues/13/comments?page=2".to_owned(),
        ]
    );
    server.shutdown().await;
}

/// The issue-page budget stops the parent walk with a continuation; resuming completes the
/// listing without repeating an item.
#[tokio::test]
async fn parent_budget_then_resume() {
    let server = TestServer::start(stand_in(Arc::default())).await;
    let github = github(server.addr(), None, TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let first = activity_audit(&repo, input(budget(1, 5))).await;
    assert_eq!(numbers(&first), [11, 12]);
    let Completion::Bounded { continuation } = &first.completion else {
        panic!("{:?}", first.completion)
    };
    assert!(continuation.child.is_none());
    assert_eq!(continuation.parent.skip, 0);

    let rest = activity_audit(
        &repo,
        AuditInput {
            from: Some(continuation.clone()),
            ..input(budget(5, 5))
        },
    )
    .await;
    assert!(matches!(rest.completion, Completion::Complete));
    assert_eq!(
        numbers(&rest),
        [13],
        "page 2 (empty) and page 3; nothing repeated"
    );
    server.shutdown().await;
}

/// The child budget bounds each child collection; each reports its own resume link.
#[tokio::test]
async fn child_budget_bounds_each_collection() {
    let server = TestServer::start(stand_in(Arc::default())).await;
    let github = github(server.addr(), None, TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let report = activity_audit(&repo, input(budget(5, 1))).await;
    assert!(matches!(report.completion, Completion::Complete));
    for entry in &report.entries {
        let completion = match entry {
            ActivityEntry::Issue { comments, .. } => &comments.completion,
            ActivityEntry::PullRequest { reviews, .. } => &reviews.completion,
        };
        let ChildCompletion::Bounded { resume } = completion else {
            panic!("{entry:?}")
        };
        assert_eq!(resume.raw_url().query(), Some("page=2"));
        assert_eq!(resume.binding().parent(), Some(entry.number().get()));
    }
    server.shutdown().await;
}

/// A parent page fails: the failed page is the restart point, and nothing after it was sent.
#[tokio::test]
async fn parent_page_failure() {
    let faults = Arc::new(Faults::default());
    faults.issues_page_2.store(1, Ordering::SeqCst);
    let server = TestServer::start(stand_in(faults)).await;
    let github = github(server.addr(), None, TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let report = activity_audit(&repo, input(budget(5, 5))).await;
    assert_eq!(numbers(&report), [11, 12]);
    let Completion::Interrupted {
        failed: FailedAt::IssuesPage { page: Some(failed) },
        continuation,
        ..
    } = &report.completion
    else {
        panic!("{:?}", report.completion)
    };
    assert_eq!(&continuation.parent.page, failed);
    assert!(
        failed
            .key()
            .unwrap()
            .raw_url()
            .query()
            .unwrap()
            .ends_with("page=2")
    );
    assert_eq!(
        server.requests().last().unwrap().target,
        format!("/repositories/101/issues?{ISSUES_QUERY}&page=2")
    );
    server.shutdown().await;
}

/// Two repositories under two tokens: every request of each audit carries its own token and
/// repository; a continuation of repository A cannot drive repository B (refused before any
/// listing request).
#[tokio::test]
async fn context_isolation_between_repositories() {
    let server = TestServer::start(stand_in(Arc::default())).await;
    let repo_a = github(server.addr(), None, TOKEN_A).repo(RepoRef::new("octo-org", "hello-world"));
    let repo_b = github(server.addr(), None, TOKEN_B).repo(RepoRef::new("octo-org", "second-repo"));

    let a = activity_audit(&repo_a, input(budget(1, 5))).await;
    let b = activity_audit(&repo_b, input(budget(5, 5))).await;
    assert!(matches!(b.completion, Completion::Complete));
    assert_eq!(numbers(&b), [21]);
    assert_eq!(comment_ids(&b.entries[0]), [2101]);
    for request in server.requests() {
        let token = if request.path().contains("second-repo") || request.path().contains("/202/") {
            TOKEN_B
        } else {
            TOKEN_A
        };
        assert_api_request(&request, token);
    }

    let continuation: AuditContinuation = a.completion.continuation().unwrap().clone();
    let before = server.requests().len();
    let misuse = activity_audit(
        &repo_b,
        AuditInput {
            from: Some(continuation),
            ..input(budget(5, 5))
        },
    )
    .await;
    let Completion::Interrupted {
        error: AuditError::GitHub(GitHubError::ContinuationMismatch { .. }),
        ..
    } = &misuse.completion
    else {
        panic!("{:?}", misuse.completion)
    };
    let sent: Vec<String> = server.requests()[before..]
        .iter()
        .map(|r| r.target.clone())
        .collect();
    assert_eq!(
        sent,
        ["/repos/octo-org/second-repo"],
        "only the repository lookup was sent"
    );
    server.shutdown().await;
}

/// A continuation whose item is no longer at its position (the listing changed) is refused rather
/// than attaching a child cursor to another issue.
#[tokio::test]
async fn stale_continuation_is_refused() {
    let faults = Arc::new(Faults::default());
    faults.comments_13_page_2.store(1, Ordering::SeqCst);
    let server = TestServer::start(stand_in(faults)).await;
    let repo = github(server.addr(), None, TOKEN_A).repo(RepoRef::new("octo-org", "hello-world"));
    let first = activity_audit(&repo, input(budget(5, 5))).await;
    let mut continuation = first.completion.continuation().unwrap().clone();
    continuation.child.as_mut().unwrap().number = IssueNumber(99);
    let resumed = activity_audit(
        &repo,
        AuditInput {
            from: Some(continuation),
            ..input(budget(5, 5))
        },
    )
    .await;
    let Completion::Interrupted {
        error: AuditError::StaleContinuation { expected, found },
        ..
    } = &resumed.completion
    else {
        panic!("{:?}", resumed.completion)
    };
    assert_eq!((*expected, *found), (IssueNumber(99), IssueNumber(13)));
    assert!(resumed.entries.is_empty());
    server.shutdown().await;
}

/// The report's `Debug` (and its errors' `Display`) carry numbers, states and outcomes; never the
/// token, titles, labels or link URLs.
#[tokio::test]
async fn report_rendering_is_redacted() {
    let faults = Arc::new(Faults::default());
    faults.comments_13_page_2.store(1, Ordering::SeqCst);
    let server = TestServer::start(stand_in(faults)).await;
    let repo = github(server.addr(), None, TOKEN_A).repo(RepoRef::new("octo-org", "hello-world"));
    let report = activity_audit(&repo, input(budget(5, 5))).await;
    let Completion::Interrupted { error, .. } = &report.completion else {
        panic!()
    };
    let shown = format!("{report:?} {report:#?} {error}");
    for secret in [
        TOKEN_A,
        "Crash on empty input",
        "/repositories/101",
        "page=2",
        "Reproduced",
    ] {
        assert!(!shown.contains(secret), "{secret} leaked");
    }
    server.shutdown().await;
}
