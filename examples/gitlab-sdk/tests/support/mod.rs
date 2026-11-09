#![allow(dead_code)] // Shared by two test binaries; each uses part of it.

use std::time::Duration;

use concord::reqwest;
use concord_example_testserver::{Reply, Script, TestServer};
use concord_sdk_gitlab::{GitLab, PrivateToken};

/// Synthetic: not a GitLab token format (no `glpat-` prefix), never valid anywhere.
pub const TOKEN: &str = "synthetic-fixture-token";
/// The project used throughout: a subgroup path, so two `/` are encoded.
pub const PROJECT_PATH: &str = "acme/tools/widgets";
pub const PROJECT_ID: u64 = 4021;
/// `/api/v4/projects/acme%2Ftools%2Fwidgets/issues`, as Concord encodes the path segment.
pub const ISSUES_PATH: &str = "/api/v4/projects/acme%2Ftools%2Fwidgets/issues";

pub const PAGE_1: &[u8] = include_bytes!("../../fixtures/issues-page-1.json");
pub const PAGE_2: &[u8] = include_bytes!("../../fixtures/issues-page-2.json");
pub const EMPTY: &[u8] = include_bytes!("../../fixtures/issues-empty.json");
pub const ISSUE: &[u8] = include_bytes!("../../fixtures/issue.json");
pub const PROJECT: &[u8] = include_bytes!("../../fixtures/project.json");
pub const PROJECT_MISMATCH: &[u8] = include_bytes!("../../fixtures/project-mismatch.json");
pub const UNAUTHORIZED: &[u8] = include_bytes!("../../fixtures/unauthorized.json");
pub const FORBIDDEN: &[u8] = include_bytes!("../../fixtures/forbidden.json");
pub const ISSUE_NOT_FOUND: &[u8] = include_bytes!("../../fixtures/issue-not-found.json");
pub const PROJECT_NOT_FOUND: &[u8] = include_bytes!("../../fixtures/project-not-found.json");
pub const VALIDATION: &[u8] = include_bytes!("../../fixtures/validation.json");
pub const RATE_LIMITED: &[u8] = include_bytes!("../../fixtures/rate-limited.txt");
pub const PROXY_ERROR: &[u8] = include_bytes!("../../fixtures/proxy-error.html");
pub const MALFORMED: &[u8] = include_bytes!("../../fixtures/malformed.json");

/// The transport every test uses: no proxy, the SDK's redirect policy, no reqwest retry, a short
/// timeout.
pub fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(concord_sdk_gitlab::redirect_policy())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

/// The base URL of a test server, as a self-managed instance's: `http://127.0.0.1:<port>/api/v4`.
pub fn base(server: &TestServer) -> String {
    format!("{}/api/v4", server.url())
}

pub fn client(server: &TestServer) -> GitLab {
    concord::BaseUrl::parse((base(server)).as_ref())
        .map(|origin| GitLab::at(http(), origin, PrivateToken::new(TOKEN).unwrap()))
        .unwrap()
}

/// A server answering with `replies` in order, and a client pointed at it.
pub async fn scripted(replies: impl IntoIterator<Item = Reply>) -> (TestServer, GitLab) {
    let script = Script::new(replies);
    let server = TestServer::start(move |_| script.next()).await;
    let client = client(&server);
    (server, client)
}

/// A JSON reply labelled as GitLab labels it.
pub fn json(status: u16, body: &[u8]) -> Reply {
    Reply::new(status)
        .header("content-type", "application/json")
        .body(body)
}

/// The documented offset headers of a page (`x-next-page`/`x-prev-page` are empty when absent),
/// and a request id.
pub fn offset_headers(reply: Reply, page: u64, per_page: u64, total: Option<u64>) -> Reply {
    let total_pages = total.map(|total| total.div_ceil(per_page));
    let next = match total_pages {
        Some(pages) if page < pages => (page + 1).to_string(),
        _ => String::new(),
    };
    let prev = if page > 1 {
        (page - 1).to_string()
    } else {
        String::new()
    };
    let mut reply = reply
        .header("x-page", page.to_string())
        .header("x-per-page", per_page.to_string())
        .header("x-next-page", next)
        .header("x-prev-page", prev)
        .header("x-request-id", format!("req-fixture-page-{page}"));
    if let (Some(total), Some(pages)) = (total, total_pages) {
        reply = reply
            .header("x-total", total.to_string())
            .header("x-total-pages", pages.to_string());
    }
    reply
}

/// A `Link` field value from `(target, rel)` pairs, in the documented form `<url>; rel="x"`.
pub fn link(entries: &[(&str, &str)]) -> String {
    entries
        .iter()
        .map(|(url, rel)| format!("<{url}>; rel=\"{rel}\""))
        .collect::<Vec<_>>()
        .join(", ")
}
