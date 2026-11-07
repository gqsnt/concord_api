//! The two connected workflows end to end against loopback fixtures:
//!
//! 1. bulk-index a bounded set of documents, then inspect every per-item result — including the
//!    required failure: a partial bulk failure is neither all-success nor a reason to replay the
//!    operations that succeeded;
//! 2. open a point in time, page with `search_after`, close it — also after a failed or
//!    incomplete page.

mod common;

use std::num::{NonZeroU32, NonZeroUsize};

use common::*;
use concord_sdk_elasticsearch::{
    Action, BulkReport, BulkRequest, Elasticsearch, Error, Hit, ItemOutcome, ItemResult, KeepAlive,
    PitSearch, Sort,
};
use serde_json::{Value, json};

/// What a test reads of one bulk item.
type Seen<'a> = (usize, Action, &'a str, &'a str, u16, Option<ItemResult>);

fn one_minute() -> KeepAlive {
    KeepAlive::minutes(NonZeroU32::new(1).unwrap())
}

/// Application code over the SDK: `?` propagates a partial failure as an error.
async fn index_articles(es: &Elasticsearch, request: BulkRequest) -> Result<BulkReport, Error> {
    let report = es.bulk(request).await?;
    Ok(report)
}

// ---------------------------------------------------------------------------------------------
// Workflow 1: bulk

