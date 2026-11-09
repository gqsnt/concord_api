//! Additive caller contract. No production Nager file or existing test changes.

#[path = "../../../github-app/tests/log_capture/mod.rs"]
mod log_capture;

use std::time::Duration;

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_nager_holidays::{ErrorKind, NagerHolidays};
use log_capture::Capture;

const HOLIDAYS: &str = include_str!("../../fixtures/first-use-at.json");

#[tokio::test(flavor = "current_thread")]
async fn ordinary_country_and_response_echo_never_enter_automatic_logs() {
    let mut canaries = Canaries::new("j1-nager");
    let country = canaries.issue(); // Ordinary input, intentionally NOT annotated sensitive.
    for case in 0..7 {
        let echo = country.clone();
        let server = TestServer::start(move |_: &Recorded| {
            let reply = match case {
                0 | 6 => Reply::json(200, HOLIDAYS),
                1 => Reply::text(400, "text/plain", &echo),
                2 => Reply::text(503, "text/plain", &echo),
                3 => Reply::json(200, format!("[invalid {echo}")),
                4 => Reply::text(200, "text/html", &echo),
                5 => Reply::json(200, "").with_body(Body::FailAfter(vec![
                    format!(r#"[{{"name":"{echo}"#).into(),
                ])),
                _ => unreachable!(),
            };
            reply.header("x-private", &echo).header("set-cookie", &echo)
        })
        .await;
        let http = concord::reqwest::Client::builder()
            .no_proxy()
            .redirect(concord::reqwest::redirect::Policy::none())
            .retry(concord::reqwest::retry::never())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let api = NagerHolidays::new(http)
            .with_base_url(if case == 6 {
                concord_example_testserver::refused_url()
            } else {
                server.url()
            })
            .unwrap();
        let capture = Capture::default();
        let dispatch = capture.dispatch();
        let _subscriber = tracing::dispatcher::set_default(&dispatch);
        let endpoint = api.holidays(country.clone(), 2026);
        let prepared = endpoint.clone().prepare().unwrap();
        // C3: the existing Prepared control still prints ordinary values, as documented.
        assert!(format!("{prepared:?}").contains(&country));
        assert!(prepared.summary().to_string().contains(&country));
        capture.assert_empty();
        let result = endpoint.send().await; // The actual SDK KeepHead policy, not a toy client.
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => assert_eq!(response.holidays.len(), 2),
            Err(error) => {
                canaries.assert_error_absent("SDK error and sources", &error);
                canaries.assert_display_absent("error summary", &error.concord().summary());
                if case == 1 {
                    assert!(matches!(error.kind(), ErrorKind::InvalidRequest));
                }
            }
        }
        capture.assert_call(
            "NagerHolidays.holidays",
            "GET",
            "api/v4/Holidays/{country_code}/{year}",
            if case == 6 {
                None
            } else {
                Some(match case {
                    1 => 400,
                    2 => 503,
                    _ => 200,
                })
            },
        );
        canaries.assert_absent("all raw Concord events and spans", &capture.raw());
        if case == 0 {
            let requests = server.requests();
            assert_eq!(requests.len(), 1);
            canaries.assert_on_wire(&requests);
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
}
