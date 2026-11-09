//! The frozen initial consumer against a loopback GitHub stand-in (stage 1). **Not edited after
//! stage 1**: at every later commit these exact wire assertions must still hold, which is the
//! omitted-field compatibility check of the SDK change.
//!
//! Fixtures are `schema_derived_synthetic`: built here with the fields of the `issue` schema of
//! the description pinned in the SDK's `PROVIDER.md` (REST API version `2026-03-10`), synthetic
//! ids, logins and tokens. See `MAINTENANCE.md`.

use std::num::NonZeroUsize;

use concord::reqwest;
use concord_check_maintenance_github::before::{
    BugQuery, DigestError, newest_bug, open_bug_digest,
};
use concord_example_github_sdk::ids::IssueNumber;
use concord_example_github_sdk::params::{PerPage, Timestamp};
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoContext, RepoRef};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::Outcome;

const TOKEN_A: &str = "ghp_MAINT_SYNTHETIC_A_000000000000000000";
const TOKEN_B: &str = "ghp_MAINT_SYNTHETIC_B_111111111111111111";

fn repo(server: &TestServer, token: &str, name: &str) -> RepoContext {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(concord_example_github_sdk::redirect_policy())
        .build()
        .unwrap();
    GitHubContext::new(
        http,
        GitHubConfig {
            origin: concord::BaseUrl::from_static("https://api.github.com"),
            token: token.to_owned(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
    .repo(RepoRef::new("octo-org", name))
}

/// One issue with the `issue` schema's fields the SDK reads; `pull` adds the Issues API's
/// `pull_request` object.
fn issue_json(repo: &str, number: u64, title: &str, pull: bool) -> String {
    let pull_request = if pull {
        format!(
            r#","pull_request":{{"url":"https://api.github.com/repos/{repo}/pulls/{number}","html_url":"https://github.com/{repo}/pull/{number}","diff_url":"https://github.com/{repo}/pull/{number}.diff","patch_url":"https://github.com/{repo}/pull/{number}.patch","merged_at":null}}"#
        )
    } else {
        String::new()
    };
    format!(
        r#"{{"id":{id},"node_id":"I_{id}","url":"https://api.github.com/repos/{repo}/issues/{number}","html_url":"https://github.com/{repo}/issues/{number}","number":{number},"state":"open","state_reason":null,"title":"{title}","body":null,"user":{{"login":"reporter","id":7,"type":"User"}},"labels":[{{"id":10,"name":"bug","color":"d73a4a"}}],"assignees":[{{"login":"maintainer","id":8,"type":"User"}}],"milestone":null,"locked":false,"comments":{number},"created_at":"2026-10-0{d}T08:00:00Z","updated_at":"2026-10-2{d}T09:00:00Z","closed_at":null,"author_association":"MEMBER"{pull_request}}}"#,
        id = 5000 + number,
        d = number % 10,
    )
}

fn page(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

fn gh_json(body: String, request_id: &str) -> Reply {
    Reply::json(200, body)
        .header("x-github-request-id", request_id)
        .header("x-ratelimit-limit", "5000")
        .header("x-ratelimit-remaining", "4990")
        .header("x-ratelimit-reset", "1798000000")
        .header("x-ratelimit-resource", "core")
}

fn assert_api_headers(request: &Recorded, token: &str) {
    let bearer = format!("Bearer {token}");
    assert_eq!(request.header("authorization"), Some(bearer.as_str()));
    assert_eq!(
        request.header("accept"),
        Some("application/vnd.github+json")
    );
    assert_eq!(request.header("x-github-api-version"), Some("2026-03-10"));
    assert_eq!(
        request.header("user-agent"),
        Some("concord-example-github-sdk")
    );
}

/// The first page's exact request target (declaration order of the query keys).
const FIRST_TARGET: &str = "/repos/octo-org/hello-world/issues?state=open&labels=bug&sort=updated&direction=desc&since=2026-10-01T00%3A00%3A00Z&per_page=2";
/// The page GitHub links to: by repository id, as GitHub's pagination guide shows.
const LINKED_TARGET: &str = "/repositories/1300192/issues?state=open&labels=bug&sort=updated&direction=desc&since=2026-10-01T00%3A00%3A00Z&per_page=2&page=2";

fn two_pages(request: &Recorded) -> Reply {
    let repo = "octo-org/hello-world";
    match request.path() {
        "/repos/octo-org/hello-world/issues" => gh_json(
            page(&[
                issue_json(repo, 11, "Crash on empty input", false),
                issue_json(repo, 12, "Fix empty input crash", true),
            ]),
            "REQ-1",
        )
        .header(
            "link",
            format!(
                r#"<http://{host}{LINKED_TARGET}>; rel="next", <http://{host}/repositories/1300192/issues?page=2>; rel="last""#,
                host = request.host
            ),
        ),
        "/repositories/1300192/issues" => gh_json(
            page(&[issue_json(repo, 13, "Timeout on large upload", false)]),
            "REQ-2",
        ),
        _ => Reply::json(404, r#"{"message":"Not Found"}"#),
    }
}

fn query(max_pages: usize) -> BugQuery {
    BugQuery {
        label: "bug".to_owned(),
        updated_since: Some(Timestamp::parse("2026-10-01T00:00:00Z").unwrap()),
        per_page: PerPage::new(2).unwrap(),
        max_pages: NonZeroUsize::new(max_pages).unwrap(),
    }
}

#[tokio::test]
async fn digest_follows_links_and_skips_pull_requests() {
    let server = TestServer::start(two_pages).await;
    let repo = repo(&server, TOKEN_A, "hello-world");

    let digest = open_bug_digest(&repo, &query(5)).await.unwrap();

    let numbers: Vec<u64> = digest.bugs.iter().map(|bug| bug.number.get()).collect();
    assert_eq!(numbers, [11, 13]);
    assert_eq!(digest.bugs[0].title, "Crash on empty input");
    assert_eq!(digest.bugs[0].comments, 11);
    assert_eq!(digest.bugs[0].assignees[0].as_str(), "maintainer");
    assert_eq!(digest.bugs[1].updated_at.as_str(), "2026-10-23T09:00:00Z");
    assert_eq!(digest.pull_requests_skipped, 1);
    assert_eq!(digest.pages, 2);
    assert_eq!(digest.request_ids, ["REQ-1", "REQ-2"]);
    assert_eq!(digest.rate_remaining, Some(4990));
    assert_eq!(digest.outcome, Outcome::Exhausted);

    let wire = server.requests();
    assert_eq!(wire.len(), 2);
    assert_eq!(wire[0].method, "GET");
    assert_eq!(wire[0].target, FIRST_TARGET);
    assert_eq!(wire[1].target, LINKED_TARGET);
    for request in &wire {
        assert_api_headers(request, TOKEN_A);
    }
    server.shutdown().await;
}

#[tokio::test]
async fn digest_is_bounded_by_the_page_budget() {
    let server = TestServer::start(two_pages).await;
    let repo = repo(&server, TOKEN_A, "hello-world");

    let digest = open_bug_digest(&repo, &query(1)).await.unwrap();

    assert_eq!(digest.bugs.len(), 1);
    assert_eq!(digest.pages, 1);
    assert_eq!(digest.outcome, Outcome::Bounded);
    assert_eq!(server.single().target, FIRST_TARGET);
    server.shutdown().await;
}

/// The generated setters, in a second repository with a second token: neither the scope nor the
/// credentials of the first context leak into it.
#[tokio::test]
async fn newest_bug_through_generated_setters_in_two_contexts() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/repos/octo-org/other-repo/issues" => {
            let repo = "octo-org/other-repo";
            gh_json(
                page(&[
                    issue_json(repo, 31, "Add retry to uploader", true),
                    issue_json(repo, 30, "Uploader drops last byte", false),
                ]),
                "REQ-B",
            )
        }
        "/repos/octo-org/hello-world/issues" => gh_json(
            page(&[issue_json("octo-org/hello-world", 12, "A pull", true)]),
            "REQ-A",
        ),
        _ => Reply::json(404, r#"{"message":"Not Found"}"#),
    })
    .await;
    let first = repo(&server, TOKEN_A, "hello-world");
    let other = repo(&server, TOKEN_B, "other-repo");

    let newest = newest_bug(&other, "bug").await.unwrap().unwrap();
    let none = newest_bug(&first, "needs-triage").await.unwrap();

    assert_eq!(newest.number, IssueNumber(30));
    assert_eq!(newest.title, "Uploader drops last byte");
    assert_eq!(none, None, "only a pull request carried the label");
    let wire = server.requests();
    assert_eq!(wire.len(), 2);
    assert_eq!(
        wire[0].target,
        "/repos/octo-org/other-repo/issues?state=open&labels=bug&sort=created&direction=desc&per_page=10"
    );
    assert_api_headers(&wire[0], TOKEN_B);
    assert_eq!(
        wire[1].target,
        "/repos/octo-org/hello-world/issues?state=open&labels=needs-triage&sort=created&direction=desc&per_page=10"
    );
    assert_api_headers(&wire[1], TOKEN_A);
    server.shutdown().await;
}

/// A rate-limited page is a typed error the application can act on; its rendering carries
/// neither the token nor a URL.
#[tokio::test]
async fn rate_limited_page_is_a_typed_error() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(403, r#"{"message":"API rate limit exceeded"}"#)
            .header("x-ratelimit-limit", "5000")
            .header("x-ratelimit-remaining", "0")
            .header("x-ratelimit-reset", "1798000000")
    })
    .await;
    let repo = repo(&server, TOKEN_A, "hello-world");

    let error = open_bug_digest(&repo, &query(5)).await.unwrap_err();

    let DigestError::GitHub(github) = &error else {
        panic!("expected a GitHub error, got {error:?}");
    };
    assert!(github.is_rate_limited());
    assert_eq!(github.status().map(|s| s.as_u16()), Some(403));
    for rendering in [error.to_string(), format!("{error:?}")] {
        assert!(!rendering.contains(TOKEN_A), "{rendering}");
        assert!(!rendering.contains("since="), "{rendering}");
    }
    assert_eq!(server.single().target, FIRST_TARGET);
    server.shutdown().await;
}

/// A label the API cannot express is refused before anything is sent.
#[tokio::test]
async fn unusable_label_sends_nothing() {
    let server = TestServer::start(two_pages).await;
    let repo = repo(&server, TOKEN_A, "hello-world");
    let mut bad = query(5);
    bad.label = "bug,ui".to_owned();

    assert!(matches!(
        open_bug_digest(&repo, &bad).await,
        Err(DigestError::Query(_))
    ));
    assert!(server.requests().is_empty());
    server.shutdown().await;
}
