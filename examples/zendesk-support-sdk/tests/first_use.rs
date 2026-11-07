//! Independent public-API consumer; all HTTP traffic resolves to a loopback fixture.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_zendesk_support::concord::ErrorKind;
use concord_sdk_zendesk_support::concord::reqwest;
use concord_sdk_zendesk_support::concord_pagination::{Outcome, PagesError, Resume};
use concord_sdk_zendesk_support::{
    Error, IdempotencyKey, IdempotencyLookup, NewTicket, PageSize, Priority, Problem, SendPolicy,
    SortField, Status, TicketSort, UncertainCause, Zendesk, ticket_pages,
};
use serde_json::{Value, json};

const TOKEN: &str = "first-use-synthetic-oauth-token";
const CURSOR: &str = "eyJpZCI6NDIwMX0=/+ cursor";

fn count(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn local_api(server: &TestServer) -> Zendesk {
    let http = reqwest::Client::builder()
        .no_proxy()
        .resolve("acme.localhost", server.addr())
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    Zendesk::connect(http, "acme", TOKEN)
        .unwrap()
        .with_base_url(format!("http://localhost:{}", server.addr().port()))
        .unwrap()
}

fn ticket(id: u64, status: &str, external_id: &str) -> Value {
    json!({
        "id": id,
        "url": format!("https://acme.zendesk.com/api/v2/tickets/{id}"),
        "external_id": external_id,
        "type": "incident",
        "subject": "Monthly export stalls at 80%",
        "description": "The November export has been stalled for fifteen minutes.",
        "priority": "high",
        "status": status,
        "requester_id": 71002,
        "submitter_id": 71001,
        "assignee_id": 71003,
        "organization_id": 9001,
        "group_id": 52,
        "tags": ["billing", "export"],
        "created_at": "2026-11-07T08:00:00Z",
        "updated_at": "2026-11-07T09:15:00Z",
        "via": {"channel": "api"}
    })
}

fn page(tickets: Vec<Value>, more: bool, after: Option<&str>) -> Reply {
    Reply::json(
        200,
        json!({
            "tickets": tickets,
            "meta": {"has_more": more, "after_cursor": after, "before_cursor": null},
            "links": {"next": null, "prev": null}
        })
        .to_string(),
    )
}

fn assert_read(request: &Recorded, path: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), path);
    assert_eq!(
        request.header_all("authorization"),
        [format!("Bearer {TOKEN}")]
    );
    assert_eq!(request.header_all("accept"), ["application/json"]);
    assert_eq!(request.header("idempotency-key"), None);
    assert!(request.body.is_empty());
}

#[tokio::test]
async fn list_cursor_pages_then_retrieve_selected_ticket() {
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&arrivals);
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        let mut times = observed.lock().unwrap();
        times.push(Instant::now());
        if times.len() == 1 {
            return Reply::json(
                429,
                r#"{"error":"TooManyRequests","description":"Wait one second"}"#,
            )
            .header("Retry-After", "1")
            .header("X-Zendesk-Request-Id", "req-list-throttle");
        }
        if request.path() == "/api/v2/tickets/4201" {
            assert_read(request, "/api/v2/tickets/4201");
            assert_eq!(request.query(), None);
            return Reply::json(
                200,
                json!({"ticket": ticket(4201, "open", "invoice-2026-11")}).to_string(),
            );
        }
        assert_read(request, "/api/v2/tickets");
        assert_eq!(request.query_all("page[size]"), ["2"]);
        assert_eq!(request.query_all("sort"), ["-updated_at"]);
        match request.query_all("page[after]").first().map(String::as_str) {
            None => page(
                vec![ticket(4201, "hold", "invoice-2026-11")],
                true,
                Some(CURSOR),
            ),
            Some(CURSOR) => page(vec![], true, Some("empty-page-cursor")),
            Some("empty-page-cursor") => page(
                vec![ticket(4202, "pending", "invoice-2026-10")],
                false,
                Some("unused-last-cursor"),
            ),
            other => panic!("unexpected cursor: {other:?}"),
        }
    })
    .await;
    let api = local_api(&server);
    let list = api
        .tickets()
        .list()
        .set_page_size(PageSize::new(2).unwrap())
        .set_sort(TicketSort::descending(SortField::UpdatedAt));
    let mut pages = ticket_pages(list, Resume::Start, count(3));
    let mut tickets = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        tickets.extend(page.items);
    }
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.checkpoint(), None);
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert_eq!(report.progress.admitted, 3);
    assert_eq!(report.progress.succeeded, 3);
    assert_eq!(report.progress.failed, 0);
    assert_eq!(
        tickets.iter().map(|t| t.id).collect::<Vec<_>>(),
        [4201, 4202]
    );
    let selected = tickets.iter().find(|t| t.status == Status::Hold).unwrap();
    let state = api.ticket(selected.id).show().send().await.unwrap().ticket;
    assert_eq!(state.id, 4201);
    assert_eq!(state.status, Status::Open);
    assert_eq!(state.external_id.as_deref(), Some("invoice-2026-11"));
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    {
        let times = arrivals.lock().unwrap();
        assert!(times[1].duration_since(times[0]) >= Duration::from_secs(1));
    }
    assert!(
        requests
            .iter()
            .all(|r| r.host == format!("acme.localhost:{}", server.addr().port()))
    );
    assert_eq!(requests[0].target, requests[1].target);
    assert_eq!(requests[2].query_all("page[after]"), [CURSOR]);
    assert_eq!(requests[3].query_all("page[after]"), ["empty-page-cursor"]);
    assert_eq!(requests[0].query_pairs().len(), 2);
    assert_eq!(requests[2].query_pairs().len(), 3);
    for request in &requests {
        assert_read(request, request.path());
    }
    assert!(
        !format!(
            "{api:?} {:?} {:?}",
            api.tickets().list(),
            api.ticket(4201).show().prepare().unwrap()
        )
        .contains(TOKEN)
    );
    server.shutdown().await;

    check_read_failures_and_partial_results().await;
}

