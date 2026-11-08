//! The two connected workflows end to end against local fixtures, with the required failures:
//! a rate-limited read that honours `Retry-After`, and an uncertain create surfaced without a
//! duplicate ticket. Expectations come from the fixtures and the documented rules, not from the
//! implementation.

mod support;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord_example_testserver::{Recorded, Reply, TestServer, paused};
use concord_sdk_zendesk_support::concord_pagination::{PagesError, Resume};
use concord_sdk_zendesk_support::{
    Error, IdempotencyKey, IdempotencyLookup, NewTicket, PageSize, PaginationError, Priority,
    SendPolicy, Status, TicketType, UncertainCause, ticket_pages,
};
use support::{Arrivals, FakeZendesk, api, api_with, json_fixture};

fn pages(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

/// Workflow 1: list every ticket with cursor pagination, pick one, retrieve it. The second page
/// is rate limited once with `Retry-After: 1`; the read waits that long and is sent again.
#[tokio::test]
async fn list_tickets_with_cursor_pagination_then_retrieve_the_selected_ticket() {
    let arrivals = Arrivals::default();
    let limited = Arc::new(AtomicUsize::new(0));
    let (log, count) = (arrivals.clone(), limited.clone());
    let server = TestServer::start(move |r: &Recorded| {
        log.record(r);
        match (r.path(), r.query_all("page[after]").as_slice()) {
            ("/api/v2/tickets", []) => json_fixture(200, "tickets-page-1.json"),
            ("/api/v2/tickets", [after]) if after == "xSynthAfter1" => {
                if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    Reply::text(429, "text/plain", "Synthetic: rate limited")
                        .header("retry-after", "1")
                } else {
                    json_fixture(200, "tickets-page-2.json")
                }
            }
            ("/api/v2/tickets/103", []) => json_fixture(200, "ticket-103.json"),
            _ => Reply::new(404),
        }
    })
    .await;
    let api = api(&server);

    let list = api
        .tickets()
        .list()
        .set_page_size(PageSize::new(2).unwrap());
    let tickets = ticket_pages(list, Resume::Start, pages(10))
        .items()
        .collect_complete()
        .await
        .unwrap();
    let ids: Vec<u64> = tickets.iter().map(|t| t.id).collect();
    assert_eq!(ids, [101, 102, 103]);

    // Select the ticket on hold, then retrieve its current state.
    let selected = tickets.iter().find(|t| t.status == Status::Hold).unwrap();
    let shown = api.ticket(selected.id).show().send().await.unwrap().ticket;
    assert_eq!(&shown, selected);
    assert_eq!(shown.tags, ["synthetic", "login"]);
    assert_eq!(shown.priority, Some(Priority::Normal));

    // The wire: page 1, page 2 rate limited, page 2 again after the delay, then the ticket.
    let seen = arrivals.all();
    let targets: Vec<&str> = seen.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(
        targets,
        [
            "GET /api/v2/tickets?page%5Bsize%5D=2",
            "GET /api/v2/tickets?page%5Bsize%5D=2&page%5Bafter%5D=xSynthAfter1",
            "GET /api/v2/tickets?page%5Bsize%5D=2&page%5Bafter%5D=xSynthAfter1",
            "GET /api/v2/tickets/103",
        ]
    );
    let waited = seen[2].1.duration_since(seen[1].1);
    assert!(waited >= Duration::from_secs(1), "waited {waited:?}");
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// Workflow 2: create a ticket, then retrieve its resulting state.
#[tokio::test]
async fn create_a_ticket_then_retrieve_its_resulting_state() {
    let fake = FakeZendesk::default();
    let responder = fake.clone();
    let server = TestServer::start(move |r: &Recorded| responder.respond(r)).await;
    let api = api(&server);

    let new = NewTicket::new("Synthetic: the export button does nothing.")
        .subject("Synthetic: export broken")
        .priority(Priority::High)
        .ticket_type(TicketType::Problem)
        .external_id("synthetic-order-77")
        .tags(["synthetic", "export"]);
    let key = IdempotencyKey::new("synthetic-order-77-create").unwrap();
    let created = api.tickets().create(new, key).send().await.unwrap();
    assert_eq!(created.idempotency, IdempotencyLookup::Miss);
    assert_eq!(created.ticket.id, 9001);
    assert_eq!(created.ticket.status, Status::New);

    let state = api
        .ticket(created.ticket.id)
        .show()
        .send()
        .await
        .unwrap()
        .ticket;
    assert_eq!(state, created.ticket);
    assert_eq!(
        state.description.as_deref(),
        Some("Synthetic: the export button does nothing.")
    );
    assert_eq!(state.external_id.as_deref(), Some("synthetic-order-77"));
    assert_eq!(state.ticket_type, Some(TicketType::Problem));
    assert_eq!(fake.ticket_count(), 1);
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// The required failure of workflow 2: the create reaches the server, which stores the ticket,
/// but the response arrives after the client's timeout. The SDK surfaces an uncertain write with
/// the key, sends nothing else, and the caller resolves it by replaying the same request.
// Paused clock: the 300 ms timeout fires only once the server holds the first create; the replay
// and the read complete while time stands still.
#[tokio::test(start_paused = true)]
async fn an_uncertain_create_is_surfaced_once_and_resolved_by_replaying_its_key() {
    let fake = FakeZendesk::default();
    let responder = fake.clone();
    let server = TestServer::start(move |r: &Recorded| responder.respond(r)).await;
    let timeout = Duration::from_millis(300);
    let api = api_with(&server, timeout, SendPolicy::default());
    fake.delay_next_create(Duration::from_secs(3));

    let new = NewTicket::new("Synthetic: refund not received.").subject("Synthetic: refund");
    let key = IdempotencyKey::new("synthetic-refund-5").unwrap();
    let create = api.tickets().create(new.clone(), key.clone()).send();
    let error = paused::time_out(&server, |_| true, timeout, create)
        .await
        .unwrap_err();
    let Error::Uncertain(write) = &error else {
        panic!("expected an uncertain write, got {error:?}")
    };
    assert_eq!(write.operation(), "Zendesk.tickets.create");
    assert_eq!(write.idempotency_key(), Some(&key));
    let UncertainCause::NoResponse(cause) = write.cause() else {
        panic!("{error:?}")
    };
    assert!(cause.transport().is_some_and(|t| t.is_timeout()));
    // The SDK sent the create exactly once; the account holds the one ticket it created.
    assert_eq!(fake.creates_received(), 1);
    assert_eq!(fake.ticket_count(), 1);

    // The caller's explicit resolution: the same body and key replay the original response.
    let replayed = paused::drive(api.tickets().create(new, key).send())
        .await
        .unwrap();
    assert_eq!(replayed.idempotency, IdempotencyLookup::Hit);
    assert_eq!(replayed.ticket.id, 9001);
    assert_eq!(fake.ticket_count(), 1, "no duplicate ticket");

    let state = paused::drive(api.ticket(9001).show().send())
        .await
        .unwrap()
        .ticket;
    assert_eq!(state.status, Status::New);
    assert_eq!(state.subject.as_deref(), Some("Synthetic: refund"));
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

/// The other documented resolution: find the ticket by the `external_id` the create carried.
#[tokio::test]
async fn an_uncertain_create_can_be_resolved_by_its_external_id() {
    let fake = FakeZendesk::default();
    let responder = fake.clone();
    let server = TestServer::start(move |r: &Recorded| responder.respond(r)).await;
    let api = api_with(&server, Duration::from_millis(300), SendPolicy::default());
    fake.delay_next_create(Duration::from_secs(3));

    let new = NewTicket::new("Synthetic: parcel lost.").external_id("synthetic-parcel-9");
    let key = IdempotencyKey::new("synthetic-parcel-9-create").unwrap();
    let error = api.tickets().create(new, key).send().await.unwrap_err();
    assert!(error.uncertain().is_some(), "{error:?}");

    let found = api
        .tickets()
        .list()
        .set_external_id("synthetic-parcel-9")
        .send()
        .await
        .unwrap();
    let [ticket] = found.tickets.as_slice() else {
        panic!("{found:?}")
    };
    assert_eq!(ticket.id, 9001);
    assert_eq!(fake.ticket_count(), 1);
    let list = &server.requests()[1];
    assert_eq!(
        list.query_pairs(),
        [
            ("page[size]".to_owned(), "100".to_owned()),
            ("external_id".to_owned(), "synthetic-parcel-9".to_owned())
        ]
    );
    server.shutdown().await;
}

/// Reusing a key with another body is refused by the provider and creates nothing.
#[tokio::test]
async fn replaying_a_key_with_another_body_is_refused_without_a_ticket() {
    let fake = FakeZendesk::default();
    let responder = fake.clone();
    let server = TestServer::start(move |r: &Recorded| responder.respond(r)).await;
    let api = api(&server);
    let key = IdempotencyKey::new("synthetic-shared-key").unwrap();
    api.tickets()
        .create(NewTicket::new("Synthetic: first."), key.clone())
        .send()
        .await
        .unwrap();
    let error = api
        .tickets()
        .create(NewTicket::new("Synthetic: different."), key)
        .send()
        .await
        .unwrap_err();
    let http = error.http().expect("an HTTP error");
    assert_eq!(http.status().as_u16(), 400);
    let problem = http.document().unwrap();
    assert_eq!(problem.error(), Some("IdempotentRequestError"));
    assert_eq!(
        error.to_string(),
        "Zendesk responded 400 Bad Request (IdempotentRequestError)"
    );
    assert_eq!(fake.ticket_count(), 1);
    server.shutdown().await;
}

/// A cap reached while the provider has more pages is a bounded stop with the cursor to resume
/// from; resuming from it reads the rest.
#[tokio::test]
async fn a_bounded_traversal_resumes_from_its_cursor() {
    let server = TestServer::start(|r: &Recorded| match r.query_all("page[after]").as_slice() {
        [] => json_fixture(200, "tickets-page-1.json"),
        [after] if after == "xSynthAfter1" => json_fixture(200, "tickets-page-2.json"),
        _ => Reply::new(404),
    })
    .await;
    let api = api(&server);
    let list = api.tickets().list();

    let incomplete = ticket_pages(list.clone(), Resume::Start, pages(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    let PagesError::Bounded { resume, .. } = incomplete.error else {
        panic!("expected a bounded stop")
    };
    assert_eq!(resume, Resume::At("xSynthAfter1".to_owned()));

    let rest = ticket_pages(list, resume, pages(1))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.iter().map(|t| t.id).collect::<Vec<_>>(), [103]);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

async fn first_traversal_error(page: &'static str) -> (Error, usize) {
    let server =
        TestServer::start(
            move |r: &Recorded| match r.query_all("page[after]").as_slice() {
                [] => json_fixture(200, page),
                // A repeated cursor: the page for `xSynthAfter1` names `xSynthAfter1` again.
                _ => json_fixture(200, "tickets-page-1.json"),
            },
        )
        .await;
    let api = api(&server);
    let error = ticket_pages(api.tickets().list(), Resume::Start, pages(5))
        .items()
        .collect_complete()
        .await
        .unwrap_err()
        .error;
    let requests = server.requests().len();
    server.shutdown().await;
    let (error, _, _) = error.into_provider_error().expect("a provider error");
    (error, requests)
}

#[tokio::test]
async fn pages_that_cannot_be_followed_end_the_traversal() {
    let (error, requests) = first_traversal_error("page-more-without-cursor.json").await;
    assert!(matches!(
        error,
        Error::Pagination(PaginationError::MissingCursor)
    ));
    assert_eq!(requests, 1);

    let (error, requests) = first_traversal_error("page-inconsistent-next.json").await;
    assert!(matches!(
        error,
        Error::Pagination(PaginationError::InconsistentNextLink)
    ));
    assert_eq!(requests, 1);

    let (error, requests) = first_traversal_error("tickets-page-1.json").await;
    assert!(matches!(
        error,
        Error::Pagination(PaginationError::RepeatedCursor)
    ));
    assert_eq!(requests, 2);
}
