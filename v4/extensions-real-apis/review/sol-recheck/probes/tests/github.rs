use concord::reqwest::{Client, Url};
use concord_example_github_sdk::{GitHubContext, GitHubConfig, GitHubError, RepoRef};
use concord_example_github_sdk::ids::IssueNumber;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Budget, Resume, Outcome, sequential};
use std::num::NonZeroUsize;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

fn context(server: &TestServer, base: &str) -> GitHubContext {
    GitHubContext::new(Client::builder().no_proxy().build().unwrap(),
        GitHubConfig { token: "synthetic".into() })
        .with_base_url(format!("{}{base}", server.url())).unwrap()
}
fn budget(n: usize) -> Budget { Budget::new(NonZeroUsize::new(n).unwrap()) }

#[tokio::test]
async fn restore_matrix() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, "[]")).await;
    let github = context(&server, "/api/v3");
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let pages = repo.issue_comment_pages(IssueNumber(11), |op| op);
    let cases = [
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2", true),
        ("/api/v3/repos/OCTO-ORG/HELLO-WORLD/issues/11/comments?page=2", true),
        ("/api/v3/repositories/101/issues/11/comments?page=2", true),
        ("/api/v3/repositories/999/issues/11/comments?page=2", true),
        ("/api/v3/repositories/000101/issues/11/comments?page=2", true),
        ("/api/v3/repos/other/hello-world/issues/11/comments?page=2", false),
        ("/api/v3/repos/octo-org/other/issues/11/comments?page=2", false),
        ("/api/v3/repositories/101/issues/12/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/12/comments?page=2", false),
        ("/api/v3/repositories/101/pulls/11/reviews?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments/1?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments/?page=2", false),
        ("/api/v30/repos/octo-org/hello-world/issues/11/comments?page=2", false),
        ("/api/v4/repos/octo-org/hello-world/issues/11/comments?page=2", false),
        ("/api/v3-other/repos/octo-org/hello-world/issues/11/comments?page=2", false),
        ("/api/v3/repos/other/repo/issues/11/comments?path=/repos/octo-org/hello-world/issues/11/comments", false),
        ("/api/v3/repos/other/repo/issues/11/comments#/repos/octo-org/hello-world/issues/11/comments", false),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2&path=/repos/other/repo", true),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2#/repos/other/repo", true),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments%2F..%2F..%2F12/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments/%2e%2e/../12/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/12/comments/../../../11/comments?page=2", false),
        ("/api/v3/repos/%6fcto-org/hello-world/issues/11/comments?page=2", false),
        ("/api/v3/repos/octo-org/%68ello-world/issues/11/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/%31%31/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/%69ssues/11/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/011/comments?page=2", false),
        ("/api/v3/REPOS/octo-org/hello-world/issues/11/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/Issues/11/comments?page=2", false),
        ("/API/v3/repos/octo-org/hello-world/issues/11/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2&after=a%2Fb%3D", true),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments?after=a%2fb%3d", true),
    ];
    for (path, accept) in cases {
        let r = pages.restore(Url::parse(&format!("{}{path}", server.url())).unwrap());
        println!("restore {} {path}", if r.is_ok() { "ACCEPT" } else { "REFUSE" });
        assert_eq!(r.is_ok(), accept, "{path}: {r:?}");
    }
    assert!(server.requests().is_empty(), "restore never sends");
    for foreign in [
        format!("http://elsewhere.test/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2"),
        format!("https://{}/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2", server.addr()),
        format!("http://127.0.0.1:1/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2"),
    ] {
        assert!(matches!(pages.restore(Url::parse(&foreign).unwrap()), Err(GitHubError::ForeignLink {..})));
    }
    for path in ["/api/v3/repos/OCTO-ORG/HELLO-WORLD/issues/11/comments?page=2", "/api/v3/repositories/101/issues/11/comments?page=2"] {
        let key = pages.restore(Url::parse(&format!("{}{path}", server.url())).unwrap()).unwrap();
        pages.fetch(Some(key)).await.unwrap();
    }
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn numeric_repo_link_is_rebound_across_repositories() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, "[]")).await;
    let github = context(&server, "");
    let a = github.repo(RepoRef::new("org", "repo-a"));
    let b = github.repo(RepoRef::new("org", "repo-b"));
    let pa = a.issue_comment_pages(IssueNumber(11), |op| op);
    let pb = b.issue_comment_pages(IssueNumber(11), |op| op);
    // This numeric URL designates repo-a on this fixture server. No mapping is supplied to SDK.
    let raw = Url::parse(&format!("{}/repositories/101/issues/11/comments?page=2", server.url())).unwrap();
    let ka = pa.restore(raw.clone()).unwrap();
    assert!(matches!(pb.fetch(Some(ka)).await, Err(GitHubError::ContinuationMismatch {..})));
    let kb = pb.restore(raw).unwrap();
    assert_eq!(kb.binding().repo(), &RepoRef::new("org", "repo-b"));
    pb.fetch(Some(kb)).await.unwrap();
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.requests()[0].path(), "/repositories/101/issues/11/comments");
    println!("numeric persistence changes binding org/repo-a -> org/repo-b; HTTP GET /repositories/101/issues/11/comments");
    server.shutdown().await;
}

