//! Both connected workflows, including a committed write with a timed-out response.

mod support;

use concord_example_testserver::{Gate, Recorded, Reply, TestServer, paused};
use concord_sdk_square_payments::{
    Error, PagesError, Resume, Square, SquareConfig, bearer, transport,
};
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::Duration,
};
use support::*;

#[tokio::test]
async fn create_then_retrieve() {
    let server = TestServer::start(|request: &Recorded| {
        headers(request);
        match (request.method.as_str(), request.target.as_str()) {
            ("POST", "/v2/payments") => {
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&request.body).unwrap(),
                    serde_json::from_str::<serde_json::Value>(REQUEST).unwrap()
                );
                Reply::json(200, PAYMENT)
            }
            ("GET", "/v2/payments/synthetic-payment-001") => Reply::json(200, PAYMENT),
            _ => panic!("unexpected fixture route"),
        }
    })
    .await;
    let api = client(&server);
    let created = api
        .payments()
        .create(create(1000))
        .send()
        .await
        .unwrap()
        .payment;
    let retrieved = api
        .payments()
        .get(&created.id)
        .send()
        .await
        .unwrap()
        .payment;
    assert_eq!(retrieved, created);
    assert_eq!(retrieved.amount_money.amount, 1000);
    assert_eq!(retrieved.status, "COMPLETED");
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

// Paused clock: the 150 ms timeout fires only once the server holds the first create; the replay,
// the read and the mismatched create complete while virtual time stands still.
#[tokio::test(start_paused = true)]
async fn timeout_exact_replay_and_mismatched_replay() {
    // A synthetic ledger commits before holding the first response head forever.
    // It compares actual key/body bytes, independently of SDK serialization.
    let committed = Arc::new(Mutex::new(None::<Vec<u8>>));
    let ledger = Arc::clone(&committed);
    let server = TestServer::start(move |request: &Recorded| {
        headers(request);
        if request.method == "GET" {
            assert_eq!(request.target, "/v2/payments/synthetic-payment-001");
            return Reply::json(200, PAYMENT);
        }
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/v2/payments");
        let json: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(json["idempotency_key"], "caller-key-001");
        let mut saved = ledger.lock().unwrap();
        match saved.as_ref() {
            None => {
                *saved = Some(request.body.clone());
                Reply::json(200, PAYMENT).after(Gate::new())
            }
            Some(body) if body == &request.body => Reply::json(200, PAYMENT),
            Some(_) => Reply::json(400, MISMATCH),
        }
    })
    .await;
    let timeout = Duration::from_millis(150);
    let api = Square::new(
        transport(timeout).unwrap(),
        SquareConfig {
            authorization: bearer(TOKEN).unwrap(),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    let original = api.payments().create(create(1000));
    let error = paused::time_out(&server, |_| true, timeout, original.clone().send())
        .await
        .unwrap_err();
    assert!(error.create_outcome_unknown());
    let Error::Concord(error) = error else {
        panic!("expected transport timeout")
    };
    assert!(error.transport().unwrap().is_timeout());
    assert_eq!(
        error.delivery(),
        concord_sdk_square_payments::concord::Delivery::Unknown
    );
    assert_eq!(server.requests().len(), 1, "no automatic replay");

    let replayed = paused::drive(original.send()).await.unwrap().payment;
    let retrieved = paused::drive(api.payments().get(&replayed.id).send())
        .await
        .unwrap()
        .payment;
    assert_eq!(retrieved, replayed);
    let error = paused::drive(api.payments().create(create(1001)).send())
        .await
        .unwrap_err();
    let Error::Provider(error) = error else {
        panic!("expected mismatch")
    };
    assert_eq!(error.meta.status().as_u16(), 400);
    assert!(error.is_idempotency_mismatch());
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests[0].body, requests[1].body,
        "exact key and body after timeout"
    );
    assert_ne!(requests[0].body, requests[3].body);
    assert_eq!(
        committed.lock().unwrap().as_ref().unwrap(),
        &requests[0].body
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn cursor_pages_then_inspect_returned_payment() {
    let server = TestServer::start(|request: &Recorded| {
        headers(request);
        assert_eq!(request.method, "GET");
        if request.path() == "/v2/payments/synthetic-payment-001" {
            assert!(request.query().is_none());
            return Reply::json(200, PAYMENT);
        }
        assert_eq!(request.path(), "/v2/payments");
        assert_eq!(request.query_all("limit"), ["1"]);
        match request.query_all("cursor").as_slice() {
            [] => Reply::json(200, PAGE_1),
            [cursor] if cursor == CURSOR => Reply::json(200, PAGE_2),
            _ => panic!("cursor changed"),
        }
    })
    .await;
    let api = client(&server);
    let mut pages = api
        .payments()
        .list()
        .set_limit(1u32)
        .pages(Resume::Start, NonZeroUsize::new(2).unwrap());
    let first = pages.next_page().await.unwrap().unwrap();
    let second = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].id, "synthetic-payment-001");
    assert_eq!(second.items[0].amount_money.amount, 250);
    assert_eq!(second.items[0].status, "FAILED");
    assert!(pages.next_page().await.unwrap().is_none());
    let inspected = api
        .payments()
        .get(&first.items[0].id)
        .send()
        .await
        .unwrap()
        .payment;
    assert_eq!(inspected, first.items[0]);
    assert_eq!(server.requests().len(), 3);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn bounded_empty_page_continuation_and_resume() {
    let server =
        TestServer::start(
            |request: &Recorded| match request.query_all("cursor").as_slice() {
                [] => Reply::json(200, include_str!("../fixtures/empty-next.json")),
                [cursor] if cursor == CURSOR => {
                    Reply::json(200, include_str!("../fixtures/empty-final.json"))
                }
                _ => panic!("unexpected cursor"),
            },
        )
        .await;
    let list = client(&server).payments().list();
    let mut pages = list
        .clone()
        .pages(Resume::Start, NonZeroUsize::new(1).unwrap());
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    let error = pages.next_page().await.unwrap_err();
    assert!(!format!("{error:?}").contains(CURSOR));
    let PagesError::Bounded { resume, .. } = error else {
        panic!("expected bounded")
    };
    let mut resumed = list.pages(resume, NonZeroUsize::new(1).unwrap());
    assert!(resumed.next_page().await.unwrap().unwrap().items.is_empty());
    assert!(resumed.next_page().await.unwrap().is_none());
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}
