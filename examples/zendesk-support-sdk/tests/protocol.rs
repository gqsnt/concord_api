//! Wire-level request shapes, every documented error class, unreadable bodies, the read/write
//! send policy, and redaction. Fixtures are listed in PROVENANCE.md.

mod support;

use std::num::NonZeroUsize;
use std::time::Duration;

use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_zendesk_support::concord::{ErrorKind, bytes::Bytes};
use concord_sdk_zendesk_support::{
    Error, IdempotencyKey, IdempotencyLookup, NewTicket, PageSize, Priority, Problem, SendPolicy,
    SortField, Status, TicketSort, TicketType, UncertainCause, Zendesk,
};
use serde_json::json;
use support::{TOKEN, api, api_with, fixture, http_builder, json_fixture};

/// A scripted reply, for tables of cases.
type Responder = Box<dyn Fn() -> Reply + Send + Sync>;
/// A parsed `Retry-After`, with the parse error erased.
type Parsed = Result<Option<Duration>, ()>;

fn key(text: &str) -> IdempotencyKey {
    IdempotencyKey::new(text).unwrap()
}

async fn serve(reply: impl Fn() -> Reply + Send + Sync + 'static) -> TestServer {
    TestServer::start(move |_: &Recorded| reply()).await
}

// ---------------------------------------------------------------- request shapes

