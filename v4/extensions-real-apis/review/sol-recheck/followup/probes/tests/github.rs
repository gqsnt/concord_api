use concord::reqwest::{Client, Url};
use concord_example_github_sdk::{GitHubContext, GitHubConfig, GitHubError, RepoRef};
use concord_example_github_sdk::ids::IssueNumber;
use concord_example_github_sdk::paging::Binding;
fn stored(binding: &Binding, url: impl std::fmt::Display) -> String {
    let parent = binding.parent().map_or_else(|| "-".into(), |p| p.to_string());
    format!("github-link/1 {} {} {parent} {url}", binding.operation(), binding.repo())
}
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
        ("/api/v3/repos/other/hello-world/issues/11/comments?page=2", true),
        ("/api/v3/repos/octo-org/other/issues/11/comments?page=2", true),
        ("/api/v3/repositories/101/issues/12/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/12/comments?page=2", false),
        ("/api/v3/repositories/101/pulls/11/reviews?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments/1?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments/?page=2", false),
        ("/api/v30/repos/octo-org/hello-world/issues/11/comments?page=2", false),
        ("/api/v4/repos/octo-org/hello-world/issues/11/comments?page=2", false),
        ("/api/v3-other/repos/octo-org/hello-world/issues/11/comments?page=2", false),
        ("/api/v3/repos/other/repo/issues/11/comments?path=/repos/octo-org/hello-world/issues/11/comments", true),
        ("/api/v3/repos/other/repo/issues/11/comments#/repos/octo-org/hello-world/issues/11/comments", true),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2&path=/repos/other/repo", true),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2#/repos/other/repo", true),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments%2F..%2F..%2F12/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/11/comments/%2e%2e/../12/comments?page=2", false),
        ("/api/v3/repos/octo-org/hello-world/issues/12/comments/../../../11/comments?page=2", false),
        ("/api/v3/repos/%6fcto-org/hello-world/issues/11/comments?page=2", true),
        ("/api/v3/repos/octo-org/%68ello-world/issues/11/comments?page=2", true),
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
        let r = pages.restore(&stored(pages.binding(), Url::parse(&format!("{}{path}", server.url())).unwrap()));
        println!("restore {} {path}", if r.is_ok() { "ACCEPT" } else { "REFUSE" });
        assert_eq!(r.is_ok(), accept, "{path}: {r:?}");
    }
    assert!(server.requests().is_empty(), "restore never sends");
    for foreign in [
        format!("http://elsewhere.test/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2"),
        format!("https://{}/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2", server.addr()),
        format!("http://127.0.0.1:1/api/v3/repos/octo-org/hello-world/issues/11/comments?page=2"),
    ] {
        assert!(matches!(pages.restore(&stored(pages.binding(), Url::parse(&foreign).unwrap())), Err(GitHubError::ForeignLink {..})));
    }
    for path in ["/api/v3/repos/OCTO-ORG/HELLO-WORLD/issues/11/comments?page=2", "/api/v3/repositories/101/issues/11/comments?page=2"] {
        let key = pages.restore(&stored(pages.binding(), Url::parse(&format!("{}{path}", server.url())).unwrap())).unwrap();
        pages.fetch(Some(key)).await.unwrap();
    }
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn numeric_repo_link_preserves_binding_across_persistence() {
    let server = TestServer::start(|r: &Recorded| {
        let reply = Reply::json(200, "[]");
        if r.path() == "/repos/org/repo-a/issues/11/comments" {
            reply.header("link", format!("<http://{}/repositories/101/issues/11/comments?page=2>; rel=\"next\"", r.host))
        } else { reply }
    }).await;
    let github = context(&server, "");
    let a = github.repo(RepoRef::new("org", "repo-a"));
    let b = github.repo(RepoRef::new("org", "repo-b"));
    let pa = a.issue_comment_pages(IssueNumber(11), |op| op);
    let pb = b.issue_comment_pages(IssueNumber(11), |op| op);
    let key = pa.fetch(None).await.unwrap().next.unwrap();
    let text = key.to_stored();
    assert_eq!(text, format!("github-link/1 issues/list-comments org/repo-a 11 {}/repositories/101/issues/11/comments?page=2", server.url()));
    assert!(matches!(pb.fetch(Some(key.clone())).await, Err(GitHubError::ContinuationMismatch {..})));
    assert!(matches!(pb.restore(&text), Err(GitHubError::ContinuationMismatch { expected, found }) if *expected == *pb.binding() && *found == *pa.binding()));
    assert_eq!(server.requests().len(), 1, "neither mismatch sends a request");
    let restored = pa.restore(&text).unwrap();
    assert_eq!(restored, key);
    pa.fetch(Some(restored)).await.unwrap();
    assert_eq!(server.requests()[1].target, "/repositories/101/issues/11/comments?page=2");
    println!("OPUS-1: A numeric link round-trips through A; B rejects stored A binding; no request on mismatch");
    server.shutdown().await;
}

