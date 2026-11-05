//! The updated consumer against a loopback GitHub stand-in (stage 3): the `type` filter on the
//! wire, the extension operation `issues/list-events` with the same service headers as the
//! declared operations, its `Link` pages, its failures and its renderings.
//!
//! Fixtures are `schema_derived_synthetic`: built here with the fields of the pinned `issue`
//! schema the SDK reads (plus the response's `type` object, which the SDK ignores) and the
//! fields every issue-event variant has (REST API `2026-03-10`, docs retrieved 2026-11-05).

use std::num::NonZeroUsize;

use concord::reqwest;
use concord_check_maintenance_github::after::{
    TrailError, TypedBugQuery, newest_of_type, typed_bug_trails,
};
use concord_check_maintenance_github::issue_events::IssueEventPages;
use concord_example_github_sdk::ids::IssueNumber;
use concord_example_github_sdk::params::{PerPage, Timestamp};
use concord_example_github_sdk::{GitHubConfig, GitHubContext, GitHubError, RepoContext, RepoRef};
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
            token: token.to_owned(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
    .repo(RepoRef::new("octo-org", name))
}

/// One issue with the `issue` schema's fields the SDK reads and an issue `type` object; `pull`
/// adds the Issues API's `pull_request` object.
fn issue_json(repo: &str, number: u64, title: &str, issue_type: &str, pull: bool) -> String {
    let pull_request = if pull {
        format!(
            r#","pull_request":{{"url":"https://api.github.com/repos/{repo}/pulls/{number}","html_url":"https://github.com/{repo}/pull/{number}","diff_url":"https://github.com/{repo}/pull/{number}.diff","patch_url":"https://github.com/{repo}/pull/{number}.patch","merged_at":null}}"#
        )
    } else {
        String::new()
    };
    format!(
        r#"{{"id":{id},"node_id":"I_{id}","url":"https://api.github.com/repos/{repo}/issues/{number}","html_url":"https://github.com/{repo}/issues/{number}","number":{number},"state":"open","state_reason":null,"title":"{title}","body":null,"user":{{"login":"reporter","id":7,"type":"User"}},"labels":[],"assignees":[],"milestone":null,"locked":false,"comments":0,"created_at":"2026-10-0{d}T08:00:00Z","updated_at":"2026-10-2{d}T09:00:00Z","closed_at":null,"author_association":"MEMBER","type":{{"id":41,"node_id":"IT_41","name":"{issue_type}","description":null,"color":"red"}}{pull_request}}}"#,
        id = 5000 + number,
        d = number % 10,
    )
}

