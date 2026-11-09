//! The artifact download on the wire: GitHub's `302` read at the native boundary, the signed URL
//! fetched from a second host with no GitHub header, `410`, client-followed redirects, unsafe
//! locations, mid-body transport failure, sink refusal, byte limits.

mod support;

use std::time::Duration;

use concord::bytes::Bytes;
use concord::reqwest::{StatusCode, redirect};
use concord_example_github_sdk::download::{
    ByteSink, DownloadLimits, LocateError, Located, StreamError, locate, stream_to,
};
use concord_example_github_sdk::ids::ArtifactId;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef};
use concord_example_testserver::{Body, Recorded, Reply, TestServer, generated_records};
use support::*;

const SIGNED_QUERY: &str = "sig=SYNTHETIC_SIGNATURE_abc123&se=2026-11-04T00%3A01%3A00Z";

/// A sink that keeps the bytes (tests only; the application's sinks are bounded) and can refuse
/// after a number of bytes.
#[derive(Default)]
struct Collect {
    bytes: Vec<u8>,
    refuse_after: Option<usize>,
    writes: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct Refused;

impl ByteSink for Collect {
    type Error = Refused;
    async fn write(&mut self, chunk: Bytes) -> Result<(), Refused> {
        self.writes += 1;
        if let Some(limit) = self.refuse_after
            && self.bytes.len() + chunk.len() > limit
        {
            return Err(Refused);
        }
        self.bytes.extend_from_slice(&chunk);
        Ok(())
    }
}

/// The API answers the download endpoint with `302` to the download host.
async fn api_redirecting(download_port: u16) -> TestServer {
    TestServer::start(move |r: &Recorded| {
        assert!(
            r.path().ends_with("/actions/artifacts/11/zip"),
            "{}",
            r.target
        );
        Reply::new(302)
            .header(
                "location",
                format!("http://{DOWNLOAD_HOST}:{download_port}/artifacts/11.zip?{SIGNED_QUERY}"),
            )
            .header("x-github-request-id", "DL:302")
            .header("x-ratelimit-remaining", "4321")
    })
    .await
}

fn github(api: &TestServer, download: &TestServer, policy: redirect::Policy) -> GitHubContext {
    GitHubContext::new(
        http_with(api.addr(), Some(download.addr()), policy),
        GitHubConfig {
            origin: concord::BaseUrl::from_static("https://api.github.com"),
            token: TOKEN_A.into(),
        },
    )
    .with_base_url(api_base(api.addr()))
    .unwrap()
}

fn archive_bytes(n: usize) -> Vec<u8> {
    // Deterministic bytes (seed 7): position-dependent so reordering would be detected.
    (0..n).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

/// The full path: `302`, then the signed URL streamed chunk by chunk into the sink. The download
/// host sees no `Authorization`, no `X-GitHub-Api-Version`, no `User-Agent`; the API request had
/// all of them.
#[tokio::test]
async fn redirect_then_stream_without_the_token() {
    let bytes = archive_bytes(40_000);
    let chunks: Vec<(Duration, Bytes)> = bytes
        .chunks(7_000)
        .map(|c| (Duration::ZERO, Bytes::copy_from_slice(c)))
        .collect();
    let download = TestServer::start(move |_: &Recorded| {
        Reply::new(200)
            .header("content-type", "application/zip")
            .with_body(Body::Chunks(chunks.clone()))
    })
    .await;
    let api = api_redirecting(download.addr().port()).await;
    let github = github(
        &api,
        &download,
        concord_example_github_sdk::redirect_policy(),
    );
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));

    let Located::Redirect(location) = locate(&repo, ArtifactId(11)).await.unwrap() else {
        panic!("expected a redirect")
    };
    assert_eq!(location.host(), DOWNLOAD_HOST);
    assert_eq!(location.meta().status(), StatusCode::FOUND);
    assert_eq!(location.meta().request_id(), Some("DL:302"));
    assert_eq!(location.meta().rate_limit().remaining, Some(4321));
    assert_hidden(&format!("{location:?}"), &["SYNTHETIC_SIGNATURE", TOKEN_A]);

