//! Regression expectations from the Mailgun sources cited in PROVENANCE.md.
mod support;

use concord_example_testserver::{Recorded, Reply, TestServer, paused};
use concord_sdk_mailgun::{
    Attachment, Error, InputError, Mailgun, TextEmail, concord::bytes::Bytes,
};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use support::*;

fn attachment(streamed: bool, polls: &Arc<AtomicUsize>) -> Attachment {
    if streamed {
        let polls = polls.clone();
        Attachment::stream(
            "report.txt",
            "text/plain",
            futures_util::stream::once(async move {
                polls.fetch_add(1, Ordering::SeqCst);
                Ok(Bytes::from_static(FILE))
            }),
        )
        .unwrap()
    } else {
        Attachment::bytes("report.txt", "text/plain", FILE).unwrap()
    }
}

fn tagged(email: TextEmail) -> TextEmail {
    email
        .tag("September newsletter")
        .unwrap()
        .tag("CRM+reports")
        .unwrap()
        .tag("third")
        .unwrap()
        .variable("customer-id", json!(42))
        .unwrap()
        .variable("context", json!({"active": true, "labels": ["a", null]}))
        .unwrap()
        .variable("string_value", json!("quote \" + café\nnext line"))
        .unwrap()
}

fn field_values<'a>(parts: &'a [(String, Vec<u8>)], name: &str) -> Vec<&'a [u8]> {
    let disposition = format!("content-disposition: form-data; name=\"{name}\"");
    parts
        .iter()
        .filter(|(headers, _)| {
            headers
                .to_ascii_lowercase()
                .lines()
                .any(|h| h == disposition)
        })
        .map(|(_, body)| body.as_slice())
        .collect()
}

fn assert_metadata(request: &Recorded) {
    assert_eq!(request.header("authorization"), Some(BASIC));
    let parts = multipart(request);
    assert_eq!(parts.len(), 12); // Existing 6 parts + 3 tags + 3 variables.
    assert_eq!(
        field_values(&parts, "o:tag"),
        [b"September newsletter".as_slice(), b"CRM+reports", b"third",]
    );
    for (name, expected) in [
        ("v:customer-id", json!(42)),
        ("v:context", json!({"active": true, "labels": ["a", null]})),
        ("v:string_value", json!("quote \" + café\nnext line")),
    ] {
        let fields = field_values(&parts, name);
        assert_eq!(fields.len(), 1, "literal multipart field {name}");
        assert_eq!(
            serde_json::from_slice::<Value>(fields[0]).unwrap(),
            expected
        );
    }
    let file = parts
        .iter()
        .find(|(h, _)| h.contains("filename=\"report.txt\""))
        .unwrap();
    assert_eq!(file.1, FILE);
    assert_eq!(
        field_values(&parts, "to"),
        [b"alice@example.test".as_slice(), b"bob@example.test"]
    );
}

#[tokio::test]
async fn metadata_has_identical_literal_fields_for_buffered_and_streamed_attachments() {
    for streamed in [false, true] {
        let server = TestServer::start(|_: &Recorded| Reply::json(200, ACCEPTED)).await;
        let polls = Arc::new(AtomicUsize::new(0));
        let make_email = || tagged(with_attachment(attachment(streamed, &polls)));
        let scope = domain(&client(&server));
        let email = make_email();
        assert!(!format!("{email:?}").contains("CRM+reports"));
        let prepared = scope.send_message(email).prepare().unwrap();
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        if streamed {
            assert!(prepared.request().try_clone().is_none());
        }
        drop(prepared);
        assert_eq!(server.connections(), 0);
        let accepted = scope.send_message(make_email()).send().await.unwrap();
        assert_eq!(accepted.value.id.as_str(), "<fixture-27@mg.example.test>");
        assert_eq!(polls.load(Ordering::SeqCst), usize::from(streamed));
        assert_eq!(server.connections(), 1);
        let request = server.single();
        assert_eq!(request.chunked, streamed);
        assert_metadata(&request);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn fourth_tag_and_invalid_names_are_refused_without_io_or_source_polls() {
    let server = TestServer::start(|_: &Recorded| panic!("validation must not send")).await;
    let polls = Arc::new(AtomicUsize::new(0));
    for streamed in [false, true] {
        let make_email = || with_attachment(attachment(streamed, &polls));
        let error = make_email()
            .tag("a")
            .unwrap()
            .tag("b")
            .unwrap()
            .tag("c")
            .unwrap()
            .tag("fourth-secret")
            .unwrap_err();
        assert!(matches!(error, Error::Input(InputError::TagLimit)));
        assert!(!format!("{error:?} {error}").contains("fourth-secret"));
        for name in [
            "",
            "v:already-prefixed",
            "a:b",
            "with space",
            "quote\"",
            "bad\r\nname",
            "café",
            "dot.name",
            "slash/name",
        ] {
            let error = make_email().variable(name, json!(true)).unwrap_err();
            assert!(
                matches!(error, Error::Input(InputError::VariableName)),
                "{name:?}"
            );
        }
        for tag in [
            "".to_owned(),
            "x".repeat(129),
            "café".to_owned(),
            "bad\n".to_owned(),
        ] {
            assert!(matches!(
                make_email().tag(tag),
                Err(Error::Input(InputError::Tag))
            ));
        }
        // Boundary values and the complete documented local name alphabet are accepted.
        make_email()
            .tag("x".repeat(128))
            .unwrap()
            .variable("AZaz09_-", json!(null))
            .unwrap();
    }
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert_eq!(server.connections(), 0);
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn a_repeated_variable_name_replaces_its_json_value() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, ACCEPTED)).await;
    domain(&client(&server))
        .send_message(
            email()
                .variable("key", json!("old"))
                .unwrap()
                .variable("key", json!({"new": true}))
                .unwrap(),
        )
        .send()
        .await
        .unwrap();
    let parts = multipart(&server.single());
    assert_eq!(parts.len(), 7);
    let values = field_values(&parts, "v:key");
    assert_eq!(values.len(), 1);
    assert_eq!(
        serde_json::from_slice::<Value>(values[0]).unwrap(),
        json!({"new": true})
    );
    server.shutdown().await;
}

#[tokio::test]
async fn metadata_does_not_grant_replay_after_an_ambiguous_send_timeout() {
    for streamed in [false, true] {
        let server = TestServer::start(|_: &Recorded| {
            Reply::json(200, ACCEPTED).delay(Duration::from_secs(5))
        })
        .await;
        let timeout = Duration::from_millis(150);
        let api = Mailgun::local(&server.url(), KEY, timeout).unwrap();
        let polls = Arc::new(AtomicUsize::new(0));
        // Paused clock: the 150 ms timeout fires only once the server has read the whole send.
        tokio::time::pause();
        let send = domain(&api)
            .send_message(tagged(with_attachment(attachment(streamed, &polls))))
            .send();
        let error = paused::time_out(&server, |_| true, timeout, send)
            .await
            .unwrap_err();
        tokio::time::resume();
        assert!(matches!(error, Error::AmbiguousSend(_)));
        assert!(
            error
                .concord_error()
                .unwrap()
                .transport()
                .unwrap()
                .is_timeout()
        );
        assert!(
            !server
                .wait_until(Duration::from_millis(200), |s| s.connections() > 1)
                .await
        );
        assert_eq!(server.connections(), 1);
        assert_eq!(polls.load(Ordering::SeqCst), usize::from(streamed));
        assert_metadata(&server.single());
        server.shutdown().await;
    }
}
