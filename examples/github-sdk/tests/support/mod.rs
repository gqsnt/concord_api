//! Shared helpers of the SDK's wire tests: loopback servers reached under GitHub-like host names,
//! fixtures from the application's fixture directory, synthetic secrets.

#![allow(dead_code)]

use std::net::SocketAddr;

use concord::reqwest;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, redirect_policy};
use concord_example_testserver::{Recorded, Reply};

/// The API host name used in tests (`api.github.test`, mapped to the loopback server).
pub const API_HOST: &str = "api.github.test";
/// The download host name used in tests.
pub const DOWNLOAD_HOST: &str = "objects.githubusercontent.test";
/// A synthetic token; a test fails if it appears in any rendering.
pub const TOKEN_A: &str = "ghp_SYNTHETIC_TOKEN_A_0000000000000000";
/// A second synthetic token for context isolation.
pub const TOKEN_B: &str = "ghp_SYNTHETIC_TOKEN_B_1111111111111111";

/// An official example of the pinned OpenAPI description (see the application's
/// `tests/fixtures/MANIFEST.json`).
#[macro_export]
macro_rules! official {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../github-app/tests/fixtures/official/",
            $name
        ))
    };
}

/// A synthetic fixture of the application's fixture directory.
#[macro_export]
macro_rules! synthetic {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../github-app/tests/fixtures/synthetic/",
            $name
        ))
    };
}

/// A client that resolves the API (and optionally the download) host to loopback servers and
/// uses the SDK's redirect policy.
pub fn http(api: SocketAddr, download: Option<SocketAddr>) -> reqwest::Client {
    http_with(api, download, redirect_policy())
}

/// [`http`] with another redirect policy.
pub fn http_with(
    api: SocketAddr,
    download: Option<SocketAddr>,
    policy: reqwest::redirect::Policy,
) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(policy)
        .resolve(API_HOST, api);
    if let Some(download) = download {
        builder = builder.resolve(DOWNLOAD_HOST, download);
    }
    builder.build().expect("client")
}

/// The base URL of the API server (the port must be in the URL: `resolve` ignores it).
pub fn api_base(api: SocketAddr) -> String {
    format!("http://{API_HOST}:{}", api.port())
}

/// A context against the API server with `token`.
pub fn context(api: SocketAddr, token: &str) -> GitHubContext {
    GitHubContext::new(
        http(api, None),
        GitHubConfig {
            origin: concord::BaseUrl::from_static("https://api.github.com"),
            token: token.to_owned(),
        },
    )
    .with_base_url(api_base(api))
    .expect("base")
}

/// A JSON reply as GitHub sends it, with a request id and rate-limit headers.
pub fn gh_json(status: u16, body: &str) -> Reply {
    Reply::json(status, body)
        .header("x-github-request-id", "0000:1111:2222:3333:4444")
        .header("x-ratelimit-limit", "5000")
        .header("x-ratelimit-remaining", "4990")
        .header("x-ratelimit-used", "10")
        .header("x-ratelimit-reset", "1798000000")
        .header("x-ratelimit-resource", "core")
}

/// Asserts the headers every API request carries.
pub fn assert_api_headers(request: &Recorded, token: &str) {
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {token}").as_str())
    );
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

/// Asserts that no rendering contains any of `secrets`.
pub fn assert_hidden(rendered: &str, secrets: &[&str]) {
    for secret in secrets {
        assert!(
            !rendered.contains(secret),
            "a secret leaked into a rendering: {rendered}"
        );
    }
}

/// A synthetic issue comment with the fields of the pinned `issue-comment` schema the SDK reads
/// (`schema_derived_synthetic`, generated; see the fixture manifest).
pub fn comment_json(id: u64, issue: u64, repo: &str) -> String {
    format!(
        r#"{{"id":{id},"node_id":"IC_{id}","url":"https://api.github.com/repos/{repo}/issues/comments/{id}","html_url":"https://github.com/{repo}/issues/{issue}#issuecomment-{id}","body":"comment {id}","user":{{"login":"octocat","id":1,"type":"User"}},"created_at":"2026-01-0{d}T00:00:00Z","updated_at":"2026-01-0{d}T00:00:00Z","issue_url":"https://api.github.com/repos/{repo}/issues/{issue}","author_association":"MEMBER"}}"#,
        d = 1 + id % 9
    )
}

/// A JSON array of synthetic comments.
pub fn comments_json(ids: &[u64], issue: u64, repo: &str) -> String {
    let items: Vec<String> = ids
        .iter()
        .map(|id| comment_json(*id, issue, repo))
        .collect();
    format!("[{}]", items.join(","))
}