async fn check_read_failures_and_partial_results() {
    // A cap preserves the successful prefix and an explicit resume identity.
    let server = TestServer::start(|_: &Recorded| -> Reply {
        page(
            vec![ticket(4201, "hold", "invoice-2026-11")],
            true,
            Some(CURSOR),
        )
    })
    .await;
    let api = local_api(&server);
    let incomplete = ticket_pages(api.tickets().list(), Resume::Start, count(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        incomplete
            .collected
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        [4201]
    );
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    assert_eq!(incomplete.error.outcome(), Outcome::Bounded);
    assert_eq!(
        incomplete.error.resume(),
        Some(&Resume::At(CURSOR.to_owned()))
    );
    assert_eq!(incomplete.error.progress().admitted, 1);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;

    // An actual later-page error also retains the prefix and the failed cursor.
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_read(request, "/api/v2/tickets");
        if request.query_all("page[after]").is_empty() {
            page(
                vec![ticket(4201, "hold", "invoice-2026-11")],
                true,
                Some(CURSOR),
            )
        } else {
            assert_eq!(request.query_all("page[after]"), [CURSOR]);
            Reply::json(
                403,
                r#"{"error":"Forbidden","description":"Agent cannot access this group"}"#,
            )
            .header("X-Zendesk-Request-Id", "req-page-denied")
        }
    })
    .await;
    let api = local_api(&server);
    let incomplete = ticket_pages(api.tickets().list(), Resume::Start, count(5))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].id, 4201);
    assert_eq!(
        incomplete.error.resume(),
        Some(&Resume::At(CURSOR.to_owned()))
    );
    let http = incomplete.error.provider_error().unwrap().http().unwrap();
    assert_eq!(http.status().as_u16(), 403);
    assert_eq!(http.attempts(), 1);
    assert_eq!(http.request_id(), Some("req-page-denied"));
    assert_eq!(http.document().unwrap().error(), Some("Forbidden"));
    assert_eq!(
        http.meta().operation().unwrap().name(),
        "Zendesk.tickets.list"
    );
    assert_eq!(incomplete.error.progress().succeeded, 1);
    assert_eq!(incomplete.error.progress().failed, 1);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;

    // Retry exhaustion is distinct from a successful end of pagination.
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::json(429, r#"{"error":"TooManyRequests"}"#)
            .header("Retry-After", "0")
            .header("X-Zendesk-Request-Id", "req-exhausted")
    })
    .await;
    let error = local_api(&server)
        .ticket(4201)
        .show()
        .send()
        .await
        .unwrap_err();
    let http = error.http().unwrap();
    assert_eq!(http.status().as_u16(), 429);
    assert_eq!(http.attempts(), 3);
    assert_eq!(http.retry_after().unwrap(), Some(Duration::ZERO));
    assert_eq!(http.request_id(), Some("req-exhausted"));
    assert_eq!(http.meta().status().as_u16(), 429);
    assert_eq!(http.meta().operation().unwrap().method(), Some("GET"));
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;

    // A missing or invalid delay, or a wait outside the policy, is surfaced once.
    for delay in [None, Some("invalid"), Some("61")] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            let reply = Reply::json(429, r#"{"error":"TooManyRequests"}"#);
            match delay {
                Some(value) => reply.header("Retry-After", value),
                None => reply,
            }
        })
        .await;
        let error = local_api(&server)
            .ticket(4201)
            .show()
            .send()
            .await
            .unwrap_err();
        let http = error.http().unwrap();
        assert_eq!(http.attempts(), 1);
        match delay {
            None => assert_eq!(http.retry_after().unwrap(), None),
            Some("invalid") => assert!(http.retry_after().is_err()),
            _ => assert_eq!(http.retry_after().unwrap(), Some(Duration::from_secs(61))),
        }
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }

    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::json(429, r#"{"error":"TooManyRequests"}"#).header("Retry-After", "1")
    })
    .await;
    let error = local_api(&server)
        .set_policy(SendPolicy::default().read_deadline(Duration::from_millis(100)))
        .ticket(4201)
        .show()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.http().unwrap().attempts(), 1);
    assert_eq!(
        error.http().unwrap().retry_after().unwrap(),
        Some(Duration::from_secs(1))
    );
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;

    // Malformed success/error bodies keep their different error boundaries.
    for (status, media, body, kind) in [
        (200, "application/json", "{broken", ErrorKind::Decode),
        (
            200,
            "text/html",
            "<h1>Proxy login</h1>",
            ErrorKind::Metadata,
        ),
        (404, "application/json", "{broken", ErrorKind::Decode),
    ] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::text(status, media, body).header("X-Zendesk-Request-Id", "req-malformed")
        })
        .await;
        let error = local_api(&server)
            .ticket(4201)
            .show()
            .send()
            .await
            .unwrap_err();
        if status == 200 {
            let Error::Concord(inner) = error else {
                panic!("expected read decode/metadata failure");
            };
            assert_eq!(inner.kind(), kind);
        } else {
            let http = error.http().unwrap();
            assert_eq!(http.status().as_u16(), 404);
            assert_eq!(http.document().unwrap_err().kind(), kind);
            assert_eq!(http.request_id(), Some("req-malformed"));
        }
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }

    // A continuation that repeats the input cursor fails, never silently ends.
    let server =
        TestServer::start(|_: &Recorded| -> Reply { page(vec![], true, Some(CURSOR)) }).await;
    let mut pages = ticket_pages(
        local_api(&server).tickets().list(),
        Resume::At(CURSOR.to_owned()),
        count(3),
    );
    let error = pages.next_page().await.unwrap_err();
    assert!(matches!(error.provider_error(), Some(Error::Pagination(_))));
    assert_eq!(error.resume(), Some(&Resume::At(CURSOR.to_owned())));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(pages.finish().outcome, Outcome::Failed);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

