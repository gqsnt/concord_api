//! Public consumer of read -> conditional replacement -> explicit conflict resolution.
//! Synthetic loopback fixtures follow the OCC example; live provider calls: NOT_RUN.
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_elasticsearch::{ApiKey, Elasticsearch, Error, GetDocument, concord::reqwest};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn read_replace_then_resolve_a_stale_write_with_an_explicit_fresh_read() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        let (status, body) = match count.fetch_add(1, Ordering::SeqCst) {
            ordinal @ (0 | 3) => {
                assert_eq!(request.method, "GET");
                assert_eq!(request.target, "/products/_doc/1567");
                let fresh = ordinal == 3;
                (200, json!({"_index":"products","_id":"1567","found":true,
                    "_seq_no":if fresh {364} else {362}, "_primary_term":if fresh {3} else {2},
                    "_source":{"product":"r2d2","details":if fresh {"another caller's edit"} else {"original"}}}))
            }
            ordinal @ (1 | 4) => {
                assert_eq!(request.method, "PUT");
                let (seq_no, term) = if ordinal == 1 {(362, 2)} else {(364, 3)};
                assert_eq!(request.target, format!("/products/_doc/1567?if_seq_no={seq_no}&if_primary_term={term}"));
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(body["tags"], json!(["droid"]));
                assert_eq!(body["details"], if ordinal == 1 {"original"} else {"another caller's edit"});
                (200, json!({"_index":"products","_id":"1567","_seq_no":seq_no+1,
                    "_primary_term":term,"_version":seq_no-360,"result":"updated",
                    "_shards":{"total":2,"successful":2,"failed":0}}))
            }
            2 => {
                assert_eq!(request.method, "PUT");
                assert_eq!(request.target, "/products/_doc/1567?if_seq_no=362&if_primary_term=2");
                (409, json!({"error":{"type":"version_conflict_engine_exception","reason":"stale revision"},"status":409}))
            }
            5 => {
                assert_eq!(request.method, "GET");
                assert_eq!(request.target, "/products/_doc/1567");
                (404, json!({"_index":"products","_id":"1567","found":false}))
            }
            _ => panic!("unexpected retry or extra request"),
        };
        Reply::json(status, serde_json::to_vec(&body).unwrap())
    }).await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let es = Elasticsearch::new(
        http,
        &server.url(),
        ApiKey::new("consumer-fixture-key").unwrap(),
    )
    .unwrap();
    let GetDocument::Found(document) = es.get_document("products", "1567").await.unwrap() else {
        panic!("document must exist before replacing it");
    };
    let mut replacement = document.source::<Value>().unwrap().unwrap();
    replacement["tags"] = json!(["droid"]);
    let saved = es
        .conditional_index("products", "1567", document.version(), &replacement)
        .await
        .unwrap();
    assert_eq!(saved.version.seq_no(), 363);

    let error = es
        .conditional_index("products", "1567", document.version(), &replacement)
        .await
        .unwrap_err();
    let Error::VersionConflict(conflict) = error else {
        panic!("expected typed conflict");
    };
    assert_eq!(
        conflict.error_type(),
        Some("version_conflict_engine_exception")
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "conflict must not trigger reread or retry"
    );

    // The caller explicitly chooses to merge its tags into a fresh source, preserving others' edits.
    let GetDocument::Found(current) = es.get_document("products", "1567").await.unwrap() else {
        panic!("document disappeared");
    };
    let mut merged = current.source::<Value>().unwrap().unwrap();
    merged["tags"] = json!(["droid"]);
    let saved = es
        .conditional_index("products", "1567", current.version(), &merged)
        .await
        .unwrap();
    assert_eq!(saved.version.seq_no(), 365);
    assert_eq!(saved.version.primary_term(), 3);
    // An external deletion is a normal read outcome; this caller chooses to stop.
    assert!(matches!(es.get_document("products", "1567").await.unwrap(),
        GetDocument::NotFound { index, id } if index == "products" && id == "1567"));
    assert_eq!(server.requests().len(), 6);
    server.shutdown().await;
}
