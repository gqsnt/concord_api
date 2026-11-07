//! Public caller contract: send metadata, retain the ID, search by tag, resume and inspect.
//! All provider-shaped responses are synthetic; only HTTP loopback is used.
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_mailgun::{
    Attachment, DeliveryStatus, Domain, Error, EventRange, LinkError, Mailgun, TextEmail,
    concord::bytes::Bytes, delivery_status, pagination::PagesError,
};
use serde_json::json;
use std::{num::NonZeroUsize, time::Duration};

fn fixture(request: &Recorded) -> Reply {
    let origin = format!("http://{}", request.host);
    let (items, next) = match (request.method.as_str(), request.path()) {
        ("POST", "/v3/mg.example.test/messages") => {
            return Reply::json(
                200,
                r#"{"id":"<metadata-27@mg.example.test>","message":"Queued. Thank you."}"#,
            );
        }
        ("GET", "/v3/mg.example.test/events") => (
            json!([{
                "id": "accepted-27", "event": "accepted", "timestamp": 1700000001.0,
                "recipient": "alice@example.test",
                "message": {"headers": {"message-id": "metadata-27@mg.example.test"}},
                "tags": ["CRM+reports"],
                "user-variables": {"customer-id": 42, "context": {"active": true}, "empty": null, "label": "Q3"}
            }]),
            format!("{origin}/v3/mg.example.test/events/next?opaque=a%2Bb"),
        ),
        ("GET", "/v3/mg.example.test/events/next") => (
            json!([{
                "id": "delivered-27", "event": "delivered", "timestamp": 1700000002.0,
                "recipient": "alice@example.test",
                "message": {"headers": {"message-id": "metadata-27@mg.example.test"}},
                "tags": ["CRM+reports"],
                "user-variables": {"customer-id": 42, "context": {"active": true}, "empty": null, "label": "Q3"}
            }]),
            format!("{origin}/v3/mg.example.test/events/end?opaque=c%2Bd"),
        ),
        ("GET", "/v3/mg.example.test/events/end") => (
            json!([]),
            format!("{origin}/v3/mg.example.test/events/unused"),
        ),
        _ => panic!("unexpected local fixture request"),
    };
    Reply::json(
        200,
        json!({"items": items, "paging": {
            "next": next, "previous": format!("{origin}/v3/mg.example.test/events/previous")
        }})
        .to_string(),
    )
}

#[tokio::test]
async fn caller_tags_file_and_stream_sends_then_searches_and_resumes_by_tag() {
    for streamed in [false, true] {
        let server = TestServer::start(fixture).await;
        let api = Mailgun::local(&server.url(), "synthetic-key", Duration::from_secs(2)).unwrap();
        let domain = api.domain(Domain::new("mg.example.test").unwrap());
        let file = if streamed {
            Attachment::stream(
                "report.txt",
                "text/plain",
                futures_util::stream::once(async {
                    Ok(Bytes::from_static(b"Report for customer 42\n"))
                }),
            )
            .unwrap()
        } else {
            Attachment::from_file(
                concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/report.txt"),
                "report.txt",
                "text/plain",
            )
            .unwrap()
        };
        let email = TextEmail::new(
            "sender@mg.example.test",
            vec!["alice@example.test".into()],
            "Customer report",
            "Please see the attached report.",
            file,
        )
        .unwrap()
        .tag("CRM+reports")
        .unwrap()
        .variable("customer-id", json!(42))
        .unwrap()
        .variable("context", json!({"active": true}))
        .unwrap()
        .variable("empty", json!(null))
        .unwrap()
        .variable("label", json!("Q3"))
        .unwrap();
        let accepted = domain.send_message(email).send().await.unwrap();
        let id = accepted.value.id;
        let retained = id.as_str().to_owned();
        assert_eq!(retained, "<metadata-27@mg.example.test>");
        let search = domain
            .events(EventRange::new(1700000000, 1700000100).unwrap())
            .set_message_id(id.clone())
            .set_recipient("alice@example.test")
            .set_tags("CRM+reports");
        let shown = format!("{search:?} {:?}", search.clone().prepare().unwrap());
        assert!(!shown.contains("CRM+reports"));

        // A bounded prefix is still only acceptance. Resume is bound to the tag filter too.
        let incomplete = search
            .clone()
            .pages(NonZeroUsize::new(1).unwrap())
            .unwrap()
            .items()
            .collect_complete()
            .await
            .unwrap_err();
        assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
        assert_eq!(
            delivery_status(&id, "alice@example.test", &incomplete.collected),
            DeliveryStatus::Accepted
        );
        let resume = incomplete.error.resume().unwrap().clone();
        let refused = search
            .clone()
            .set_tags("another-tag")
            .pages_from(resume.clone(), NonZeroUsize::new(2).unwrap());
        assert!(matches!(refused, Err(Error::Link(LinkError::Context))));
        assert_eq!(server.requests().len(), 2);
        let rest = search
            .pages_from(resume, NonZeroUsize::new(2).unwrap())
            .unwrap()
            .items()
            .collect_complete()
            .await
            .unwrap();
        let mut events = incomplete.collected;
        events.extend(rest);
        assert_eq!(events.len(), 2);
        for event in &events {
            assert_eq!(event.tags, ["CRM+reports"]);
            assert_eq!(event.user_variables["customer-id"], json!(42));
            assert_eq!(event.user_variables["context"], json!({"active": true}));
            assert_eq!(event.user_variables["empty"], json!(null));
            assert_eq!(event.user_variables["label"], json!("Q3"));
            assert!(!format!("{event:?}").contains("CRM+reports"));
        }
        assert_eq!(
            delivery_status(&id, "alice@example.test", &events),
            DeliveryStatus::Delivered
        );
        let requests = server.requests();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[1].query_all("tags"), ["CRM+reports"]);
        assert_eq!(requests[1].query_all("message-id"), [retained]);
        assert_eq!(requests[1].query_all("recipient"), ["alice@example.test"]);
        // Continuations stay provider-owned; the initial tag filter is never re-appended.
        assert_eq!(
            requests[2].target,
            "/v3/mg.example.test/events/next?opaque=a%2Bb"
        );
        assert_eq!(
            requests[3].target,
            "/v3/mg.example.test/events/end?opaque=c%2Bd"
        );
        assert_eq!(requests[0].chunked, streamed);
        server.shutdown().await;
    }
}