    let mut sink = Collect::default();
    let receipt = stream_to(
        github.http(),
        &location,
        &mut sink,
        DownloadLimits { max_bytes: 1 << 20 },
    )
    .await
    .unwrap();
    assert_eq!(receipt.bytes, 40_000);
    assert!(receipt.chunks >= 2, "streamed in several chunks");
    assert_eq!(receipt.content_type.as_deref(), Some("application/zip"));
    assert_eq!(sink.bytes, bytes);

    let api_request = api.single();
    assert_api_headers(&api_request, TOKEN_A);
    let download_request = download.single();
    assert_eq!(
        download_request.target,
        format!("/artifacts/11.zip?{SIGNED_QUERY}")
    );
    for header in [
        "authorization",
        "x-github-api-version",
        "user-agent",
        "cookie",
    ] {
        assert!(
            download_request.header(header).is_none(),
            "{header} forwarded"
        );
    }
    assert!(download_request.host.starts_with(DOWNLOAD_HOST));
    api.shutdown().await;
    download.shutdown().await;
}

/// `410 Gone` (an expired artifact) is an outcome, not a decoding attempt; the download host is
/// never contacted.
#[tokio::test]
async fn gone_artifact() {
    let download = TestServer::start(|_: &Recorded| Reply::new(200)).await;
    let api = TestServer::start(|_: &Recorded| {
        gh_json(410, r#"{"message":"Artifact has expired","status":"410"}"#)
    })
    .await;
    let github = github(
        &api,
        &download,
        concord_example_github_sdk::redirect_policy(),
    );
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    match locate(&repo, ArtifactId(11)).await.unwrap() {
        Located::Gone { meta } => assert_eq!(meta.status(), StatusCode::GONE),
        other => panic!("expected Gone, got {other:?}"),
    }
    assert_eq!(download.requests().len(), 0);
    api.shutdown().await;
    download.shutdown().await;
}

/// With reqwest's default policy the client follows the redirect itself: reqwest strips
/// `Authorization` across hosts but forwards GitHub's other headers. The SDK refuses that
/// response instead of streaming it.
#[tokio::test]
async fn client_followed_redirect_is_refused() {
    let download = TestServer::start(|_: &Recorded| Reply::new(200).body("PK")).await;
    let api = api_redirecting(download.addr().port()).await;
    let github = github(&api, &download, redirect::Policy::default());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let error = locate(&repo, ArtifactId(11)).await.unwrap_err();
    assert!(
        matches!(error, LocateError::RedirectFollowed { .. }),
        "{error:?}"
    );
    // What reqwest's own policy sent (observed, for the record): no token, but the API headers.
    let followed = download.single();
    assert!(followed.header("authorization").is_none());
    assert_eq!(followed.header("x-github-api-version"), Some("2026-03-10"));
    api.shutdown().await;
    download.shutdown().await;
}

/// `redirect::Policy::none()` works as well as the SDK's policy.
#[tokio::test]
async fn no_redirect_policy_also_works() {
    let download = TestServer::start(|_: &Recorded| Reply::new(200).body("PK\x03\x04")).await;
    let api = api_redirecting(download.addr().port()).await;
    let github = github(&api, &download, redirect::Policy::none());
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let Located::Redirect(location) = locate(&repo, ArtifactId(11)).await.unwrap() else {
        panic!("expected a redirect")
    };
    let mut sink = Collect::default();
    let receipt = stream_to(
        github.http(),
        &location,
        &mut sink,
        DownloadLimits { max_bytes: 10 },
    )
    .await
    .unwrap();
    assert_eq!(receipt.bytes, 4);
    api.shutdown().await;
    download.shutdown().await;
}

/// Unusable redirects: no `Location`, user information in it, or a non-redirect status.
#[tokio::test]
async fn unusable_redirects() {
    let api = TestServer::start(|r: &Recorded| match r.path() {
        p if p.ends_with("/11/zip") => Reply::new(302),
        p if p.ends_with("/12/zip") => Reply::new(302).header(
            "location",
            "http://user:pass@objects.githubusercontent.test/x",
        ),
        p if p.ends_with("/13/zip") => gh_json(404, r#"{"message":"Not Found","status":"404"}"#),
        _ => Reply::new(200),
    })
    .await;
    let github = context(api.addr(), TOKEN_A);
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    assert!(matches!(
        locate(&repo, ArtifactId(11)).await,
        Err(LocateError::MissingLocation { .. })
    ));
    let unsafe_location = locate(&repo, ArtifactId(12)).await.unwrap_err();
    assert!(matches!(
        unsafe_location,
        LocateError::UnsafeLocation { .. }
    ));
    assert_hidden(&format!("{unsafe_location} {unsafe_location:?}"), &["pass"]);
    match locate(&repo, ArtifactId(13)).await.unwrap_err() {
        LocateError::Api(error) => {
            assert_eq!(error.status(), Some(StatusCode::NOT_FOUND));
            assert_eq!(
                error.to_string(),
                "GET GitHub.repo.actions.download_artifact: HTTP status 404 Not Found"
            );
        }
        other => panic!("expected a status error, got {other:?}"),
    }
    assert!(matches!(
        locate(&repo, ArtifactId(14)).await,
        Err(LocateError::NoRedirect { .. })
    ));
    api.shutdown().await;
}

/// A connection dropped mid-body: the error reports the bytes already received and written.
#[tokio::test]
async fn transport_failure_mid_body() {
    let download = TestServer::start(|_: &Recorded| {
        Reply::new(200).with_body(Body::FailAfter(vec![
            Bytes::from_static(b"PK\x03\x04first-part"),
            Bytes::from_static(b"second-part"),
        ]))
    })
    .await;
    let api = api_redirecting(download.addr().port()).await;
    let github = github(
        &api,
        &download,
        concord_example_github_sdk::redirect_policy(),
    );
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let Located::Redirect(location) = locate(&repo, ArtifactId(11)).await.unwrap() else {
        panic!()
    };
    let mut sink = Collect::default();
    let error = stream_to(
        github.http(),
        &location,
        &mut sink,
        DownloadLimits { max_bytes: 1 << 20 },
    )
    .await
    .unwrap_err();
    match &error {
        StreamError::Read { received, .. } => {
            assert_eq!(*received, sink.bytes.len() as u64);
            assert!(*received > 0, "a prefix arrived before the failure");
        }
        other => panic!("expected a read error, got {other:?}"),
    }
    assert_hidden(
        &format!("{error} {error:?}"),
        &["SYNTHETIC_SIGNATURE", DOWNLOAD_HOST],
    );
    api.shutdown().await;
    download.shutdown().await;
}

/// The sink refuses (cancels) after a prefix of a large generated body: the transfer stops, the
/// response is dropped, the server observes the connection closing long before the body ends.
#[tokio::test]
async fn sink_refusal_stops_the_transfer() {
    const TOTAL: u64 = 64 * 1024 * 1024; // generated on demand, never materialized
    let download = TestServer::start(|_: &Recorded| {
        Reply::new(200).with_body(Body::Generate(
            generated_records(TOTAL / 1024, 16 * 1024, |i| vec![(i % 251) as u8; 1024]),
            None,
        ))
    })
    .await;
    let api = api_redirecting(download.addr().port()).await;
    let github = github(
        &api,
        &download,
        concord_example_github_sdk::redirect_policy(),
    );
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let Located::Redirect(location) = locate(&repo, ArtifactId(11)).await.unwrap() else {
        panic!()
    };
    let mut sink = Collect {
        refuse_after: Some(100_000),
        ..Collect::default()
    };
    let error = stream_to(
        github.http(),
        &location,
        &mut sink,
        DownloadLimits { max_bytes: TOTAL },
    )
    .await
    .unwrap_err();
    let written = match error {
        StreamError::Sink { written, error } => {
            assert_eq!(error, Refused);
            written
        }
        other => panic!("expected a sink error, got {other:?}"),
    };
    assert_eq!(written, sink.bytes.len() as u64);
    assert!(written <= 100_000);
    let writes_at_stop = sink.writes;
    // The response was dropped: the server's connection ends and the body is not sent to the end.
    assert!(
        download
            .wait_until(Duration::from_secs(5), |s| s.active_bodies() == 0)
            .await
    );
    assert!(
        download.body_bytes_sent() < TOTAL,
        "the body was not sent to its end"
    );
    assert_eq!(sink.writes, writes_at_stop, "no write after the refusal");
    api.shutdown().await;
    download.shutdown().await;
}

/// Byte limits: a declared length over the limit is refused before reading; a chunked body that
/// grows past it is cut off without writing the crossing chunk.
#[tokio::test]
async fn byte_limits() {
    let download = TestServer::start(|r: &Recorded| {
        if r.path().contains("declared") {
            Reply::new(200).body(vec![0u8; 5_000])
        } else {
            Reply::new(200).with_body(Body::Chunks(vec![
                (Duration::ZERO, Bytes::from(vec![1u8; 3_000])),
                (Duration::ZERO, Bytes::from(vec![2u8; 3_000])),
            ]))
        }
    })
    .await;
    let port = download.addr().port();
    let api = TestServer::start(move |r: &Recorded| {
        let name = if r.path().contains("/11/") {
            "declared"
        } else {
            "chunked"
        };
        Reply::new(302).header("location", format!("http://{DOWNLOAD_HOST}:{port}/{name}"))
    })
    .await;
    let github = github(
        &api,
        &download,
        concord_example_github_sdk::redirect_policy(),
    );
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let limits = DownloadLimits { max_bytes: 4_000 };

    let Located::Redirect(declared) = locate(&repo, ArtifactId(11)).await.unwrap() else {
        panic!()
    };
    let mut sink = Collect::default();
    match stream_to(github.http(), &declared, &mut sink, limits).await {
        Err(StreamError::TooLarge {
            declared: Some(5_000),
            written: 0,
            limit: 4_000,
        }) => {}
        other => panic!("expected TooLarge before reading, got {other:?}"),
    }
    assert_eq!(sink.writes, 0);

    let Located::Redirect(chunked) = locate(&repo, ArtifactId(12)).await.unwrap() else {
        panic!()
    };
    let mut sink = Collect::default();
    match stream_to(github.http(), &chunked, &mut sink, limits).await {
        Err(StreamError::TooLarge {
            declared: None,
            written: 3_000,
            ..
        }) => {}
        other => panic!("expected TooLarge while streaming, got {other:?}"),
    }
    assert_eq!(sink.bytes.len(), 3_000);
    api.shutdown().await;
    download.shutdown().await;
}

/// The download host refuses the signed URL (it expired after GitHub's one minute): a status
/// error with the response unread, no endpoint name, no URL in the rendering.
#[tokio::test]
async fn expired_signed_url() {
    let download = TestServer::start(|_: &Recorded| {
        Reply::text(
            403,
            "application/xml",
            "<Error><Code>AuthenticationFailed</Code></Error>",
        )
    })
    .await;
    let api = api_redirecting(download.addr().port()).await;
    let github = github(
        &api,
        &download,
        concord_example_github_sdk::redirect_policy(),
    );
    let repo = github.repo(RepoRef::new("octo-org", "hello-world"));
    let Located::Redirect(location) = locate(&repo, ArtifactId(11)).await.unwrap() else {
        panic!()
    };
    let mut sink = Collect::default();
    let error = stream_to(
        github.http(),
        &location,
        &mut sink,
        DownloadLimits { max_bytes: 100 },
    )
    .await
    .unwrap_err();
    let StreamError::Request(error) = error else {
        panic!("expected a request error")
    };
    assert_eq!(error.status(), Some(StatusCode::FORBIDDEN));
    assert_eq!(error.endpoint(), None);
    assert_hidden(&format!("{error} {error:?}"), &["SYNTHETIC_SIGNATURE"]);
    let document = error.into_response().unwrap().text().await.unwrap();
    assert!(document.contains("AuthenticationFailed"));
    api.shutdown().await;
    download.shutdown().await;
}
