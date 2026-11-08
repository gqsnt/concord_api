//! Wire-level request shapes, every error class, malformed / truncated / oversized / wrong-media
//! bodies, response-contract violations, input validation and redaction.

mod common;

use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use common::*;
use concord_example_testserver::{Body, Recorded, Reply, Script, TestServer, paused};
use concord_sdk_elasticsearch::concord::bytes::Bytes;
use concord_sdk_elasticsearch::concord::{Delivery, ErrorKind, reqwest};
use concord_sdk_elasticsearch::{
    Action, ApiKey, BulkLimits, BulkRequest, Elasticsearch, Error, IndexName, Invalid, ItemResult,
    KeepAlive, PitSearch, Protocol, RESPONSE_LIMIT, Refresh, Sort,
};
use serde_json::{Value, json, value::RawValue};

fn one_minute() -> KeepAlive {
    KeepAlive::minutes(NonZeroU32::new(1).unwrap())
}

/// The four operations of the bulk reference's "Multiple operations" example.
fn documented_operations() -> BulkRequest {
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
    request
}

/// The three operations of the bulk reference's "Filter for failed operations" request.
fn failed_actions_operations() -> BulkRequest {
    let mut request = BulkRequest::new();
    request
        .update("index1", "5", &json!({ "my_field": "foo" }))
        .unwrap()
        .update("index1", "6", &json!({ "my_field": "foo" }))
        .unwrap()
        .create("index1", "7", &json!({ "my_field": "foo" }))
        .unwrap();
    request
}

fn guide_search() -> PitSearch {
    let sort = Sort::clause(json!({
        "@timestamp": { "order": "asc", "format": "strict_date_optional_time_nanos" }
    }))
    .unwrap();
    PitSearch::new(vec![sort], 10_000)
        .unwrap()
        .with_query(json!({ "match": { "user.id": "elkbee" } }))
        .unwrap()
}

// ---------------------------------------------------------------------------------------------
// Request shapes

#[tokio::test]
async fn bulk_request_is_the_documented_ndjson_body() {
    let (server, es) = serve(vec![json(200, "bulk-multiple-operations-response.json")]).await;
    let request = documented_operations();
    let bytes = request.bytes();
    let report = es.bulk(request).await.unwrap();
    assert_eq!(report.items().len(), 4);

    let sent = server.single();
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.target, "/_bulk");
    assert_eq!(sent.header("content-type"), Some("application/x-ndjson"));
    assert_eq!(sent.header("accept"), Some("application/json"));
    assert_eq!(
        sent.header("authorization"),
        Some(format!("ApiKey {KEY}").as_str())
    );
    assert_eq!(
        sent.body.len(),
        bytes,
        "the request's byte accounting is the body length"
    );
    // Line by line, the documented curl example's body (key order inside an object is not
    // significant in JSON; the framing is checked byte-wise by `ndjson_lines`).
    let documented = ndjson_lines(&fixture("bulk-multiple-operations.ndjson"));
    assert_eq!(ndjson_lines(&sent.body), documented);
    assert_eq!(documented.len(), 7, "delete has no source line");
    server.shutdown().await;
}

#[tokio::test]
async fn bulk_refresh_is_a_query_parameter() {
    let (server, es) = serve(vec![json(200, "bulk-multiple-operations-response.json")]).await;
    let mut request = documented_operations();
    request.set_refresh(Some(Refresh::WaitFor));
    es.bulk(request).await.unwrap();
    assert_eq!(server.single().target, "/_bulk?refresh=wait_for");
    server.shutdown().await;
}

#[tokio::test]
async fn open_pit_is_a_post_to_the_index_with_keep_alive() {
    let (server, es) = serve(vec![json(200, "pit-open-response.json")]).await;
    let pit = es
        .open_point_in_time("my-index-000001", one_minute())
        .await
        .unwrap();
    assert_eq!(pit.id().as_str(), GUIDE_PIT_ID);
    assert_eq!(pit.index().as_str(), "my-index-000001");
    let sent = server.single();
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.target, "/my-index-000001/_pit?keep_alive=1m");
    assert!(sent.body.is_empty());
    assert_eq!(sent.header("content-type"), None);
    server.shutdown().await;
}

