//! Connected workflows run exclusively against TestServer fixtures.
mod support;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_mailgun::{
    concord::{ErrorKind, bytes::Bytes},
    pagination::{Outcome, PagesError, Resume},
    *,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use support::*;

#[tokio::test]
async fn send_file_retain_id_search_page_and_inspect_each_recipient() {
    let server = TestServer::start(workflow).await;
    let api = client(&server);
    let scope = domain(&api);
    let attachment = Attachment::from_file(
        concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/report.txt"),
        "report.txt",
        "text/plain",
    )
    .unwrap();
    let accepted = scope
        .send_message(with_attachment(attachment))
        .send()
        .await
        .unwrap()
        .value;
    let retained = accepted.id.as_str().to_owned(); // application persistence boundary
    assert_eq!(retained, "<fixture-27@mg.example.test>");
    assert_eq!(
        delivery_status(&accepted.id, "alice@example.test", &[]),
        DeliveryStatus::Unknown
    );

    let mut pages = scope
        .events(range())
        .set_message_id(accepted.id.clone())
        .set_limit(PageSize::new(2).unwrap())
        .pages(cap(3))
        .unwrap();
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(
        delivery_status(&accepted.id, "alice@example.test", &first.items),
        DeliveryStatus::Accepted
    );
    assert_eq!(
        delivery_status(&accepted.id, "bob@example.test", &first.items),
        DeliveryStatus::TemporaryFailure
    );
    let second = pages.next_page().await.unwrap().unwrap();
    let mut events = first.items;
    events.extend(second.items);
    let terminal = pages.next_page().await.unwrap().unwrap();
    assert!(terminal.items.is_empty());
    assert!(terminal.next.is_none()); // the fixture still has a next URL
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    assert_eq!(
        delivery_status(&accepted.id, "alice@example.test", &events),
        DeliveryStatus::Delivered
    );
    assert_eq!(
        delivery_status(&accepted.id, "bob@example.test", &events),
        DeliveryStatus::PermanentFailure
    );
    assert_eq!(
        delivery_status(&accepted.id, "nobody@example.test", &events),
        DeliveryStatus::Unknown
    );
    assert_eq!(
        delivery_status(
            &MessageId::new("unrelated").unwrap(),
            "alice@example.test",
            &events
        ),
        DeliveryStatus::Unknown
    );
    let wire = server.requests();
    assert_eq!(wire.len(), 4);
    assert_email_wire(&wire[0]);
    assert_eq!(wire[1].query_all("message-id"), [retained]);
    assert_eq!(
        wire[2].target,
        "/v3/mg.example.test/events/page-two?opaque=a%2Bb%3D&limit=2"
    );
    assert_eq!(wire[3].target, "/v3/mg.example.test/events/empty");
    for request in &wire {
        assert_eq!(request.header("authorization"), Some(BASIC));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn bounded_search_keeps_prefix_and_resumes_only_its_original_context() {
    let server = TestServer::start(workflow).await;
    let api = client(&server);
    let query = domain(&api)
        .events(range())
        .set_recipient("alice@example.test");
    let incomplete = query
        .clone()
        .pages(cap(1))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    let resume = incomplete.error.resume().unwrap().clone();
    assert!(matches!(resume, Resume::At(_)));
    assert!(matches!(
        query
            .clone()
            .set_recipient("bob@example.test")
            .pages_from(resume.clone(), cap(3)),
        Err(Error::Link(LinkError::Context))
    ));
    let different_key =
        Mailgun::local(&server.url(), "another-key", Duration::from_secs(1)).unwrap();
    assert!(matches!(
        domain(&different_key)
            .events(range())
            .set_recipient("alice@example.test")
            .pages_from(resume.clone(), cap(3)),
        Err(Error::Link(LinkError::Context))
    ));
    assert!(matches!(
        api.domain(Domain::new("other.example.test").unwrap())
            .events(range())
            .set_recipient("alice@example.test")
            .pages_from(resume.clone(), cap(3)),
        Err(Error::Link(LinkError::Context))
    ));
    assert_eq!(server.requests().len(), 1);
    let tail = query
        .pages_from(resume, cap(2))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(tail.len(), 2);
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_page_keeps_prefix_and_cannot_report_completion() {
    let server = TestServer::start(|request: &Recorded| {
        if request.path().ends_with("/events") {
            page(FIRST, request)
        } else {
            Reply::json(503, PROBLEM)
        }
    })
    .await;
    let incomplete = domain(&client(&server))
        .events(range())
        .pages(cap(10))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    let (error, failed, progress) = incomplete.error.into_provider_error().unwrap();
    assert!(matches!(error, Error::Provider(_)));
    assert!(matches!(failed, Resume::At(_)));
    assert_eq!(progress.admitted, 2);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn provider_link_cycle_stops_before_a_duplicate_request() {
    let server = TestServer::start(|request: &Recorded| page(FIRST, request)).await;
    let incomplete = domain(&client(&server))
        .events(range())
        .pages(cap(10))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 4);
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::Link(LinkError::Cycle))
    ));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn one_shot_attachment_prepares_without_polling_and_sends_once() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, ACCEPTED)).await;
    let polls = Arc::new(AtomicUsize::new(0));
    let seen = polls.clone();
    let source = futures_util::stream::once(async move {
        seen.fetch_add(1, Ordering::SeqCst);
        Ok(Bytes::from_static(FILE))
    });
    let attachment = Attachment::stream("report.txt", "text/plain", source).unwrap();
    let op = domain(&client(&server)).send_message(with_attachment(attachment));
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let prepared = op.prepare().unwrap();
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert!(prepared.request().try_clone().is_none());
    // Drop the preparation-only probe without polling; send a fresh one-shot source below.
    drop(prepared);
    let seen = polls.clone();
    let source = futures_util::stream::once(async move {
        seen.fetch_add(1, Ordering::SeqCst);
        Ok(Bytes::from_static(FILE))
    });
    let attachment = Attachment::stream("report.txt", "text/plain", source).unwrap();
    let response = domain(&client(&server))
        .send_message(with_attachment(attachment))
        .send()
        .await
        .unwrap();
    assert_eq!(response.value.id.as_str(), "<fixture-27@mg.example.test>");
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    let request = server.single();
    assert!(request.chunked);
    // reqwest writes lowercase multipart header names; field/body semantics must be identical.
    let mut normalized = request.clone();
    normalized.body = request
        .body_text()
        .replace("content-disposition:", "Content-Disposition:")
        .replace("content-type:", "Content-Type:")
        .into_bytes();
    assert_email_wire(&normalized);
    server.shutdown().await;
}