#[tokio::test]
async fn renamed_repository_link_follows_in_memory_but_cannot_restore() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/repos/org/old-name/issues/11/comments" => Reply::new(301).header("location", format!("http://{}/repos/org/new-name/issues/11/comments",r.host)),
        "/repos/org/new-name/issues/11/comments" => {
            if r.query().is_none() {
                Reply::json(200,"[]").header("link",format!("<http://{}/repos/org/new-name/issues/11/comments?page=2>; rel=\"next\"",r.host))
            } else {Reply::json(200,"[]")}
        },
        _ => Reply::new(500)
    }).await;
    let github = context(&server, "");
    let repo = github.repo(RepoRef::new("org", "old-name"));
    let pages = repo.issue_comment_pages(IssueNumber(11), |op| op);
    let key = pages.fetch(None).await.unwrap().next.unwrap();
    assert!(matches!(pages.restore(key.raw_url().clone()), Err(GitHubError::ContinuationMismatch {..})));
    pages.fetch(Some(key)).await.unwrap();
    println!("same-origin rename redirect: returned new-name Link works in memory; raw_url -> restore fails closed through old-name factory");
    server.shutdown().await;
}

#[tokio::test]
async fn progress_compares_current_url_on_third_request() {
    let server = TestServer::start(|r: &Recorded| {
        let page = match r.query() { None => 2, Some("page=2") => 3, Some("page=3") => 3, _ => 99 };
        Reply::json(200, "[]").header("link", format!("<http://{}/repositories/101/issues/11/comments?page={page}>; rel=\"next\"", r.host))
    }).await;
    let github = context(&server, "");
    let repo = github.repo(RepoRef::new("org", "repo"));
    let pages = repo.issue_comment_pages(IssueNumber(11), |op| op);
    let mut chain = sequential(Resume::Start, budget(50), |k| pages.fetch(k));
    assert!(chain.next().await.unwrap().is_ok());
    assert!(chain.next().await.unwrap().is_ok());
    assert!(matches!(chain.next().await.unwrap(), Err(GitHubError::LinkDidNotAdvance { .. })));
    assert!(chain.next().await.is_none());
    assert_eq!(chain.finish().outcome, Outcome::Failed);
    assert_eq!(server.requests().len(), 3);
    println!("GitHub Start -> page=2 -> page=3 -> page=3: error at request 3");
    server.shutdown().await;
}

#[tokio::test]
async fn changing_fragment_bypasses_nonadvancing_wire_request_check() {
    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    let server = TestServer::start(move |r: &Recorded| {
        let fragment = c.fetch_add(1, Ordering::SeqCst);
        Reply::json(200, "[]").header("link", format!("<http://{}/repositories/101/issues/11/comments?page=2#v{fragment}>; rel=\"next\"", r.host))
    }).await;
    let github = context(&server, "");
    let repo = github.repo(RepoRef::new("org", "repo"));
    let pages = repo.issue_comment_pages(IssueNumber(11), |op| op);
    let mut chain = sequential(Resume::Start, budget(5), |k| pages.fetch(k));
    while let Some(p) = chain.next().await { p.unwrap(); }
    assert_eq!(chain.finish().outcome, Outcome::Bounded);
    let targets: Vec<_> = server.requests().iter().map(|r| r.target.clone()).collect();
    assert_eq!(targets[1..], ["/repositories/101/issues/11/comments?page=2"; 4]);
    println!("changing fragments: 5 successes, Outcome::Bounded, identical wire requests after first: {targets:?}");
    server.shutdown().await;
}
