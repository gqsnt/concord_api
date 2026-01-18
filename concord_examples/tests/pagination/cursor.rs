use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Item {
    id: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Page {
    items: Vec<Item>,
    next: Option<String>,
}

impl PageItems for Page {
    type Item = Item;
    type IntoIter = std::vec::IntoIter<Item>;
    fn len(&self) -> usize {
        self.items.len()
    }
    fn inner_into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl HasNextCursor for Page {
    type Cursor = String;
    fn next_cursor(&self) -> Option<&Self::Cursor> {
        self.next.as_ref()
    }
}

api! {
    client ApiCursor {
        scheme: https,
        host: "example.com",
    }

    GET List "x"
    query {
        "pageCursor" as page_cursor?: String,
        "pageSize"   as page_size: u64 = 2
    }
    paginate CursorPagination {
        cursor   = ep.page_cursor,
        per_page = ep.page_size
    }
    -> Json<Page>;
}

#[tokio::test(flavor = "current_thread")]
async fn pagination_cursor__keys_flow__first_cursor_omitted() {
    use api_cursor::*;

    let p1 = Page {
        items: vec![Item { id: "1".into() }],
        next: Some("c1".into()),
    };
    let p2 = Page {
        items: vec![Item { id: "2".into() }],
        next: None,
    };

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&p1)),
            MockReply::ok_json(json_bytes(&p2)),
        ])
        .build();

    let api = ApiCursor::new_with_transport(transport);
    let out = api
        .request(endpoints::List::new())
        .paginate()
        .collect()
        .await
        .unwrap();

    assert_eq!(out.len(), 2);

    h.assert_recorded_len(2);
    let reqs = h.recorded();

    // page 0: pageSize present, pageCursor absent
    assert_request(&reqs[0])
        .page_index(0)
        .query_has("pageSize", "2")
        .query_absent("pageCursor")
        .query_absent("per_page")
        .query_absent("cursor")
        .query_absent("page")
        .query_absent("offset")
        .query_absent("limit");

    // page 1: pageCursor=c1 present
    assert_request(&reqs[1])
        .page_index(1)
        .query_has("pageSize", "2")
        .query_has("pageCursor", "c1")
        .query_absent("per_page")
        .query_absent("cursor")
        .query_absent("page")
        .query_absent("offset")
        .query_absent("limit");

    h.finish();
}

#[tokio::test(flavor = "current_thread")]
async fn pagination_cursor__loop_detection__and__max_pages() {
    use api_cursor::*;

    // loop detected: next cursor repeats
    {
        let p = Page {
            items: vec![Item { id: "1".into() }],
            next: Some("same".into()),
        };

        let (transport, h) = mock()
            .replies([
                MockReply::ok_json(json_bytes(&p)),
                MockReply::ok_json(json_bytes(&p)),
            ])
            .build();

        let api = ApiCursor::new_with_transport(transport);

        let err = api
            .request(endpoints::List::new())
            .paginate()
            .detect_loops(true)
            .collect()
            .await
            .unwrap_err();

        match err {
            ApiClientError::Pagination { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }

        h.assert_recorded_len(2);
        let reqs = h.recorded();
        assert_request(&reqs[0])
            .page_index(0)
            .query_has("pageSize", "2")
            .query_absent("pageCursor");
        assert_request(&reqs[1])
            .page_index(1)
            .query_has("pageSize", "2")
            .query_has("pageCursor", "same");

        h.finish();
    }

    // max_pages reached
    {
        let p = Page {
            items: vec![Item { id: "1".into() }],
            next: Some("c".into()),
        };

        // Provide exactly 2 replies: correct behavior is to error before sending page 3.
        let (transport, h) = mock()
            .replies([
                MockReply::ok_json(json_bytes(&p)),
                MockReply::ok_json(json_bytes(&p)),
            ])
            .build();

        let api = ApiCursor::new_with_transport(transport);

        let err = api
            .request(endpoints::List::new())
            .paginate()
            .max_pages(2)
            .detect_loops(false)
            .collect()
            .await
            .unwrap_err();

        match err {
            ApiClientError::PaginationLimit { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }

        h.assert_recorded_len(2);
        let reqs = h.recorded();
        assert_request(&reqs[0]).page_index(0).query_absent("pageCursor");
        assert_request(&reqs[1]).page_index(1).query_has("pageCursor", "c");

        h.finish();
    }
}
