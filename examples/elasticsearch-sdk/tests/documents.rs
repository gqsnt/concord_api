//! Focused document protocol regressions. All replies are local, synthetic fixtures based on
//! the GET/index/OCC references read 2026-11-07; live provider calls: NOT_RUN.
mod common;

use common::*;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_elasticsearch::{
    ApiKey, DocumentId, DocumentSource, DocumentVersion, Elasticsearch, Error, GetDocument,
    IndexName, IndexResult, Invalid, Protocol,
    concord::{Delivery, ErrorKind, reqwest},
    document::DOCUMENT_LIMIT,
};
use serde_json::{Value, json};
use std::time::Duration;

fn found() -> Value {
    json!({"_index":"products","_id":"1567","_version":1,"_seq_no":362,
        "_primary_term":2,"found":true,"_source":{"product":"r2d2",
        "details":"A resourceful astromech droid"}})
}

fn indexed() -> Value {
    json!({"_index":"products","_id":"1567","_version":2,"_seq_no":363,
        "_primary_term":2,"result":"updated","_shards":{"total":2,"successful":2,"failed":0}})
}

fn reply(status: u16, value: Value) -> Reply {
    Reply::json(status, serde_json::to_vec(&value).unwrap())
}

#[tokio::test]
async fn generated_document_endpoints_keep_the_token_and_typed_absence() {
    let (server, es) = serve(vec![
        reply(200, found()),
        reply(200, indexed()),
        reply(
            404,
            json!({"_index":"products","_id":"absent","found":false}),
        ),
    ])
    .await;
    let scope = es.api().index(IndexName::new("products").unwrap());
    let GetDocument::Found(document) = scope
        .get_document(DocumentId::new("1567").unwrap())
        .send()
        .await
        .unwrap()
    else {
        panic!("expected found");
    };
    assert_eq!(document.index(), "products");
    assert_eq!(document.id(), "1567");
    assert_eq!(document.version().seq_no(), 362);
    assert_eq!(document.version().primary_term(), 2);
    let mut source = document.source::<Value>().unwrap().unwrap();
    source["tags"] = json!(["droid"]);
    let source = DocumentSource::new(&source).unwrap();
    let endpoint =
        scope.conditional_index(DocumentId::new("1567").unwrap(), source, document.version());
    assert!(!format!("{document:?} {endpoint:?}").contains("resourceful astromech"));
    let answer = endpoint.send().await.unwrap();
    assert_eq!(answer.result, IndexResult::Updated);
    assert_eq!(answer.version, DocumentVersion::new(363, 2).unwrap());
    assert_eq!(answer.document_version, 2);
    assert_eq!(answer.shards.failed, 0);
    assert!(
        matches!(scope.get_document(DocumentId::new("absent").unwrap()).send().await.unwrap(),
        GetDocument::NotFound { index, id } if index == "products" && id == "absent")
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].target, "/products/_doc/1567");
    assert!(requests[0].body.is_empty());
    assert_eq!(requests[1].method, "PUT");
    assert_eq!(
        requests[1].target,
        "/products/_doc/1567?if_seq_no=362&if_primary_term=2"
    );
    assert_eq!(requests[1].header("content-type"), Some("application/json"));
    assert_eq!(
        body_json(&requests[1]),
        json!({"product":"r2d2",
        "details":"A resourceful astromech droid","tags":["droid"]})
    );
    for request in requests {
        assert_eq!(
            request.header("authorization"),
            Some(format!("ApiKey {KEY}").as_str())
        );
        assert_eq!(request.header("accept"), Some("application/json"));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn missing_index_and_unreadable_404s_are_errors_and_status_stays_authoritative() {
    for response in [
        reply(
            404,
            json!({"error":{"type":"index_not_found_exception","reason":"private reason"},"status":404}),
        ),
        Reply::json(404, b"{"),
        Reply::text(404, "text/html", "missing proxy route"),
        reply(404, json!({"found":false})),
        Reply::json(
            404,
            vec![b' '; concord_sdk_elasticsearch::ERROR_DOCUMENT_LIMIT + 1],
        ),
    ] {
        let (server, es) = serve(vec![response]).await;
        let error = es.get_document("products", "1567").await.unwrap_err();
        assert_eq!(error.status(), Some(404));
        assert!(!error.is_uncertain());
        assert!(matches!(&error, Error::Provider(_)));
        assert!(!format!("{error} {error:?}").contains("private reason"));
        server.single();
        server.shutdown().await;
    }
}

#[tokio::test]
async fn missing_or_invalid_tokens_and_contradictory_found_status_are_never_a_document() {
    let mut missing_seq = found();
    missing_seq.as_object_mut().unwrap().remove("_seq_no");
    let mut missing_term = found();
    missing_term
        .as_object_mut()
        .unwrap()
        .remove("_primary_term");
    let mut zero_term = found();
    zero_term["_primary_term"] = json!(0);
    let mut negative_seq = found();
    negative_seq["_seq_no"] = json!(-1);
    for value in [missing_seq, missing_term, zero_term, negative_seq] {
        let (server, es) = serve(vec![reply(200, value)]).await;
        let error = es.get_document("products", "1567").await.unwrap_err();
        assert_eq!(concord_kind(&error), Some(ErrorKind::Decode));
        server.shutdown().await;
    }
    for (status, value) in [
        (200, json!({"_index":"products","_id":"1567","found":false})),
        (404, found()),
    ] {
        let (server, es) = serve(vec![reply(status, value)]).await;
        assert!(matches!(
            es.get_document("products", "1567").await,
            Err(Error::Protocol(Protocol::DocumentStatus))
        ));
        server.shutdown().await;
    }
}

#[tokio::test]
async fn conflict_is_typed_even_if_its_error_body_is_unreadable_and_is_never_retried() {
    for response in [
        reply(
            409,
            json!({"error":{"type":"version_conflict_engine_exception",
            "reason":"private document content"},"status":409}),
        ),
        Reply::json(409, b"{"),
        Reply::text(409, "text/html", "private document content"),
        Reply::json(
            409,
            vec![b' '; concord_sdk_elasticsearch::ERROR_DOCUMENT_LIMIT + 1],
        ),
    ] {
        let (server, es) = serve(vec![response.header("x-request-id", "conflict-fixture")]).await;
        let error = es
            .api()
            .index(IndexName::new("products").unwrap())
            .conditional_index(
                DocumentId::new("1567").unwrap(),
                DocumentSource::new(&json!({"tags":["droid"]})).unwrap(),
                DocumentVersion::new(362, 2).unwrap(),
            )
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status(), Some(409));
        assert!(!error.is_uncertain());
        assert!(!format!("{error} {error:?}").contains("private document content"));
        let Error::VersionConflict(provider) = error else {
            panic!("expected actionable conflict");
        };
        assert_eq!(
            provider.meta().headers()["x-request-id"],
            "conflict-fixture"
        );
        if let Ok(document) = provider.document() {
            assert_eq!(document.status, Some(409));
            assert_eq!(
                provider.error_type(),
                Some("version_conflict_engine_exception")
            );
        }
        server.single();
        server.shutdown().await;
    }
}