#[tokio::test]
async fn post_upload_timeout_is_ambiguous_and_never_replays_either_source() {
    for streamed in [false, true] {
        for after_head in [false, true] {
            let server = TestServer::start(move |_: &Recorded| {
                if after_head {
                    Reply::json(200, "").with_body(Body::Stall)
                } else {
                    Reply::json(200, ACCEPTED).delay(Duration::from_secs(10))
                }
            })
            .await;
            let api = Mailgun::local(&server.url(), KEY, Duration::from_millis(150)).unwrap();
            let polls = Arc::new(AtomicUsize::new(0));
            let attachment = if streamed {
                let seen = polls.clone();
                Attachment::stream(
                    "report.txt",
                    "text/plain",
                    futures_util::stream::once(async move {
                        seen.fetch_add(1, Ordering::SeqCst);
                        Ok(Bytes::from_static(FILE))
                    }),
                )
                .unwrap()
            } else {
                Attachment::bytes("report.txt", "text/plain", FILE).unwrap()
            };
            let error = domain(&api)
                .send_message(with_attachment(attachment))
                .send()
                .await
                .unwrap_err();
            assert!(matches!(error, Error::AmbiguousSend(_)));
            let cause = error.concord_error().unwrap();
            assert_eq!(cause.kind(), ErrorKind::Transport);
            assert!(cause.transport().unwrap().is_timeout());
            assert_eq!(cause.head().is_some(), after_head);
            assert_eq!(server.requests().len(), 1); // fixture fully read the POST before withholding its reply
            assert_eq!(server.connections(), 1);
            assert_eq!(polls.load(Ordering::SeqCst), usize::from(streamed));
            server.shutdown().await;
        }
    }
}