const EXTERNAL_ID: &str = "export/2026-11:customer 9001";
const KEY: &str = "support-export-9001-20261107";
const COMMENT: &str = "The November export has been stalled for fifteen minutes.";

fn new_ticket() -> NewTicket {
    NewTicket::new(COMMENT)
        .subject("Monthly export stalls at 80%")
        .priority(Priority::High)
        .external_id(EXTERNAL_ID)
        .tags(["billing", "export"])
}

fn assert_create(request: &Recorded) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.path(), "/api/v2/tickets");
    assert_eq!(request.query(), None);
    assert_eq!(
        request.header_all("authorization"),
        [format!("Bearer {TOKEN}")]
    );
    assert_eq!(request.header_all("accept"), ["application/json"]);
    assert_eq!(request.header_all("content-type"), ["application/json"]);
    assert_eq!(request.header_all("idempotency-key"), [KEY]);
    assert_eq!(
        serde_json::from_slice::<Value>(&request.body).unwrap(),
        json!({
            "ticket": {
                "comment": {"body": COMMENT},
                "subject": "Monthly export stalls at 80%",
                "priority": "high",
                "external_id": EXTERNAL_ID,
                "tags": ["billing", "export"]
            }
        })
    );
}

#[tokio::test]
async fn create_ticket_then_retrieve_state_without_duplicate_on_uncertainty() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        if request.method == "POST" {
            assert_create(request);
            Reply::json(
                201,
                json!({"ticket": ticket(4301, "open", EXTERNAL_ID), "audit": {"id": 81001}})
                    .to_string(),
            )
            .header("x-idempotency-lookup", "miss")
            .header("X-Zendesk-Request-Id", "req-created")
        } else {
            assert_read(request, "/api/v2/tickets/4301");
            assert_eq!(request.query(), None);
            Reply::json(
                200,
                json!({"ticket": ticket(4301, "pending", EXTERNAL_ID)}).to_string(),
            )
        }
    })
    .await;
    let api = local_api(&server);
    let key = IdempotencyKey::new(KEY).unwrap(); // Persisted business key, chosen before sending.
    let created = api
        .tickets()
        .create(new_ticket(), key)
        .send()
        .await
        .unwrap();
    assert_eq!(created.ticket.id, 4301);
    assert_eq!(created.ticket.status, Status::Open);
    assert_eq!(created.idempotency, IdempotencyLookup::Miss);
    assert_eq!(created.ticket.priority, Some(Priority::High));
    let state = api
        .ticket(created.ticket.id)
        .show()
        .send()
        .await
        .unwrap()
        .ticket;
    assert_eq!(state.id, created.ticket.id);
    assert_eq!(state.status, Status::Pending);
    assert_eq!(state.external_id.as_deref(), Some(EXTERNAL_ID));
    assert_eq!(state.description.as_deref(), Some(COMMENT));
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|r| r.method.as_str())
            .collect::<Vec<_>>(),
        ["POST", "GET"]
    );
    server.shutdown().await;

    // All three uncertain outcomes may have committed. Reconcile by reads only.
    for failure in ["lost-response", "server-error", "malformed-success"] {
        let committed = Arc::new(Mutex::new(None::<Value>));
        let stored = Arc::clone(&committed);
        let server = TestServer::start(move |request: &Recorded| -> Reply {
            if request.method == "POST" {
                assert_create(request);
                let mut state = stored.lock().unwrap();
                assert!(state.is_none(), "the SDK must not issue a duplicate create");
                *state = Some(ticket(4302, "open", EXTERNAL_ID));
                match failure {
                    "lost-response" => Reply::hang_up(),
                    "server-error" => Reply::json(503, r#"{"error":"InternalError"}"#)
                        .header("Retry-After", "0")
                        .header("X-Zendesk-Request-Id", "req-write-503"),
                    _ => Reply::json(201, "{truncated"),
                }
            } else if request.path() == "/api/v2/tickets" {
                assert_read(request, "/api/v2/tickets");
                assert_eq!(request.query_all("external_id"), [EXTERNAL_ID]);
                assert_eq!(request.query_all("page[size]"), ["100"]);
                assert_eq!(request.query_pairs().len(), 2);
                page(vec![stored.lock().unwrap().clone().unwrap()], false, None)
            } else {
                assert_read(request, "/api/v2/tickets/4302");
                assert_eq!(request.query(), None);
                Reply::json(
                    200,
                    json!({"ticket": stored.lock().unwrap().clone().unwrap()}).to_string(),
                )
            }
        })
        .await;
        let api = local_api(&server);
        let error = api
            .tickets()
            .create(new_ticket(), IdempotencyKey::new(KEY).unwrap())
            .send()
            .await
            .unwrap_err();
        let uncertain = error
            .uncertain()
            .expect("must surface uncertain create to application");
        assert_eq!(uncertain.operation(), "Zendesk.tickets.create");
        assert_eq!(uncertain.idempotency_key().unwrap().as_str(), KEY);
        match (failure, uncertain.cause()) {
            ("lost-response", UncertainCause::NoResponse(inner)) => {
                assert_eq!(inner.kind(), ErrorKind::Transport)
            }
            ("server-error", UncertainCause::ServerError(http)) => {
                assert_eq!(http.status().as_u16(), 503);
                assert_eq!(http.attempts(), 1);
                assert_eq!(http.retry_after().unwrap(), Some(Duration::ZERO));
                assert_eq!(http.request_id(), Some("req-write-503"));
                assert_eq!(http.document().unwrap().error(), Some("InternalError"));
            }
            ("malformed-success", UncertainCause::UnreadableSuccess(inner)) => {
                assert_eq!(inner.kind(), ErrorKind::Decode)
            }
            _ => panic!("unexpected uncertainty cause"),
        }
        assert!(!format!("{error:?} {error}").contains(TOKEN));
        assert_eq!(server.requests().len(), 1, "no hidden write retry");
        assert!(committed.lock().unwrap().is_some());
        let matches = ticket_pages(
            api.tickets().list().set_external_id(EXTERNAL_ID),
            Resume::Start,
            count(5),
        )
        .items()
        .collect_complete()
        .await
        .unwrap();
        // external_id is not unique: only an unambiguous match resolves this fixture.
        assert_eq!(
            matches.len(),
            1,
            "leave missing/ambiguous reconciliation unresolved"
        );
        let resulting = api
            .ticket(matches[0].id)
            .show()
            .send()
            .await
            .unwrap()
            .ticket;
        assert_eq!(resulting.id, 4302);
        assert_eq!(resulting.external_id.as_deref(), Some(EXTERNAL_ID));
        assert_eq!(resulting.status, Status::Open);
        let requests = server.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests.iter().filter(|r| r.method == "POST").count(), 1);
        server.shutdown().await;
    }

    check_create_rejections_and_inputs().await;
}