#[tokio::test]
async fn timed_out_conditional_write_is_uncertain_before_or_after_the_head_and_sent_once() {
    for after_head in [false, true] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            if after_head {
                Reply::new(200)
                    .header("content-type", "application/json")
                    .with_body(Body::Stall)
            } else {
                reply(200, indexed()).delay(Duration::from_secs(5))
            }
        })
        .await;
        let http = reqwest::Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(300))
            .build()
            .unwrap();
        let es = Elasticsearch::new(http, &server.url(), ApiKey::new(KEY).unwrap()).unwrap();
        let error = es
            .conditional_index(
                "products",
                "1567",
                DocumentVersion::new(362, 2).unwrap(),
                &json!({"tags":["droid"]}),
            )
            .await
            .unwrap_err();
        assert!(error.is_uncertain());
        let inner = error.concord().unwrap();
        assert_eq!(inner.kind(), ErrorKind::Transport);
        assert_eq!(
            inner.delivery(),
            if after_head {
                Delivery::Answered
            } else {
                Delivery::Unknown
            }
        );
        assert_eq!(server.single().method, "PUT");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn other_write_failures_preserve_rejection_or_uncertainty_without_replay() {
    for (response, status, uncertain) in [
        (Reply::hang_up(), None, true),
        (
            reply(
                503,
                json!({"error":{"type":"unavailable_shards_exception"},"status":503}),
            ),
            Some(503),
            true,
        ),
        (
            reply(
                403,
                json!({"error":{"type":"security_exception"},"status":403}),
            ),
            Some(403),
            false,
        ),
        (Reply::json(200, b"{"), None, true),
    ] {
        let (server, es) = serve(vec![response]).await;
        let error = es
            .conditional_index(
                "products",
                "1567",
                DocumentVersion::new(0, 1).unwrap(),
                &json!({"product":"r2d2"}),
            )
            .await
            .unwrap_err();
        assert_eq!(error.status(), status);
        assert_eq!(error.is_uncertain(), uncertain);
        assert_eq!(
            server.single().target,
            "/products/_doc/1567?if_seq_no=0&if_primary_term=1"
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn ids_are_encoded_as_one_path_segment_and_response_ids_are_checked() {
    let id = "article/1?part#x %";
    let mut get = found();
    get["_id"] = json!(id);
    let mut put = indexed();
    put["_id"] = json!(id);
    let (server, _) = serve(vec![
        reply(200, get),
        reply(200, put),
        reply(200, found()),
        reply(200, indexed()),
    ])
    .await;
    let es = Elasticsearch::new(
        http(),
        &format!("{}/es", server.url()),
        ApiKey::new(KEY).unwrap(),
    )
    .unwrap();
    let GetDocument::Found(doc) = es.get_document("products", id).await.unwrap() else {
        panic!("expected found");
    };
    es.conditional_index("products", id, doc.version(), &json!({"tags":[]}))
        .await
        .unwrap();
    for error in [
        es.get_document("products", id).await.unwrap_err(),
        es.conditional_index("products", id, doc.version(), &json!({"tags":[]}))
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(error, Error::Protocol(Protocol::DocumentId)));
    }
    let requests = server.requests();
    assert_eq!(
        requests[0].target,
        "/es/products/_doc/article%2F1%3Fpart%23x%20%25"
    );
    assert_eq!(
        requests[1].target,
        "/es/products/_doc/article%2F1%3Fpart%23x%20%25?if_seq_no=362&if_primary_term=2"
    );
    drop(es);
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_inputs_and_oversized_sources_are_refused_before_sending() {
    let (server, es) = serve(vec![]).await;
    let version = DocumentVersion::new(362, 2).unwrap();
    assert!(matches!(
        DocumentVersion::new(1, 0),
        Err(Error::Invalid(Invalid::DocumentVersion))
    ));
    assert!(matches!(
        es.get_document("products", "").await,
        Err(Error::Invalid(Invalid::DocumentId))
    ));
    assert!(matches!(
        es.conditional_index("_bulk", "1567", version, &json!({}))
            .await,
        Err(Error::Invalid(Invalid::IndexName))
    ));
    assert!(matches!(
        es.conditional_index("products", "1567", version, &json!([1]))
            .await,
        Err(Error::Invalid(Invalid::DocumentNotObject))
    ));
    let large = json!({"value":"x".repeat(DOCUMENT_LIMIT)});
    assert!(matches!(
        es.conditional_index("products", "1567", version, &large)
            .await,
        Err(Error::Invalid(Invalid::DocumentTooLarge))
    ));
    assert!(server.requests().is_empty());
    server.shutdown().await;
}