#[tokio::test]
async fn bulk_then_inspect_every_item() {
    let (server, es) = serve(vec![json(200, "bulk-multiple-operations-response.json")]).await;
    // The bulk reference's "Multiple operations" example, as a bounded request.
    let mut request = BulkRequest::new();
    request
        .index("test", "1", &json!({ "field1": "value1" }))
        .unwrap()
        .delete("test", "2")
        .unwrap()
        .create("test", "3", &json!({ "field1": "value3" }))
        .unwrap()
        .update("test", "1", &json!({ "field2": "value2" }))
        .unwrap();
    assert_eq!(request.len(), 4);

    let report = index_articles(&es, request).await.unwrap();
    assert!(report.is_success());
    assert_eq!(report.took(), 30);
    // Every item, matched with its operation by position, as the documented answer reports it.
    let seen: Vec<Seen<'_>> = report
        .items()
        .iter()
        .map(|i| {
            (
                i.position(),
                i.action(),
                i.index(),
                i.id(),
                i.status(),
                i.result(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (
                0,
                Action::Index,
                "test",
                "1",
                201,
                Some(ItemResult::Created)
            ),
            // The documented "successful" delete of a missing document: 404, `not_found`.
            (
                1,
                Action::Delete,
                "test",
                "2",
                404,
                Some(ItemResult::NotFound)
            ),
            (
                2,
                Action::Create,
                "test",
                "3",
                201,
                Some(ItemResult::Created)
            ),
            (
                3,
                Action::Update,
                "test",
                "1",
                200,
                Some(ItemResult::Updated)
            ),
        ]
    );
    let ItemOutcome::Applied {
        version,
        seq_no,
        primary_term,
        ..
    } = report.items()[3].outcome()
    else {
        panic!("{:?}", report.items()[3])
    };
    assert_eq!(
        (*version, *seq_no, *primary_term),
        (Some(2), Some(3), Some(4))
    );
    assert_eq!(report.failed().count(), 0);
    assert!(
        report.resubmit(|_| true).is_none(),
        "nothing failed, nothing to resubmit"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_partial_failure_is_an_error_and_never_replays_applied_operations() {
    let (server, es) = serve(vec![
        json(200, "bulk-failed-actions-response.json"),
        json(200, "bulk-resubmitted-missing-response.json"),
    ])
    .await;
    // The bulk reference's failed-actions example: two updates of missing documents, one create.
    let mut request = BulkRequest::new();
    request
        .update("index1", "5", &json!({ "my_field": "foo" }))
        .unwrap()
        .update("index1", "6", &json!({ "my_field": "foo" }))
        .unwrap()
        .create("index1", "7", &json!({ "my_field": "foo" }))
        .unwrap();

    // Not all-success: the application's `?` stops here.
    let error = index_articles(&es, request).await.unwrap_err();
    assert!(!error.is_uncertain(), "every outcome is known");
    assert_eq!(
        error.to_string(),
        "bulk request partially failed: 2 of 3 operations failed"
    );
    let report = error.into_partial_bulk().unwrap();
    assert!(!report.is_success());

    let failed: Vec<(usize, u16, &str)> = report
        .failed()
        .map(|i| (i.position(), i.status(), i.error().unwrap().kind()))
        .collect();
    assert_eq!(
        failed,
        [
            (0, 404, "document_missing_exception"),
            (1, 404, "document_missing_exception")
        ]
    );
    assert_eq!(
        report.items()[0].error().unwrap().reason(),
        Some("[5]: document missing")
    );
    let applied: Vec<(usize, &str, Option<ItemResult>)> = report
        .applied()
        .map(|i| (i.position(), i.id(), i.result()))
        .collect();
    assert_eq!(applied, [(2, "7", Some(ItemResult::Created))]);

    // A missing document is not a transient rejection: the built-in rule resubmits nothing.
    assert!(report.resubmit_rejected().is_none());
    // Even a caller that selects everything only gets the failed operations back.
    let mut offered = Vec::new();
    let again = report
        .resubmit(|item| {
            offered.push(item.position());
            true
        })
        .unwrap();
    assert_eq!(offered, [0, 1], "an applied item is never offered");
    assert_eq!(again.len(), 2);
    // The documents are still missing: the resubmission fails the same way, and is reported
    // the same way.
    let again = es
        .bulk(again)
        .await
        .unwrap_err()
        .into_partial_bulk()
        .unwrap();
    let positions: Vec<(usize, &str)> = again.failed().map(|i| (i.position(), i.id())).collect();
    assert_eq!(positions, [(0, "5"), (1, "6")]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let first = ndjson_lines(&requests[0].body);
    let second = ndjson_lines(&requests[1].body);
    assert_eq!(first, ndjson_lines(&fixture("bulk-failed-actions.ndjson")));
    assert_eq!(second, first[..4], "updates 5 and 6, in order");
    let create = json!({ "create": { "_index": "index1", "_id": "7" } });
    assert!(
        !second.contains(&create),
        "the applied create is never sent again"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn rejected_operations_are_resubmitted_alone() {
    let (server, es) = serve(vec![
        json(200, "bulk-rejected-item-response.json"),
        json(200, "bulk-resubmitted-response.json"),
    ])
    .await;
    let mut request = BulkRequest::new();
    for (id, title) in [("a-1", "one"), ("a-2", "two")] {
        request
            .index("articles", id, &json!({ "title": title }))
            .unwrap();
    }
    request
        .create("articles", "a-3", &json!({ "title": "three" }))
        .unwrap();
    request
        .index("articles", "a-4", &json!({ "title": "four" }))
        .unwrap();

    let report = es
        .bulk(request)
        .await
        .unwrap_err()
        .into_partial_bulk()
        .unwrap();
    let outcomes: Vec<(&str, bool, bool)> = report
        .items()
        .iter()
        .map(|i| (i.id(), i.is_applied(), i.is_rejected()))
        .collect();
    assert_eq!(
        outcomes,
        [
            ("a-1", true, false),
            ("a-2", false, true),  // 429: rejected
            ("a-3", false, false), // 409: a conflict is not resubmitted
            ("a-4", true, false),
        ]
    );
    let again = report.resubmit_rejected().unwrap();
    assert_eq!(again.len(), 1);
    let retried = es.bulk(again).await.unwrap();
    assert!(retried.is_success());
    assert_eq!(retried.items()[0].id(), "a-2");

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let first = ndjson_lines(&requests[0].body);
    let second = ndjson_lines(&requests[1].body);
    // Exactly the rejected operation's two lines, as first sent.
    assert_eq!(second, first[2..4]);
    assert_eq!(requests[1].body, {
        let text = String::from_utf8(requests[0].body.clone()).unwrap();
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        lines[2..4].concat().into_bytes()
    });
    server.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Workflow 2: point in time + search_after

fn guide_search() -> PitSearch {
    // The pagination guide's PIT search: its query, its sort, `"size": 10000`.
    let sort = Sort::clause(json!({
        "@timestamp": { "order": "asc", "format": "strict_date_optional_time_nanos" }
    }))
    .unwrap();
    PitSearch::new(vec![sort], 10_000)
        .unwrap()
        .with_query(json!({ "match": { "user.id": "elkbee" } }))
        .unwrap()
}

fn ids(hits: &[Hit]) -> Vec<&str> {
    hits.iter().map(Hit::id).collect()
}

fn delete_body(id: &str) -> Value {
    json!({ "id": id })
}

#[tokio::test]
async fn open_search_after_close() {
    let (server, es) = serve(vec![
        json(200, "pit-open-response.json"),
        json(200, "pit-search-page-1.json"),
        json(200, "pit-search-page-2.json"),
        json(200, "pit-search-page-empty.json"),
        json(200, "pit-close-response.json"),
    ])
    .await;
    let scan = es
        .scan(
            "my-index-000001",
            one_minute(),
            &guide_search(),
            NonZeroUsize::new(10).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        ids(&scan.hits),
        ["SYNTHETIC-HIT-1", "FaslK3QBySSL_rrj9zM5", "SYNTHETIC-HIT-3"]
    );
    let closed = scan.close.unwrap();
    assert_eq!(closed.succeeded, Some(true));

    let r = server.requests();
    assert_eq!(r.len(), 5);
    assert_eq!(
        (r[0].method.as_str(), r[0].target.as_str()),
        ("POST", "/my-index-000001/_pit?keep_alive=1m")
    );
    for search in &r[1..4] {
        assert_eq!(
            (search.method.as_str(), search.target.as_str()),
            ("GET", "/_search")
        );
    }
    // First page: the opened id, no search_after.
    assert_eq!(body_json(&r[1]), guide_search_request(GUIDE_PIT_ID, None));
    // Second page: the guide's next-page request — the id the first page returned and the
    // sort values of its last hit.
    assert_eq!(
        body_json(&r[2]),
        guide_search_request(
            "SYNTHETIC-PIT-ID-2",
            Some(json!(["2021-05-20T05:30:04.832Z", 4294967298u64]))
        )
    );
    assert_eq!(
        body_json(&r[2]),
        {
            let mut documented = fixture_json("pit-search-request-next.json");
            documented["pit"]["id"] = Value::from("SYNTHETIC-PIT-ID-2");
            documented
        },
        "apart from the updated id, the guide's own next-page request"
    );
    assert_eq!(
        body_json(&r[3]),
        guide_search_request(
            "SYNTHETIC-PIT-ID-3",
            Some(json!(["2021-05-20T05:30:05.000Z", 8589934593u64]))
        )
    );
    // Closed by the latest id (from the last, empty page).
    assert_eq!(
        (r[4].method.as_str(), r[4].target.as_str()),
        ("DELETE", "/_pit")
    );
    assert_eq!(body_json(&r[4]), delete_body("SYNTHETIC-PIT-ID-4"));
    server.shutdown().await;
}

#[tokio::test]
async fn the_same_workflow_step_by_step() {
    let (server, es) = serve(vec![
        json(200, "pit-open-response.json"),
        json(200, "pit-search-page-1.json"),
        json(200, "pit-search-page-2.json"),
        json(200, "pit-search-page-empty.json"),
        json(200, "pit-close-response.json"),
    ])
    .await;
    let pit = es
        .open_point_in_time("my-index-000001", one_minute())
        .await
        .unwrap();
    let mut hits = es
        .search_after(&pit, &guide_search(), NonZeroUsize::new(10).unwrap())
        .items();
    let mut seen = Vec::new();
    while let Some(hit) = hits.next().await.unwrap() {
        let source: Value = hit.source().unwrap().unwrap();
        seen.push((
            hit.id().to_owned(),
            source["message"].as_str().unwrap().to_owned(),
        ));
    }
    assert_eq!(
        seen,
        [
            ("SYNTHETIC-HIT-1".to_owned(), "first".to_owned()),
            ("FaslK3QBySSL_rrj9zM5".to_owned(), "second".to_owned()),
            ("SYNTHETIC-HIT-3".to_owned(), "third".to_owned()),
        ]
    );
    assert_eq!(pit.id().as_str(), "SYNTHETIC-PIT-ID-4");
    es.close_point_in_time(pit).await.unwrap();
    assert_eq!(
        body_json(&server.requests()[4]),
        delete_body("SYNTHETIC-PIT-ID-4")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_failed_page_still_closes_the_pit() {
    let (server, es) = serve(vec![
        json(200, "pit-open-response.json"),
        json(200, "pit-search-page-1.json"),
        json(503, "error-unavailable.json"),
        json(200, "pit-close-response.json"),
    ])
    .await;
    let failed = es
        .scan(
            "my-index-000001",
            one_minute(),
            &guide_search(),
            NonZeroUsize::new(10).unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(failed.error.status(), Some(503));
    assert_eq!(
        ids(&failed.collected),
        ["SYNTHETIC-HIT-1", "FaslK3QBySSL_rrj9zM5"]
    );
    assert!(failed.close.unwrap().is_ok());
    let r = server.requests();
    assert_eq!(r.len(), 4);
    assert_eq!(body_json(&r[3]), delete_body("SYNTHETIC-PIT-ID-2"));
    server.shutdown().await;
}

#[tokio::test]
async fn an_incomplete_page_stops_the_traversal_and_closes_the_pit() {
    let (server, es) = serve(vec![
        json(200, "pit-open-response.json"),
        json(200, "pit-search-page-1.json"),
        json(200, "pit-search-shard-failure.json"),
        json(200, "pit-close-response.json"),
    ])
    .await;
    let failed = es
        .scan(
            "my-index-000001",
            one_minute(),
            &guide_search(),
            NonZeroUsize::new(10).unwrap(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        failed.error,
        Error::PartialSearch {
            shards_failed: 1,
            timed_out: false
        }
    ));
    assert_eq!(
        failed.collected.len(),
        2,
        "the incomplete page's hit is not kept"
    );
    // The refused page still returned the latest id, and the PIT is closed by it.
    assert_eq!(
        body_json(&server.requests()[3]),
        delete_body("SYNTHETIC-PIT-ID-3")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_page_cap_is_not_completion() {
    let (server, es) = serve(vec![
        json(200, "pit-open-response.json"),
        json(200, "pit-search-page-1.json"),
        json(200, "pit-close-response.json"),
    ])
    .await;
    let failed = es
        .scan(
            "my-index-000001",
            one_minute(),
            &guide_search(),
            NonZeroUsize::new(1).unwrap(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(failed.error, Error::PageCap { pages: 1 }),
        "{:?}",
        failed.error
    );
    assert_eq!(failed.collected.len(), 2);
    let r = server.requests();
    assert_eq!(r.len(), 3, "no page beyond the cap is requested");
    assert_eq!(body_json(&r[2]), delete_body("SYNTHETIC-PIT-ID-2"));
    server.shutdown().await;
}

#[tokio::test]
async fn a_failed_open_has_nothing_to_close() {
    let (server, es) = serve(vec![json(404, "error-index-not-found.json")]).await;
    let failed = es
        .scan(
            "missing-index",
            one_minute(),
            &guide_search(),
            NonZeroUsize::new(10).unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(failed.error.status(), Some(404));
    assert!(failed.close.is_none() && failed.collected.is_empty());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn a_failed_close_does_not_hide_complete_hits() {
    let (server, es) = serve(vec![
        json(200, "pit-open-response.json"),
        json(200, "pit-search-page-empty.json"),
        json(404, "error-index-not-found.json"),
    ])
    .await;
    let scan = es
        .scan(
            "my-index-000001",
            one_minute(),
            &guide_search(),
            NonZeroUsize::new(10).unwrap(),
        )
        .await
        .unwrap();
    assert!(scan.hits.is_empty());
    assert_eq!(scan.close.unwrap_err().status(), Some(404));
    server.shutdown().await;
}
