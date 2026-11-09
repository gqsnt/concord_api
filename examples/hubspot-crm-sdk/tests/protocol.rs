//! Offline protocol and failure checks.
use concord::{
    ErrorKind,
    reqwest::{self, header::HeaderValue},
};
use concord_example_testserver::{Body, Reply, Script, TestServer, paused};
use concord_sdk_hubspot_crm::{
    BATCH_READ_MAX, BatchReadRequest, CrmApi, CrmApiConfig, Error, HubSpot, ItemError,
    SearchRequest,
};
use std::time::Duration;

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}
fn api(url: &str) -> CrmApi {
    concord::BaseUrl::parse(url)
        .map(|origin| {
            CrmApi::new(
                http(),
                CrmApiConfig {
                    origin,
                    authorization: HeaderValue::from_static("Bearer synthetic-secret"),
                },
            )
        })
        .unwrap()
}

#[tokio::test]
async fn wire_and_redaction() {
    let server =
        TestServer::start(|_| Reply::json(200, include_str!("../fixtures/contact.json"))).await;
    let api = api(&server.url());
    assert!(!format!("{api:?}").contains("synthetic-secret"));
    let op = api.contacts().get("abc/def ?");
    assert!(!format!("{op:?}").contains("synthetic-secret"));
    op.send().await.unwrap();
    let r = server.single();
    assert_eq!(r.method, "GET");
    assert_eq!(r.target, "/crm/v3/objects/contacts/abc%2Fdef%20%3F");
    assert_eq!(r.header("authorization"), Some("Bearer synthetic-secret"));
    assert_eq!(r.header("accept"), Some("application/json"));
    server.shutdown().await;
}

