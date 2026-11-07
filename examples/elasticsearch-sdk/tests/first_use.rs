//! Fresh caller application, authored from README and public rustdoc only.
//! All requests use owned loopback fixtures; live provider calls: NOT_RUN.

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_elasticsearch::{
    Action, ApiKey, BulkLimits, BulkRequest, Elasticsearch, Error, ItemOutcome, ItemResult,
    KeepAlive, PitSearch, Refresh, Sort,
    concord::{Delivery, ErrorKind, reqwest},
};
use serde_json::{Value, json};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

const KEY: &str = "Zmlyc3QtdXNlOmZpeHR1cmUta2V5";
const INDEX: &str = "articles-2026.11";
const PRIVATE_TITLE: &str = "unpublished acquisition article";
const PRIVATE_REASON: &str = "rejected unpublished acquisition article";
const CURSOR_ONE: &str = "[1794009600.12300, 9007199254740993]";
const CURSOR_TWO: &str = "[1794009601.45600, 9007199254740995]";

fn client(server: &TestServer) -> Elasticsearch {
    let http = reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    Elasticsearch::new(http, &server.url(), ApiKey::new(KEY).unwrap()).unwrap()
}

fn authenticated(request: &Recorded) {
    assert_eq!(
        request.header("authorization"),
        Some(format!("ApiKey {KEY}").as_str())
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert!(request.host.starts_with("127.0.0.1:"));
}

fn assert_redacted(rendered: &str) {
    for secret in [
        KEY,
        PRIVATE_TITLE,
        PRIVATE_REASON,
        "fixture-pit-",
        CURSOR_ONE,
        CURSOR_TWO,
    ] {
        assert!(
            !rendered.contains(secret),
            "sensitive fixture value appeared in diagnostics"
        );
    }
}

fn json_reply(body: Value) -> Reply {
    Reply::json(200, serde_json::to_vec(&body).unwrap())
}

#[tokio::test]
async fn bulk_index_inspect_every_item_and_resubmit_only_rejection() {
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/_bulk?refresh=wait_for");
        authenticated(request);
        assert_eq!(request.header("content-type"), Some("application/x-ndjson"));
        match calls.fetch_add(1, Ordering::SeqCst) {
            0 => json_reply(json!({"took": 17, "errors": true, "items": [
                {"index": {"_index": INDEX, "_id": "article-101", "status": 201,
                    "result": "created", "_version": 1, "_seq_no": 41, "_primary_term": 3}},
                {"create": {"_index": INDEX, "_id": "article-102", "status": 429,
                    "error": {"type": "es_rejected_execution_exception", "reason": PRIVATE_REASON}}},
                {"index": {"_index": INDEX, "_id": "article-103", "status": 400,
                    "error": {"type": "mapper_parsing_exception", "reason": PRIVATE_REASON}}},
                {"create": {"_index": INDEX, "_id": "article-104", "status": 409,
                    "error": {"type": "version_conflict_engine_exception", "reason": PRIVATE_REASON}}},
                {"delete": {"_index": INDEX, "_id": "article-retired", "status": 404,
                    "result": "not_found"}}
            ]})),
            1 => json_reply(json!({"took": 3, "errors": false, "items": [
                {"create": {"_index": INDEX, "_id": "article-102", "status": 201,
                    "result": "created", "_version": 1, "_seq_no": 42, "_primary_term": 3}}
            ]})),
            _ => panic!("unexpected replay of a bulk request"),
        }
    }).await;
    let es = client(&server);
    assert_redacted(&format!("{es:?}"));
    let mut batch = BulkRequest::with_limits(BulkLimits::new(5, 16_384).unwrap());
    batch.set_refresh(Some(Refresh::WaitFor));
    batch
        .index(
            INDEX,
            "article-101",
            &json!({"title": PRIVATE_TITLE, "views": 12}),
        )
        .unwrap()
        .create(
            INDEX,
            "article-102",
            &json!({"title": "Search after PIT", "views": 0}),
        )
        .unwrap()
        .index(
            INDEX,
            "article-103",
            &json!({"title": "Broken mapping", "views": "many"}),
        )
        .unwrap()
        .create(
            INDEX,
            "article-104",
            &json!({"title": "Already published", "views": 9}),
        )
        .unwrap()
        .delete(INDEX, "article-retired")
        .unwrap();
    let byte_count = batch.bytes();
    let refused = batch
        .index(INDEX, "article-over-cap", &json!({"title": "sixth"}))
        .unwrap_err();
    assert!(matches!(refused, Error::Invalid(_)));
    assert_eq!(batch.len(), 5);
    assert_eq!(batch.bytes(), byte_count);
    assert!(server.requests().is_empty());
    assert_redacted(&format!("{batch:?}"));

    // HTTP 200 must not make a partially failed batch an all-success result.
    let error = es.bulk(batch).await.unwrap_err();
    assert!(matches!(&error, Error::PartialBulk(_)));
    assert!(!error.is_uncertain());
    assert_eq!(error.status(), None);
    assert_redacted(&format!("{error} {error:?}"));
    let report = error.into_partial_bulk().unwrap();
    assert_eq!(report.took(), 17);
    assert!(!report.is_success());
    assert_eq!(report.applied().count(), 2);
    assert_eq!(report.failed().count(), 3);
    let expected = [
        (
            Action::Index,
            "article-101",
            201,
            Some(ItemResult::Created),
            None,
        ),
        (
            Action::Create,
            "article-102",
            429,
            None,
            Some("es_rejected_execution_exception"),
        ),
        (
            Action::Index,
            "article-103",
            400,
            None,
            Some("mapper_parsing_exception"),
        ),
        (
            Action::Create,
            "article-104",
            409,
            None,
            Some("version_conflict_engine_exception"),
        ),
        (
            Action::Delete,
            "article-retired",
            404,
            Some(ItemResult::NotFound),
            None,
        ),
    ];
    assert_eq!(report.items().len(), expected.len());
    for (position, (item, (action, id, status, result, kind))) in
        report.items().iter().zip(expected).enumerate()
    {
        assert_eq!(item.position(), position);
        assert_eq!(item.action(), action);
        assert_eq!(item.index(), INDEX);
        assert_eq!(item.id(), id);
        assert_eq!(item.status(), status);
        assert_eq!(item.result(), result);
        assert_eq!(item.error().map(|cause| cause.kind()), kind);
        assert_eq!(item.is_applied(), result.is_some());
        assert_eq!(item.is_failed(), result.is_none());
        assert_eq!(item.is_rejected(), status == 429);
        if let Some(cause) = item.error() {
            assert_eq!(cause.reason(), Some(PRIVATE_REASON));
        }
    }
    assert!(matches!(
        report.items()[0].outcome(),
        ItemOutcome::Applied {
            version: Some(1),
            seq_no: Some(41),
            primary_term: Some(3),
            ..
        }
    ));
    let mut offered = Vec::new();
    let all_failed = report
        .resubmit(|item| {
            offered.push(item.position());
            true
        })
        .unwrap();
    assert_eq!(offered, [1, 2, 3]);
    assert_eq!(all_failed.len(), 3);
    assert!(report.resubmit(|_| false).is_none());
    assert_eq!(count.load(Ordering::SeqCst), 1);

    let retry = report.resubmit_rejected().unwrap();
    assert_eq!(retry.len(), 1);
    assert_eq!(retry.limits().max_operations(), 5);
    assert_eq!(retry.limits().max_bytes(), 16_384);
    assert_eq!(retry.refresh(), Some(Refresh::WaitFor));
    let applied = es.bulk(retry).await.unwrap();
    assert!(applied.is_success());
    assert_eq!(applied.took(), 3);
    assert_eq!(applied.items().len(), 1);
    assert_eq!(applied.items()[0].id(), "article-102");
    assert_eq!(applied.items()[0].result(), Some(ItemResult::Created));
    assert!(applied.resubmit_rejected().is_none());
    assert_redacted(&format!("{report:?} {applied:?}"));

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body_len, byte_count as u64);
    assert!(requests[0].body.ends_with(b"\n"));
    let text = requests[0].body_text();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 9);
    for (line, expected) in lines.iter().zip([
        json!({"index":{"_index":INDEX,"_id":"article-101"}}),
        json!({"title":PRIVATE_TITLE,"views":12}),
        json!({"create":{"_index":INDEX,"_id":"article-102"}}),
        json!({"title":"Search after PIT","views":0}),
        json!({"index":{"_index":INDEX,"_id":"article-103"}}),
        json!({"title":"Broken mapping","views":"many"}),
        json!({"create":{"_index":INDEX,"_id":"article-104"}}),
        json!({"title":"Already published","views":9}),
        json!({"delete":{"_index":INDEX,"_id":"article-retired"}}),
    ]) {
        assert_eq!(serde_json::from_str::<Value>(line).unwrap(), expected);
    }
    // Exact original action/source bytes, with no successful or permanent-failure replay.
    assert_eq!(
        requests[1].body_text(),
        format!("{}\n{}\n", lines[2], lines[3])
    );
    server.shutdown().await;

    let mut invalid = BulkRequest::new();
    assert!(matches!(
        invalid.index(INDEX, "array-source", &json!([1, 2])),
        Err(Error::Invalid(_))
    ));
    assert!(invalid.is_empty());
    assert_eq!(invalid.bytes(), 0);

    // A caller must distinguish known whole-request rejection from uncertain delivery.
    for case in [
        "forbidden",
        "unavailable",
        "malformed",
        "mismatched-id",
        "contradiction",
        "no-head",
    ] {
        let server = TestServer::start(move |request: &Recorded| -> Reply {
            authenticated(request);
            assert_eq!(request.method, "POST");
            assert_eq!(request.target, "/_bulk");
            assert_eq!(request.header("content-type"), Some("application/x-ndjson"));
            match case {
                "forbidden" => Reply::json(
                    403,
                    serde_json::to_vec(&json!({
                        "error":{"type":"security_exception","reason":PRIVATE_REASON,
                            "root_cause":[{"type":"security_exception","reason":PRIVATE_REASON}]},
                        "status":403
                    }))
                    .unwrap(),
                )
                .header("x-request-id", "fixture-bulk-forbidden")
                .header("set-cookie", "session=private-fixture-cookie"),
                "unavailable" => {
                    Reply::text(503, "text/html", "<h1>Unavailable</h1>").header("retry-after", "2")
                }
                "malformed" => {
                    Reply::json(200, b"{").header("x-request-id", "fixture-bulk-malformed")
                }
                "mismatched-id" => json_reply(json!({"took":1,"errors":false,"items":[
                    {"index":{"_index":INDEX,"_id":"another-id","status":201,"result":"created"}}
                ]})),
                "contradiction" => json_reply(json!({"took":1,"errors":false,"items":[
                    {"index":{"_index":INDEX,"_id":"uncertain-article","status":429,
                        "error":{"type":"es_rejected_execution_exception","reason":PRIVATE_REASON}}}
                ]})),
                "no-head" => Reply::hang_up(),
                _ => unreachable!(),
            }
        })
        .await;
        let es = client(&server);
        let mut batch = BulkRequest::new();
        batch
            .index(INDEX, "uncertain-article", &json!({"title":PRIVATE_TITLE}))
            .unwrap();
        let error = es.bulk(batch).await.unwrap_err();
        assert_eq!(error.is_uncertain(), case != "forbidden", "{case}");
        assert!(error.partial_bulk().is_none());
        assert_redacted(&format!("{error} {error:?}"));
        assert!(!format!("{error:?}").contains("private-fixture-cookie"));
        match (case, &error) {
            ("forbidden", Error::Provider(provider)) => {
                assert_eq!(error.status(), Some(403));
                assert_eq!(provider.status().as_u16(), 403);
                assert!(provider.is_rejection());
                assert_eq!(provider.error_type(), Some("security_exception"));
                assert_eq!(provider.document().unwrap().status, Some(403));
                assert_eq!(provider.meta().status().as_u16(), 403);
                assert!(provider.meta().operation().is_some());
                assert_eq!(
                    provider.meta().headers()["x-request-id"],
                    "fixture-bulk-forbidden"
                );
                assert_eq!(
                    provider.meta().headers()["set-cookie"],
                    "session=private-fixture-cookie"
                );
            }
            ("unavailable", Error::Provider(provider)) => {
                assert_eq!(error.status(), Some(503));
                assert!(!provider.is_rejection());
                assert_eq!(provider.meta().headers()["retry-after"], "2");
                assert_eq!(provider.document().unwrap_err().kind(), ErrorKind::Metadata);
            }
            ("malformed", Error::Concord(inner)) => {
                assert_eq!(inner.kind(), ErrorKind::Decode);
                assert_eq!(inner.delivery(), Delivery::Answered);
                assert_eq!(inner.head_meta().unwrap().status().as_u16(), 200);
                assert_eq!(
                    inner.head_meta().unwrap().headers()["x-request-id"],
                    "fixture-bulk-malformed"
                );
            }
            ("mismatched-id" | "contradiction", Error::Protocol(_)) => {}
            ("no-head", Error::Concord(inner)) => {
                assert_eq!(inner.kind(), ErrorKind::Transport);
                assert_eq!(inner.delivery(), Delivery::Unknown);
                assert!(inner.head_meta().is_none());
            }
            _ => panic!("unexpected {case} error: {error:?}"),
        }
        let request = server.single(); // No automatic replay, including on 503/reset.
        let lines: Vec<Value> = request
            .body_text()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            lines,
            [
                json!({"index":{"_index":INDEX,"_id":"uncertain-article"}}),
                json!({"title":PRIVATE_TITLE})
            ]
        );
        server.shutdown().await;
    }
}

