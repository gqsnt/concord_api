"""Adapt the previous independent probes; preserve the originals."""
from pathlib import Path
base = Path(__file__).resolve().parent
old = base.parent/'probes/tests'
dest = base/'probes/tests'
s = (old/'github.rs').read_text()
s = s.replace('use concord_example_github_sdk::ids::IssueNumber;', '''use concord_example_github_sdk::ids::IssueNumber;
use concord_example_github_sdk::paging::Binding;
fn stored(binding: &Binding, url: impl std::fmt::Display) -> String {
    let parent = binding.parent().map_or_else(|| "-".into(), |p| p.to_string());
    format!("github-link/1 {} {} {parent} {url}", binding.operation(), binding.repo())
}''')
s = s.replace('pages.restore(Url::parse(&format!("{}{path}", server.url())).unwrap())', 'pages.restore(&stored(pages.binding(), Url::parse(&format!("{}{path}", server.url())).unwrap()))')
s = s.replace('pages.restore(Url::parse(&foreign).unwrap())', 'pages.restore(&stored(pages.binding(), Url::parse(&foreign).unwrap()))')
# The stored binding is authoritative for repository identity; URL names may be aliases.
for text in [
    '/api/v3/repos/other/hello-world/issues/11/comments?page=2',
    '/api/v3/repos/octo-org/other/issues/11/comments?page=2',
    '/api/v3/repos/other/repo/issues/11/comments?path=/repos/octo-org/hello-world/issues/11/comments',
    '/api/v3/repos/other/repo/issues/11/comments#/repos/octo-org/hello-world/issues/11/comments',
    '/api/v3/repos/%6fcto-org/hello-world/issues/11/comments?page=2',
    '/api/v3/repos/octo-org/%68ello-world/issues/11/comments?page=2',
]:
    s = s.replace(f'("{text}", false)', f'("{text}", true)')
start = s.index('#[tokio::test]\nasync fn numeric_repo_link')
end = s.index('#[tokio::test]\nasync fn renamed_repository', start)
s = s[:start] + '''#[tokio::test]
async fn numeric_repo_link_preserves_binding_across_persistence() {
    let server = TestServer::start(|r: &Recorded| {
        let reply = Reply::json(200, "[]");
        if r.path() == "/repos/org/repo-a/issues/11/comments" {
            reply.header("link", format!("<http://{}/repositories/101/issues/11/comments?page=2>; rel=\\"next\\"", r.host))
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

''' + s[end:]
s = s.replace('renamed_repository_link_follows_in_memory_but_cannot_restore', 'renamed_repository_link_round_trips_through_original_factory')
s = s.replace('assert!(matches!(pages.restore(key.raw_url().clone()), Err(GitHubError::ContinuationMismatch {..})));', '''let restored = pages.restore(&key.to_stored()).unwrap();
    assert_eq!(restored, key);
    assert_eq!(restored.binding(), pages.binding());
    pages.fetch(Some(restored)).await.unwrap();''')
s = s.replace('same-origin rename redirect: returned new-name Link works in memory; raw_url -> restore fails closed through old-name factory', 'SOL-R3: same-origin rename redirect; provider new-name Link round-trips through old-name factory and fetches')
s = s.replace('changing_fragment_bypasses_nonadvancing_wire_request_check', 'changing_fragment_is_rejected_on_first_repeated_http_request')
s = s.replace('while let Some(p) = chain.next().await { p.unwrap(); }\n    assert_eq!(chain.finish().outcome, Outcome::Bounded);', '''let first = chain.next().await.unwrap().unwrap();
    assert_eq!(first.next.as_ref().unwrap().raw_url().fragment(), None);
    assert!(!first.next.as_ref().unwrap().to_stored().contains('#'));
    assert!(matches!(chain.next().await.unwrap(), Err(GitHubError::LinkDidNotAdvance {..})));
    assert!(chain.next().await.is_none());
    assert_eq!(chain.finish().outcome, Outcome::Failed);''')
s = s.replace('assert_eq!(targets[1..], ["/repositories/101/issues/11/comments?page=2"; 4]);', 'assert_eq!(targets, ["/repos/org/repo/issues/11/comments", "/repositories/101/issues/11/comments?page=2"]);')
s = s.replace('changing fragments: 5 successes, Outcome::Bounded, identical wire requests after first:', 'SOL-R1: changing fragments removed, error at request 2, Outcome::Failed:')
(dest/'github.rs').write_text(s)
s = (old/'shopify.rs').read_text().replace('../../../examples/', '../../../../examples/')
(dest/'shopify.rs').write_text(s)
