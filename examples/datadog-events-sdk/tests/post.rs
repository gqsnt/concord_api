//! Posting regressions. Documentation read 2026-11-07; live calls: NOT_RUN.

use std::error::Error as _;
use std::time::Duration;

use concord_example_testserver::{Body, Gate, Recorded, Reply, TestServer};
use concord_sdk_datadog_events::concord::{Delivery, ErrorKind, reqwest};
use concord_sdk_datadog_events::{
    AlertType, AuthError, Credentials, DatadogEvents, Error, InputError, NewEvent, RESPONSE_LIMIT,
    Site,
};
use serde_json::{Value, json};

const API_KEY: &str = "synthetic-post-eu-api-secret";
const APP_KEY: &str = "synthetic-post-eu-app-secret";
const ACCEPTED: &str = include_str!("../fixtures/post-202.doc.json");

fn client(http: reqwest::Client, base: &str) -> DatadogEvents {
    DatadogEvents::new(http, Site::Eu, Credentials::new(API_KEY, APP_KEY).unwrap())
        .with_base_url(base)
        .unwrap()
}

fn event() -> NewEvent {
    NewEvent::new("Example-Event", "A text message.")
        .unwrap()
        .tags(["test:ExampleEvent"])
}

fn assert_redacted(text: &str) {
    assert!(!text.contains(API_KEY), "API key in diagnostic");
    assert!(!text.contains(APP_KEY), "application key in diagnostic");
}

fn assert_error_redacted(error: &Error) {
    assert_redacted(&format!("{error} {error:?}"));
    let mut source = error.source();
    while let Some(error) = source {
        assert_redacted(&format!("{error} {error:?}"));
        source = error.source();
    }
}

#[test]
fn posts_keep_every_site_host_and_both_sensitive_keys() {
    let sites: Value = serde_json::from_str(include_str!("../fixtures/sites.doc.json")).unwrap();
    for row in sites.as_array().unwrap() {
        let site = Site::from_dd_site(row["dd_site"].as_str().unwrap()).unwrap();
        let client = DatadogEvents::new(
            DatadogEvents::http_client().unwrap(),
            site,
            Credentials::new(API_KEY, APP_KEY).unwrap(),
        );
        let operation = client.events().post(event());
        assert_redacted(&format!("{client:?} {operation:?}"));
        let prepared = operation.prepare().unwrap();
        assert_eq!(
            prepared.request().url().as_str(),
            format!(
                "https://{}/api/v1/events",
                row["api_host"].as_str().unwrap()
            )
        );
        for header in ["dd-api-key", "dd-application-key"] {
            assert!(prepared.request().headers()[header].is_sensitive());
        }
        assert_redacted(&format!("{prepared:?} {:?}", prepared.request()));
        assert!(
            prepared.request().try_clone().is_none(),
            "a post must not be replayable"
        );
    }
}

#[tokio::test]
async fn documented_requests_and_acceptance_are_preserved() {
    let server = TestServer::start(|_: &Recorded| -> Reply { Reply::json(202, ACCEPTED) }).await;
    let client = client(DatadogEvents::http_client().unwrap(), &server.url());
    for fixture in [
        include_str!("../fixtures/post-request.doc.json"),
        include_str!("../fixtures/post-long-title.doc.json"),
    ] {
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let event = NewEvent::new(expected["title"].as_str().unwrap(), "A text message.")
            .unwrap()
            .tags(["test:ExampleEvent"]);
        let acceptance = client.post(event).send().await.unwrap();
        let response: Value = serde_json::from_str(ACCEPTED).unwrap();
        assert_eq!(acceptance.status.as_deref(), response["status"].as_str());
        assert_eq!(acceptance.event.as_ref(), response["event"].as_object());
        let request = server.requests().pop().unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/api/v1/events");
        assert_eq!(request.header("accept"), Some("application/json"));
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.header("dd-api-key"), Some(API_KEY));
        assert_eq!(request.header("dd-application-key"), Some(APP_KEY));
        assert_eq!(
            serde_json::from_slice::<Value>(&request.body).unwrap(),
            expected
        );
    }
    assert_eq!(server.requests().len(), 2, "no search or follow-up request");
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_input_is_refused_locally_and_unicode_boundaries_are_accepted() {
    let server = TestServer::start(|_: &Recorded| -> Reply { Reply::json(202, ACCEPTED) }).await;
    for unit in ["x", "é", "🚀"] {
        assert_eq!(
            NewEvent::new("title", unit.repeat(4_001)).unwrap_err(),
            InputError::EventTextTooLong
        );
        assert_eq!(
            event().aggregation_key(unit.repeat(101)).unwrap_err(),
            InputError::AggregationKeyTooLong
        );
    }
    assert!(server.requests().is_empty());
    assert_redacted(&InputError::EventTextTooLong.to_string());
    let client = client(DatadogEvents::http_client().unwrap(), &server.url());
    let text = "🚀".repeat(4_000);
    let key = "é".repeat(100);
    client
        .post(
            NewEvent::new("a long title ".repeat(20), &text)
                .unwrap()
                .aggregation_key(&key)
                .unwrap(),
        )
        .send()
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&server.single().body).unwrap();
    assert_eq!(body["text"], text);
    assert_eq!(body["aggregation_key"], key);
    server.shutdown().await;
}