fn page(pit_id: &str, hits: &str) -> Reply {
    Reply::json(
        200,
        format!(
            r#"{{"pit_id":"{pit_id}","took":4,"timed_out":false,
        "_shards":{{"total":2,"successful":2,"failed":0}},"hits":{{"hits":{hits}}}}}"#
        ),
    )
}

fn search() -> PitSearch {
    PitSearch::new(vec![Sort::asc("published_at")], 2)
        .unwrap()
        .with_query(json!({"term":{"topic":"elasticsearch"}}))
        .unwrap()
}

fn keep_alive() -> KeepAlive {
    KeepAlive::minutes(NonZeroU32::new(1).unwrap())
}

#[tokio::test]
async fn open_pit_consume_search_after_incrementally_and_close_latest_id() {
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        authenticated(request);
        match calls.fetch_add(1, Ordering::SeqCst) {
            0 => json_reply(json!({"id":"fixture-pit-open", "_shards":{"total":2,"successful":2,"failed":0}})),
            1 => page("fixture-pit-page-one", &format!(r#"[
                {{"_index":"{INDEX}","_id":"article-101","_score":null,
                  "_source":{{"title":"{PRIVATE_TITLE}"}},"sort":[1794009600.10000,9007199254740992]}},
                {{"_index":"{INDEX}","_id":"article-102","_score":null,
                  "_source":{{"title":"Search after PIT"}},"sort":{CURSOR_ONE}}}
            ]"#)),
            2 => page("fixture-pit-page-two", &format!(r#"[
                {{"_index":"{INDEX}","_id":"article-105","_score":null,
                  "_source":{{"title":"Last article"}},"sort":{CURSOR_TWO}}}
            ]"#)),
            3 => page("fixture-pit-empty", "[]"),
            4 => json_reply(json!({"succeeded":true,"num_freed":2})),
            _ => panic!("search continued beyond completion or repeated close"),
        }
    }).await;
    let es = client(&server);
    let pit = es.open_point_in_time(INDEX, keep_alive()).await.unwrap();
    let mut hits = es
        .search_after(&pit, &search(), NonZeroUsize::new(3).unwrap())
        .items();
    assert_eq!(
        server.requests().len(),
        1,
        "constructing a traversal must be lazy"
    );
    let first = hits.next().await.unwrap().unwrap();
    assert_eq!(first.id(), "article-101");
    assert_eq!(first.index(), INDEX);
    assert_eq!(first.score(), None);
    assert_eq!(
        first.source::<Value>().unwrap().unwrap(),
        json!({"title":PRIVATE_TITLE})
    );
    assert_redacted(&format!("{first:?} {pit:?}"));
    assert_eq!(server.requests().len(), 2);
    let second = hits.next().await.unwrap().unwrap();
    assert_eq!(second.id(), "article-102");
    assert_eq!(second.sort().unwrap().as_str(), CURSOR_ONE);
    assert_eq!(
        server.requests().len(),
        2,
        "no read-ahead while consuming a page"
    );
    let third = hits.next().await.unwrap().unwrap();
    assert_eq!(third.id(), "article-105");
    assert_eq!(third.sort().unwrap().as_str(), CURSOR_TWO);
    assert_eq!(
        server.requests().len(),
        3,
        "a short page does not end the search"
    );
    assert!(hits.next().await.unwrap().is_none());
    assert!(hits.next().await.unwrap().is_none());
    assert_eq!(server.requests().len(), 4);
    assert_eq!(hits.progress().items, 3);
    assert_eq!(hits.progress().pages.succeeded, 3);
    assert!(hits.checkpoint().is_none());
    drop(hits);
    let closed = es.close_point_in_time(pit).await.unwrap();
    assert_eq!(closed.succeeded, Some(true));
    assert_eq!(closed.num_freed, Some(2));

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].target, format!("/{INDEX}/_pit?keep_alive=1m"));
    assert!(requests[0].body.is_empty());
    for (offset, previous_id) in [
        "fixture-pit-open",
        "fixture-pit-page-one",
        "fixture-pit-page-two",
    ]
    .iter()
    .enumerate()
    {
        let request = &requests[offset + 1];
        assert_eq!(request.method, "GET");
        assert_eq!(request.target, "/_search");
        assert_eq!(request.header("content-type"), Some("application/json"));
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["size"], 2);
        assert_eq!(body["pit"], json!({"id":previous_id,"keep_alive":"1m"}));
        assert_eq!(body["query"], json!({"term":{"topic":"elasticsearch"}}));
        assert_eq!(body["sort"], json!([{"published_at":"asc"}]));
        assert_eq!(body["track_total_hits"], false);
        assert!(body.get("from").is_none());
        match offset {
            0 => assert!(body.get("search_after").is_none()),
            1 => assert!(
                request
                    .body_text()
                    .contains(&format!("\"search_after\":{CURSOR_ONE}"))
            ),
            2 => assert!(
                request
                    .body_text()
                    .contains(&format!("\"search_after\":{CURSOR_TWO}"))
            ),
            _ => unreachable!(),
        }
    }
    assert_eq!(requests[4].method, "DELETE");
    assert_eq!(requests[4].target, "/_pit");
    assert_eq!(requests[4].header("content-type"), Some("application/json"));
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[4].body).unwrap(),
        json!({"id":"fixture-pit-empty"})
    );
    server.shutdown().await;

    assert!(matches!(PitSearch::new(vec![], 2), Err(Error::Invalid(_))));
    assert!(matches!(
        PitSearch::new(vec![Sort::asc("published_at")], 0),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        search().with_query(json!(["not an object"])),
        Err(Error::Invalid(_))
    ));

    // The bounded convenience path must keep the successful prefix and attempt cleanup.
    for case in [
        "provider",
        "malformed",
        "partial",
        "protocol",
        "cap",
        "close",
        "open",
    ] {
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let server = TestServer::start(move |request: &Recorded| -> Reply {
            authenticated(request);
            let ordinal = calls.fetch_add(1, Ordering::SeqCst);
            if ordinal == 0 {
                assert_eq!(request.method, "POST");
                assert_eq!(request.target, format!("/{INDEX}/_pit?keep_alive=1m"));
                return if case == "open" {
                    Reply::json(403, br#"{"error":{"type":"security_exception","reason":"denied"},"status":403}"#)
                } else { json_reply(json!({"id":"fixture-pit-open"})) };
            }
            if request.method == "DELETE" {
                assert_eq!(request.target, "/_pit");
                assert_eq!(request.header("content-type"), Some("application/json"));
                return if case == "close" {
                    Reply::json(403, br#"{"error":{"type":"security_exception","reason":"denied"},"status":403}"#)
                } else { json_reply(json!({"succeeded":true,"num_freed":1})) };
            }
            assert_eq!(request.method, "GET");
            assert_eq!(request.target, "/_search");
            if ordinal == 1 {
                return page("fixture-pit-prefix", &format!(r#"[
                    {{"_index":"{INDEX}","_id":"article-prefix","_score":null,
                      "_source":{{"title":"{PRIVATE_TITLE}"}},"sort":{CURSOR_ONE}}}
                ]"#));
            }
            assert_eq!(ordinal, 2, "a failed search must not continue or retry");
            match case {
                "provider" => Reply::json(503, serde_json::to_vec(&json!({
                    "error":{"type":"search_phase_execution_exception","reason":PRIVATE_REASON},"status":503
                })).unwrap()).header("x-request-id", "fixture-search-unavailable"),
                "malformed" => Reply::json(200, b"{"),
                "partial" => Reply::json(200, serde_json::to_vec(&json!({
                    "pit_id":"fixture-pit-failed", "took":5,"timed_out":true,
                    "_shards":{"total":2,"successful":1,"failed":1},
                    "hits":{"hits":[{"_index":INDEX,"_id":"must-not-emit",
                        "_source":{"title":"partial page"},"sort":[1794009601,2]}]}
                })).unwrap()),
                "protocol" => json_reply(json!({"pit_id":"fixture-pit-failed", "took":1,
                    "timed_out":false,"_shards":{"failed":0},
                    "hits":{"hits":[{"_index":INDEX,"_id":"missing-sort"}]}})),
                "close" => page("fixture-pit-empty", "[]"),
                _ => panic!("cap/open failure must not fetch this page"),
            }
        }).await;
        let es = client(&server);
        let cap = NonZeroUsize::new(if case == "cap" { 1 } else { 3 }).unwrap();
        let result = es.scan(INDEX, keep_alive(), &search(), cap).await;
        if case == "close" {
            let complete = result.unwrap();
            assert_eq!(complete.hits.len(), 1);
            assert_eq!(complete.hits[0].id(), "article-prefix");
            let close_error = complete.close.unwrap_err();
            assert_eq!(close_error.status(), Some(403));
            assert_redacted(&format!("{close_error:?}"));
        } else {
            let incomplete = result.unwrap_err();
            assert_redacted(&format!("{incomplete} {incomplete:?}"));
            if case == "open" {
                assert_eq!(incomplete.error.status(), Some(403));
                assert!(incomplete.collected.is_empty());
                assert!(incomplete.close.is_none());
            } else {
                assert_eq!(incomplete.collected.len(), 1);
                assert_eq!(incomplete.collected[0].id(), "article-prefix");
                assert_eq!(
                    incomplete.collected[0].source::<Value>().unwrap().unwrap(),
                    json!({"title":PRIVATE_TITLE})
                );
                assert_eq!(incomplete.close.unwrap().unwrap().succeeded, Some(true));
                match (case, &incomplete.error) {
                    ("provider", Error::Provider(provider)) => {
                        assert_eq!(provider.status().as_u16(), 503);
                        assert_eq!(
                            provider.error_type(),
                            Some("search_phase_execution_exception")
                        );
                        assert_eq!(
                            provider.meta().headers()["x-request-id"],
                            "fixture-search-unavailable"
                        );
                    }
                    ("malformed", Error::Concord(inner)) => {
                        assert_eq!(inner.kind(), ErrorKind::Decode)
                    }
                    (
                        "partial",
                        Error::PartialSearch {
                            timed_out: true,
                            shards_failed: 1,
                        },
                    ) => {}
                    ("protocol", Error::Protocol(_)) => {}
                    ("cap", Error::PageCap { pages: 1 }) => {}
                    _ => panic!("unexpected {case} scan error: {:?}", incomplete.error),
                }
            }
        }
        let requests = server.requests();
        let expected_count = match case {
            "open" => 1,
            "cap" => 3,
            _ => 4,
        };
        assert_eq!(
            requests.len(),
            expected_count,
            "{case}: no replay/continuation after failure"
        );
        if case != "open" {
            let close = requests.last().unwrap();
            assert_eq!(close.method, "DELETE");
            let latest = match case {
                "partial" | "protocol" => "fixture-pit-failed",
                "close" => "fixture-pit-empty",
                _ => "fixture-pit-prefix",
            };
            assert_eq!(
                serde_json::from_slice::<Value>(&close.body).unwrap(),
                json!({"id":latest}),
                "{case}"
            );
        }
        server.shutdown().await;
    }
}
