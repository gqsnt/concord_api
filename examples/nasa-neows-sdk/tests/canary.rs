//! Secrets: NeoWsConfig.api_key -> api_key query parameter (one distinct canary).
//! Read-only SDK: lookup covers success, provider 4xx/5xx, invalid JSON/media, broken body,
//! and refused connection, both generated and native Prepared sends. Echoed headers, quota
//! headers and ignored provider links also contain the key. No wire exemptions.
//! Excludes native reqwest values, *_unredacted accessors and caller-visible asteroid fields.

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
    response
        .header("x-request-id", echo)
        .header("set-cookie", echo)
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

use concord_sdk_nasa_neows::{NeoWs, NeoWsConfig, SendMetered};

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("nasa-neows");
    let key = c.issue();
    for case in 0..7 {
        let echo = key.clone();
        let server = TestServer::start(move |_| {
            let success = format!(r#"{{"id":"433","name":"Eros","links":{{"self":"https://unused.invalid/?api_key={echo}"}}}}"#);
            reply(case, &success, &format!(r#"{{"error":{{"code":"API_KEY_INVALID","message":"{echo}"}}}}"#), &echo)
                .header("x-ratelimit-limit", &echo)
                .header("x-ratelimit-remaining", &echo)
        }).await;
        let config = NeoWsConfig {
            api_key: key.clone(),
        };
        c.assert_debug_absent("config", &config);
        let api = NeoWs::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let scope = api.neows();
        let op = scope.lookup("433");
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("scope", &scope);
        c.assert_debug_absent("endpoint", &op);
        let prepared = op.clone().prepare().unwrap();
        c.assert_debug_absent("Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        if let Err(error) = &native {
            concord_error(&mut c, error);
        }
        assert_eq!(native.is_ok(), case == 0);
        let result = op.send_metered().await;
        c.assert_debug_absent("metered Result", &result);
        match result {
            Ok(response) => {
                assert_eq!(case, 0);
                c.assert_debug_absent("Metered", &response);
                c.assert_debug_absent("RateLimit", &response.rate_limit);
                c.assert_on_wire(&server.requests());
            }
            Err(error) => {
                assert_ne!(case, 0);
                c.assert_debug_absent("SDK error", &error);
                c.assert_error_absent("SDK error and sources", &error);
                c.assert_debug_absent("error quota report", &error.rate_limit);
            }
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 100);
    println!(
        "nasa-neows: {} renderings; positive control passed",
        c.checked()
    );
}