#[tokio::test]
async fn renamed_repository_link_round_trips_through_original_factory() {
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
    let restored = pages.restore(&key.to_stored()).unwrap();
    assert_eq!(restored, key);
    assert_eq!(restored.binding(), pages.binding());
    pages.fetch(Some(restored)).await.unwrap();
    pages.fetch(Some(key)).await.unwrap();
    println!("SOL-R3: same-origin rename redirect; provider new-name Link round-trips through old-name factory and fetches");
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
async fn changing_fragment_is_rejected_on_first_repeated_http_request() {
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
    let first = chain.next().await.unwrap().unwrap();
    assert_eq!(first.next.as_ref().unwrap().raw_url().fragment(), None);
    assert!(!first.next.as_ref().unwrap().to_stored().contains('#'));
    assert!(matches!(chain.next().await.unwrap(), Err(GitHubError::LinkDidNotAdvance {..})));
    assert!(chain.next().await.is_none());
    assert_eq!(chain.finish().outcome, Outcome::Failed);
    let targets: Vec<_> = server.requests().iter().map(|r| r.target.clone()).collect();
    assert_eq!(targets, ["/repos/org/repo/issues/11/comments", "/repositories/101/issues/11/comments?page=2"]);
    println!("SOL-R1: changing fragments removed, error at request 2, Outcome::Failed: {targets:?}");
    server.shutdown().await;
}

#[tokio::test]
async fn literal_fragment_only_targets_and_restored_fragments_do_not_advance() {
    // A fragment-only relative reference resolves against the response's complete URL.
    for target in ["#v1", "#", ""] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(200,"[]").header("link",format!("<{target}>; rel=\"next\""))).await;
        let github = context(&server, "");
        let repo = github.repo(RepoRef::new("org","repo"));
        let pages = repo.issue_comment_pages(IssueNumber(11),|op|op);
        assert!(matches!(pages.fetch(None).await,Err(GitHubError::LinkDidNotAdvance {..})),"{target:?}");
        assert_eq!(server.requests().len(),1);
        println!("fragment-only target {target:?}: LinkDidNotAdvance on first request");
        server.shutdown().await;
    }
    for target in ["#new", "?page=2&cursor=a%23b#new"] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(200,"[]").header("link",format!("<{target}>; rel=\"next\""))).await;
        let github = context(&server, "");
        let repo = github.repo(RepoRef::new("org","repo"));
        let pages = repo.issue_comment_pages(IssueNumber(11),|op|op);
        let text=stored(pages.binding(),format!("{}/repositories/101/issues/11/comments?page=2&cursor=a%23b#stored",server.url()));
        let key=pages.restore(&text).unwrap();
        assert_eq!(key.raw_url().fragment(),None);
        assert_eq!(key.raw_url().query(),Some("page=2&cursor=a%23b"));
        let error=pages.fetch(Some(key)).await.unwrap_err();
        assert!(matches!(error,GitHubError::LinkDidNotAdvance {..}));
        assert!(error.meta().is_some());
        assert_eq!(server.requests()[0].target,"/repositories/101/issues/11/comments?page=2&cursor=a%23b");
        println!("restored #stored stripped, escaped %23 kept; {target:?} fails on first continued request with response metadata");
        server.shutdown().await;
    }
}