async fn check_create_rejections_and_inputs() {
    for status in [422, 429] {
        let server = TestServer::start(move |request: &Recorded| -> Reply {
            assert_create(request);
            Reply::json(
                status,
                json!({
                    "error": "RecordInvalid",
                    "description": format!("Rejected submitted token {TOKEN}"),
                    "details": {"base": [{"error": "InvalidValue", "description": COMMENT}]}
                })
                .to_string(),
            )
            .header("Retry-After", "1")
            .header("X-Zendesk-Request-Id", "req-rejected")
            .header("Set-Cookie", format!("secret={TOKEN}"))
        })
        .await;
        let error = local_api(&server)
            .tickets()
            .create(new_ticket(), IdempotencyKey::new(KEY).unwrap())
            .send()
            .await
            .unwrap_err();
        assert!(error.uncertain().is_none());
        let http = error.http().unwrap();
        assert_eq!(http.status().as_u16(), status);
        assert_eq!(http.attempts(), 1);
        assert_eq!(http.retry_after().unwrap(), Some(Duration::from_secs(1)));
        assert_eq!(http.request_id(), Some("req-rejected"));
        assert_eq!(
            http.meta().operation().unwrap().name(),
            "Zendesk.tickets.create"
        );
        let Problem::Document(document) = http.document().unwrap() else {
            panic!("expected provider document");
        };
        assert_eq!(document.error(), "RecordInvalid");
        assert!(document.description_unredacted().unwrap().contains(TOKEN));
        assert_eq!(
            document.details_unredacted().unwrap()["base"][0]["description"],
            COMMENT
        );
        let formatted = format!("{error} {error:?} {http:?} {:?}", http.meta());
        assert!(!formatted.contains(TOKEN));
        assert!(!formatted.contains(COMMENT));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }

    let server = TestServer::start(|_: &Recorded| -> Reply {
        panic!("invalid inputs must not send");
    })
    .await;
    let api = local_api(&server);
    for size in [0, 101] {
        assert!(matches!(
            PageSize::new(size),
            Err(Error::InvalidInput { .. })
        ));
    }
    for key in ["", "key with spaces", "key\r\nInjected", "clé"] {
        assert!(matches!(
            IdempotencyKey::new(key),
            Err(Error::InvalidInput { .. })
        ));
    }
    for token in ["", "token with spaces", "token\n", "tökén"] {
        let error = Zendesk::connect(
            concord_sdk_zendesk_support::http_client().unwrap(),
            "acme",
            token,
        )
        .unwrap_err();
        assert!(matches!(error, Error::Credential(_)));
        if !token.is_empty() {
            assert!(!format!("{error:?} {error}").contains(token));
        }
    }
    let error = api
        .set_subdomain("bad/name")
        .tickets()
        .list()
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Concord(ref inner) if inner.kind() == ErrorKind::Build));
    assert_eq!(server.requests().len(), 0);
    server.shutdown().await;
}
