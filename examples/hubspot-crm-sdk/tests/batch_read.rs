//! Consumer tests for batch read, written from the README and the `batch_read` rustdoc.
//! All credentials and contacts are synthetic; all network traffic is loopback.
use concord::{ErrorKind, reqwest};
use concord_example_testserver::{Gate, Recorded, Reply, TestServer};
use concord_pagination::Resume;
use concord_sdk_hubspot_crm::{BatchReadRequest, Error, HubSpot, ItemError, SearchRequest};
use std::{
    future::{Future, poll_fn},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::Poll,
    time::Duration,
};

const TOKEN: &str = "/oauth/2026-09/token";
const SEARCH: &str = "/crm/v3/objects/contacts/search";
const BATCH: &str = "/crm/v3/objects/contacts/batch/read";
const PROVIDER_MESSAGE: &str = "synthetic-provider-message-999";

fn client(server: &TestServer) -> HubSpot {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    HubSpot::new(
        http,
        "synthetic-client".into(),
        "synthetic-secret".into(),
        "synthetic-refresh-0".into(),
    )
    .unwrap()
    .with_base_url(&server.url())
    .unwrap()
}

fn request(ids: &[&str]) -> BatchReadRequest {
    BatchReadRequest {
        ids: ids.iter().map(|id| id.to_string()).collect(),
        properties: vec!["email".into(), "firstname".into()],
    }
}

