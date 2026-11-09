//! Repository identity and native extension paths read the same generated scope facts.
use concord::reqwest::{self, Method};
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef};

#[test]
fn repository_view_and_native_requests_read_scope_facts() {
    let context = GitHubContext::new(
        reqwest::Client::new(),
        GitHubConfig {
            origin: concord::BaseUrl::parse("https://git.example.test/api/v3").unwrap(),
            token: "synthetic-construction-token".into(),
        },
    );
    let repo = context.repo(RepoRef::new("octo-org", "hello-world"));
    assert_eq!(repo.repo(), RepoRef::new("octo-org", "hello-world"));
    let mut detached = repo.repo();
    detached.name = concord_example_github_sdk::ids::RepoName::new("another-repo");
    assert_eq!(repo.repo(), RepoRef::new("octo-org", "hello-world"));
    let native = repo.request(Method::GET, &["issues"]).unwrap();
    let generated = repo.handle().issues().prepare().unwrap();
    assert_eq!(native.url(), generated.request().url());
    assert_eq!(native.headers(), generated.request().headers());
    assert!(format!("{repo:?}").contains("octo-org/hello-world"));
    assert!(!format!("{repo:?}").contains("synthetic-construction-token"));
}
