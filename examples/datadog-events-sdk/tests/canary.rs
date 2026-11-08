//! Secrets: Credentials::new's API key and application key (src/credentials.rs:47),
//! #[sensitive] headers (src/api.rs:28,30); distinct canaries travel as DD-API-KEY and
//! DD-APPLICATION-KEY. The server echoes the API key into withheld provider errors, headers
//! and corrupt bodies. Read list and write post cover seven scenarios with native Prepared
//! receives; search pagination is also exercised. No wire exemptions.
//! Excludes native reqwest, explicit unredacted header/body access, ProviderError::messages /
//! document (raw provider text), and event/search model fields returned in clear.
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

// Echo only into withheld diagnostics, never clear success-model fields.
fn reply(case: usize, status: u16, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(status, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(status, format!("{{invalid {echo}")),
        4 => Reply::text(status, "text/html", echo),
        5 => Reply::json(status, "").with_body(Body::FailAfter(vec![
            format!("{{\"incomplete\":\"{echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response.header("x-canary", echo).header("set-cookie", echo)
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
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_datadog_events::{
    Credentials, DatadogApiConfig, DatadogEvents, Error, NewEvent, SearchRequest, Site,
};

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error metadata", &error.meta());
    c.assert_debug_absent("error operation", &error.operation());
    if let Some(p) = error.provider() {
        c.assert_debug_absent("provider failure", p);
        c.assert_error_absent("provider failure and sources", p);
        c.assert_debug_absent("provider metadata", p.meta());
        if let Err(e) = p.document() {
            c.assert_debug_absent("error body failure", e);
            c.assert_error_absent("error body failure and sources", e);
            concord_error(c, e.concord());
        }
    }
    match error {
        Error::Concord { source, .. }
        | Error::PostUncertain {
            source: Some(source),
            ..
        } => concord_error(c, source),
        Error::Authentication(e) => {
            c.assert_debug_absent("authentication error", e);
            c.assert_error_absent("authentication error and sources", e);
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("datadog-events");
    let key = c.issue();
    let application_key = c.issue();
    let credentials = Credentials::new(&key, &application_key).unwrap();
    c.assert_debug_absent("credentials", &credentials);
    let config = DatadogApiConfig {
        api_key: key.clone(),
        application_key: application_key.clone(),
    };
    c.assert_debug_absent("generated config", &config);
    for case in 0..7 {
        let echo = key.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let posting = r.path() == "/api/v1/events";
            let success = if posting {
                r#"{"status":"ok","event":{"id_str":"one"}}"#
            } else {
                r#"{"data":[],"meta":{"status":"done"}}"#
            };
            reply(
                case,
                if posting { 202 } else { 200 },
                success,
                &format!(r#"{{"errors":["{echo}"]}}"#),
                &echo,
            )
        })
        .await;
        let api = DatadogEvents::new(http(), Site::Eu, credentials.clone())
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let scope = api.events();
        let read = scope.list();
        let event = NewEvent::new("Canary test", "Synthetic event").unwrap();
        c.assert_debug_absent("post payload (clear nonsecret data)", &event);
        let write = api.post(event);
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("generated client", api.api());
        c.assert_debug_absent("events scope", &scope);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("read Prepared", &read.clone().prepare().unwrap());
        c.assert_debug_absent("write Prepared", &write.clone().prepare().unwrap());
        let native = receive(&mut c, read.clone().prepare().unwrap()).await;
        c.assert_debug_absent("native read Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = native {
            concord_error(&mut c, &e);
        }
        let native = receive(&mut c, write.clone().prepare().unwrap()).await;
        c.assert_debug_absent("native write Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = native {
            concord_error(&mut c, &e);
        }
        let result = read.send().await;
        c.assert_debug_absent("read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            error(&mut c, e);
            if case == 1 {
                assert_eq!(e.status().unwrap().as_u16(), 401);
                assert_eq!(e.operation().unwrap().name(), "DatadogApi.events.list");
                assert_eq!(e.provider().unwrap().messages().unwrap(), [key.as_str()]);
            }
        }
        let result = write.send().await;
        c.assert_debug_absent("write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            error(&mut c, e);
        }
        let search = api.search(SearchRequest::new());
        c.assert_debug_absent("search endpoint", &search);
        c.assert_debug_absent("search Prepared", &search.prepare().unwrap());
        let mut pages = api.search_pages(SearchRequest::new());
        c.assert_debug_absent("search traversal", &pages);
        let page = pages.next_page().await;
        c.assert_debug_absent("search page Result", &page);
        if let Err(e) = page {
            c.assert_error_absent("PagesError and sources", &e);
            c.assert_display_absent("PagesError", &e);
            error(&mut c, e.provider_error().unwrap());
        }
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    // Refused credentials must also retain useful local diagnosis without their values.
    let e = Credentials::new(format!("{key}\n"), &application_key).unwrap_err();
    c.assert_debug_absent("invalid credential error", &e);
    c.assert_error_absent("invalid credential error", &e);
    assert!(e.to_string().contains("API key"));
    assert!(c.checked() >= 200);
    println!(
        "datadog-events: {} renderings; positive control passed",
        c.checked()
    );
}