#[tokio::test]
async fn non_rewindable_send_on_429_or_500_is_not_replayed() {
    for status in [429, 500] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, PROBLEM).header("retry-after", "0")
        })
        .await;
        let polls = Arc::new(AtomicUsize::new(0));
        let seen = polls.clone();
        let attachment = Attachment::stream(
            "report.txt",
            "text/plain",
            futures_util::stream::once(async move {
                seen.fetch_add(1, Ordering::SeqCst);
                Ok(Bytes::from_static(FILE))
            }),
        )
        .unwrap();
        let error = domain(&client(&server))
            .send_message(with_attachment(attachment))
            .send()
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Provider(_)));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn source_failure_and_late_size_overflow_remain_ambiguous_without_replay() {
    for oversized in [false, true] {
        let server = TestServer::start(|_: &Recorded| Reply::json(200, ACCEPTED)).await;
        let polls = Arc::new(AtomicUsize::new(0));
        let seen = polls.clone();
        let source = futures_util::stream::once(async move {
            seen.fetch_add(1, Ordering::SeqCst);
            if oversized {
                Ok(Bytes::from(vec![0; MAX_ATTACHMENT_BYTES + 1]))
            } else {
                Err(std::io::Error::other("synthetic-secret-source"))
            }
        });
        let attachment = Attachment::stream("report.txt", "text/plain", source).unwrap();
        let error = domain(&client(&server))
            .send_message(with_attachment(attachment))
            .send()
            .await
            .unwrap_err();
        assert!(matches!(error, Error::AmbiguousSend(_)));
        assert!(!format!("{error:?} {error}").contains("synthetic-secret-source"));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        assert!(server.connections() <= 1);
        server.shutdown().await;
    }
}

#[test]
fn delivery_projection_is_per_recipient_and_never_infers_from_tracking() {
    let id = MessageId::new("<fixture-27@mg.example.test>").unwrap();
    let mut value: serde_json::Value = serde_json::from_str(SECOND).unwrap();
    value["items"][0]["event"] = "opened".into();
    let page: EventPage = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        delivery_status(&id, "alice@example.test", &page.items),
        DeliveryStatus::Unknown
    );
    for (kind, severity, expected) in [
        (
            "failed",
            "new-severity",
            DeliveryStatus::FailedUnknownSeverity,
        ),
        ("rejected", "", DeliveryStatus::Rejected),
        ("new-event-kind", "", DeliveryStatus::Unknown),
    ] {
        value["items"][0]["event"] = kind.into();
        value["items"][0]["severity"] = severity.into();
        let page: EventPage = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            delivery_status(&id, "alice@example.test", &page.items),
            expected
        );
    }
    value["items"][0]["event"] = "delivered".into();
    value["items"][0]["flags"] = serde_json::json!({"is-test-mode": true});
    let page: EventPage = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        delivery_status(&id, "alice@example.test", &page.items),
        DeliveryStatus::TestMode
    );
    value["items"][0]["flags"] = serde_json::json!({"is-test-mode": false});
    let mut same_time = value["items"][0].clone();
    same_time["event"] = "failed".into();
    same_time["severity"] = "permanent".into();
    value["items"].as_array_mut().unwrap().push(same_time);
    let page: EventPage = serde_json::from_value(value).unwrap();
    assert_eq!(
        delivery_status(&id, "alice@example.test", &page.items),
        DeliveryStatus::Conflicting
    );
}
