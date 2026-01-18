use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;

api! {
    client ApiPaged {
        scheme: https,
        host: "example.com",
    }

    // Use custom keys "p"/"sz" to detect accidental injection of "page"/"per_page".
    GET List "x"
    query {
        "p"  as page: u32 = 1,
        "sz" as page_size: u32 = 2
    }
    paginate PagedPagination {
        // Ensure the controller uses the endpoint wire keys (p/sz), not the defaults (page/per_page).
        page_key     = "p".into(),
        per_page_key = "sz".into(),
        page     = ep.page as u64,
        per_page = ep.page_size as u64
    }
    -> Json<Vec<String>>;
}

#[tokio::test(flavor = "current_thread")]
async fn paged__page_increments__stops_on_short_page() {
    use api_paged::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&vec!["a".to_string(), "b".to_string()])),
            MockReply::ok_json(json_bytes(&vec!["c".to_string(), "d".to_string()])),
            MockReply::ok_json(json_bytes(&vec!["e".to_string()])), // short page => stop
        ])
        .build();

    let api = ApiPaged::new_with_transport(transport);

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
        .query_has("p", "1")
        .query_has("sz", "2")
        .query_absent("page")
        .query_absent("per_page")
        .query_absent("cursor")
        .query_absent("offset")
        .query_absent("limit");

    assert_request(&reqs[1])
        .page_index(1)
        .query_has("p", "2")
        .query_has("sz", "2")
        .query_absent("page")
        .query_absent("per_page");

    assert_request(&reqs[2])
        .page_index(2)
        .query_has("p", "3")
        .query_has("sz", "2")
        .query_absent("page")
        .query_absent("per_page");

    h.finish();
}

#[tokio::test(flavor = "current_thread")]
async fn paged__max_items_truncates_and_limits_requests() {
    use api_paged::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&vec!["a".to_string(), "b".to_string()])),
            MockReply::ok_json(json_bytes(&vec!["c".to_string(), "d".to_string()])),
        ])
        .build();

    let api = ApiPaged::new_with_transport(transport);

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

    h.assert_recorded_len(2);
    let reqs = h.recorded();

    assert_request(&reqs[0])
        .page_index(0)
        .query_has("p", "1")
        .query_has("sz", "2");

    assert_request(&reqs[1])
        .page_index(1)
        .query_has("p", "2")
        .query_has("sz", "2");

    h.finish();
}

#[tokio::test(flavor = "current_thread")]
async fn paged__max_pages_errors() {
    use api_paged::*;

    // Provide exactly 2 replies: correct behavior is to error before sending page 3.
    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&vec!["a".to_string(), "b".to_string()])),
            MockReply::ok_json(json_bytes(&vec!["c".to_string(), "d".to_string()])),
        ])
        .build();

    let api = ApiPaged::new_with_transport(transport);

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
        .query_has("p", "1")
        .query_has("sz", "2");

    assert_request(&reqs[1])
        .page_index(1)
        .query_has("p", "2")
        .query_has("sz", "2");

    h.finish();
}