#[tokio::test]
async fn caller_searches_then_batch_reads_chosen_and_stored_ids() {
    let server = TestServer::start(|r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", TOKEN) => Reply::json(200, include_str!("../fixtures/oauth.json")),
        ("POST", SEARCH) => {
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            if body.get("after").is_some() {
                Reply::json(200, include_str!("../fixtures/search-next.json"))
            } else {
                Reply::json(200, include_str!("../fixtures/search-first.json"))
            }
        }
        ("POST", BATCH) => {
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            if body["inputs"].as_array().unwrap().len() == 3 {
                Reply::json(207, include_str!("../fixtures/batch-read-partial.json"))
            } else {
                Reply::json(200, include_str!("../fixtures/batch-read-complete.json"))
            }
        }
        _ => panic!("unexpected fixture route"),
    })
    .await;
    let sdk = client(&server);
    let found = sdk
        .search(SearchRequest::default(), Resume::Start)
        .items()
        .collect_complete()
        .await
        .unwrap();
    // Contacts chosen from search, plus an ID the application stored earlier.
    let mut ids: Vec<&str> = found.iter().map(|c| c.id.as_str()).collect();
    ids.push("999");

    let partial = sdk.batch_read(request(&ids)).await.unwrap();
    let outcomes: Vec<_> = partial.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(outcomes, ["101", "102", "999"]); // Request order, not provider order.
    let first = partial.get("101").unwrap().as_ref().unwrap();
    assert_eq!(
        first.properties["email"].as_deref(),
        Some("anna.smith@example.com")
    );
    assert_eq!(partial.get("102").unwrap().as_ref().unwrap().id, "102");
    let missing = partial.get("999").unwrap().as_ref().unwrap_err();
    assert!(matches!(missing, ItemError::Provider { category } if category == "OBJECT_NOT_FOUND"));
    assert_eq!(
        missing.to_string(),
        "HubSpot batch item failed (OBJECT_NOT_FOUND)"
    );
    assert!(partial.unmatched.is_empty() && partial.unattributed.is_empty());
    assert!(!format!("{partial:?} {missing:?} {missing}").contains(PROVIDER_MESSAGE));

    // A complete 200 answer for the IDs that exist.
    let complete = sdk.batch_read(request(&["101", "102"])).await.unwrap();
    assert!(complete.items.iter().all(|i| i.result.is_ok()));
    assert_eq!(complete.items[0].id, "101");

    let requests = server.requests();
    assert_eq!(requests.len(), 5); // One refresh shared by search and both batches.
    assert_eq!(requests[0].path(), TOKEN);
    for r in &requests[3..] {
        assert_eq!((r.method.as_str(), r.target.as_str()), ("POST", BATCH));
        assert_eq!(r.header("authorization"), Some("Bearer synthetic-access-1"));
        assert_eq!(r.header("content-type"), Some("application/json"));
    }
    let body: serde_json::Value = serde_json::from_slice(&requests[3].body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"inputs": [{"id": "101"}, {"id": "102"}, {"id": "999"}],
            "properties": ["email", "firstname"], "propertiesWithHistory": []})
    );
    assert_eq!(sdk.refresh_token().unwrap(), "synthetic-refresh-1");
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn batch_read_joins_the_shared_refresh_and_keeps_the_replacement() {
    let gate = Gate::new();
    let held = gate.clone();
    let exchanges = Arc::new(AtomicUsize::new(0));
    let seen = exchanges.clone();
    let server = TestServer::start(move |r: &Recorded| match r.path() {
        TOKEN => {
            assert_eq!(seen.fetch_add(1, Ordering::SeqCst), 0, "second exchange");
            Reply::json(
                200,
                r#"{"access_token":"access-new","refresh_token":"refresh-new","expires_in":1800,"token_type":"bearer"}"#,
            )
            .after(held.clone())
        }
        BATCH => Reply::json(200, include_str!("../fixtures/batch-read-complete.json")),
        "/crm/v3/objects/contacts/101" => Reply::json(
            200,
            r#"{"id":"101","properties":{"email":"anna.smith@example.com"}}"#,
        ),
        _ => panic!("unexpected fixture route"),
    })
    .await;
    let sdk = client(&server);
    let leader = {
        let sdk = sdk.clone();
        tokio::spawn(async move { sdk.get("101").await })
    };
    assert!(
        server
            .wait_until(Duration::from_secs(2), |_| exchanges.load(Ordering::SeqCst)
                == 1)
            .await
    );
    let mut follower = Box::pin(sdk.batch_read(request(&["101", "102"])));
    // The batch read is polled while the refresh response is held: it waits on the same flight.
    poll_fn(|cx| {
        assert!(follower.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    gate.open();
    let (single, batch) = tokio::join!(leader, follower);
    assert_eq!(single.unwrap().unwrap().id, "101");
    assert!(batch.unwrap().items.iter().all(|i| i.result.is_ok()));
    assert_eq!(exchanges.load(Ordering::SeqCst), 1);
    assert_eq!(sdk.refresh_token().unwrap(), "refresh-new");
    let batch_request = server
        .requests()
        .into_iter()
        .find(|r| r.path() == BATCH)
        .unwrap();
    assert_eq!(
        batch_request.header("authorization"),
        Some("Bearer access-new")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn caller_decides_whether_to_repeat_a_failed_batch() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = attempts.clone();
    let server = TestServer::start(move |r: &Recorded| match r.path() {
        TOKEN => Reply::json(200, include_str!("../fixtures/oauth.json")),
        BATCH if counted.fetch_add(1, Ordering::SeqCst) == 0 => Reply::json(
            429,
            format!(
                r#"{{"status":"error","category":"RATE_LIMITS","message":"{PROVIDER_MESSAGE}"}}"#
            ),
        )
        .header("retry-after", "1"),
        BATCH => Reply::json(200, include_str!("../fixtures/batch-read-complete.json")),
        _ => panic!("unexpected fixture route"),
    })
    .await;
    let sdk = client(&server);
    let error = sdk.batch_read(request(&["101", "102"])).await.unwrap_err();
    let Error::Concord(inner) = &error else {
        panic!("expected a Concord status error")
    };
    assert_eq!(inner.kind(), ErrorKind::Status);
    assert_eq!(inner.status().unwrap().as_u16(), 429);
    assert_eq!(inner.response().unwrap().headers()["retry-after"], "1");
    assert!(!format!("{error:?} {error}").contains(PROVIDER_MESSAGE));
    assert_eq!(attempts.load(Ordering::SeqCst), 1); // The SDK did not repeat it.

    // A batch read is safe to repeat; doing so is the caller's decision.
    let read = sdk.batch_read(request(&["101", "102"])).await.unwrap();
    assert!(read.items.iter().all(|i| i.result.is_ok()));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    server.shutdown().await;
}