/// One issue event with the fields every variant has; `label` for label events.
fn event_json(repo: &str, id: u64, event: &str, actor: &str, label: Option<&str>) -> String {
    let label = label
        .map(|name| format!(r#","label":{{"name":"{name}","color":"d73a4a"}}"#))
        .unwrap_or_default();
    format!(
        r#"{{"id":{id},"node_id":"IE_{id}","url":"https://api.github.com/repos/{repo}/issues/events/{id}","actor":{{"login":"{actor}","id":9,"type":"User"}},"event":"{event}","commit_id":null,"commit_url":null,"created_at":"2026-10-1{d}T10:00:00Z","performed_via_github_app":null{label}}}"#,
        d = id % 10,
    )
}

fn array(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

fn gh_json(body: String, request_id: &str) -> Reply {
    Reply::json(200, body)
        .header("x-github-request-id", request_id)
        .header("x-ratelimit-limit", "5000")
        .header("x-ratelimit-remaining", "4990")
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

const ISSUES_TARGET: &str = "/repos/octo-org/hello-world/issues?state=open&type=Bug&sort=updated&direction=desc&since=2026-10-01T00%3A00%3A00Z&per_page=2";
const EVENTS_21: &str = "/repos/octo-org/hello-world/issues/21/events?per_page=100";
const EVENTS_21_LINKED: &str = "/repositories/1300192/issues/21/events?per_page=100&page=2";
const EVENTS_23: &str = "/repos/octo-org/hello-world/issues/23/events?per_page=100";

fn hello_world(request: &Recorded) -> Reply {
    let repo = "octo-org/hello-world";
    match request.path() {
        "/repos/octo-org/hello-world/issues" => gh_json(
            array(&[
                issue_json(repo, 21, "Crash on empty input", "Bug", false),
                issue_json(repo, 22, "Fix empty input crash", "Bug", true),
                issue_json(repo, 23, "Timeout on large upload", "Bug", false),
            ]),
            "REQ-ISSUES",
        ),
        "/repos/octo-org/hello-world/issues/21/events" => gh_json(
            array(&[
                event_json(repo, 901, "labeled", "triager", Some("needs-repro")),
                event_json(repo, 902, "closed", "maintainer", None),
            ]),
            "REQ-EV-21-1",
        )
        .header(
            "link",
            format!(
                r#"<http://{host}{EVENTS_21_LINKED}>; rel="next""#,
                host = request.host
            ),
        ),
        "/repositories/1300192/issues/21/events" => gh_json(
            array(&[event_json(repo, 903, "reopened", "reporter", None)]),
            "REQ-EV-21-2",
        ),
        "/repos/octo-org/hello-world/issues/23/events" => gh_json(array(&[]), "REQ-EV-23"),
        _ => Reply::json(404, r#"{"message":"Not Found"}"#),
    }
}

fn query() -> TypedBugQuery {
    TypedBugQuery {
        issue_type: "Bug".to_owned(),
        updated_since: Some(Timestamp::parse("2026-10-01T00:00:00Z").unwrap()),
        per_page: PerPage::new(2).unwrap(),
        max_pages: NonZeroUsize::new(5).unwrap(),
        event_pages: NonZeroUsize::new(5).unwrap(),
    }
}

/// The `type` filter and the extension operation together, end to end.
#[tokio::test]
async fn typed_bugs_with_their_events() {
    let server = TestServer::start(hello_world).await;
    let repo = repo(&server, TOKEN_A, "hello-world");

    let digest = typed_bug_trails(&repo, &query()).await.unwrap();

    let numbers: Vec<u64> = digest.bugs.iter().map(|t| t.bug.number.get()).collect();
    assert_eq!(numbers, [21, 23]);
    assert_eq!(digest.pull_requests_skipped, 1);
    assert_eq!(digest.issue_pages, 1);
    assert_eq!(digest.request_ids, ["REQ-ISSUES"]);
    assert_eq!(digest.outcome, Outcome::Exhausted);

    let first = &digest.bugs[0];
    let events: Vec<(u64, &str)> = first
        .events
        .iter()
        .map(|e| (e.id.0, e.event.as_str()))
        .collect();
    assert_eq!(
        events,
        [(901, "labeled"), (902, "closed"), (903, "reopened")]
    );
    assert_eq!(first.events[0].label.as_ref().unwrap().name, "needs-repro");
    assert_eq!(first.events[0].created_at.as_str(), "2026-10-11T10:00:00Z");
    assert_eq!(first.last_labeled_by().map(|l| l.as_str()), Some("triager"));
    assert_eq!(first.times_reopened(), 1);
    assert_eq!(first.events_outcome, Outcome::Exhausted);
    assert_eq!(first.event_request_ids, ["REQ-EV-21-1", "REQ-EV-21-2"]);
    assert!(digest.bugs[1].events.is_empty());

    let wire = server.requests();
    let targets: Vec<&str> = wire.iter().map(|r| r.target.as_str()).collect();
    assert_eq!(
        targets,
        [ISSUES_TARGET, EVENTS_21, EVENTS_21_LINKED, EVENTS_23]
    );
    // The extension's requests carry exactly the declared operations' service headers.
    for request in &wire {
        assert_eq!(request.method, "GET");
        assert_api_headers(request, TOKEN_A);
    }
    server.shutdown().await;
}

/// The event page budget bounds each bug's history; the issue listing is unaffected.
#[tokio::test]
async fn event_pages_are_bounded_per_bug() {
    let server = TestServer::start(hello_world).await;
    let repo = repo(&server, TOKEN_A, "hello-world");
    let mut bounded = query();
    bounded.event_pages = NonZeroUsize::new(1).unwrap();

    let digest = typed_bug_trails(&repo, &bounded).await.unwrap();

    assert_eq!(digest.bugs[0].events.len(), 2);
    assert_eq!(digest.bugs[0].events_outcome, Outcome::Bounded);
    assert_eq!(digest.bugs[1].events_outcome, Outcome::Exhausted);
    let targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(targets, [ISSUES_TARGET, EVENTS_21, EVENTS_23]);
    server.shutdown().await;
}

/// A `410` on an event page is the extension's typed error with the page's metadata; its
/// renderings carry neither the token nor a URL.
#[tokio::test]
async fn gone_event_page_keeps_status_and_metadata() {
    let server = TestServer::start(|request: &Recorded| {
        if request.path().ends_with("/events") {
            Reply::json(410, r#"{"message":"Gone"}"#).header("x-github-request-id", "REQ-GONE")
        } else {
            hello_world(request)
        }
    })
    .await;
    let repo = repo(&server, TOKEN_A, "hello-world");

    let error = typed_bug_trails(&repo, &query()).await.unwrap_err();

    let TrailError::Events {
        number,
        error: events,
    } = &error
    else {
        panic!("expected an events error, got {error:?}");
    };
    assert_eq!(*number, IssueNumber(21));
    assert_eq!(events.status().map(|s| s.as_u16()), Some(410));
    assert_eq!(events.meta().and_then(|m| m.request_id()), Some("REQ-GONE"));
    for rendering in [error.to_string(), format!("{error:?}")] {
        assert!(!rendering.contains(TOKEN_A), "{rendering}");
        assert!(!rendering.contains("127.0.0.1"), "{rendering}");
    }
    let targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(targets, [ISSUES_TARGET, EVENTS_21]);
    server.shutdown().await;
}

/// A `rel="next"` outside the API origin is refused and never requested: the token stays on
/// the API origin, as with the SDK's own page factories.
#[tokio::test]
async fn foreign_event_link_is_not_followed() {
    let server = TestServer::start(|request: &Recorded| {
        if request.path() == "/repos/octo-org/hello-world/issues/21/events" {
            gh_json(array(&[]), "REQ-EV").header(
                "link",
                r#"<http://elsewhere.test/repositories/1300192/issues/21/events?page=2>; rel="next""#,
            )
        } else {
            hello_world(request)
        }
    })
    .await;
    let repo = repo(&server, TOKEN_A, "hello-world");

    let error = typed_bug_trails(&repo, &query()).await.unwrap_err();

    assert!(matches!(
        error,
        TrailError::Events {
            error: GitHubError::ForeignLink { meta: Some(_) },
            ..
        }
    ));
    assert!(!format!("{error:?}").contains("elsewhere.test"));
    let targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(targets, [ISSUES_TARGET, EVENTS_21]);
    server.shutdown().await;
}

/// A continuation belongs to its issue and repository.
#[tokio::test]
async fn event_continuation_is_bound_to_its_issue() {
    let server = TestServer::start(hello_world).await;
    let repo = repo(&server, TOKEN_A, "hello-world");
    let pages_21 = IssueEventPages::new(&repo, IssueNumber(21)).per_page(PerPage::MAX);
    let pages_23 = IssueEventPages::new(&repo, IssueNumber(23));

    let first = pages_21.fetch(None).await.unwrap();
    let next = first.next.expect("GitHub linked a second page");
    assert_eq!(next.binding(), pages_21.binding());
    assert!(!format!("{next:?}").contains("1300192"));

    assert!(matches!(
        pages_23.fetch(Some(next)).await,
        Err(GitHubError::ContinuationMismatch { .. })
    ));
    assert_eq!(
        server.requests().len(),
        1,
        "the mismatched continuation sent nothing"
    );
    server.shutdown().await;
}

/// Two contexts (two repositories, two tokens): the generated `set_issue_type` setter and the
/// extension operation each use their own context's scope and credentials.
#[tokio::test]
async fn generated_setter_and_extension_in_two_contexts() {
    let server = TestServer::start(|request: &Recorded| {
        let other = "octo-org/other-repo";
        match request.path() {
            "/repos/octo-org/other-repo/issues" => gh_json(
                array(&[
                    issue_json(other, 41, "Typed pull", "Task", true),
                    issue_json(other, 40, "Write the migration guide", "Task", false),
                ]),
                "REQ-B",
            ),
            "/repos/octo-org/other-repo/issues/40/events" => gh_json(
                array(&[event_json(other, 950, "assigned", "lead", None)]),
                "REQ-B-EV",
            ),
            _ => hello_world(request),
        }
    })
    .await;
    let a = repo(&server, TOKEN_A, "hello-world");
    let b = repo(&server, TOKEN_B, "other-repo");

    let newest = newest_of_type(&b, "*").await.unwrap().unwrap();
    let events_b = IssueEventPages::new(&b, newest.number)
        .fetch(None)
        .await
        .unwrap();
    let events_a = IssueEventPages::new(&a, IssueNumber(23))
        .fetch(None)
        .await
        .unwrap();

    assert_eq!(newest.number, IssueNumber(40));
    assert_eq!(events_b.items[0].event, "assigned");
    assert!(events_a.items.is_empty());
    let wire = server.requests();
    assert_eq!(
        wire[0].target,
        "/repos/octo-org/other-repo/issues?state=open&type=*&sort=created&direction=desc&per_page=10"
    );
    assert_api_headers(&wire[0], TOKEN_B);
    assert_eq!(
        wire[1].target,
        "/repos/octo-org/other-repo/issues/40/events"
    );
    assert_api_headers(&wire[1], TOKEN_B);
    assert_eq!(
        wire[2].target,
        "/repos/octo-org/hello-world/issues/23/events"
    );
    assert_api_headers(&wire[2], TOKEN_A);
    server.shutdown().await;
}
