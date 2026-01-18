use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;

api! {
    client ApiOffsetLimit {
        scheme: https,
        host: "example.com",
    }

    // Use custom keys to detect accidental injection of "offset"/"limit".
    GET List "x"
    query {
        "start" as start: u64 = 0,
        "count" as count: u64 = 2
    }
    paginate OffsetLimitPagination {
        offset = ep.start,
        limit  = ep.count
    }
    -> Json<Vec<String>>;
}

#[tokio::test(flavor = "current_thread")]
async fn offset_limit__offset_increments__stops_on_short_page() {
    use api_offset_limit::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&vec!["a".to_string(), "b".to_string()])),
            MockReply::ok_json(json_bytes(&vec!["c".to_string(), "d".to_string()])),
            MockReply::ok_json(json_bytes(&vec!["e".to_string()])), // short page => stop
        ])
        .build();

    let api = ApiOffsetLimit::new_with_transport(transport);

    let out = api
        .request(endpoints::List::new())
        .paginate()
        .collect()
        .await
        .unwrap();

    assert_eq!(out, vec!["a", "b", "c", "d", "e"]);

    h.assert_recorded_len(3);
    let reqs = h.recorded();

    assert_request(&reqs[0])
        .page_index(0)
        .query_has("start", "0")
        .query_has("count", "2")
        .query_absent("offset")
        .query_absent("limit")
        .query_absent("page")
        .query_absent("per_page")
        .query_absent("cursor");

    assert_request(&reqs[1])
        .page_index(1)
        .query_has("start", "2")
        .query_has("count", "2")
        .query_absent("offset")
        .query_absent("limit");

    assert_request(&reqs[2])
        .page_index(2)
        .query_has("start", "4")
        .query_has("count", "2")
        .query_absent("offset")
        .query_absent("limit");

    h.finish();
}

#[tokio::test(flavor = "current_thread")]
async fn offset_limit__max_items_truncates_and_limits_requests() {
    use api_offset_limit::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&vec!["a".to_string(), "b".to_string()])),
            MockReply::ok_json(json_bytes(&vec!["c".to_string(), "d".to_string()])),
        ])
        .build();

    let api = ApiOffsetLimit::new_with_transport(transport);

    // New behavior: hitting max_items returns an error (no truncation).
    let err = api
        .request(endpoints::List::new())
        .paginate()
        .max_items(3)
        .collect()
        .await
        .unwrap_err();

    match err {
        ApiClientError::PaginationLimit { .. } => {}
        other => panic!("unexpected error: {other:?}"),
    }

    // Still only 2 requests were sent (the 2 replies we provided).
    h.assert_recorded_len(2);
    let reqs = h.recorded();

    assert_request(&reqs[0])
        .page_index(0)
        .query_has("start", "0")
        .query_has("count", "2");

    assert_request(&reqs[1])
        .page_index(1)
        .query_has("start", "2")
        .query_has("count", "2");

    h.finish();
}

#[tokio::test(flavor = "current_thread")]
async fn offset_limit__max_pages_errors() {
    use api_offset_limit::*;

    // Provide exactly 2 replies: correct behavior is to error before sending page 3.
    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&vec!["a".to_string(), "b".to_string()])),
            MockReply::ok_json(json_bytes(&vec!["c".to_string(), "d".to_string()])),
        ])
        .build();

    let api = ApiOffsetLimit::new_with_transport(transport);

    let err = api
        .request(endpoints::List::new())
        .paginate()
        .max_pages(2)
        .collect()
        .await
        .unwrap_err();

    match err {
        ApiClientError::PaginationLimit { .. } => {}
        other => panic!("unexpected error: {other:?}"),
    }

    h.assert_recorded_len(2);
    let reqs = h.recorded();

    assert_request(&reqs[0])
        .page_index(0)
        .query_has("start", "0")
        .query_has("count", "2");

    assert_request(&reqs[1])
        .page_index(1)
        .query_has("start", "2")
        .query_has("count", "2");

    h.finish();
}