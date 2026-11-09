//! Secrets: distinct personal API key and OAuth access token -> Authorization header;
//! The API key also exercises the public LinearApiConfig credential constructor.
//! Linear is read-only despite using POST: lookup runs all seven response/transport cases
//! for both auth constructors. Public wire request fields have no public constructor.
//! Excludes native reqwest and *_unredacted accessors. GraphQL errors, raw HTTP bodies,
//! issue fields and cursors are deliberately returned in clear and are not seeded as secrets.
//! Headers echo credentials; no canary is exempted from the wire check.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Reply, TestServer};
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

// Every credential is echoed in headers and in diagnostics whose formatting is withheld.
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
    response
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_linear::{Error, Linear, LinearApi, LinearApiConfig};

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("linear");
    let key = c.issue();
    let token = c.issue();
    // Wire request fields are private and have no public constructor or Deserialize.
    // LinearApi's public config/client can still be checked without manufacturing an endpoint.
    for case in 0..7 {
        let echo = format!("{key} {token}");
        let server = TestServer::start(move |_| reply(case,
            r#"{"data":{"issue":{"id":"one","title":"Title","description":null,"dueDate":null}}}"#,
            r#"{"errors":[{"message":"Unauthorized","extensions":{"code":"UNAUTHENTICATED"}}]}"#, &echo)).await;
        for oauth in [false, true] {
            let api = if oauth {
                Linear::oauth(http(), &token)
            } else {
                Linear::api_key(http(), &key)
            }
            .unwrap()
            .with_base_url(&if case == 6 { refused() } else { server.url() })
            .unwrap();
            c.assert_debug_absent("client", &api);
            let result = api.issue("one").await;
            c.assert_debug_absent("SDK Result", &result);
            match result {
                Ok(response) => {
                    assert_eq!(case, 0);
                    c.assert_debug_absent("response", &response);
                    c.assert_debug_absent("metadata", &response.meta);
                }
                Err(error) => {
                    assert_ne!(case, 0);
                    c.assert_error_absent("SDK error and sources", &error);
                    match &error {
                        Error::Concord(e) | Error::Response { source: e, .. } => {
                            concord_error(&mut c, e)
                        }
                        Error::Http { meta, .. } => c.assert_debug_absent("HTTP metadata", meta),
                        _ => {}
                    }
                }
            }
        }
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    let config = LinearApiConfig {
        authorization: concord::reqwest::header::HeaderValue::from_str(&key).unwrap(),
    };
    c.assert_debug_absent("public wire config", &config);
    let wire = LinearApi::new(http(), config);
    c.assert_debug_absent("public wire client", &wire);
    assert!(c.checked() >= 100);
    println!(
        "linear: {} renderings; positive control passed",
        c.checked()
    );
}
