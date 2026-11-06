//! Every declared operation on the wire: request line, query, service headers, and typed decoding
//! of the provider's official example for that operation. Plus context isolation, conditional
//! requests, status errors with metadata, the extension boundary and redaction.

mod support;

use std::time::Duration;

use concord::ErrorKind;
use concord::reqwest::{Method, StatusCode};
use concord_example_github_sdk::filters::{IssueFilter, RunFilter};
use concord_example_github_sdk::ids::{
    ArtifactId, CommentId, IssueNumber, JobId, PullNumber, ReviewId, RunId, WorkflowRef,
};
use concord_example_github_sdk::meta::{
    Conditional, send_conditional, send_edited, send_with_meta,
};
use concord_example_github_sdk::params::{
    ApiVersion, CommentSort, Direction, ETag, IssueSort, JobFilter, Labels, PerPage, PullSort,
    RunStatus, StateFilter, Timestamp,
};
use concord_example_github_sdk::{GitHubError, RepoRef};
use concord_example_testserver::{Recorded, Reply, TestServer};
use support::*;

const OWNER_REPO: &str = "/repos/octo-org/hello-world";

fn route(request: &Recorded) -> Reply {
    let path = request.path();
    let Some(rest) = path.strip_prefix(OWNER_REPO) else {
        return gh_json(404, r#"{"message":"Not Found","status":"404"}"#);
    };
    let body = match rest {
        "" => official!("full-repository-default-response.json"),
        "/labels" => official!("label-items.json"),
        "/issues" => official!("issue-items.json"),
        "/issues/1347" => official!("issue.json"),
        "/issues/1347/comments" | "/issues/comments" => official!("issue-comment-items.json"),
        "/issues/comments/1" => official!("issue-comment.json"),
        "/pulls" => official!("pull-request-simple-items.json"),
        "/pulls/1347" => official!("pull-request.json"),
        "/pulls/1347/reviews" => official!("pull-request-review-items.json"),
        "/pulls/1347/reviews/80" => official!("pull-request-review-4.json"),
        "/pulls/1347/files" => official!("diff-entry-items.json"),
        "/actions/workflows" => official!("workflow-paginated.json"),
        "/actions/runs" | "/actions/workflows/ci.yml/runs" => {
            official!("workflow-run-paginated.json")
        }
        "/actions/runs/30433642" => official!("workflow-run.json"),
        "/actions/runs/30433642/jobs" => official!("job-paginated.json"),
        "/actions/jobs/399444496" => official!("job.json"),
        "/actions/artifacts" | "/actions/runs/30433642/artifacts" => {
            official!("artifact-paginated.json")
        }
        "/actions/artifacts/11" => official!("artifact.json"),
        _ => return gh_json(404, r#"{"message":"Not Found","status":"404"}"#),
    };
    gh_json(200, body)
}

/// All 22 JSON operations, each sent once, with its exact request target asserted. (The 23rd,
/// the artifact download, is covered by `tests/download.rs`.)
#[tokio::test]
async fn every_operation_on_the_wire() {
    let server = TestServer::start(route).await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let h = repo.handle();
    let a = repo.actions();
    let per_page = PerPage::new(50).unwrap();
    let since = Timestamp::parse("2026-01-02T03:04:05Z").unwrap();

    let r = h.get().send().await.unwrap();
    assert_eq!(r.full_name, "octocat/Hello-World");
    assert_eq!(
        h.labels()
            .set_per_page(per_page)
            .send()
            .await
            .unwrap()
            .len(),
        2
    );
    let filter = IssueFilter {
        state: Some(StateFilter::All),
        labels: Some(Labels::new(["bug", "help wanted"]).unwrap()),
        since: Some(since.clone()),
        sort: Some(IssueSort::Updated),
        direction: Some(Direction::Asc),
        assignee: Some("*".into()),
        issue_type: Some("Bug".into()),
        creator: Some("octocat".into()),
        mentioned: Some("hubot".into()),
        milestone: Some("none".into()),
        per_page: Some(per_page),
    };
    assert_eq!(repo.issues(&filter).send().await.unwrap().len(), 1);
    assert!(
        h.issue(IssueNumber(1347))
            .send()
            .await
            .unwrap()
            .is_pull_request()
    );
    h.issue_comments(IssueNumber(1347))
        .set_since(since.clone())
        .send()
        .await
        .unwrap();
    h.repo_comments()
        .set_sort(CommentSort::Updated)
        .set_direction(Direction::Desc)
        .send()
        .await
        .unwrap();
    h.comment(CommentId(1)).send().await.unwrap();
    h.pulls()
        .set_state(StateFilter::Closed)
        .set_base("main")
        .set_head("octo-org:feature")
        .set_sort(PullSort::LongRunning)
        .send()
        .await
        .unwrap();
    h.pull(PullNumber(1347)).send().await.unwrap();
    h.pull_reviews(PullNumber(1347)).send().await.unwrap();
    h.pull_review(PullNumber(1347), ReviewId(80))
        .send()
        .await
        .unwrap();
    h.pull_files(PullNumber(1347)).send().await.unwrap();
    a.workflows().send().await.unwrap();
    let run_filter = RunFilter {
        branch: Some("main".into()),
        event: Some("push".into()),
        status: Some(RunStatus::Success),
        exclude_pull_requests: Some(true),
        ..RunFilter::default()
    };
    repo.runs(&run_filter).send().await.unwrap();
    a.workflow_runs(WorkflowRef::File("ci.yml".into()))
        .set_status(RunStatus::Completed)
        .send()
        .await
        .unwrap();
    a.run(RunId(30433642)).send().await.unwrap();
    a.run_jobs(RunId(30433642))
        .set_filter(JobFilter::All)
        .send()
        .await
        .unwrap();
    a.job(JobId(399444496)).send().await.unwrap();
    a.artifacts().set_name("Rails").send().await.unwrap();
    a.run_artifacts(RunId(30433642))
        .set_direction(Direction::Asc)
        .send()
        .await
        .unwrap();
    a.artifact(ArtifactId(11)).send().await.unwrap();

    let requests = server.requests();
    let targets: Vec<&str> = requests.iter().map(|r| r.target.as_str()).collect();
    let p = OWNER_REPO;
    assert_eq!(
        targets,
        [
            p.to_owned(),
            format!("{p}/labels?per_page=50"),
            format!(
                "{p}/issues?milestone=none&state=all&assignee=*&type=Bug&creator=octocat&mentioned=hubot\
                 &labels=bug%2Chelp+wanted&sort=updated&direction=asc&since=2026-01-02T03%3A04%3A05Z\
                 &per_page=50"
            ),
            format!("{p}/issues/1347"),
            format!("{p}/issues/1347/comments?since=2026-01-02T03%3A04%3A05Z"),
            format!("{p}/issues/comments?sort=updated&direction=desc"),
            format!("{p}/issues/comments/1"),
            format!("{p}/pulls?state=closed&head=octo-org%3Afeature&base=main&sort=long-running"),
            format!("{p}/pulls/1347"),
            format!("{p}/pulls/1347/reviews"),
            format!("{p}/pulls/1347/reviews/80"),
            format!("{p}/pulls/1347/files"),
            format!("{p}/actions/workflows"),
            format!(
                "{p}/actions/runs?branch=main&event=push&status=success&exclude_pull_requests=true"
            ),
            format!("{p}/actions/workflows/ci.yml/runs?status=completed"),
            format!("{p}/actions/runs/30433642"),
            format!("{p}/actions/runs/30433642/jobs?filter=all"),
            format!("{p}/actions/jobs/399444496"),
            format!("{p}/actions/artifacts?name=Rails"),
            format!("{p}/actions/runs/30433642/artifacts?direction=asc"),
            format!("{p}/actions/artifacts/11"),
        ]
    );
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert_api_headers(request, TOKEN_A);
        assert!(request.header("if-none-match").is_none());
    }
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

/// Two GitHub contexts (two tokens) and two repositories, interleaved: each request carries its
/// own token and repository path; changing one context's API version or user agent does not
/// change the other or handles created earlier.
#[tokio::test]
async fn contexts_and_repositories_are_isolated() {
    let server = TestServer::start(|r: &Recorded| {
        gh_json(200, official!("issue-items.json")).header("x-seen-path", r.path().to_owned())
    })
    .await;
    let github_a = context(server.addr(), TOKEN_A);
    let github_b = context(server.addr(), TOKEN_B);
    let a1 = github_a.repo(RepoRef::new("octo-org", "hello-world"));
    let b2 = github_b.repo(RepoRef::new("other-org", "second-repo"));
    let early = a1.handle().issues(); // created before the version change below
    let github_a_next = github_a
        .clone()
        .set_api_version(ApiVersion::new("2022-11-28").unwrap())
        .set_user_agent("audit-bot");
    let a1_next = github_a_next.repo(RepoRef::new("octo-org", "hello-world"));

    a1.handle().issues().send().await.unwrap();
    b2.handle().issues().send().await.unwrap();
    early.send().await.unwrap();
    a1_next.handle().issues().send().await.unwrap();
    b2.handle().issues().send().await.unwrap();

    let requests = server.requests();
    let seen: Vec<(&str, &str, &str, &str)> = requests
        .iter()
        .map(|r| {
            (
                r.path(),
                r.header("authorization").unwrap(),
                r.header("x-github-api-version").unwrap(),
                r.header("user-agent").unwrap(),
            )
        })
        .collect();
    let bearer_a = format!("Bearer {TOKEN_A}");
    let bearer_b = format!("Bearer {TOKEN_B}");
    let ua = "concord-example-github-sdk";
    assert_eq!(
        seen,
        [
            (
                "/repos/octo-org/hello-world/issues",
                bearer_a.as_str(),
                "2026-03-10",
                ua
            ),
            (
                "/repos/other-org/second-repo/issues",
                bearer_b.as_str(),
                "2026-03-10",
                ua
            ),
            (
                "/repos/octo-org/hello-world/issues",
                bearer_a.as_str(),
                "2026-03-10",
                ua
            ),
            (
                "/repos/octo-org/hello-world/issues",
                bearer_a.as_str(),
                "2022-11-28",
                "audit-bot"
            ),
            (
                "/repos/other-org/second-repo/issues",
                bearer_b.as_str(),
                "2026-03-10",
                ua
            ),
        ]
    );
    server.shutdown().await;
}

/// `If-None-Match` from a stored entity tag; `304` is `NotModified` (no body decoded), a changed
/// resource is `Modified` with its new tag. The metadata (request id, rate limit) is kept either
/// way.
#[tokio::test]
async fn conditional_refresh_with_etag() {
    let server = TestServer::start(|r: &Recorded| match r.header("if-none-match") {
        Some("W/\"repo-v1\"") => Reply::new(304)
            .header("etag", "W/\"repo-v1\"")
            .header("x-github-request-id", "AAAA:304")
            .header("x-ratelimit-remaining", "4990"),
        _ => gh_json(200, official!("full-repository-default-response.json"))
            .header("etag", "W/\"repo-v2\""),
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let first = send_with_meta(repo.handle().get().prepare().unwrap())
        .await
        .unwrap();
    assert_eq!(first.meta.etag(), Some(&ETag::new("W/\"repo-v2\"")));

    let same = repo
        .handle()
        .get()
        .set_if_none_match(ETag::new("W/\"repo-v1\""))
        .prepare()
        .unwrap();
    match send_conditional(same).await.unwrap() {
        Conditional::NotModified { meta } => {
            assert_eq!(meta.status(), StatusCode::NOT_MODIFIED);
            assert_eq!(meta.request_id(), Some("AAAA:304"));
            assert_eq!(meta.rate_limit().remaining, Some(4990));
        }
        other => panic!("expected 304, got {other:?}"),
    }
    let stale = repo
        .handle()
        .get()
        .set_if_none_match(ETag::new("W/\"repo-v0\""))
        .prepare()
        .unwrap();
    match send_conditional(stale).await.unwrap() {
        Conditional::Modified(with) => {
            assert_eq!(with.value.full_name, "octocat/Hello-World");
            assert_eq!(with.meta.etag(), Some(&ETag::new("W/\"repo-v2\"")));
        }
        other => panic!("expected 200, got {other:?}"),
    }
    // Plain `send()` treats 304 as what Concord says it is: a non-2xx status error.
    let error = repo
        .handle()
        .get()
        .set_if_none_match(ETag::new("W/\"repo-v1\""))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(StatusCode::NOT_MODIFIED));

    let wire = server.requests();
    assert_eq!(wire[0].header("if-none-match"), None);
    assert_eq!(wire[1].header("if-none-match"), Some("W/\"repo-v1\""));
    assert_eq!(wire[2].header("if-none-match"), Some("W/\"repo-v0\""));
    server.shutdown().await;
}

/// A status error keeps Concord's unread response and the captured metadata; GitHub's rate-limit
/// signals are recognized; nothing is retried.
#[tokio::test]
async fn status_errors_keep_response_and_metadata() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path().ends_with("/issues/404") {
            gh_json(404, r#"{"message":"Not Found","documentation_url":"https://docs.github.com/rest","status":"404"}"#)
        } else {
            Reply::json(403, r#"{"message":"API rate limit exceeded","status":"403"}"#)
                .header("x-ratelimit-remaining", "0")
                .header("x-ratelimit-reset", "1798000000")
                .header("x-github-request-id", "RATE:LIMITED")
        }
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let error = send_with_meta(repo.handle().issue(IssueNumber(404)).prepare().unwrap())
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(StatusCode::NOT_FOUND));
    assert!(!error.is_rate_limited());
    assert_eq!(
        error.meta().unwrap().request_id(),
        Some("0000:1111:2222:3333:4444")
    );
    assert_eq!(
        error.to_string(),
        "GET GitHub.repo.issue: HTTP status 404 Not Found"
    );
    let response = error.into_concord().unwrap().into_response().unwrap();
    let document = response.text().await.unwrap();
    assert!(document.contains("Not Found"));

    let limited = send_with_meta(repo.handle().issue(IssueNumber(1)).prepare().unwrap())
        .await
        .unwrap_err();
    assert!(limited.is_rate_limited());
    assert_eq!(limited.meta().unwrap().rate_limit().reset, Some(1798000000));
    assert_eq!(server.requests().len(), 2, "no retry");
    server.shutdown().await;
}

/// On every send path, the metadata of a response that arrived survives a refusal of its head
/// (`Metadata`) or of its body (`Decode`); a transport failure has none, since no head arrived.
/// A refused edit sends nothing.
#[tokio::test]
async fn metadata_is_kept_after_the_head_and_absent_without_a_response() {
    let server = TestServer::start(|r: &Recorded| {
        let path = r.path();
        if path.ends_with("/issues/1") {
            gh_json(200, r#"{"number":"not a number"}"#).header("etag", "W/\"undecodable\"")
        } else if path.ends_with("/issues/2") {
            Reply::text(200, "text/html", "<html></html>").header("x-github-request-id", "HTML:2")
        } else {
            Reply::hang_up()
        }
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let issue = |n| repo.handle().issue(IssueNumber(n)).prepare().unwrap();

    let decode = send_with_meta(issue(1)).await.unwrap_err();
    assert_eq!(decode.concord().unwrap().kind(), ErrorKind::Decode);
    let meta = decode.meta().unwrap();
    assert_eq!(meta.status(), StatusCode::OK);
    assert_eq!(meta.request_id(), Some("0000:1111:2222:3333:4444"));
    assert_eq!(meta.rate_limit().remaining, Some(4990));
    assert_eq!(meta.etag(), Some(&ETag::new("W/\"undecodable\"")));

    let conditional = send_conditional(issue(1)).await.unwrap_err();
    assert_eq!(conditional.concord().unwrap().kind(), ErrorKind::Decode);
    assert_eq!(
        conditional.meta().unwrap().etag(),
        Some(&ETag::new("W/\"undecodable\""))
    );

    // The edited request goes where the edit says; its response's head is refused unread.
    let head = send_edited(issue(3), |request| {
        request
            .url_mut()
            .set_path("/repos/octo-org/hello-world/issues/2");
        Ok(())
    })
    .await
    .unwrap_err();
    assert_eq!(head.concord().unwrap().kind(), ErrorKind::Metadata);
    assert_eq!(head.meta().unwrap().request_id(), Some("HTML:2"));
    assert!(head.concord().unwrap().response().is_some());

    let lost = [
        send_with_meta(issue(3)).await.map(drop).unwrap_err(),
        send_conditional(issue(3)).await.map(drop).unwrap_err(),
        send_edited(issue(3), |_| Ok(()))
            .await
            .map(drop)
            .unwrap_err(),
    ];
    for error in &lost {
        let concord = error.concord().unwrap();
        assert_eq!(concord.kind(), ErrorKind::Transport);
        assert_eq!(concord.endpoint(), Some("GitHub.repo.issue"));
        assert!(error.meta().is_none(), "{error:?}");
        assert_hidden(&format!("{error} {error:?}"), &[TOKEN_A, API_HOST]);
    }

    let sent = server.requests().len();
    let refused = send_edited(issue(1), |_| Err(GitHubError::MalformedStoredLink))
        .await
        .unwrap_err();
    assert!(matches!(refused, GitHubError::MalformedStoredLink));
    assert_eq!(
        server.requests().len(),
        sent,
        "a refused edit sends nothing"
    );
    server.shutdown().await;
}

/// The extension boundary: a native request built by the context carries the same service
/// headers as a generated endpoint, the token flagged sensitive, and is executed once by a
/// Concord decoder (`Decoder::execute`): head first, then the declared handling.
#[tokio::test]
async fn undeclared_operation_through_the_context() {
    let server =
        TestServer::start(|_: &Recorded| gh_json(200, r#"[{"id":1,"event":"closed"}]"#)).await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let declared = repo.handle().labels().prepare().unwrap();
    let native = repo.request(Method::GET, &["issues", "events"]).unwrap();
    for name in [
        "authorization",
        "accept",
        "x-github-api-version",
        "user-agent",
    ] {
        assert_eq!(
            declared.request().headers().get(name),
            native.headers().get(name),
            "{name}"
        );
    }
    assert!(native.headers()["authorization"].is_sensitive());

    #[derive(serde::Deserialize)]
    struct Event {
        id: u64,
        event: String,
    }
    let received = concord::Decoder::<Vec<Event>>::new::<concord::Json>()
        .execute(repo.context().http(), native)
        .await
        .unwrap();
    assert_eq!(
        received.head().headers()["x-github-request-id"],
        "0000:1111:2222:3333:4444"
    );
    let events = received.decode().await.unwrap();
    assert_eq!((events[0].id, events[0].event.as_str()), (1, "closed"));
    let wire = server.single();
    assert_eq!(wire.target, "/repos/octo-org/hello-world/issues/events");
    assert_api_headers(&wire, TOKEN_A);

    assert_eq!(
        repo.request(Method::GET, &[".."]).unwrap_err().to_string(),
        "GitHub context: request not built: path segment 0 is empty, `.` or `..`"
    );
    server.shutdown().await;
}

/// The `type` filter of `issues/list-for-repo`: absent unless set (so a request built without it
/// is byte-identical to one built before the filter existed), sent as `type`, in the position
/// GitHub documents it (after `assignee`), form-encoded like every other query value.
#[test]
fn issue_type_filter_on_the_wire() {
    let github = context("127.0.0.1:9".parse().unwrap(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let query = |filter: IssueFilter| {
        let prepared = repo.issues(&filter).prepare().unwrap();
        prepared.request().url().query().map(str::to_owned)
    };
    let state = || IssueFilter {
        state: Some(StateFilter::Open),
        assignee: Some("*".into()),
        ..IssueFilter::default()
    };
    assert_eq!(query(state()).as_deref(), Some("state=open&assignee=*"));
    for (value, wire) in [
        ("Bug", "type=Bug"),
        ("Feature request", "type=Feature+request"),
        ("*", "type=*"),
        ("none", "type=none"),
    ] {
        let filter = IssueFilter {
            issue_type: Some(value.into()),
            ..state()
        };
        assert_eq!(
            query(filter).as_deref(),
            Some(format!("state=open&assignee=*&{wire}").as_str())
        );
    }
    let generated = repo
        .handle()
        .issues()
        .set_issue_type("Bug")
        .prepare()
        .unwrap();
    assert_eq!(generated.request().url().query(), Some("type=Bug"));
}

/// Path values are one segment each; a dot segment is a build error before anything is sent.
#[tokio::test]
async fn path_values_are_single_segments() {
    let github = context("127.0.0.1:9".parse().unwrap(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo org", "a/b"));
    let prepared = repo.handle().get().prepare().unwrap();
    assert_eq!(prepared.request().url().path(), "/repos/octo%20org/a%2Fb");
    let dot = github.repo(RepoRef::new("octo-org", ".."));
    let error = dot.handle().get().prepare().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
}

/// Validated parameters refuse values GitHub would reject or silently reinterpret.
#[test]
fn parameters_are_validated() {
    assert!(PerPage::new(0).is_err() && PerPage::new(101).is_err());
    assert_eq!(PerPage::new(100).unwrap(), PerPage::MAX);
    assert!(Timestamp::parse("2026-01-02T03:04:05Z").is_ok());
    for bad in [
        "2026-01-02",
        "2026-13-02T03:04:05Z",
        "2026-01-02T24:00:00Z",
        "2026-01-02T03:04:05+01:00",
        "2026-01-02T03:04:05.123Z",
    ] {
        assert!(Timestamp::parse(bad).is_err(), "{bad}");
    }
    assert!(Labels::new(Vec::<String>::new()).is_err());
    assert!(Labels::new(["a,b"]).is_err());
    assert!(ApiVersion::new("2026-3-10").is_err());
}

/// No rendering of a context, a handle, a prepared request or an error contains the token.
#[tokio::test]
async fn token_never_rendered() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::new(200)
            .header("content-type", "text/html")
            .body("<html></html>")
    })
    .await;
    let github = context(server.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let prepared = repo.handle().get().prepare().unwrap();
    let error: GitHubError = send_with_meta(repo.handle().get().prepare().unwrap())
        .await
        .unwrap_err();
    assert_eq!(error.concord().unwrap().kind(), ErrorKind::Metadata);
    let renderings = [
        format!("{github:?}"),
        format!("{repo:?}"),
        format!("{:?}", repo.handle()),
        format!("{:?}", github.client()),
        format!("{prepared:?}"),
        format!("{error}"),
        format!("{error:?}"),
    ];
    for rendering in &renderings {
        assert_hidden(rendering, &[TOKEN_A]);
    }
    assert!(renderings[0].contains("<redacted>"));
    let _ = tokio::time::timeout(Duration::from_secs(5), server.shutdown()).await;
}