#[tokio::test]
async fn status_and_response_error_classes() {
    for status in [400, 401, 403, 404, 409, 423, 429, 500, 503] {
        let server =
            TestServer::start(move |_| Reply::json(status, r#"{"message":"synthetic-secret"}"#))
                .await;
        let e = api(&server.url())
            .contacts()
            .get("x")
            .send()
            .await
            .unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Status);
        assert_eq!(e.status().unwrap().as_u16(), status);
        assert!(!format!("{e:?} {e}").contains("synthetic-secret"));
        server.shutdown().await;
    }
    for (body, media, kind) in [
        ("{", "application/json", ErrorKind::Decode),
        ("{}", "text/html", ErrorKind::Metadata),
    ] {
        let server = TestServer::start(move |_| Reply::text(200, media, body)).await;
        assert_eq!(
            api(&server.url())
                .contacts()
                .get("x")
                .send()
                .await
                .unwrap_err()
                .kind(),
            kind
        );
        server.shutdown().await;
    }
    let server = TestServer::start(|_| Reply::json(200, vec![b' '; 1024 * 1024 + 1])).await;
    assert_eq!(
        api(&server.url())
            .contacts()
            .get("x")
            .send()
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::Limit
    );
    server.shutdown().await;
    let server = TestServer::start(|_| {
        Reply::json(200, "").with_body(Body::FailAfter(vec![bytes::Bytes::from_static(
            b"{\"id\":",
        )]))
    })
    .await;
    assert_eq!(
        api(&server.url())
            .contacts()
            .get("x")
            .send()
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::Transport
    );
    server.shutdown().await;
}

#[tokio::test]
async fn refresh_rejection_and_uncertainty_are_redacted() {
    for (status, uncertain) in [(401, false), (500, true)] {
        let server =
            TestServer::start(move |_| Reply::json(status, r#"{"message":"synthetic-secret"}"#))
                .await;
        let hub = HubSpot::new(
            http(),
            "client".into(),
            "synthetic-secret".into(),
            "refresh-secret".into(),
        )
        .unwrap()
        .with_base_url(&server.url())
        .unwrap();
        let e = hub.get("123").await.unwrap_err();
        assert!(
            matches!(e, Error::Refresh { status: Some(s), uncertain: u } if s == status && u == uncertain)
        );
        assert!(!format!("{hub:?} {e:?} {e}").contains("synthetic-secret"));
        if uncertain {
            assert!(matches!(hub.refresh_token(), Err(Error::RefreshUncertain)));
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn rejects_invalid_inputs_before_contact_request() {
    let server =
        TestServer::start(|_| Reply::json(200, include_str!("../fixtures/oauth.json"))).await;
    let hub = HubSpot::new(http(), "a".into(), "b".into(), "c".into())
        .unwrap()
        .with_base_url(&server.url())
        .unwrap();
    assert!(matches!(hub.get("").await, Err(Error::Invalid(_))));
    assert!(matches!(
        hub.search_page(
            SearchRequest {
                limit: Some(201),
                ..Default::default()
            },
            None
        )
        .await,
        Err(Error::Invalid(_))
    ));
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn create_reports_definite_and_uncertain_outcomes() {
    use concord_sdk_hubspot_crm::CreateContact;
    for (status, body, uncertain) in [
        (400, "{}", false),
        (408, "{}", true),
        (500, "{}", true),
        (201, "{", true),
    ] {
        let server = TestServer::start(move |r| {
            if r.path().contains("/oauth/") {
                Reply::json(200, include_str!("../fixtures/oauth.json"))
            } else {
                Reply::json(status, body)
            }
        })
        .await;
        let hub = HubSpot::new(http(), "a".into(), "b".into(), "c".into())
            .unwrap()
            .with_base_url(&server.url())
            .unwrap();
        let result = hub.create(CreateContact::default()).await;
        if uncertain {
            assert!(matches!(result, Err(Error::CreateUncertain)));
        } else {
            assert!(
                matches!(result, Err(Error::Concord(e)) if e.status().unwrap().as_u16() == status)
            );
        }
        assert_eq!(
            server
                .requests()
                .iter()
                .filter(|r| r.path() == "/crm/v3/objects/contacts")
                .count(),
            1
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unreadable_refresh_latches_and_does_not_refresh_again() {
    let server = TestServer::start(|_| Reply::json(200, "{broken")).await;
    let hub = HubSpot::new(http(), "a".into(), "secret".into(), "refresh".into())
        .unwrap()
        .with_base_url(&server.url())
        .unwrap();
    assert!(matches!(
        hub.get("x").await,
        Err(Error::Refresh {
            uncertain: true,
            ..
        })
    ));
    assert!(matches!(
        hub.get("x").await,
        Err(Error::Refresh {
            uncertain: true,
            ..
        })
    ));
    assert!(matches!(hub.refresh_token(), Err(Error::RefreshUncertain)));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

fn batch_hub(url: &str, timeout: Duration) -> HubSpot {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(timeout)
        .build()
        .unwrap();
    HubSpot::new(http, "a".into(), "b".into(), "c".into())
        .unwrap()
        .with_base_url(url)
        .unwrap()
}

fn batch(ids: &[&str]) -> BatchReadRequest {
    BatchReadRequest {
        ids: ids.iter().map(|id| id.to_string()).collect(),
        properties: vec!["email".into()],
    }
}

const BATCH: &str = "/crm/v3/objects/contacts/batch/read";

#[tokio::test]
async fn batch_read_refuses_invalid_input_before_any_request() {
    let server =
        TestServer::start(|_| Reply::json(200, include_str!("../fixtures/oauth.json"))).await;
    let hub = batch_hub(&server.url(), Duration::from_secs(2));
    let too_many: Vec<String> = (0..=BATCH_READ_MAX).map(|n| n.to_string()).collect();
    for ids in [
        Vec::new(),
        too_many,
        vec!["1".into(), String::new()],
        vec!["7".into(), "7".into()],
    ] {
        let request = BatchReadRequest {
            ids,
            properties: Vec::new(),
        };
        assert!(matches!(
            hub.batch_read(request).await,
            Err(Error::Invalid(_))
        ));
    }
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn batch_read_attributes_each_requested_id() {
    let script = Script::new([
        // 207: one found, one named by context.ids (twice: the first category is kept), one
        // named only by the error-instance `id` (not a contact ID), one unrequested result.
        Reply::json(
            207,
            r#"{"status":"COMPLETE","numErrors":3,"results":[
                {"id":"9","properties":{}},{"id":"1","properties":{"email":null}}],
              "errors":[
                {"status":"error","category":"OBJECT_NOT_FOUND","message":"synthetic-secret",
                 "context":{"ids":["8","2"]},"errors":[],"links":{}},
                {"status":"error","category":"VALIDATION_ERROR","message":"synthetic-secret",
                 "id":"3","context":{"propertyName":["x"]},"errors":[],"links":{}},
                {"status":"error","category":"LATER","message":"synthetic-secret",
                 "context":{"ids":["2"]},"errors":[],"links":{}}]}"#,
        ),
        // 200 in provider order, silently omitting a requested ID.
        Reply::json(
            200,
            r#"{"status":"COMPLETE","results":[{"id":"3","properties":{}},{"id":"1","properties":{}}]}"#,
        ),
        // Errors without usable context are unattributed.
        Reply::json(
            207,
            r#"{"status":"COMPLETE","results":[],"errors":[
                {"status":"error","category":"A","message":"synthetic-secret"},
                {"status":"error","category":"B","message":"synthetic-secret","context":null}]}"#,
        ),
        // Contradictions fail the whole call.
        Reply::json(
            200,
            r#"{"status":"COMPLETE","results":[{"id":"1","properties":{}},{"id":"1","properties":{}}]}"#,
        ),
        Reply::json(
            207,
            r#"{"status":"COMPLETE","results":[{"id":"1","properties":{}}],"errors":[
                {"status":"error","category":"OBJECT_NOT_FOUND","context":{"ids":["1"]}}]}"#,
        ),
        // The documented maximum is accepted.
        Reply::json(200, r#"{"status":"COMPLETE","results":[]}"#),
    ]);
    let replies = script.clone();
    let server = TestServer::start(move |r| {
        if r.path().contains("/oauth/") {
            Reply::json(200, include_str!("../fixtures/oauth.json"))
        } else {
            replies.next()
        }
    })
    .await;
    let hub = batch_hub(&server.url(), Duration::from_secs(2));

    let read = hub.batch_read(batch(&["1", "2", "3"])).await.unwrap();
    let ids: Vec<_> = read.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["1", "2", "3"]);
    assert_eq!(read.get("1").unwrap().as_ref().unwrap().id, "1");
    let not_found = read.get("2").unwrap().as_ref().unwrap_err();
    assert_eq!(
        not_found,
        &ItemError::Provider {
            category: "OBJECT_NOT_FOUND".into()
        }
    );
    assert_eq!(read.get("3").unwrap(), &Err(ItemError::Unreported));
    assert!(read.get("8").is_none());
    assert_eq!(read.unmatched.len(), 1);
    assert_eq!(read.unmatched[0].id, "9");
    assert_eq!(read.unattributed, ["VALIDATION_ERROR"]);
    let shown = format!("{read:?} {not_found} {not_found:?}");
    assert!(!shown.contains("synthetic-secret"));
    assert!(shown.contains("OBJECT_NOT_FOUND"));

    let read = hub.batch_read(batch(&["1", "2", "3"])).await.unwrap();
    let ids: Vec<_> = read.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["1", "2", "3"]);
    assert!(read.get("1").unwrap().is_ok() && read.get("3").unwrap().is_ok());
    assert_eq!(read.get("2").unwrap(), &Err(ItemError::Unreported));

    let read = hub.batch_read(batch(&["1"])).await.unwrap();
    assert_eq!(read.get("1").unwrap(), &Err(ItemError::Unreported));
    assert_eq!(read.unattributed, ["A", "B"]);

    for _ in 0..2 {
        let e = hub.batch_read(batch(&["1", "2"])).await.unwrap_err();
        assert!(matches!(e, Error::Invalid(_)));
    }

    let max: Vec<String> = (1..=BATCH_READ_MAX).map(|n| n.to_string()).collect();
    let read = hub
        .batch_read(BatchReadRequest {
            ids: max,
            properties: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(read.items.len(), BATCH_READ_MAX);
    assert!(
        read.items
            .iter()
            .all(|i| i.result == Err(ItemError::Unreported))
    );

    assert_eq!(script.remaining(), 0);
    let requests = server.requests();
    assert_eq!(requests.len(), 7); // One refresh, then each batch exactly once.
    let body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"inputs": [{"id": "1"}, {"id": "2"}, {"id": "3"}],
            "properties": ["email"], "propertiesWithHistory": []})
    );
    for r in &requests[1..] {
        assert_eq!(r.method, "POST");
        assert_eq!(r.target, BATCH);
        assert_eq!(r.header("authorization"), Some("Bearer synthetic-access-1"));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn batch_read_failures_are_sent_once() {
    for status in [401, 429, 500, 502, 503, 504] {
        let server = TestServer::start(move |r| {
            if r.path().contains("/oauth/") {
                Reply::json(200, include_str!("../fixtures/oauth.json"))
            } else {
                Reply::json(
                    status,
                    r#"{"category":"RATE_LIMITS","message":"synthetic-secret"}"#,
                )
                .header("retry-after", "1")
            }
        })
        .await;
        let hub = batch_hub(&server.url(), Duration::from_secs(2));
        let e = hub.batch_read(batch(&["1"])).await.unwrap_err();
        assert!(
            matches!(&e, Error::Concord(c) if c.kind() == ErrorKind::Status
                && c.status().unwrap().as_u16() == status)
        );
        assert!(!format!("{e:?} {e}").contains("synthetic-secret"));
        let requests = server.requests();
        assert_eq!(requests.len(), 2, "status {status}"); // No retry and no 401 refresh/replay.
        assert_eq!(requests[1].target, BATCH);
        server.shutdown().await;
    }
    for hang_up in [false, true] {
        let server = TestServer::start(move |r| {
            if r.path().contains("/oauth/") {
                Reply::json(200, include_str!("../fixtures/oauth.json"))
            } else if hang_up {
                Reply::hang_up()
            } else {
                Reply::json(200, include_str!("../fixtures/batch-read-complete.json"))
                    .delay(Duration::from_millis(400))
            }
        })
        .await;
        let timeout = Duration::from_millis(150);
        let hub = batch_hub(&server.url(), timeout);
        // Paused clock: the token request and the delivery of the batch complete in frozen time;
        // the 150 ms timeout fires only once the server holds the batch (it answers 400 ms later).
        tokio::time::pause();
        let read = hub.batch_read(batch(&["101"]));
        let e = if hang_up {
            paused::drive(read).await
        } else {
            paused::time_out(&server, |r| r.target == BATCH, timeout, read).await
        }
        .unwrap_err();
        tokio::time::resume();
        assert!(matches!(&e, Error::Concord(c) if c.kind() == ErrorKind::Transport));
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(server.requests().len(), 2);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn a_refused_connection_on_create_stays_uncertain() {
    let server = TestServer::start(|request| {
        if request.path().contains("/oauth/") {
            Reply::json(200, include_str!("../fixtures/oauth.json"))
        } else {
            Reply::json(200, include_str!("../fixtures/contact.json"))
        }
    })
    .await;
    let hub = HubSpot::new(http(), "a".into(), "b".into(), "c".into())
        .unwrap()
        .with_base_url(&server.url())
        .unwrap();
    hub.get("x").await.unwrap(); // Keep a valid access token while testing the create connection.
    let reserved = tokio::net::TcpSocket::new_v4().unwrap();
    reserved.bind(([127, 0, 0, 1], 0).into()).unwrap();
    let origin = format!("http://{}", reserved.local_addr().unwrap());
    let hub = hub.with_base_url(&origin).unwrap();
    let error = hub.create(Default::default()).await.unwrap_err();
    // Concord cannot prove a refused connection sent nothing (a redirect may follow an accepted
    // create), so the SDK keeps the create uncertain; the fixture saw no create request.
    assert!(matches!(error, Error::CreateUncertain), "{error:?}");
    assert_eq!(server.requests().len(), 2);
    drop(reserved);
    server.shutdown().await;
}