#[tokio::test]
async fn pit_search_names_the_pit_in_the_body_and_no_index_in_the_path() {
    let (server, es) = serve(vec![
        json(200, "pit-open-response.json"),
        json(200, "pit-search-page-empty.json"),
    ])
    .await;
    let pit = es
        .open_point_in_time("my-index-000001", one_minute())
        .await
        .unwrap();
    let mut pages = es.search_after(&pit, &guide_search(), NonZeroUsize::new(4).unwrap());
    // An empty page is the last one: returned without a next key, then the end.
    let page = pages.next_page().await.unwrap().unwrap();
    assert!(page.items.is_empty() && page.next.is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    let sent = &server.requests()[1];
    assert_eq!(sent.method, "GET");
    assert_eq!(sent.target, "/_search");
    assert_eq!(sent.header("content-type"), Some("application/json"));
    assert_eq!(body_json(sent), guide_search_request(GUIDE_PIT_ID, None));
    server.shutdown().await;
}

#[tokio::test]
async fn close_pit_is_a_delete_with_the_id_in_the_body() {
    let (server, es) = serve(vec![
        json(200, "pit-open-response.json"),
        json(200, "pit-close-response.json"),
    ])
    .await;
    let pit = es
        .open_point_in_time("my-index-000001", one_minute())
        .await
        .unwrap();
    let closed = es.close_point_in_time(pit).await.unwrap();
    assert_eq!((closed.succeeded, closed.num_freed), (Some(true), Some(1)));
    let sent = &server.requests()[1];
    assert_eq!(sent.method, "DELETE");
    assert_eq!(sent.target, "/_pit");
    assert_eq!(sent.header("content-type"), Some("application/json"));
    assert_eq!(body_json(sent), fixture_json("pit-close-request.json"));
    server.shutdown().await;
}

#[tokio::test]
async fn sort_values_are_sent_back_byte_for_byte() {
    let (server, es) = serve(vec![
        json(200, "pit-open-response.json"),
        json(200, "pit-search-raw-sort.json"),
        json(200, "pit-search-page-empty.json"),
    ])
    .await;
    let pit = es
        .open_point_in_time("my-index-000001", one_minute())
        .await
        .unwrap();
    let search = PitSearch::new(vec![Sort::asc("n")], 10).unwrap();
    let hits = es
        .search_after(&pit, &search, NonZeroUsize::new(4).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].sort().unwrap().as_str(),
        r#"[18446744073709551615, 1.10, "b"]"#
    );
    // A re-encoding would print `1.1`; the exact text goes back.
    let next = server.requests()[2].body_text();
    assert!(
        next.contains(r#""search_after":[18446744073709551615, 1.10, "b"]"#),
        "{next}"
    );
    server.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Error answers (non-2xx): the status is authoritative

#[tokio::test]
async fn every_error_status_keeps_its_status_and_type() {
    let cases = [
        (
            400,
            "error-illegal-argument.json",
            "illegal_argument_exception",
            false,
        ),
        (401, "error-security.json", "security_exception", false),
        (
            404,
            "error-index-not-found.json",
            "index_not_found_exception",
            false,
        ),
        (
            429,
            "error-rejected.json",
            "es_rejected_execution_exception",
            false,
        ),
        (
            503,
            "error-unavailable.json",
            "unavailable_shards_exception",
            true,
        ),
    ];
    for (status, name, kind, uncertain) in cases {
        let (server, es) = serve(vec![json(status, name)]).await;
        let error = es.bulk(documented_operations()).await.unwrap_err();
        let Error::Provider(provider) = &error else {
            panic!("{status}: {error:?}")
        };
        assert_eq!(provider.status().as_u16(), status);
        assert_eq!(provider.error_type(), Some(kind));
        assert_eq!(provider.is_rejection(), status < 500);
        assert_eq!(error.is_uncertain(), uncertain, "{status}");
        assert_eq!(error.status(), Some(status));
        let document = provider.document().unwrap();
        assert_eq!(document.status, Some(status));
        assert_eq!(
            error.to_string(),
            format!("Elasticsearch answered HTTP {status} ({kind})")
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn open_pit_on_a_missing_index_is_a_provider_error() {
    let (server, es) = serve(vec![json(404, "error-index-not-found.json")]).await;
    let error = es
        .open_point_in_time("missing-index", one_minute())
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(404));
    let Error::Provider(provider) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(provider.error_type(), Some("index_not_found_exception"));
    let document = provider.document().unwrap();
    let Some(concord_sdk_elasticsearch::ErrorField::Cause(cause)) = &document.error else {
        panic!("{document:?}")
    };
    assert_eq!(cause.root_cause()[0].kind(), "index_not_found_exception");
    assert_eq!(cause.reason(), Some("no such index [missing-index]"));
    server.shutdown().await;
}

#[tokio::test]
async fn an_unreadable_error_document_keeps_the_status() {
    let oversized = format!("{{\"error\":\"{}\"}}", "x".repeat(70 * 1024));
    let cases = [
        // A proxy's HTML page: not JSON, refused before it is read.
        (
            502,
            Reply::text(502, "text/html", "<html>bad gateway</html>"),
            ErrorKind::Metadata,
        ),
        // Labelled JSON, but malformed.
        (
            500,
            Reply::json(500, "{\"error\": {\"type\""),
            ErrorKind::Decode,
        ),
        // Larger than the error-document bound.
        (400, Reply::json(400, oversized), ErrorKind::Limit),
        // No body and no media type.
        (413, Reply::new(413), ErrorKind::Metadata),
    ];
    for (status, reply, kind) in cases {
        let (server, es) = serve(vec![reply]).await;
        let error = es.bulk(documented_operations()).await.unwrap_err();
        let Error::Provider(provider) = &error else {
            panic!("{status}: {error:?}")
        };
        assert_eq!(provider.status().as_u16(), status);
        assert_eq!(provider.document().unwrap_err().kind(), kind, "{status}");
        assert_eq!(provider.error_type(), None);
        assert_eq!(error.is_uncertain(), status >= 500);
        assert_eq!(
            error.to_string(),
            format!("Elasticsearch answered HTTP {status}")
        );
        server.shutdown().await;
    }
}

// ---------------------------------------------------------------------------------------------
// 2xx answers whose body cannot be believed: uncertain, never success

#[tokio::test]
async fn malformed_truncated_oversized_and_mislabelled_bulk_answers_are_uncertain() {
    let documented = fixture("bulk-multiple-operations-response.json");
    let mut generated = 0usize;
    let oversized = Body::Generate(
        Box::new(move || {
            generated += 1;
            match generated {
                1 => Some(Bytes::from_static(
                    b"{\"took\":1,\"errors\":false,\"items\":[",
                )),
                n if n < 2 + RESPONSE_LIMIT / 65536 + 2 => Some(Bytes::from(vec![b' '; 65536])),
                _ => None,
            }
        }),
        None,
    );
    let cases: Vec<(&str, Reply, ErrorKind)> = vec![
        (
            "malformed",
            Reply::json(200, "{\"took\":1,"),
            ErrorKind::Decode,
        ),
        (
            "truncated",
            Reply::new(200)
                .header("content-type", "application/json")
                .with_body(Body::FailAfter(vec![Bytes::copy_from_slice(
                    &documented[..documented.len() / 2],
                )])),
            ErrorKind::Transport,
        ),
        (
            "oversized",
            Reply::new(200)
                .header("content-type", "application/json")
                .with_body(oversized),
            ErrorKind::Limit,
        ),
        (
            "wrong media",
            Reply::text(200, "text/html", documented.clone()),
            ErrorKind::Metadata,
        ),
        (
            "unlabelled",
            Reply::new(200).body(documented.clone()),
            ErrorKind::Metadata,
        ),
        (
            // The bulk reference's `filter_path=items.*.error` answer: items without status,
            // without `took` and `errors`; it cannot be matched with the operations.
            "filtered",
            json(200, "bulk-filtered-errors-response.json"),
            ErrorKind::Decode,
        ),
    ];
    for (name, reply, kind) in cases {
        let (server, es) = serve(vec![reply]).await;
        let error = es.bulk(failed_actions_operations()).await.unwrap_err();
        assert_eq!(concord_kind(&error), Some(kind), "{name}: {error:?}");
        assert!(error.is_uncertain(), "{name}");
        assert_eq!(
            error.concord().unwrap().delivery(),
            Delivery::Answered,
            "{name}"
        );
        assert!(error.partial_bulk().is_none());
        server.shutdown().await;
    }
}

#[tokio::test]
async fn no_answer_is_uncertain() {
    let (server, es) = serve(vec![Reply::hang_up()]).await;
    let error = es.bulk(documented_operations()).await.unwrap_err();
    assert_eq!(concord_kind(&error), Some(ErrorKind::Transport));
    assert_eq!(error.concord().unwrap().delivery(), Delivery::Unknown);
    assert!(error.is_uncertain());
    server.shutdown().await;
}

// Paused clock: the 300 ms timeout fires only once the server has written the head, so it ends the
// stalled body and never the wait for the head.
#[tokio::test(start_paused = true)]
async fn a_stalled_answer_is_bounded_by_the_transport_timeout() {
    let script = Script::new(vec![
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Stall),
    ]);
    let server = TestServer::start(move |_: &Recorded| script.next()).await;
    let timeout = Duration::from_millis(300);
    let http = reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .timeout(timeout)
        .build()
        .unwrap();
    let es = Elasticsearch::new(http, &server.url(), ApiKey::new(KEY).unwrap()).unwrap();
    let operations = documented_operations();
    let error = paused::time_out_in_body(&server, |_| true, timeout, es.bulk(operations))
        .await
        .unwrap_err();
    assert_eq!(concord_kind(&error), Some(ErrorKind::Transport));
    assert_eq!(error.concord().unwrap().delivery(), Delivery::Answered);
    assert!(error.is_uncertain(), "a 2xx head arrived, its body did not");
    server.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// 2xx answers that contradict the request

#[tokio::test]
async fn items_that_cannot_be_matched_with_operations_are_protocol_errors() {
    // 4 operations, 3 items.
    let (server, es) = serve(vec![json(200, "bulk-failed-actions-response.json")]).await;
    let error = es.bulk(documented_operations()).await.unwrap_err();
    assert!(matches!(
        error,
        Error::Protocol(Protocol::ItemCount {
            submitted: 4,
            returned: 3
        })
    ));
    assert!(error.is_uncertain());
    server.shutdown().await;

    // Position 2 submitted as `index`, answered as `create`.
    let (server, es) = serve(vec![json(200, "bulk-failed-actions-response.json")]).await;
    let mut request = BulkRequest::new();
    request
        .update("index1", "5", &json!({ "my_field": "foo" }))
        .unwrap()
        .update("index1", "6", &json!({ "my_field": "foo" }))
        .unwrap()
        .index("index1", "7", &json!({ "my_field": "foo" }))
        .unwrap();
    let error = es.bulk(request).await.unwrap_err();
    assert!(matches!(
        error,
        Error::Protocol(Protocol::ItemAction { position: 2 })
    ));
    server.shutdown().await;

    // Position 1 submitted for id `8`, answered for id `6`.
    let (server, es) = serve(vec![json(200, "bulk-failed-actions-response.json")]).await;
    let mut request = BulkRequest::new();
    request
        .update("index1", "5", &json!({ "my_field": "foo" }))
        .unwrap()
        .update("index1", "8", &json!({ "my_field": "foo" }))
        .unwrap()
        .create("index1", "7", &json!({ "my_field": "foo" }))
        .unwrap();
    let error = es.bulk(request).await.unwrap_err();
    assert!(matches!(
        error,
        Error::Protocol(Protocol::ItemId { position: 1 })
    ));
    server.shutdown().await;
}

#[tokio::test]
async fn an_errors_flag_that_contradicts_the_items_is_never_success() {
    // The documented all-successful answer with `errors: true`.
    let flipped = String::from_utf8(fixture("bulk-multiple-operations-response.json"))
        .unwrap()
        .replace("\"errors\": false", "\"errors\": true");
    // The documented failed-actions answer with `errors: false`.
    let hidden = String::from_utf8(fixture("bulk-failed-actions-response.json"))
        .unwrap()
        .replace("\"errors\": true", "\"errors\": false");
    for (body, request) in [
        (flipped, documented_operations()),
        (hidden, failed_actions_operations()),
    ] {
        let (server, es) = serve(vec![Reply::json(200, body)]).await;
        let error = es.bulk(request).await.unwrap_err();
        assert!(
            matches!(error, Error::Protocol(Protocol::ErrorsFlag)),
            "{error:?}"
        );
        assert!(error.is_uncertain());
        server.shutdown().await;
    }
}

#[tokio::test]
async fn pit_answers_that_break_the_contract_are_protocol_errors() {
    let page_without_pit_id = String::from_utf8(fixture("pit-search-page-1.json"))
        .unwrap()
        .replace("\"pit_id\" : \"SYNTHETIC-PIT-ID-2\",", "");
    let page_without_sort = String::from_utf8(fixture("pit-search-page-2.json"))
        .unwrap()
        .replace("\"sort\" :", "\"unsorted\" :");
    let page_with_empty_sort = String::from_utf8(fixture("pit-search-page-2.json"))
        .unwrap()
        .replace("\"2021-05-20T05:30:05.000Z\",\n          8589934593", "");
    let cases: Vec<(Vec<Reply>, u32, Protocol)> = vec![
        (
            vec![Reply::json(200, page_without_pit_id)],
            10,
            Protocol::MissingPitId,
        ),
        (
            vec![Reply::json(200, page_without_sort)],
            10,
            Protocol::MissingSort { position: 0 },
        ),
        (
            vec![Reply::json(200, page_with_empty_sort)],
            10,
            Protocol::MissingSort { position: 0 },
        ),
        (
            vec![json(200, "pit-search-page-1.json")],
            1,
            Protocol::TooManyHits {
                requested: 1,
                returned: 2,
            },
        ),
        (
            // The same last sort values twice: the cursor does not advance.
            vec![
                json(200, "pit-search-page-2.json"),
                json(200, "pit-search-page-2.json"),
            ],
            10,
            Protocol::StalledCursor,
        ),
    ];
    for (pages, size, expected) in cases {
        let mut replies = vec![json(200, "pit-open-response.json")];
        replies.extend(pages);
        let (server, es) = serve(replies).await;
        let pit = es
            .open_point_in_time("my-index-000001", one_minute())
            .await
            .unwrap();
        let search = PitSearch::new(vec![Sort::asc("n")], size).unwrap();
        let failed = es
            .search_after(&pit, &search, NonZeroUsize::new(4).unwrap())
            .items()
            .collect_complete()
            .await
            .unwrap_err();
        let error = failed.error.into_provider_error().unwrap().0;
        assert!(
            matches!(&error, Error::Protocol(p) if *p == expected),
            "{expected:?}: {error:?}"
        );
        server.shutdown().await;
    }

    let (server, es) = serve(vec![Reply::json(200, r#"{"id":"","_shards":{}}"#)]).await;
    let error = es
        .open_point_in_time("my-index-000001", one_minute())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Protocol(Protocol::EmptyPitId)));
    server.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Inputs refused before anything is sent

#[tokio::test]
async fn invalid_inputs_are_refused_before_sending() {
    for name in ["", "_bulk", "a,b", "logs-*", "a/b", "a b", "a\nb"] {
        assert!(
            matches!(
                IndexName::new(name),
                Err(Error::Invalid(Invalid::IndexName))
            ),
            "{name:?}"
        );
    }
    let mut request = BulkRequest::new();
    let refused = |r: Result<&mut BulkRequest, Error>| match r {
        Err(Error::Invalid(invalid)) => invalid,
        other => panic!("{other:?}"),
    };
    assert_eq!(refused(request.delete("test", "")), Invalid::DocumentId);
    assert_eq!(refused(request.delete("_all", "1")), Invalid::IndexName);
    assert_eq!(
        refused(request.index("test", "1", &[1, 2])),
        Invalid::DocumentNotObject
    );
    assert_eq!(
        refused(request.create("test", "1", "text")),
        Invalid::DocumentNotObject
    );
    assert_eq!(
        refused(request.update("test", "1", &Value::Null)),
        Invalid::DocumentNotObject
    );
    let pretty = RawValue::from_string("{\n\"a\": 1}".to_owned()).unwrap();
    assert_eq!(
        refused(request.index("test", "1", &pretty)),
        Invalid::DocumentLineBreak
    );
    assert!(request.is_empty());

    for (ops, bytes) in [(0, 10), (10_001, 10), (1, 0), (1, 100 * 1024 * 1024 + 1)] {
        assert!(matches!(
            BulkLimits::new(ops, bytes),
            Err(Error::Invalid(Invalid::BulkLimits))
        ));
    }
    let mut request = BulkRequest::with_limits(BulkLimits::new(2, 1024).unwrap());
    request
        .delete("test", "1")
        .unwrap()
        .delete("test", "2")
        .unwrap();
    assert_eq!(
        refused(request.delete("test", "3")),
        Invalid::TooManyOperations
    );
    assert_eq!(request.len(), 2);
    let mut request = BulkRequest::with_limits(BulkLimits::new(10, 64).unwrap());
    request.delete("test", "1").unwrap();
    let before = request.bytes();
    assert_eq!(
        before,
        "{\"delete\":{\"_index\":\"test\",\"_id\":\"1\"}}\n".len()
    );
    assert_eq!(refused(request.delete("test", "2")), Invalid::TooManyBytes);
    assert_eq!((request.len(), request.bytes()), (1, before));

    assert!(matches!(
        PitSearch::new(vec![], 10),
        Err(Error::Invalid(Invalid::Sort))
    ));
    for size in [0, 10_001] {
        assert!(matches!(
            PitSearch::new(vec![Sort::asc("a")], size),
            Err(Error::Invalid(Invalid::PageSize))
        ));
    }
    assert!(matches!(
        Sort::clause(json!({ "a": "asc", "b": "desc" })),
        Err(Error::Invalid(Invalid::Sort))
    ));
    assert!(matches!(
        PitSearch::new(vec![Sort::asc("a")], 1)
            .unwrap()
            .with_query(json!([1])),
        Err(Error::Invalid(Invalid::Query))
    ));
    for key in ["", "has space", "caf\u{e9}"] {
        assert!(matches!(
            ApiKey::new(key),
            Err(Error::Invalid(Invalid::ApiKey))
        ));
    }

    // An empty bulk request and an invalid index never reach the server.
    let (server, es) = serve(vec![]).await;
    assert!(matches!(
        es.bulk(BulkRequest::new()).await,
        Err(Error::Invalid(Invalid::EmptyBulk))
    ));
    assert!(matches!(
        es.open_point_in_time("_bulk", one_minute()).await,
        Err(Error::Invalid(Invalid::IndexName))
    ));
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Redaction

#[tokio::test]
async fn secrets_and_document_content_stay_out_of_debug_and_display() {
    let key = ApiKey::new(KEY).unwrap();
    assert_eq!(format!("{key:?}"), "ApiKey(<redacted>)");

    let (server, es) = serve(vec![
        json(400, "error-illegal-argument.json"),
        json(200, "bulk-secret-reason-response.json"),
        json(200, "pit-open-response.json"),
        json(200, "pit-search-page-1.json"),
    ])
    .await;
    let shown = format!("{es:?}");
    assert!(!shown.contains(KEY), "{shown}");
    assert!(shown.contains("<redacted>"), "{shown}");

    let mut request = BulkRequest::new();
    request
        .index(
            "articles",
            "a-1",
            &json!({ "count": "SYNTHETIC-SECRET-4111" }),
        )
        .unwrap();
    let shown = format!("{request:?}");
    assert!(
        !shown.contains("SYNTHETIC-SECRET") && !shown.contains("a-1"),
        "{shown}"
    );
    let prepared = es.api().bulk(request.lines()).prepare().unwrap();
    let shown = format!("{prepared:?}");
    assert!(shown.contains("authorization: <redacted>"), "{shown}");
    assert!(
        !shown.contains(KEY) && !shown.contains("SYNTHETIC-SECRET"),
        "{shown}"
    );
    assert!(prepared.request().headers()["authorization"].is_sensitive());

    // An error document whose reason quotes document content.
    let error = es.bulk(request.clone()).await.unwrap_err();
    for shown in [error.to_string(), format!("{error:?}")] {
        assert!(
            !shown.contains("SYNTHETIC-SECRET") && !shown.contains(KEY),
            "{shown}"
        );
    }

    // A failed item whose reason quotes the document: withheld unless asked for.
    let error = es.bulk(request).await.unwrap_err();
    for shown in [error.to_string(), format!("{error:?}")] {
        assert!(!shown.contains("SYNTHETIC-SECRET"), "{shown}");
    }
    let report = error.into_partial_bulk().unwrap();
    let cause = report.items()[0].error().unwrap();
    assert_eq!(cause.kind(), "document_parsing_exception");
    assert!(cause.reason().unwrap().contains("SYNTHETIC-SECRET-4111"));

    // PIT ids, sort values and sources.
    let pit = es
        .open_point_in_time("my-index-000001", one_minute())
        .await
        .unwrap();
    let shown = format!("{pit:?}");
    assert!(!shown.contains(GUIDE_PIT_ID), "{shown}");
    let search = PitSearch::new(vec![Sort::asc("n")], 10).unwrap();
    let queried = search
        .clone()
        .with_query(json!({ "term": { "card": "SYNTHETIC-SECRET-4111" } }))
        .unwrap();
    let shown = format!("{queried:?} {:?}", Sort::asc("SYNTHETIC-SECRET-FIELD"));
    assert!(!shown.contains("SYNTHETIC-SECRET"), "{shown}");
    let mut pages = es.search_after(&pit, &search, NonZeroUsize::new(1).unwrap());
    let page = pages.next_page().await.unwrap().unwrap();
    let shown = format!("{:?} {:?}", page.items, page.items[0].sort());
    assert!(
        !shown.contains("elkbee") && !shown.contains("2021-05-20"),
        "{shown}"
    );
    assert_eq!(
        page.items[0].source::<Value>().unwrap().unwrap()["user"]["id"],
        "elkbee"
    );
    let shown = format!("{:?}", search.body(&pit, page.next.clone()));
    assert!(
        !shown.contains("SYNTHETIC-PIT-ID") && !shown.contains("2021"),
        "{shown}"
    );
    server.shutdown().await;

    // A transport failure: no key, no URL.
    let es = Elasticsearch::new(http(), &server_gone().await, ApiKey::new(KEY).unwrap()).unwrap();
    let error = es.bulk(documented_operations()).await.unwrap_err();
    assert_eq!(concord_kind(&error), Some(ErrorKind::Transport));
    for shown in [error.to_string(), format!("{error:?}")] {
        assert!(
            !shown.contains(KEY) && !shown.contains("127.0.0.1"),
            "{shown}"
        );
    }
}

/// The URL of a server that no longer listens.
async fn server_gone() -> String {
    let (server, _) = serve(vec![]).await;
    let url = server.url();
    server.shutdown().await;
    url
}

#[test]
fn header_names_are_valid() {
    concord_sdk_elasticsearch::ElasticsearchApi::validate_header_names().unwrap();
}

#[test]
fn the_report_types_say_what_each_item_reports() {
    // The documented results and actions decode as named.
    let response: concord_sdk_elasticsearch::BulkResponse =
        serde_json::from_slice(&fixture("bulk-multiple-operations-response.json")).unwrap();
    let actions: Vec<Action> = response.items.iter().map(|i| i.action()).collect();
    assert_eq!(
        actions,
        [
            Action::Index,
            Action::Delete,
            Action::Create,
            Action::Update
        ]
    );
    let results: Vec<Option<ItemResult>> =
        response.items.iter().map(|i| i.status().result).collect();
    assert_eq!(
        results,
        [
            Some(ItemResult::Created),
            Some(ItemResult::NotFound),
            Some(ItemResult::Created),
            Some(ItemResult::Updated)
        ]
    );
}
