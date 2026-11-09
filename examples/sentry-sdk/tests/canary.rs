//! Secrets: distinct bearer token, issue-list cursor/query, and event-list cursor/query.
//! Each travels in Authorization or its own sensitive query parameter. Headers and withheld
//! provider detail echo the token. Read (issues) and write (status update) cover all seven
//! scenarios. Event listing is also sent successfully for the positive control.
//! Excludes native reqwest and explicit unredacted accessors/clear field inspection.
//! No wire exemptions; native Prepared sends additionally expose concord::Error.

use concord_check_canary::Canaries;
use concord_testserver::{Body, Reply, TestServer};
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
    concord_testserver::refused_url()
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

async fn receive<T, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) -> Result<T, concord::Error<D>> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error.widen()),
    }
}

fn concord_error<D: 'static>(c: &mut Canaries, error: &concord::Error<D>) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_sentry::{Error, SendPolicy, Sentry, SentryConfig, Settings, StatusUpdate};
use std::num::{NonZeroU32, NonZeroUsize};

fn policy() -> SendPolicy {
    SendPolicy::new(Settings {
        max_attempts: NonZeroUsize::new(1).unwrap(),
        requests: NonZeroU32::new(1000).unwrap(),
        ..Settings::default()
    })
    .unwrap()
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Uncertain { transport, report } => {
            c.assert_debug_absent("uncertain retry report", report);
            if let Some(e) = transport {
                concord_error(c, e);
            }
        }
        Error::Provider(provider) => {
            c.assert_debug_absent("provider failure", provider);
            c.assert_debug_absent("metadata", &provider.meta);
            c.assert_debug_absent("provider document", &provider.document);
            c.assert_debug_absent("retry report", &provider.retry);
            if let Err(e) = &provider.document {
                concord_error(c, e);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("sentry");
    let token = c.issue();
    let cursor = c.issue();
    let query = c.issue();
    let event_cursor = c.issue();
    let event_query = c.issue();
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |r: &concord_testserver::Recorded| {
            let issue = format!(r#"{{"id":"1","shortId":"ONE-1","title":"{echo}","status":"unresolved"}}"#);
            let success = if r.method == "PUT" { issue }
                else if r.path().contains("/events/") {
                    format!(r#"[{{"id":"event","eventID":"e","groupID":"1","title":"{echo}","entries":["{echo}"]}}]"#)
                } else { format!("[{issue}]") };
            reply(case, &success, &format!(r#"{{"detail":"{echo}"}}"#), &echo)
        }).await;
        let mut authorization =
            concord::reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")).unwrap();
        authorization.set_sensitive(true);
        let config = SentryConfig {
            deployment: concord_sdk_sentry::Deployment::parse(server.url()).unwrap(),
            authorization,
            policy: policy(),
        };
        c.assert_debug_absent("config", &config);
        c.assert_debug_absent("policy", &config.policy);
        let api = Sentry::at(
            http(),
            concord_sdk_sentry::Deployment::parse(if case == 6 { refused() } else { server.url() })
                .unwrap(),
            &token,
            policy(),
        )
        .unwrap();
        let org = api.organization("acme");
        let issue = org.issue("1");
        let read = org.issues().set_cursor(&cursor).set_query(&query);
        let write = issue.update_status(StatusUpdate::Resolved);
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("organization scope", &org);
        c.assert_debug_absent("issue scope", &issue);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("read Prepared", &read.clone().prepare().unwrap());
        let prepared = write.clone().prepare().unwrap();
        c.assert_debug_absent("write Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let read_result = read.send().await;
        let write_result = write.send().await;
        c.assert_debug_absent("read Result", &read_result);
        c.assert_debug_absent("write Result", &write_result);
        assert_eq!(read_result.is_ok(), case == 0);
        assert_eq!(write_result.is_ok(), case == 0);
        match read_result {
            Ok(response) => {
                c.assert_debug_absent("read response", &response);
                c.assert_debug_absent("issue models", &response.value);
                c.assert_debug_absent("read metadata", &response.meta);
                c.assert_debug_absent("read retry report", &response.retry);
                c.assert_debug_absent("read rates", &response.rates);
            }
            Err(e) => error(&mut c, &e),
        }
        match write_result {
            Ok(response) => {
                c.assert_debug_absent("write response", &response);
                c.assert_debug_absent("write metadata", &response.meta);
                c.assert_debug_absent("write retry report", &response.retry);
            }
            Err(e) => error(&mut c, &e),
        }
        if case == 0 {
            let op = issue
                .events()
                .set_cursor(&event_cursor)
                .set_query(&event_query);
            c.assert_debug_absent("events endpoint", &op);
            c.assert_debug_absent("events Prepared", &op.clone().prepare().unwrap());
            let response = op.send().await.unwrap();
            c.assert_debug_absent("events response", &response);
            c.assert_debug_absent("event models", &response.value);
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 200);
    println!(
        "sentry: {} renderings; positive control passed",
        c.checked()
    );
}
