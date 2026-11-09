//! Secrets (one canary each): the `#[sensitive]` bearer token (src/api.rs:53, given through
//! `GitHubConfig` to `GitHubContext::new`, src/context.rs:114), sent in `Authorization`; the
//! signature of the signed artifact URL in GitHub's `302 Location` (a bearer credential withheld
//! by `ArtifactLocation`/`ResponseMeta`, src/download.rs:50, src/meta.rs:148), sent by
//! `stream_to` in the download request's target; the cursor of a `Link rel="next"` URL (withheld
//! by `Links`/`ResponseMeta`/`NextLink`/`GitHubError`, src/link.rs:58, src/meta.rs:19,
//! src/paging.rs:174, src/error.rs:5), sent by the traversal in the second page's target. Both
//! response-only values reach the SDK through server fixtures. The SDK has no writes: the repository
//! read (`send()`, native `Prepared`, `send_with_meta`), the issue traversal's linked page, the
//! artifact `locate` and the signed-URL `stream_to` each see all seven scenarios. Error bodies,
//! the truncated/malformed/HTML bodies and non-printed headers echo the token. Not covered:
//! native reqwest values (`RepoContext::request`), the explicit unredacted accessors
//! (`raw_location`, `raw_url`, `NextLink::to_stored`, `into_response`), decoded models (returned in
//! clear), and `x-github-request-id`/`x-ratelimit-resource`, which `ResponseMeta` prints by design.
//! No wire exemptions: every canary is on the wire in the success case.

use concord::bytes::Bytes;
use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::time::Duration;

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_example_testserver::refused_url()
}

fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(503, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "").with_body(Body::FailAfter(vec![
            format!("{{\"incomplete\":\"{echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error(c: &mut Canaries, e: &concord::Error) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

async fn receive<T: std::fmt::Debug>(c: &mut Canaries, prepared: concord::Prepared<T>) {
    c.assert_debug_absent("Prepared", &prepared);
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode().await.map(|_| ());
            c.assert_debug_absent("native decoded Result", &decoded);
            if let Err(e) = decoded {
                concord_error(c, &e);
            }
        }
        Err(e) => concord_error(c, &e),
    }
}

use concord_example_github_sdk::download::{
    ByteSink, DownloadLimits, LocateError, Located, StreamError, locate, stream_to,
};
use concord_example_github_sdk::filters::IssueFilter;
use concord_example_github_sdk::ids::ArtifactId;
use concord_example_github_sdk::meta::send_with_meta;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, GitHubError, RepoRef};

#[derive(Default)]
struct Collect(Vec<u8>);

impl ByteSink for Collect {
    type Error = std::convert::Infallible;
    async fn write(&mut self, chunk: Bytes) -> Result<(), Self::Error> {
        self.0.extend_from_slice(&chunk);
        Ok(())
    }
}

fn error(c: &mut Canaries, e: &GitHubError) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error", e);
    c.assert_debug_absent("SDK error metadata", &e.meta());
    c.assert_debug_absent("SDK error status", &e.status());
    if let Some(e) = e.concord() {
        concord_error(c, e);
    }
}

fn locate_error(c: &mut Canaries, e: &LocateError) {
    c.assert_debug_absent("locate error", e);
    c.assert_error_absent("locate error", e);
    c.assert_debug_absent("locate error metadata", &e.meta());
    if let LocateError::Api(e) = e {
        error(c, e);
    }
}

fn stream_error<E: 'static>(c: &mut Canaries, e: &StreamError<E>) {
    c.assert_debug_absent("stream error", e);
    c.assert_error_absent("stream error", e);
    match e {
        StreamError::Request(e) => concord_error(c, e),
        StreamError::Read { error, .. } => c.assert_error_absent("stream source error", error),
        _ => {}
    }
}

