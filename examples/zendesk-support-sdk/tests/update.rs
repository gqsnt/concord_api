//! Collision-safe update, exercised only against the loopback fixture server.
mod support;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_zendesk_support::{Error, Status, TicketUpdate, UncertainCause};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use support::{api, fixture, json_fixture};

#[tokio::test]
async fn update_wire_and_status_classification() {
    for (status, collision) in [(409, true), (400, false)] {
        let server = TestServer::start(move |r: &Recorded| {
            if r.method == "GET" {
                json_fixture(200, "ticket-103.json")
            } else {
                Reply::json(status, "null")
            }
        })
        .await;
        let api = api(&server);
        let ticket = api.ticket(103).show().send().await.unwrap().ticket;
        let err = api
            .ticket(ticket.id)
            .update(TicketUpdate::new(&ticket, Status::Solved, "Resolved"))
            .send()
            .await
            .unwrap_err();
        assert_eq!(matches!(err, Error::Collision(_)), collision);
        assert_eq!(err.status().unwrap().as_u16(), status);
        assert_eq!(err.http().unwrap().attempts(), 1);
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        let put = &requests[1];
        assert_eq!(put.method, "PUT");
        assert_eq!(put.path(), "/api/v2/tickets/103");
        assert_eq!(put.header("content-type"), Some("application/json"));
        assert_eq!(put.header("idempotency-key"), None);
        assert_eq!(
            serde_json::from_slice::<Value>(&put.body).unwrap(),
            json!({"ticket": {
                "safe_update": true, "updated_stamp": "2026-10-05T16:45:00Z",
                "status": "solved", "comment": {"body": "Resolved"}
            }})
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn consumer_rereads_after_collision_and_chooses_a_new_update() {
    let state = Arc::new(Mutex::new((false, false)));
    let fake = Arc::clone(&state);
    let server = TestServer::start(move |r: &Recorded| {
        let mut state = fake.lock().unwrap();
        let mut doc: Value = serde_json::from_slice(&fixture("ticket-103.json")).unwrap();
        if r.method == "GET" {
            if state.0 {
                doc["ticket"]["updated_at"] = json!("2026-10-05T17:00:00Z");
            }
            if state.1 {
                doc["ticket"]["status"] = json!("solved");
            }
            return Reply::json(200, doc.to_string());
        }
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        if !state.0 {
            state.0 = true; // Another agent changed the ticket since the read.
            return Reply::json(409, "null");
        }
        assert_eq!(body["ticket"]["updated_stamp"], "2026-10-05T17:00:00Z");
        state.1 = true;
        doc["ticket"]["updated_at"] = json!("2026-10-05T17:01:00Z");
        doc["ticket"]["status"] = body["ticket"]["status"].clone();
        Reply::json(200, doc.to_string())
    })
    .await;
    let api = api(&server);
    let first = api.ticket(103).show().send().await.unwrap().ticket;
    let error = api
        .ticket(first.id)
        .update(TicketUpdate::new(&first, Status::Solved, "Done"))
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Collision(_)));
    let fresh = api.ticket(first.id).show().send().await.unwrap().ticket;
    assert_ne!(first.updated_at, fresh.updated_at);
    let result = api
        .ticket(fresh.id)
        .update(TicketUpdate::new(
            &fresh,
            Status::Solved,
            "Done after review",
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(result.ticket.status, Status::Solved);
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|r| r.method.as_str())
            .collect::<Vec<_>>(),
        ["GET", "PUT", "GET", "PUT"]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn lost_update_response_is_uncertain_and_never_replayed() {
    let server = TestServer::start(|r: &Recorded| {
        if r.method == "GET" {
            json_fixture(200, "ticket-103.json")
        } else {
            Reply::hang_up()
        }
    })
    .await;
    let api = api(&server);
    let ticket = api.ticket(103).show().send().await.unwrap().ticket;
    let error = api
        .ticket(ticket.id)
        .update(TicketUpdate::new(&ticket, Status::Open, "Investigating"))
        .send()
        .await
        .unwrap_err();
    let write = error.uncertain().unwrap();
    assert!(matches!(write.cause(), UncertainCause::NoResponse(_)));
    assert_eq!(write.idempotency_key(), None);
    assert!(error.to_string().contains("re-read"));
    assert!(!error.to_string().contains("idempotency key"));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn update_server_failure_and_unreadable_success_are_uncertain() {
    for status in [503, 200] {
        let server = TestServer::start(move |r: &Recorded| {
            if r.method == "GET" {
                json_fixture(200, "ticket-103.json")
            } else if status == 503 {
                Reply::json(503, "null").header("retry-after", "0")
            } else {
                Reply::json(200, "{broken")
            }
        })
        .await;
        let api = api(&server);
        let ticket = api.ticket(103).show().send().await.unwrap().ticket;
        let error = api
            .ticket(ticket.id)
            .update(TicketUpdate::new(&ticket, Status::Open, "Investigating"))
            .send()
            .await
            .unwrap_err();
        let write = error.uncertain().unwrap();
        assert_eq!(write.idempotency_key(), None);
        if status == 503 {
            assert!(matches!(write.cause(), UncertainCause::ServerError(_)));
        } else {
            assert!(matches!(
                write.cause(),
                UncertainCause::UnreadableSuccess(_)
            ));
        }
        assert_eq!(server.requests().len(), 2);
        server.shutdown().await;
    }
}
