//! Shared helpers of the workflow tests: a client that maps GitHub-like host names to loopback
//! servers, fixtures (`tests/fixtures/MANIFEST.json`), synthetic secrets.

#![allow(dead_code)]

use std::net::SocketAddr;

use concord::reqwest;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, redirect_policy};
use concord_example_testserver::{Recorded, Reply};

/// The API host used in tests.
pub const API_HOST: &str = "api.github.test";
/// The artifact download host used in tests.
pub const DOWNLOAD_HOST: &str = "pipelines.actions.githubusercontent.test";
/// Synthetic tokens; a test fails if one appears where it must not.
pub const TOKEN_A: &str = "ghp_SYNTHETIC_TOKEN_A_0000000000000000";
pub const TOKEN_B: &str = "ghp_SYNTHETIC_TOKEN_B_1111111111111111";

/// A synthetic fixture (`tests/fixtures/synthetic/`).
#[macro_export]
macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/synthetic/",
            $name
        ))
    };
}

/// A client resolving the API host (and the download host) to loopback servers, with the SDK's
/// redirect policy.
pub fn http(api: SocketAddr, download: Option<SocketAddr>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(redirect_policy())
        .resolve(API_HOST, api);
    if let Some(download) = download {
        builder = builder.resolve(DOWNLOAD_HOST, download);
    }
    builder.build().expect("client")
}

/// A configured SDK context with `token`, against the API server.
pub fn github(api: SocketAddr, download: Option<SocketAddr>, token: &str) -> GitHubContext {
    GitHubContext::new(
        http(api, download),
        GitHubConfig {
            token: token.to_owned(),
        },
    )
    .with_base_url(format!("http://{API_HOST}:{}", api.port()))
    .expect("base")
}

/// A JSON reply with GitHub's request id and rate-limit headers; `n` makes the request id unique.
pub fn gh_json(body: &str, n: usize) -> Reply {
    Reply::json(200, body)
        .header("x-github-request-id", format!("REQ:{n:04}"))
        .header("x-ratelimit-limit", "5000")
        .header("x-ratelimit-remaining", (4999 - n).to_string())
        .header("x-ratelimit-used", (n + 1).to_string())
        .header("x-ratelimit-reset", "1798000000")
        .header("x-ratelimit-resource", "core")
}

/// GitHub's error document with a status.
pub fn gh_error(status: u16, body: &str) -> Reply {
    Reply::json(status, body).header("x-github-request-id", "REQ:FAIL")
}

/// The `Link` URL base of the recorded request (`http://api.github.test:<port>`).
pub fn origin(request: &Recorded) -> String {
    format!("http://{}", request.host)
}

/// Asserts the headers every API request of a context with `token` carries.
pub fn assert_api_request(request: &Recorded, token: &str) {
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {token}").as_str()),
        "{}",
        request.target
    );
    assert_eq!(
        request.header("accept"),
        Some("application/vnd.github+json")
    );
    assert_eq!(request.header("x-github-api-version"), Some("2026-03-10"));
    assert!(request.header("user-agent").is_some());
}