#[tokio::test]
async fn list_tickets_request_on_the_wire() {
    let server = serve(|| json_fixture(200, "tickets-page-2.json")).await;
    let api = api(&server);
    api.tickets().list().send().await.unwrap();
    api.tickets()
        .list()
        .set_page_size(PageSize::new(25).unwrap())
        .set_after("xSynthAfter1")
        .set_sort(TicketSort::descending(SortField::UpdatedAt))
        .set_external_id("synthetic ext/1")
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    let port = server.addr().port();
    let [plain, full] = requests.as_slice() else {
        panic!()
    };
    assert_eq!(plain.method, "GET");
    assert_eq!(plain.target, "/api/v2/tickets?page%5Bsize%5D=100");
    assert_eq!(plain.host, format!("acme.zendesk.test:{port}"));
    assert_eq!(
        plain.header("authorization"),
        Some(&*format!("Bearer {TOKEN}"))
    );
    assert_eq!(plain.header("accept"), Some("application/json"));
    assert_eq!(plain.header("content-type"), None);
    assert!(plain.body.is_empty());
    assert_eq!(
        full.target,
        "/api/v2/tickets?page%5Bsize%5D=25&page%5Bafter%5D=xSynthAfter1&sort=-updated_at\
         &external_id=synthetic+ext%2F1"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn show_ticket_request_on_the_wire() {
    let server = serve(|| json_fixture(200, "ticket-103.json")).await;
    let shown = api(&server).ticket(103).show().send().await.unwrap().ticket;
    assert_eq!(shown.id, 103);
    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, "/api/v2/tickets/103");
    assert_eq!(request.header("accept"), Some("application/json"));
    server.shutdown().await;
}

#[tokio::test]
async fn create_ticket_request_on_the_wire() {
    let server = serve(|| {
        let mut ticket: serde_json::Value =
            serde_json::from_slice(&fixture("ticket-103.json")).unwrap();
        ticket["ticket"]["status"] = json!("new");
        Reply::json(201, ticket.to_string()).header("x-idempotency-lookup", "miss")
    })
    .await;
    let api = api(&server);
    let full = NewTicket::new("Synthetic body")
        .subject("Synthetic subject")
        .priority(Priority::Urgent)
        .ticket_type(TicketType::Incident)
        .external_id("synthetic-ext")
        .tags(["a", "b"]);
    api.tickets().create(full, key("k-1")).send().await.unwrap();
    api.tickets()
        .create(NewTicket::new("Only a comment"), key("k-2"))
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    let [full, minimal] = requests.as_slice() else {
        panic!()
    };
    assert_eq!(full.method, "POST");
    assert_eq!(full.target, "/api/v2/tickets");
    assert_eq!(full.header("content-type"), Some("application/json"));
    assert_eq!(full.header("accept"), Some("application/json"));
    assert_eq!(full.header("idempotency-key"), Some("k-1"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&full.body).unwrap(),
        json!({"ticket": {
            "comment": {"body": "Synthetic body"},
            "subject": "Synthetic subject",
            "priority": "urgent",
            "type": "incident",
            "external_id": "synthetic-ext",
            "tags": ["a", "b"]
        }})
    );
    assert_eq!(
        minimal.body_text(),
        r#"{"ticket":{"comment":{"body":"Only a comment"}}}"#
    );
    assert_eq!(minimal.header("idempotency-key"), Some("k-2"));
    server.shutdown().await;
}

#[tokio::test]
async fn the_idempotency_lookup_is_read_from_the_create_response_head() {
    for (headers, expected) in [
        (vec!["miss"], IdempotencyLookup::Miss),
        (vec!["hit"], IdempotencyLookup::Hit),
        (vec!["HIT"], IdempotencyLookup::Hit),
        (vec![], IdempotencyLookup::Absent),
        (vec!["maybe"], IdempotencyLookup::Unrecognized),
        (vec!["hit", "miss"], IdempotencyLookup::Unrecognized),
    ] {
        let server = TestServer::start(move |_: &Recorded| {
            let mut reply = json_fixture(201, "ticket-103.json");
            for value in &headers {
                reply = reply.header("x-idempotency-lookup", *value);
            }
            reply
        })
        .await;
        let created = api(&server)
            .tickets()
            .create(NewTicket::new("x"), key("k"))
            .send()
            .await
            .unwrap();
        assert_eq!(created.idempotency, expected);
        assert_eq!(created.ticket.id, 103);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unknown_enumeration_values_are_kept() {
    let server = serve(|| json_fixture(200, "ticket-unknown-values.json")).await;
    let ticket = api(&server).ticket(104).show().send().await.unwrap().ticket;
    assert_eq!(
        ticket.status,
        Status::Other("synthetic_future_status".into())
    );
    assert_eq!(
        ticket.priority,
        Some(Priority::Other("synthetic_future_priority".into()))
    );
    assert_eq!(
        ticket.ticket_type,
        Some(TicketType::Other("synthetic_future_type".into()))
    );
    assert_eq!(ticket.subject, None);
    assert!(ticket.tags.is_empty());
    server.shutdown().await;
}

// ---------------------------------------------------------------- inputs refused before I/O

#[tokio::test]
async fn invalid_inputs_are_refused_before_anything_is_sent() {
    assert!(matches!(
        PageSize::new(0),
        Err(Error::InvalidInput { what: "page size" })
    ));
    assert!(PageSize::new(101).is_err());
    assert_eq!(PageSize::new(1).unwrap().get(), 1);
    assert_eq!(PageSize::MAX.get(), 100);
    for bad in ["", "a b", "caf\u{e9}", "line\nbreak"] {
        assert!(IdempotencyKey::new(bad).is_err(), "{bad:?}");
    }
    for bad in ["", "two words", "tab\there", "caf\u{e9}"] {
        let error = Zendesk::connect(Default::default(), "acme", bad).unwrap_err();
        assert!(matches!(error, Error::Credential(_)), "{bad:?}");
    }

    let server = serve(|| json_fixture(200, "ticket-103.json")).await;
    for subdomain in ["acme.evil", "", "-acme", "ac_me"] {
        let api = Zendesk::for_fixture(
            http_builder(Duration::from_secs(5)),
            subdomain,
            TOKEN,
            server.addr(),
        )
        .unwrap();
        let error = api.ticket(1).show().send().await.unwrap_err();
        let Error::Concord(error) = error else {
            panic!("{subdomain:?}: {error:?}")
        };
        assert_eq!(error.kind(), ErrorKind::Build, "{subdomain:?}");
    }
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[test]
fn header_names_are_valid_for_every_endpoint() {
    Zendesk::validate_header_names().unwrap();
}

// ---------------------------------------------------------------- documented error classes

#[tokio::test]
async fn every_documented_error_status_keeps_its_status_and_document() {
    let cases: [(u16, &str, &str, Option<&str>); 9] = [
        (
            400,
            "application/json",
            "error-idempotent-mismatch.json",
            Some("IdempotentRequestError"),
        ),
        (401, "text/plain", "error-unauthorized.txt", None),
        (
            403,
            "application/json",
            "error-forbidden.json",
            Some("Forbidden"),
        ),
        (
            404,
            "application/json",
            "error-not-found.json",
            Some("RecordNotFound"),
        ),
        (404, "application/json", "error-null.json", None),
        (409, "application/json", "error-null.json", None),
        (
            422,
            "application/json",
            "error-record-invalid.json",
            Some("RecordInvalid"),
        ),
        (
            500,
            "text/plain; charset=utf-8",
            "error-unauthorized.txt",
            None,
        ),
        (503, "application/json", "error-null.json", None), // no Retry-After: not repeated
    ];
    for (status, media, name, code) in cases {
        let server = serve(move || Reply::text(status, media, fixture(name))).await;
        let error = api(&server).ticket(1).show().send().await.unwrap_err();
        let http = error
            .http()
            .unwrap_or_else(|| panic!("{status}: {error:?}"));
        assert_eq!(http.status().as_u16(), status);
        assert_eq!(http.attempts(), 1);
        assert_eq!(http.retry_after(), Ok(None));
        let problem = http.document().unwrap();
        assert_eq!(problem.error(), code, "{status} {name}");
        match name {
            "error-null.json" => assert_eq!(problem, &Problem::Null),
            "error-unauthorized.txt" => {
                assert_eq!(
                    problem.text_unredacted(),
                    Some("Synthetic: Couldn't authenticate you")
                );
            }
            _ => {}
        }
        assert_eq!(server.requests().len(), 1, "{status}: never repeated");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn record_invalid_details_are_available_but_never_formatted() {
    let server = serve(|| json_fixture(422, "error-record-invalid.json")).await;
    let error = api(&server)
        .tickets()
        .create(NewTicket::new(""), key("k"))
        .send()
        .await
        .unwrap_err();
    let Problem::Document(document) = error.http().unwrap().document().unwrap() else {
        panic!()
    };
    assert_eq!(
        document.description_unredacted(),
        Some("RecordValidation errors")
    );
    let details = document.details_unredacted().unwrap();
    assert_eq!(details["description"][0]["error"], "BlankValue");
    let shown = format!("{error} {error:?}");
    assert!(shown.contains("RecordInvalid"));
    assert!(
        !shown.contains("cannot be blank") && !shown.contains("RecordValidation"),
        "{shown}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn unreadable_error_bodies_keep_the_status() {
    let oversized = || {
        let mut sent = 0usize;
        Reply::new(422)
            .header("content-type", "application/json")
            .with_body(Body::Generate(
                Box::new(move || {
                    (sent < 70 * 1024).then(|| {
                        sent += 1024;
                        Bytes::from(vec![b' '; 1024])
                    })
                }),
                None,
            ))
    };
    let cases: Vec<(u16, ErrorKind, Responder)> = vec![
        (
            404,
            ErrorKind::Metadata,
            Box::new(|| Reply::text(404, "text/html", "<h1>proxy</h1>")),
        ),
        (429, ErrorKind::Metadata, Box::new(|| Reply::new(429))),
        (
            422,
            ErrorKind::Decode,
            Box::new(|| Reply::json(422, "{\"error\": 7}")),
        ),
        (
            400,
            ErrorKind::Decode,
            Box::new(|| Reply::json(400, "{\"error\":")),
        ),
        (422, ErrorKind::Limit, Box::new(oversized)),
        (
            500,
            ErrorKind::Transport,
            Box::new(|| {
                Reply::new(500)
                    .header("content-type", "application/json")
                    .with_body(Body::FailAfter(vec![Bytes::from_static(b"{\"error\"")]))
            }),
        ),
    ];
    for (status, kind, reply) in cases {
        let server = serve(reply).await;
        let error = api(&server).ticket(1).show().send().await.unwrap_err();
        let http = error
            .http()
            .unwrap_or_else(|| panic!("{status}: {error:?}"));
        assert_eq!(http.status().as_u16(), status);
        assert_eq!(http.document().unwrap_err().kind(), kind, "{status}");
        server.shutdown().await;
    }
}

// ---------------------------------------------------------------- unreadable success bodies

#[tokio::test]
async fn unreadable_success_bodies_of_a_read_are_concord_errors() {
    let oversized = || {
        let mut sent = 0usize;
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Generate(
                Box::new(move || {
                    (sent <= concord_sdk_zendesk_support::TICKET_BODY_LIMIT).then(|| {
                        sent += 64 * 1024;
                        Bytes::from(vec![b' '; 64 * 1024])
                    })
                }),
                None,
            ))
    };
    let cases: Vec<(ErrorKind, Responder)> = vec![
        (
            ErrorKind::Decode,
            Box::new(|| json_fixture(200, "ticket-malformed.json")),
        ),
        (
            ErrorKind::Decode,
            Box::new(|| json_fixture(200, "ticket-wrong-shape.json")),
        ),
        (
            ErrorKind::Metadata,
            Box::new(|| Reply::text(200, "text/html", fixture("ticket-103.json"))),
        ),
        (
            ErrorKind::Metadata,
            Box::new(|| Reply::new(200).body(fixture("ticket-103.json"))),
        ),
        (
            ErrorKind::Transport,
            Box::new(|| {
                Reply::new(200)
                    .header("content-type", "application/json")
                    .with_body(Body::FailAfter(vec![Bytes::from_static(b"{\"ticket\": {")]))
            }),
        ),
        (ErrorKind::Limit, Box::new(oversized)),
    ];
    for (kind, reply) in cases {
        let server = serve(reply).await;
        let error = api(&server).ticket(103).show().send().await.unwrap_err();
        let Error::Concord(error) = error else {
            panic!("{kind:?}: {error:?}")
        };
        assert_eq!(error.kind(), kind);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

// ---------------------------------------------------------------- reads honour Retry-After

#[tokio::test]
async fn a_rate_limited_read_is_repeated_after_retry_after() {
    for (status, retry_after) in [
        (429, "0"),
        (503, "0"),
        (429, "Thu, 01 Jan 1970 00:00:00 GMT"),
    ] {
        let script = concord_example_testserver::Script::new([
            Reply::text(status, "text/plain", "Synthetic: slow down")
                .header("retry-after", retry_after),
            json_fixture(200, "ticket-103.json"),
        ]);
        let server = TestServer::start(move |_: &Recorded| script.next()).await;
        let shown = api(&server).ticket(103).show().send().await.unwrap();
        assert_eq!(shown.ticket.id, 103);
        assert_eq!(server.requests().len(), 2, "{status} {retry_after}");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn a_read_is_not_repeated_without_a_usable_retry_after() {
    let cases: [(Option<&str>, Parsed); 3] = [
        (Some("120"), Ok(Some(Duration::from_secs(120)))), // over the 60 s default
        (None, Ok(None)),
        (Some("soon"), Err(())),
    ];
    for (header, parsed) in cases {
        let server = serve(move || {
            let reply = Reply::text(429, "text/plain", "Synthetic: slow down");
            match header {
                Some(value) => reply.header("retry-after", value),
                None => reply,
            }
        })
        .await;
        let error = api(&server).ticket(103).show().send().await.unwrap_err();
        let http = error.http().unwrap();
        assert_eq!(http.status().as_u16(), 429);
        assert_eq!(http.attempts(), 1);
        assert_eq!(http.retry_after().map_err(|_| ()), parsed);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn read_attempts_and_the_read_deadline_are_bounded() {
    let limited = || Reply::text(429, "text/plain", "Synthetic").header("retry-after", "0");
    let server = serve(limited).await;
    let error = api(&server).ticket(1).show().send().await.unwrap_err();
    assert_eq!(error.http().unwrap().attempts(), 3);
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;

    let server = serve(limited).await;
    let once = SendPolicy::default().read_attempts(NonZeroUsize::MIN);
    let error = api_with(&server, Duration::from_secs(5), once)
        .ticket(1)
        .show()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.http().unwrap().attempts(), 1);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;

    // A wait longer than what is left of the deadline is not started: the 429 is returned.
    let server =
        serve(|| Reply::text(429, "text/plain", "Synthetic").header("retry-after", "5")).await;
    let bounded = SendPolicy::default().read_deadline(Duration::from_secs(2));
    let error = api_with(&server, Duration::from_secs(5), bounded)
        .ticket(1)
        .show()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.http().unwrap().attempts(), 1);
    server.shutdown().await;

    // No response within the deadline at all.
    let server = serve(|| json_fixture(200, "ticket-103.json").delay(Duration::from_secs(3))).await;
    let bounded = SendPolicy::default().read_deadline(Duration::from_millis(200));
    let error = api_with(&server, Duration::from_secs(5), bounded)
        .ticket(103)
        .show()
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::DeadlineExceeded), "{error:?}");
    server.shutdown().await;
}

// ---------------------------------------------------------------- writes are sent once

#[tokio::test]
async fn a_write_is_never_repeated() {
    let create = |api: &Zendesk| {
        api.tickets()
            .create(NewTicket::new("x"), key("k-once"))
            .send()
    };

    let server =
        serve(|| Reply::text(429, "text/plain", "Synthetic").header("retry-after", "0")).await;
    let error = create(&api(&server)).await.unwrap_err();
    let http = error
        .http()
        .expect("a 429 on a write is an HTTP error: it was not processed");
    assert_eq!(http.retry_after(), Ok(Some(Duration::ZERO)));
    assert_eq!(http.attempts(), 1);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;

    for status in [500, 502, 503] {
        let server = serve(move || {
            Reply::text(status, "text/plain", "Synthetic").header("retry-after", "0")
        })
        .await;
        let error = create(&api(&server)).await.unwrap_err();
        let write = error
            .uncertain()
            .unwrap_or_else(|| panic!("{status}: {error:?}"));
        assert_eq!(write.idempotency_key(), Some(&key("k-once")));
        assert!(matches!(write.cause(), UncertainCause::ServerError(_)));
        assert_eq!(error.status().map(|s| s.as_u16()), Some(status));
        assert_eq!(server.requests().len(), 1, "{status}");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn a_write_without_a_response_is_uncertain() {
    let cases: [Responder; 2] = [
        Box::new(|| json_fixture(201, "ticket-103.json").delay(Duration::from_secs(3))),
        Box::new(Reply::hang_up),
    ];
    for reply in cases {
        let server = serve(reply).await;
        let api = api_with(&server, Duration::from_millis(300), SendPolicy::default());
        let error = api
            .tickets()
            .create(NewTicket::new("x"), key("k-lost"))
            .send()
            .await
            .unwrap_err();
        let write = error.uncertain().unwrap_or_else(|| panic!("{error:?}"));
        assert_eq!(
            write.idempotency_key().map(IdempotencyKey::as_str),
            Some("k-lost")
        );
        assert!(
            matches!(write.cause(), UncertainCause::NoResponse(e) if e.kind() == ErrorKind::Transport)
        );
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn a_refused_connection_on_a_create_is_uncertain() {
    // Review b23-review2-sol finding 1: this caller-built client follows redirects, so a connect
    // error does not prove that a create was not performed.
    // Bound but never listening: connections are refused, and while it lives no other test's
    // server can take the port (a dropped listener's port could be reused concurrently).
    let reserved = tokio::net::TcpSocket::new_v4().unwrap();
    reserved.bind(([127, 0, 0, 1], 0).into()).unwrap();
    let port = reserved.local_addr().unwrap().port();
    let http = concord_sdk_zendesk_support::concord::reqwest::Client::builder();
    let api = Zendesk::for_fixture(http, "acme", TOKEN, ([127, 0, 0, 1], port).into()).unwrap();
    let error = api
        .tickets()
        .create(NewTicket::new("x"), key("k"))
        .send()
        .await
        .unwrap_err();
    let Error::Uncertain(write) = error else {
        panic!("{error:?}")
    };
    assert_eq!(write.idempotency_key(), Some(&key("k")));
    let UncertainCause::NoResponse(error) = write.cause() else {
        panic!("{write:?}")
    };
    assert!(error.transport().is_some_and(|t| t.is_connect()));
    drop(reserved);
}

#[tokio::test]
async fn an_unreadable_create_success_is_uncertain() {
    let cases: [Responder; 3] = [
        Box::new(|| json_fixture(201, "ticket-malformed.json")),
        Box::new(|| Reply::text(201, "text/html", fixture("ticket-103.json"))),
        Box::new(|| {
            Reply::new(201)
                .header("content-type", "application/json")
                .with_body(Body::FailAfter(vec![Bytes::from_static(b"{\"ticket\"")]))
        }),
    ];
    for reply in cases {
        let server = serve(reply).await;
        let error = api(&server)
            .tickets()
            .create(NewTicket::new("x"), key("k-unread"))
            .send()
            .await
            .unwrap_err();
        let write = error.uncertain().unwrap_or_else(|| panic!("{error:?}"));
        assert!(matches!(
            write.cause(),
            UncertainCause::UnreadableSuccess(_)
        ));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

// ---------------------------------------------------------------- redaction

#[tokio::test]
async fn the_token_and_provider_text_are_never_formatted() {
    let server = serve(|| json_fixture(200, "ticket-103.json")).await;
    let api = api(&server);
    let mut shown = vec![
        format!("{api:?}"),
        format!("{:?}", api.tickets()),
        format!("{:?}", api.ticket(1)),
    ];
    let prepared = api
        .tickets()
        .create(NewTicket::new("x"), key("k"))
        .prepare()
        .unwrap();
    assert!(prepared.request().headers()["authorization"].is_sensitive());
    // The idempotency key is an identifier, not a secret: it stays readable for debugging.
    let debug = format!("{prepared:?}");
    assert!(debug.contains("authorization: <redacted>"), "{debug}");
    assert!(debug.contains("idempotency-key: k"), "{debug}");
    shown.push(debug);
    shown.push(format!("{:?}", api.tickets().list().prepare().unwrap()));
    server.shutdown().await;

    // A provider body that echoes the token, on a read and on a write.
    let echo = format!("Synthetic: token {TOKEN} is invalid");
    let reply = move || Reply::text(401, "text/plain", echo.clone());
    let server = serve(reply.clone()).await;
    let read = api_with(&server, Duration::from_secs(5), SendPolicy::default())
        .ticket(1)
        .show()
        .send()
        .await
        .unwrap_err();
    assert!(
        read.http()
            .unwrap()
            .document()
            .unwrap()
            .text_unredacted()
            .unwrap()
            .contains(TOKEN)
    );
    server.shutdown().await;
    let server = serve(move || reply().header("retry-after", "0").delay(Duration::ZERO)).await;
    let write = api_with(&server, Duration::from_secs(5), SendPolicy::default())
        .tickets()
        .create(NewTicket::new("x"), key("k"))
        .send()
        .await
        .unwrap_err();
    server.shutdown().await;
    let server = serve(|| Reply::text(503, "text/plain", format!("Synthetic {TOKEN}"))).await;
    let uncertain = api_with(&server, Duration::from_secs(5), SendPolicy::default())
        .tickets()
        .create(NewTicket::new("x"), key("k"))
        .send()
        .await
        .unwrap_err();
    assert!(uncertain.uncertain().is_some());
    server.shutdown().await;
    let credential =
        Zendesk::connect(Default::default(), "acme", &format!("{TOKEN} x")).unwrap_err();

    for error in [read, write, uncertain, credential] {
        shown.push(error.to_string());
        shown.push(format!("{error:?}"));
        let mut source = std::error::Error::source(&error);
        while let Some(inner) = source {
            shown.push(inner.to_string());
            source = inner.source();
        }
    }
    for text in &shown {
        assert!(!text.contains(TOKEN), "{text}");
        assert!(!text.contains("is invalid"), "{text}");
    }
}

#[test]
fn fixture_constructor_requires_a_loopback_socket() {
    for bad in ["192.0.2.1:1234", "127.0.0.1:0", "[2001:db8::1]:1234"] {
        assert!(matches!(
            Zendesk::for_fixture(
                concord_sdk_zendesk_support::concord::reqwest::Client::builder(),
                "acme",
                TOKEN,
                bad.parse().unwrap(),
            ),
            Err(Error::InvalidInput {
                what: "fixture address"
            })
        ));
    }
}
