//! Secret input: bearer API key / RestCountriesConfig.authorization (src/lib.rs:41,58).
//! Sent in Authorization. Echoes enter response headers and the provider message whose
//! ProviderError formatting withholds text. Read-only SDK; seven scenarios plus paging.
//! Excludes native reqwest, explicit unredacted accessors, clear Country data and the
//! explicitly returned ErrorDocument/ErrorEntry/messages() values. No wire exemptions.

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

fn reply(case: usize, media: &str, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::text(400, media, problem),
        2 => Reply::text(503, media, problem),
        3 => Reply::text(200, media, format!("<{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::text(200, media, "").with_body(Body::FailAfter(vec![
            format!("<{{incomplete {echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error<D: 'static>(c: &mut Canaries, e: &concord::Error<D>) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

async fn receive<T: std::fmt::Debug, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) {
    c.assert_debug_absent("Prepared", &prepared);
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode().await;
            c.assert_debug_absent("native decoded Result", &decoded);
            if let Err(e) = decoded {
                concord_error(c, &e);
            }
        }
        Err(e) => concord_error(c, &e),
    }
}

use concord_sdk_rest_countries::{
    Error, Property, RestCountries, RestCountriesConfig, bearer, pages,
};

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    c.assert_debug_absent("error operation", &e.operation());
    if let Some(p) = e.provider() {
        c.assert_debug_absent("ProviderError", p);
        c.assert_error_absent("ProviderError/source chain", p);
        c.assert_debug_absent("provider metadata", &p.meta);
        if let Err(e) = &p.document {
            c.assert_error_absent("error body", e);
        }
    }
    if let Error::Concord(e) = e {
        concord_error(c, e);
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("rest-countries");
    let key = c.issue();
    for case in 0..7 {
        let echo = key.clone();
        let server = TestServer::start(move |_: &Recorded| {
            let problem = format!(r#"{{"errors":[{{"message":"{echo}"}}]}}"#);
            reply(
                case,
                "application/json",
                include_str!("../fixtures/synthetic-no-match.json"),
                &problem,
                &echo,
            )
        })
        .await;
        let config = RestCountriesConfig {
            authorization: bearer(&key).unwrap(),
        };
        c.assert_debug_absent("config", &config);
        let api = RestCountries::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let scope = api.property(Property::REGION);
        let op = scope.read("Europe");
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("property scope", &scope);
        c.assert_debug_absent("read endpoint", &op);
        receive(&mut c, op.clone().prepare().unwrap()).await;
        let result = op.clone().send().await;
        c.assert_debug_absent("SDK Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = result {
            error(&mut c, &e);
            if case == 1 {
                assert_eq!(e.status().unwrap().as_u16(), 400);
                assert!(e.to_string().contains("400"));
                assert_eq!(e.provider().unwrap().messages().next(), Some(key.as_str()));
            }
        }
        let mut walk = pages(op);
        c.assert_debug_absent("country traversal", &walk);
        let page = walk.next_page().await;
        c.assert_debug_absent("page Result", &page);
        if let Err(e) = page {
            c.assert_error_absent("PagesError", &e);
        }
        c.assert_debug_absent("traversal checkpoint", &walk.checkpoint());
        c.assert_debug_absent("traversal progress", &walk.progress());
        c.assert_debug_absent("traversal report", &walk.finish());
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    let invalid = bearer(&format!("{key}\n")).unwrap_err();
    c.assert_debug_absent("credential validation error", &invalid);
    c.assert_error_absent("credential validation error", &invalid);
    assert!(c.checked() >= 150);
    println!(
        "rest-countries: {} renderings; positive control passed",
        c.checked()
    );
}
