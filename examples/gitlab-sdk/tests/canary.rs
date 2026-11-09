//! Secrets: PrivateToken / GitLabConfig.token -> PRIVATE-TOKEN header,
//! and NextPage's URL -> continuation request query.
//! Single-request coverage: issue listing covers 2xx, documented message-shaped 4xx/5xx,
//! malformed JSON, wrong media, truncated body and refused connection, including native sends.
//! Excludes native reqwest, explicit unredacted accessors, and provider messages/models,
//! which the SDK explicitly documents as printed/returned in clear. Response header values
//! echo the token to test response metadata redaction, except X-Request-Id: `ResponseMeta`
//! documents that it prints the request id (`src/meta.rs`), and a server echoing the caller's
//! secret into a field documented as printed is outside I2 (it measures what the SDK prints of
//! the caller's inputs and of fields it promises to withhold). No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Reply, Script, TestServer};
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

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response.header("set-cookie", echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error),
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_gitlab::{GitLab, GitLabConfig, GitLabError, PrivateToken, ProjectRef};

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("gitlab");
    let secret = c.issue();
    for case in 0..7 {
        let echo = secret.clone();
        let server = TestServer::start(move |_| {
            reply(case, "[]", r#"{"message":"401 Unauthorized"}"#, &echo)
        })
        .await;
        let token = PrivateToken::new(&secret).unwrap();
        c.assert_debug_absent("PrivateToken", &token);
        let config = GitLabConfig {
            token: token.clone().into_header_value(),
        };
        c.assert_debug_absent("config", &config);
        let api = GitLab::with_token(http(), token)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let scope = api.project(ProjectRef::id(1).unwrap());
        let op = scope.issues();
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("scope", &scope);
        c.assert_debug_absent("endpoint", &op);
        let prepared = op.clone().prepare().unwrap();
        c.assert_debug_absent("Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(error) = &native {
            concord_error(&mut c, error);
        }
        let result = op.send().await;
        c.assert_debug_absent("SDK Result", &result);
        match result {
            Ok(response) => {
                assert_eq!(case, 0);
                c.assert_debug_absent("response", &response);
                c.assert_debug_absent("metadata", &response.meta);
                c.assert_on_wire(&server.requests());
            }
            Err(error) => {
                assert_ne!(case, 0);
                c.assert_error_absent("SDK error and sources", &error);
                if let Some(meta) = error.meta() {
                    c.assert_debug_absent("error metadata", meta);
                }
                match &error {
                    GitLabError::Request { error, .. } => concord_error(&mut c, error),
                    GitLabError::Api(api) => {
                        c.assert_debug_absent("ApiError", api);
                        c.assert_display_absent("ApiError", api);
                        c.assert_debug_absent("ErrorBody", &api.body);
                    }
                    _ => {}
                }
            }
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 100);
    println!(
        "gitlab: {} renderings; positive control passed",
        c.checked()
    );
}

#[tokio::test]
async fn continuation_debug_withholds_the_link_in_both_modes() {
    use concord_sdk_gitlab::{GitLab, PrivateToken, ProjectRef};

    let mut c = Canaries::new("gitlab-next-page");
    let cursor = c.issue();
    let script = Script::new([]);
    let responder = script.clone();
    let server = TestServer::start(move |_| responder.next()).await;
    let next_url = format!("{}/projects/1/issues?page=2&cursor={cursor}", server.url());
    script.push(Reply::json(200, "[]").header("link", format!("<{next_url}>; rel=\"next\"")));
    script.push(Reply::json(200, "[]"));
    let api = GitLab::with_token(
        http(),
        PrivateToken::new("synthetic-fixture-token").unwrap(),
    )
    .with_base_url(server.url())
    .unwrap();
    let mut pages = api.project(ProjectRef::id(1).unwrap()).issues().pages();
    let first = pages.next_page().await.unwrap().expect("first page");
    let next = first.next.as_ref().expect("continuation from Link header");
    assert_eq!(next.raw_url().as_str(), next_url);
    c.assert_debug_absent("NextPage", next);

    let last = pages.next_page().await.unwrap().expect("linked page");
    assert!(last.next.is_none());
    assert_eq!(server.requests().len(), 2);
    c.assert_on_wire(&server.requests());
    server.shutdown().await;
}
