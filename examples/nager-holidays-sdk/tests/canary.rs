//! No secret input: the client is unauthenticated (`NagerHolidays::new(http)`, src/lib.rs:22),
//! the declaration has no #[sensitive] value and no credential constructor exists. Control:
//! a canary as the ordinary `country_code` path segment (the most request-specific string it
//! sends), echoed by the server into response headers and into 400/503, malformed, wrong-type
//! and truncated bodies. `Error` promises Display/Debug omit URLs, header values and bodies
//! (src/error.rs:26); Concord's `ResponseMeta` prints header names only. The generated
//! handle and its `Prepared` print ordinary path values by design (asserted, not
//! absence-checked); the decoded `Holiday` models are clear data, so success bodies carry no
//! canary. Read-only SDK: seven scenarios on `holidays`. Excludes native reqwest values and
//! `Error::into_concord().into_response()` / `body_unredacted`. No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_nager_holidays::{Error, ErrorKind, NagerHolidays};
use std::{net::TcpListener, time::Duration};

const AT: &str = include_str!("../fixtures/first-use-at.json");

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
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

fn reply(case: usize, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, AT),
        1 => Reply::text(
            400,
            "text/plain",
            format!("Synthetic rejection of {echo}; no provider error schema is assumed."),
        ),
        2 => Reply::text(503, "text/plain", format!("Synthetic outage {echo}")),
        3 => Reply::json(200, format!("[{{invalid {echo}")),
        4 => Reply::text(200, "text/html", format!("<p>{echo}</p>")),
        5 => Reply::json(200, "").with_body(Body::FailAfter(vec![
            format!(r#"[{{"date":"2026-01-01","name":"{echo}"#).into(),
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
    c.assert_debug_absent("error operation", &e.operation());
}

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    c.assert_debug_absent("SDK error kind", &e.kind());
    c.assert_debug_absent("SDK error status", &e.status());
    c.assert_debug_absent("SDK error metadata", &e.meta());
    concord_error(c, e.concord());
}

async fn receive<T: std::fmt::Debug>(c: &mut Canaries, prepared: concord::Prepared<T>) -> bool {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode_with_meta().await;
            match decoded {
                Ok((_, meta)) => {
                    c.assert_debug_absent("native decoded metadata", &meta);
                    true
                }
                Err(e) => {
                    concord_error(c, &e);
                    false
                }
            }
        }
        Err(e) => {
            concord_error(c, &e);
            false
        }
    }
}

#[tokio::test]
async fn request_control_never_enters_diagnostics() {
    let mut c = Canaries::new("nager-holidays");
    let country = c.issue();
    for case in 0..7 {
        let echo = country.clone();
        let server = TestServer::start(move |_: &Recorded| reply(case, &echo)).await;
        let api = NagerHolidays::new(http())
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        c.assert_debug_absent("client", &api);
        let op = api.holidays(country.clone(), 2026);
        // The generated handle and Prepared deliberately print ordinary path values.
        assert!(format!("{op:?} {op:#?}").contains(&country));
        let prepared = op.clone().prepare().unwrap();
        assert!(format!("{prepared:?}").contains(&country));
        assert_eq!(receive(&mut c, prepared).await, case == 0);
        let result = op.send().await;
        c.assert_debug_absent("holidays Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("holiday response", &response);
                c.assert_debug_absent("holiday metadata", &response.meta);
                assert_eq!(response.holidays.len(), 2);
                assert_eq!(response.meta.headers()["x-private"], country.as_str());
            }
            Err(e) => {
                error(&mut c, &e);
                let text = e.to_string();
                match case {
                    1 => {
                        assert_eq!(e.kind(), ErrorKind::InvalidRequest);
                        assert_eq!(e.status().unwrap().as_u16(), 400);
                        assert!(text.contains("400"), "{text}");
                        assert!(text.contains("holidays"), "{text}");
                    }
                    2 => {
                        assert_eq!(e.kind(), ErrorKind::Http);
                        assert_eq!(e.status().unwrap().as_u16(), 503);
                    }
                    3 | 5 => assert_eq!(e.status().unwrap().as_u16(), 200),
                    4 => assert_eq!(e.kind(), ErrorKind::Concord(concord::ErrorKind::Metadata)),
                    6 => {
                        assert_eq!(e.kind(), ErrorKind::Concord(concord::ErrorKind::Transport));
                        assert!(e.status().is_none());
                    }
                    _ => unreachable!(),
                }
            }
        }
        if case == 0 {
            let requests = server.requests();
            assert_eq!(requests.len(), 2);
            c.assert_on_wire(&requests);
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 100);
    println!(
        "nager-holidays: {} renderings; ordinary-string positive control passed (no secret input)",
        c.checked()
    );
}
