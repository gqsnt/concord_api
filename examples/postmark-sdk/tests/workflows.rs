//! Connected offline workflows.

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_postmark::{Email, Error, Postmark, PostmarkConfig};
use std::time::Duration;

fn email() -> Email {
    Email {
        from: "sender@example.com".into(),
        to: "receiver@example.com".into(),
        subject: "Password Reset".into(),
        text_body: Some("Click here to reset your password.".into()),
        html_body: None,
        message_stream: Some("outbound".into()),
    }
}
fn client(server: &TestServer, timeout: Duration) -> Postmark {
    let http = concord_sdk_postmark::http_client_builder()
        .timeout(timeout)
        .build()
        .unwrap();
    Postmark::new(
        http,
        PostmarkConfig {
            token: "secret-token".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

#[tokio::test]
async fn send_then_inspect_and_list_then_inspect() {
    let server = TestServer::start(|r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", "/email") => Reply::json(200, include_str!("../fixtures/acceptance.json")),
        ("GET", "/messages/outbound") => Reply::json(200, include_str!("../fixtures/list.json")),
        ("GET", "/messages/outbound/0a129aee-e1cd-480d-b08d-4f48548ff48d/details") => Reply::json(
            200,
            include_str!("../fixtures/details.json").replace(
                "07311c54-0687-4ab9-b034-b54b5bad88ba",
                "0a129aee-e1cd-480d-b08d-4f48548ff48d",
            ),
        ),
        ("GET", "/messages/outbound/0ac29aee-e1cd-480d-b08d-4f48548ff48d/details") => Reply::json(
            200,
            include_str!("../fixtures/details.json").replace(
                "07311c54-0687-4ab9-b034-b54b5bad88ba",
                "0ac29aee-e1cd-480d-b08d-4f48548ff48d",
            ),
        ),
        _ => Reply::new(404),
    })
    .await;
    let api = client(&server, Duration::from_secs(2));
    let outbound = api.outbound();
    let accepted = outbound.send_email(email()).send().await.unwrap();
    let details = outbound.details(&accepted.message_id).send().await.unwrap();
    assert_eq!(details.message_id, accepted.message_id);
    assert_eq!(details.fields["MessageEvents"][0]["Type"], "Delivered");
    let page = outbound.list(50, 0).send().await.unwrap();
    assert_eq!(page.total_count, 194);
    let listed = &page.messages[0];
    assert_eq!(
        outbound
            .details(&listed.message_id)
            .send()
            .await
            .unwrap()
            .message_id,
        listed.message_id
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests[0].header("x-postmark-server-token"),
        Some("secret-token")
    );
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
    assert_eq!(requests[0].header("accept"), Some("application/json"));
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"From":"sender@example.com","To":"receiver@example.com","Subject":"Password Reset","TextBody":"Click here to reset your password.","MessageStream":"outbound"})
    );
    assert_eq!(requests[2].query_all("count"), ["50"]);
    assert_eq!(requests[2].query_all("offset"), ["0"]);
    for request in &requests {
        assert_eq!(
            request.header("x-postmark-server-token"),
            Some("secret-token")
        );
        assert_eq!(request.header("accept"), Some("application/json"));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn timed_out_send_is_unknown_and_never_repeated() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(200, include_str!("../fixtures/acceptance.json")).delay(Duration::from_secs(2))
    })
    .await;
    let error = client(&server, Duration::from_millis(100))
        .outbound()
        .send_email(email())
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::UnknownOutcome(_)));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn disconnect_and_redirect_never_resubmit_send() {
    for redirect in [false, true] {
        let server = TestServer::start(move |_: &Recorded| {
            if redirect {
                Reply::new(307).header("location", "/email")
            } else {
                Reply::hang_up()
            }
        })
        .await;
        let error = client(&server, Duration::from_secs(1))
            .outbound()
            .send_email(email())
            .send()
            .await
            .unwrap_err();
        if redirect {
            assert!(matches!(error, Error::Api { status, .. } if status.as_u16() == 307));
        } else {
            assert!(matches!(error, Error::UnknownOutcome(_)));
        }
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unusable_success_response_leaves_send_outcome_unknown() {
    for bad_media in [false, true] {
        let server = TestServer::start(move |_: &Recorded| {
            if bad_media {
                Reply::text(200, "text/html", "not json")
            } else {
                Reply::json(200, "{malformed")
            }
        })
        .await;
        let error = client(&server, Duration::from_secs(1))
            .outbound()
            .send_email(email())
            .send()
            .await
            .unwrap_err();
        assert!(matches!(error, Error::UnknownOutcome(_)));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
