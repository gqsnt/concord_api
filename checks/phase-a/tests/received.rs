//! The owned response boundary: one execution, head before body, one consumer, identity and
//! redaction on every failure stage.

use std::time::Duration;

use concord::{ErrorKind, Operation};
use concord_check_phase_a::{Reports, Row};
use concord_example_testserver::{Body, Reply, TestServer};

fn api(server: &TestServer) -> Reports {
    Reports::new(
        concord::reqwest::Client::new(),
        concord_check_phase_a::ReportsConfig {
            token: "Bearer s3cr3t-token".to_owned(),
        },
    )
    .with_base_url(format!("{}/v1", server.url()))
    .unwrap()
}

/// A provider-owned metadata value, parsed from the head before the body is consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Meta {
    status: u16,
    request_id: Option<String>,
}

impl Meta {
    fn capture(head: &concord::ResponseHead<'_>) -> Self {
        Self {
            status: head.status().as_u16(),
            request_id: head
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
        }
    }
}

/// The ordinary SDK metadata path: no `into_parts`, no `execute`, no impossible branch.
async fn summary_with_meta(
    api: &Reports,
    id: &str,
) -> Result<(Row, Meta), (concord::Error, Option<Meta>)> {
    let received = api
        .summary(id)
        .prepare()
        .map_err(|e| (e, None))?
        .receive()
        .await
        .map_err(|e| (e, None))?;
    let meta = Meta::capture(&received.head());
    match received.decode().await {
        Ok(row) => Ok((row, meta)),
        Err(e) => Err((e, Some(meta))),
    }
}

#[tokio::test]
async fn receive_executes_once_and_returns_before_the_body() {
    // The head arrives, the body never does: `receive` must return without polling it.
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/json")
            .header("x-request-id", "req-1")
            .with_body(Body::Stall)
    })
    .await;
    let received = tokio::time::timeout(
        Duration::from_secs(5),
        api(&server).summary("a/b").prepare().unwrap().receive(),
    )
    .await
    .expect("receive waits only for the head")
    .unwrap();
    assert_eq!(received.status(), 200);
    assert_eq!(
        Meta::capture(&received.head()).request_id.as_deref(),
        Some("req-1")
    );
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.single().target, "/v1/reports/a%2Fb/summary");
    drop(received); // releases the connection without reading
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn metadata_survives_a_decode_failure_and_success_pairs_it() {
    let server = TestServer::start(|r| {
        let body = if r.target.contains("bad") {
            "{\"id\": \"x\"}"
        } else {
            "{\"id\":1,\"amount\":2}"
        };
        Reply::json(200, body).header("x-request-id", "req-9")
    })
    .await;
    let api = api(&server);
    let (row, meta) = summary_with_meta(&api, "good").await.unwrap();
    assert_eq!(row, Row { id: 1, amount: 2 });
    assert_eq!(
        meta,
        Meta {
            status: 200,
            request_id: Some("req-9".into())
        }
    );

    let (error, meta) = summary_with_meta(&api, "bad").await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert_eq!(error.endpoint(), Some("Reports.summary"));
    assert_eq!(meta.unwrap().request_id.as_deref(), Some("req-9"));
}

#[tokio::test]
async fn a_status_keeps_the_response_unread_with_the_endpoint() {
    let server = TestServer::start(|_| {
        Reply::json(429, "{\"error\":\"slow down\"}").header("retry-after", "7")
    })
    .await;
    let received = api(&server)
        .summary("x")
        .prepare()
        .unwrap()
        .receive()
        .await
        .unwrap();
    // Any status reaches the SDK boundary.
    assert_eq!(received.status(), 429);
    assert_eq!(
        received.operation().map(|op| op.name()),
        Some("Reports.summary")
    );
    let error = received.decode().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(429));
    assert_eq!(error.operation().unwrap().method(), Some("GET"));
    // The provider error document is read explicitly by the SDK.
    let response = error.into_response().unwrap();
    assert_eq!(response.headers()["retry-after"], "7");
    assert_eq!(response.text().await.unwrap(), "{\"error\":\"slow down\"}");
}

#[tokio::test]
async fn a_transport_failure_names_the_endpoint_and_has_no_response() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let error = api(&server)
        .summary("x")
        .prepare()
        .unwrap()
        .receive()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert_eq!(error.endpoint(), Some("Reports.summary"));
    assert!(error.response().is_none());
    assert!(!error.to_string().contains("127.0.0.1"), "{error}");
}

#[tokio::test]
async fn a_natively_edited_request_keeps_identity_without_impossible_branches() {
    let server = TestServer::start(|_| Reply::json(200, "{\"id\":3,\"amount\":4}")).await;
    let (client, mut request, decoder) = api(&server).summary("x").prepare().unwrap().into_parts();
    request
        .headers_mut()
        .insert("x-trace", "t-1".parse().unwrap());
    *request.timeout_mut() = Some(Duration::from_secs(5));
    let received = decoder.execute(&client, request).await.unwrap();
    assert_eq!(received.operation(), Some(decoder.operation().unwrap()));
    assert_eq!(received.decode().await.unwrap(), Row { id: 3, amount: 4 });
    assert_eq!(server.single().header("x-trace"), Some("t-1"));

    // A transport failure of a native execution: synchronous conversion, same identity.
    let dead = TestServer::start(|_| Reply::hang_up()).await;
    let (client, request, decoder) = api(&dead).summary("x").prepare().unwrap().into_parts();
    let error = match client.execute(request).await {
        Ok(_) => panic!("the server hangs up"),
        Err(e) => decoder.transport_error(e),
    };
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert_eq!(error.endpoint(), Some("Reports.summary"));
}

#[tokio::test]
async fn debug_never_prints_the_url_or_header_values() {
    let server =
        TestServer::start(|_| Reply::json(200, "{}").header("set-cookie", "session=hunter2")).await;
    let received = api(&server)
        .events()
        .set_resume("cursor-secret".to_owned())
        .prepare()
        .unwrap()
        .receive()
        .await
        .unwrap();
    let shown = format!("{received:?}");
    assert!(shown.contains("Reports.events"), "{shown}");
    assert!(shown.contains("set-cookie"), "{shown}");
    for secret in ["hunter2", "cursor-secret", "s3cr3t-token", "127.0.0.1"] {
        assert!(!shown.contains(secret), "{secret} in {shown}");
    }
}

#[tokio::test]
async fn a_native_response_opens_with_a_chosen_label() {
    let server = TestServer::start(|_| Reply::new(403)).await;
    let response = concord::reqwest::get(format!("{}/signed?sig=abc", server.url()))
        .await
        .unwrap();
    let error = concord::Incoming::new(response, Some(Operation::label("Reports.signed_download")))
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.endpoint(), None, "a label is not a declared endpoint");
    assert_eq!(error.operation().unwrap().name(), "Reports.signed_download");
    assert!(
        error.to_string().starts_with("Reports.signed_download: "),
        "{error}"
    );
    assert!(!error.to_string().contains("sig=abc"));
}
