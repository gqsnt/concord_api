//! F2 / X3: one bounded typed-document operation on an any-status response. The format's own head
//! checks and context run before the body (no JSON-only `decode_body((), ..)` shortcut), the head
//! is returned beside the outcome, and every way the document can fail keeps the status.

use std::sync::atomic::Ordering;
use std::time::Duration;

use concord::{ErrorKind, Json};
use concord_check_core_concepts::{Problem, Rates, VERSIONED_CONTEXTS, Versioned};
use concord_example_testserver::{Body, Reply, TestServer};

const LIMIT: usize = 1024;

fn rates(server: &TestServer) -> Rates {
    Rates::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap()
}

async fn document_for<F: concord::Decode<Problem>>(
    reply: impl Fn() -> Reply + Send + Sync + 'static,
) -> (
    concord::ResponseMeta,
    Result<Problem, concord::Error>,
    TestServer,
) {
    let server = TestServer::start(move |_| reply()).await;
    let received = rates(&server)
        .rate("EUR", "XXX")
        .prepare()
        .unwrap()
        .receive()
        .await
        .unwrap();
    let (meta, document) = received.document::<F, Problem>(LIMIT).await;
    (meta, document, server)
}

#[tokio::test]
async fn a_typed_error_document_and_its_status() {
    let (meta, document, _s) = document_for::<Json>(|| {
        Reply::json(422, br#"{"message":"unknown currency","code":"bad_quote"}"#)
            .header("x-request-id", "req-7")
    })
    .await;
    assert_eq!(meta.status().as_u16(), 422);
    assert_eq!(meta.headers()["x-request-id"], "req-7");
    assert_eq!(meta.operation().unwrap().name(), "Rates.rate");
    assert_eq!(
        document.unwrap(),
        Problem {
            message: "unknown currency".into(),
            code: Some("bad_quote".into())
        }
    );
}

#[tokio::test]
async fn every_document_failure_keeps_the_status() {
    // (reply, expected kind)
    type Case = (fn() -> Reply, ErrorKind);
    let cases: [Case; 5] = [
        (|| Reply::json(500, b""), ErrorKind::Decode),
        (|| Reply::json(500, br#"{"message":"#), ErrorKind::Decode),
        (
            || Reply::json(500, format!(r#"{{"message":"{}"}}"#, "x".repeat(4096))),
            ErrorKind::Limit,
        ),
        (
            || {
                Reply::json(500, "")
                    .with_body(Body::FailAfter(vec![br#"{"mess"#.as_slice().into()]))
            },
            ErrorKind::Transport,
        ),
        (
            || Reply::text(503, "text/html", "<html>gateway</html>"),
            ErrorKind::Metadata,
        ),
    ];
    for (reply, kind) in cases {
        let (meta, document, _s) = document_for::<Json>(reply).await;
        let error = document.unwrap_err();
        assert_eq!(error.kind(), kind);
        assert!(meta.status().is_server_error());
        assert_eq!(
            error.head().map(|h| h.status()),
            Some(meta.status()),
            "the error carries the head too"
        );
        assert_eq!(error.operation().unwrap().name(), "Rates.rate");
    }
}

#[tokio::test]
async fn a_media_refusal_does_not_wait_for_a_body_that_never_ends() {
    let (meta, document, _s) = tokio::time::timeout(
        Duration::from_secs(5),
        document_for::<Json>(|| {
            Reply::new(502)
                .header("content-type", "text/html")
                .with_body(Body::Stall)
        }),
    )
    .await
    .expect("refused on the head alone");
    assert_eq!(meta.status().as_u16(), 502);
    let error = document.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert!(error.response().is_some(), "kept unread");
}

#[tokio::test]
async fn a_contextual_format_runs_its_head_contract_once_before_the_body() {
    VERSIONED_CONTEXTS.store(0, Ordering::SeqCst);
    let (meta, document, _s) = document_for::<Versioned>(|| {
        Reply::text(409, "text/plain; charset=utf-8", "conflict: already exists")
            .header("x-doc-version", "2")
    })
    .await;
    assert_eq!(meta.status().as_u16(), 409);
    assert_eq!(
        document.unwrap(),
        Problem {
            message: "already exists".into(),
            code: Some("v2/conflict".into())
        }
    );
    assert_eq!(VERSIONED_CONTEXTS.load(Ordering::SeqCst), 1);

    // An unsupported version is a head refusal: the stalled body is never polled.
    let (meta, document, _s) = tokio::time::timeout(
        Duration::from_secs(5),
        document_for::<Versioned>(|| {
            Reply::new(409)
                .header("content-type", "text/plain")
                .header("x-doc-version", "3")
                .with_body(Body::Stall)
        }),
    )
    .await
    .expect("refused before the body");
    assert_eq!(meta.status().as_u16(), 409);
    let error = document.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(error.codec_error().unwrap().get("version"), Some(3));
}

#[tokio::test]
async fn a_success_status_document_is_read_the_same_way() {
    let (meta, document, _s) =
        document_for::<Json>(|| Reply::json(202, br#"{"message":"queued"}"#)).await;
    assert_eq!(meta.status().as_u16(), 202);
    assert_eq!(document.unwrap().message, "queued");
}