const REPO: &str = include_str!("../../github-app/tests/fixtures/synthetic/repository-a.json");
const LIMITS: DownloadLimits = DownloadLimits { max_bytes: 1 << 16 };

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("github");
    let token = c.issue();
    let signature = c.issue();
    let cursor = c.issue();
    let config = GitHubConfig {
        origin: concord::BaseUrl::from_static("https://api.github.com"),
        token: token.clone(),
    };
    c.assert_debug_absent("config", &config);
    for case in 0..7 {
        let (echo, sig, cur) = (token.clone(), signature.clone(), cursor.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let origin = format!("http://{}", r.host);
            let path = r.path();
            let problem = format!(
                r#"{{"message":"Bad credentials {echo}","documentation_url":"https://docs.github.com/rest","status":"401"}}"#
            );
            // Artifact 11's locate sees the scenario; artifact 12 always redirects and its
            // signed URL sees the scenario instead.
            if path.ends_with("/artifacts/12/zip") || (case == 0 && path.ends_with("/11/zip")) {
                let id = if path.ends_with("/12/zip") { 12 } else { 11 };
                return Reply::new(302)
                    .header("location", format!("{origin}/signed/{id}.zip?sig={sig}"))
                    .header("x-private", &echo);
            }
            if path.starts_with("/signed/") {
                return match (path, case) {
                    ("/signed/12.zip", 1) => Reply::text(403, "application/xml", &problem),
                    ("/signed/12.zip", 2) => Reply::text(500, "application/xml", &problem),
                    ("/signed/12.zip", 5) => Reply::new(200)
                        .with_body(Body::FailAfter(vec![format!("PK{echo}").into()])),
                    _ => Reply::new(200)
                        .header("content-type", "application/zip")
                        .body("PK\x03\x04"),
                }
                .header("x-private", &echo);
            }
            if path.ends_with("/issues") {
                if r.query().is_some_and(|q| q.contains(&cur)) {
                    return reply(case, "[]", &problem, &echo);
                }
                let next = format!("{origin}/repos/octo-org/hello-world/issues?after={cur}");
                return Reply::json(200, "[]")
                    .header("link", format!("<{next}>; rel=\"next\""))
                    .header("x-private", &echo);
            }
            reply(case, REPO, &problem, &echo)
        })
        .await;
        let github = GitHubContext::new(http(), config.clone())
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
        c.assert_debug_absent("context", &github);
        c.assert_debug_absent("generated client", github.client());
        c.assert_debug_absent("repository context", &repo);
        c.assert_debug_absent("repository handle", repo.handle());
        c.assert_debug_absent("actions handle", repo.actions());
        c.assert_debug_absent("context error", &repo.request(Default::default(), &[".."]));

        // The repository read: native Prepared, generated send(), send_with_meta.
        let read = repo.handle().get();
        c.assert_debug_absent("read endpoint", &read);
        receive(&mut c, read.clone().prepare().unwrap()).await;
        let native = read.clone().send().await.map(|_| ());
        c.assert_debug_absent("generated send Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let result = send_with_meta(read.prepare().unwrap()).await;
        c.assert_debug_absent("send_with_meta Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(with) => c.assert_debug_absent("response metadata", &with.meta),
            Err(e) => {
                error(&mut c, &e);
                if case == 1 {
                    // Withholding keeps the diagnosis: status, operation and metadata.
                    assert_eq!(e.status().unwrap().as_u16(), 401);
                    assert_eq!(e.meta().unwrap().status().as_u16(), 401);
                    assert!(e.to_string().contains("401"), "{e}");
                    assert!(e.concord().unwrap().endpoint().is_some());
                }
            }
        }

        // The issue traversal: the second page follows the Link (cursor canary) and fails.
        let listing = repo.issue_pages(repo.issues(&IssueFilter::default()));
        c.assert_debug_absent("listing", &listing);
        let mut pages = listing.pages();
        c.assert_debug_absent("page traversal", &pages);
        loop {
            let page = pages.next_page().await;
            c.assert_debug_absent("page Result", &page);
            c.assert_debug_absent("traversal checkpoint", &pages.checkpoint());
            match page {
                Ok(Some(page)) => {
                    c.assert_debug_absent("page info", &page.meta);
                    c.assert_debug_absent("next link", &page.next);
                }
                Ok(None) => {
                    assert_eq!(case, 0);
                    break;
                }
                Err(e) => {
                    c.assert_error_absent("PagesError", &e);
                    error(&mut c, e.provider_error().unwrap());
                    assert_eq!(pages.progress().succeeded, u64::from(case != 6));
                    break;
                }
            }
        }
        c.assert_debug_absent("traversal report", &pages.finish());

        // The artifact download: locate (artifact 11), then the signed URL (artifact 12).
        let op = repo.actions().download_artifact(ArtifactId(11));
        c.assert_debug_absent("download endpoint", &op);
        c.assert_debug_absent("download Prepared", &op.prepare().unwrap());
        for id in [11, 12] {
            let located = locate(&repo, ArtifactId(id)).await;
            c.assert_debug_absent("locate Result", &located);
            assert_eq!(located.is_ok(), case == 0 || (id == 12 && case != 6));
            let location = match located {
                Ok(Located::Redirect(location)) => location,
                Ok(other) => panic!("unexpected {other:?}"),
                Err(e) => {
                    locate_error(&mut c, &e);
                    continue;
                }
            };
            c.assert_debug_absent("artifact location", &location);
            c.assert_debug_absent("location metadata", location.meta());
            let streamed =
                stream_to(github.http(), &location, &mut Collect::default(), LIMITS).await;
            c.assert_debug_absent("stream Result", &streamed);
            assert_eq!(streamed.is_ok(), id == 11 || ![1, 2, 5].contains(&case));
            if let Err(e) = &streamed {
                stream_error(&mut c, e);
                if case == 1 {
                    assert!(e.to_string().contains("403"), "{e}");
                }
            }
        }
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }

    // Refused links and continuations carrying the cursor canary are never followed or printed.
    let (cur, sig) = (cursor.clone(), signature.clone());
    let server = TestServer::start(move |r: &Recorded| {
        let origin = format!("http://{}", r.host);
        let link = match r.path() {
            "/repos/o/foreign/issues" => {
                format!("<http://elsewhere.test/issues?after={cur}>; rel=\"next\"")
            }
            "/repos/o/malformed/issues" => {
                format!("<{origin}/repos/o/malformed/issues?after={cur}")
            }
            _ => {
                return Reply::new(302)
                    .header("location", format!("https://{sig}@elsewhere.test/a.zip"));
            }
        };
        Reply::json(200, "[]").header("link", link)
    })
    .await;
    let github = GitHubContext::new(http(), config)
        .with_base_url(server.url())
        .unwrap();
    for name in ["foreign", "malformed"] {
        let repo = github.repo(RepoRef::new("o", name));
        let listing = repo.issue_pages(repo.issues(&IssueFilter::default()));
        let e = listing.pages().next_page().await.unwrap_err();
        c.assert_error_absent("refused link PagesError", &e);
        let refused = e.provider_error().unwrap();
        error(&mut c, refused);
        assert!(matches!(
            (name, refused),
            ("foreign", GitHubError::ForeignLink { .. }) | ("malformed", GitHubError::Link { .. })
        ));
        let origin = server.url();
        for (i, stored) in [
            format!(
                "github-link/1 issues/list-for-repo o/{name} - {origin}/repos/o/{name}/issues?after={cursor}"
            ),
            format!(
                "github-link/1 issues/list-for-repo o/other - {origin}/repos/o/other/issues?after={cursor}"
            ),
            format!(
                "github-link/1 issues/list-for-repo o/{name} - http://elsewhere.test/?after={cursor}"
            ),
            format!("github-link/0 {cursor}"),
        ]
        .into_iter()
        .enumerate()
        {
            let restored = listing.restore(&stored);
            c.assert_debug_absent("restored continuation", &restored);
            assert_eq!(restored.is_ok(), i == 0);
            if let Err(e) = &restored {
                error(&mut c, e);
            }
        }
    }
    let e = locate(&github.repo(RepoRef::new("o", "r")), ArtifactId(13))
        .await
        .unwrap_err();
    assert!(matches!(e, LocateError::UnsafeLocation { .. }));
    locate_error(&mut c, &e);
    server.shutdown().await;

    assert!(c.checked() >= 400);
    println!(
        "github: {} renderings; positive control passed (token, signed URL, Link cursor)",
        c.checked()
    );
}
