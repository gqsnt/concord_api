//! New consumer contract: submit once, inspect acceptance, never infer search visibility.
//! Live provider calls: NOT_RUN. Values and responses below are synthetic fixtures.

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_datadog_events::{
    AlertType, Credentials, DatadogEvents, Destination, NewEvent, Site,
};
use serde_json::{Value, json};

#[tokio::test]
async fn caller_posts_an_event_and_receives_only_provider_acceptance() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/api/v1/events");
        assert_eq!(request.header_all("DD-API-KEY"), ["fixture-post-api"]);
        assert_eq!(
            request.header_all("DD-APPLICATION-KEY"),
            ["fixture-post-app"]
        );
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(
            body,
            json!({
                "title": "Checkout deployed",
                "text": "Version 42 is serving traffic.",
                "tags": ["service:checkout", "env:prod"],
                "alert_type": "success",
                "aggregation_key": "checkout-deployments",
                "source_type_name": "jenkins"
            })
        );
        Reply::json(
            202,
            json!({
                "status": "ok",
                "event": {"id": 9_007_199_254_740_993_i64, "id_str": "9007199254740993",
                    "url": "https://app.datadoghq.eu/event/event?id=9007199254740993"}
            })
            .to_string(),
        )
    })
    .await;

    let events = DatadogEvents::new(
        DatadogEvents::http_client().unwrap(),
        Destination::at(Site::Eu, server.url()).unwrap(),
        Credentials::new("fixture-post-api", "fixture-post-app").unwrap(),
    );
    let event = NewEvent::new("Checkout deployed", "Version 42 is serving traffic.")
        .unwrap()
        .tags(["service:checkout", "env:prod"])
        .alert_type(AlertType::Success)
        .aggregation_key("checkout-deployments")
        .unwrap()
        .source("jenkins");
    let accepted = events.post(event).send().await.unwrap();
    assert_eq!(accepted.status.as_deref(), Some("ok"));
    let provider_event = accepted.event.as_ref().unwrap();
    assert_eq!(provider_event["id_str"], "9007199254740993");
    assert_eq!(provider_event["id"].as_i64(), Some(9_007_199_254_740_993));
    assert_eq!(events.site(), Site::Eu);
    assert_eq!(
        server.requests().len(),
        1,
        "acceptance must not start a search or follow the URL"
    );
    server.shutdown().await;
}