#[test]
fn all_documented_alert_types_have_their_wire_spelling() {
    for (kind, wire) in [
        (AlertType::Error, "error"),
        (AlertType::Warning, "warning"),
        (AlertType::Info, "info"),
        (AlertType::Success, "success"),
        (AlertType::UserUpdate, "user_update"),
        (AlertType::Recommendation, "recommendation"),
        (AlertType::Snapshot, "snapshot"),
    ] {
        assert_eq!(
            serde_json::to_value(event().alert_type(kind)).unwrap()["alert_type"],
            wire
        );
    }
}

#[tokio::test]
async fn wrong_region_and_missing_application_key_stay_redacted_authentication_errors() {
    let error: Error = Credentials::new(API_KEY, "").unwrap_err().into();
    assert!(matches!(
        error,
        Error::Authentication(AuthError::MissingApplicationKey)
    ));
    assert!(error.is_authentication());
    assert_error_redacted(&error);

    for status in [401, 403] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::json(
                status,
                json!({"errors": [format!("wrong region {API_KEY} {APP_KEY}")]}).to_string(),
            )
        })
        .await;
        // api.us3.datadoghq.com resolves only to the loopback fixture. Never a live call.
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .resolve("api.us3.datadoghq.com", server.addr())
            .build()
            .unwrap();
        let client = client(
            http,
            &format!("http://api.us3.datadoghq.com:{}", server.addr().port()),
        );
        assert_eq!(client.site(), Site::Eu);
        let error = client.post(event()).send().await.unwrap_err();
        assert!(error.is_authentication());
        assert!(!error.is_post_uncertain());
        let Error::Authentication(AuthError::Rejected(ref rejected)) = error else {
            panic!("{error:?}")
        };
        assert_eq!(rejected.site(), Some(Site::Us3));
        assert_eq!(rejected.host(), "api.us3.datadoghq.com");
        assert_eq!(error.status().unwrap().as_u16(), status);
        assert!(error.provider().unwrap().messages().unwrap()[0].contains(APP_KEY));
        assert_error_redacted(&error);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

fn retrying_http(timeout: Option<Duration>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(
            reqwest::retry::for_host("127.0.0.1")
                .no_budget()
                .max_retries_per_request(3)
                .classify_fn(|attempt| attempt.retryable()),
        );
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    builder.build().unwrap()
}

#[tokio::test]
async fn timeout_after_receipt_is_uncertain_and_never_replayed() {
    for after_head in [false, true] {
        let gate = Gate::new();
        let response_gate = gate.clone();
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            if after_head {
                Reply::new(202)
                    .header("content-type", "application/json")
                    .with_body(Body::Stall)
            } else {
                Reply::json(202, ACCEPTED).after(response_gate.clone())
            }
        })
        .await;
        let client = client(
            retrying_http(Some(Duration::from_millis(250))),
            &server.url(),
        );
        let error = client.post(event()).send().await.unwrap_err();
        assert!(error.is_post_uncertain(), "{error:?}");
        let Error::PostUncertain {
            source: Some(ref source),
            ..
        } = error
        else {
            panic!("{error:?}")
        };
        assert_eq!(source.kind(), ErrorKind::Transport);
        assert!(source.transport().unwrap().is_timeout());
        assert_eq!(
            source.delivery(),
            if after_head {
                Delivery::Answered
            } else {
                Delivery::Unknown
            }
        );
        assert_eq!(
            error.status().map(|s| s.as_u16()),
            after_head.then_some(202)
        );
        assert_eq!(error.operation().unwrap().name(), "DatadogApi.events.post");
        assert!(error.to_string().contains("may exist"));
        assert_error_redacted(&error);
        assert_eq!(
            server.requests().len(),
            1,
            "even a retrying transport must send once"
        );
        assert_eq!(server.single().target, "/api/v1/events");
        gate.open();
        server.shutdown().await;
    }
}

#[tokio::test]
async fn provider_failures_keep_their_class_and_cannot_trigger_transport_replays() {
    for status in [400, 429, 500, 503, 302] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::json(
                status,
                json!({"errors": [format!("{API_KEY} {APP_KEY}")]}).to_string(),
            )
            .header("retry-after", "1")
        })
        .await;
        let error = client(retrying_http(None), &server.url())
            .post(event())
            .send()
            .await
            .unwrap_err();
        match (status, &error) {
            (400, Error::BadRequest(_))
            | (429, Error::RateLimited(_))
            | (500 | 503 | 302, Error::Provider(_)) => {}
            _ => panic!("{error:?}"),
        }
        assert_eq!(error.status().unwrap().as_u16(), status);
        assert_eq!(error.meta().unwrap().headers()["retry-after"], "1");
        assert_error_redacted(&error);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unusable_acceptance_and_unexpected_success_are_uncertain() {
    let cases = [
        Reply::text(202, "text/html", ACCEPTED),
        Reply::json(202, "{truncated"),
        Reply::json(202, r#"{"event":"not an object"}"#),
        Reply::json(202, vec![b' '; RESPONSE_LIMIT + 1]),
        Reply::new(202)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![b"{\"status\":".to_vec().into()])),
        Reply::json(200, ACCEPTED),
        Reply::new(204),
        Reply::hang_up(),
    ];
    for reply in cases {
        let reply = std::sync::Mutex::new(Some(reply));
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            reply.lock().unwrap().take().unwrap()
        })
        .await;
        let error = client(DatadogEvents::http_client().unwrap(), &server.url())
            .post(event())
            .send()
            .await
            .unwrap_err();
        assert!(error.is_post_uncertain(), "{error:?}");
        assert_eq!(error.operation().unwrap().name(), "DatadogApi.events.post");
        assert_error_redacted(&error);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
