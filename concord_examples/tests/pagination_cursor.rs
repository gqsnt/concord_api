mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Item { id: String }

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Page {
    items: Vec<Item>,
    next: Option<String>,
}
impl PageItems for Page {
    type Item = Item;
    type IntoIter = std::vec::IntoIter<Item>;
    fn len(&self) -> usize { self.items.len() }
    fn inner_into_iter(self) -> Self::IntoIter { self.items.into_iter() }
}
impl HasNextCursor for Page {
    type Cursor = String;
    fn next_cursor(&self) -> Option<&Self::Cursor> { self.next.as_ref() }
}

#[tokio::test]
async fn cursor_pagination_keys_flow_and_first_cursor_omitted() {
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
    use api_cursor::*;

    let p1 = Page { items: vec![Item{ id: "1".into() }], next: Some("c1".into()) };
    let p2 = Page { items: vec![Item{ id: "2".into() }], next: None };

    let (transport, recorded) = MockTransport::new(vec![
        MockReply::ok_json(json_bytes(&p1)),
        MockReply::ok_json(json_bytes(&p2)),
    ]);

    let api = ApiClient::<Cx>::with_transport(Vars::new(), transport);
    let out: Vec<Item> = api.collect_all_items(endpoints::List::new()).await.unwrap();
    assert_eq!(out.len(), 2);

    let reqs = recorded.lock().unwrap();
    assert_eq!(reqs.len(), 2);

    // page 0: pageSize present, pageCursor absent
    {
        let q0: Vec<(String,String)> = reqs[0].url.query_pairs()
            .map(|(k,v)| (k.to_string(), v.to_string()))
            .collect();
        assert!(q0.iter().any(|(k,v)| k=="pageSize" && v=="2"));
        assert!(!q0.iter().any(|(k,_)| k=="pageCursor"));
        assert!(!q0.iter().any(|(k,_)| k=="per_page"));
        assert!(!q0.iter().any(|(k,_)| k=="cursor"));
    }

    // page 1: pageCursor=c1 present
    {
        let q1: Vec<(String,String)> = reqs[1].url.query_pairs()
            .map(|(k,v)| (k.to_string(), v.to_string()))
            .collect();
        assert!(q1.iter().any(|(k,v)| k=="pageSize" && v=="2"));
        assert!(q1.iter().any(|(k,v)| k=="pageCursor" && v=="c1"));
        assert!(!q1.iter().any(|(k,_)| k=="per_page"));
        assert!(!q1.iter().any(|(k,_)| k=="cursor"));
    }
}

#[tokio::test]
async fn cursor_loop_detection_and_max_pages() {
    api! {
      client ApiCursorLoop {
        scheme: https,
        host: "example.com",
      }

      GET List "x"
      query {
        "pageCursor" as page_cursor?: String,
        "pageSize"   as page_size: u64 = 1
      }
      paginate CursorPagination {
        cursor   = ep.page_cursor,
        per_page = ep.page_size
      }
      -> Json<Page>;
    }
    use api_cursor_loop::*;

    // loop: next cursor always "same"
    let p = Page { items: vec![Item{ id: "1".into() }], next: Some("same".into()) };

    // 3 replies are enough to detect loop at page 1 (same key repeated)
    let (transport, _recorded) = MockTransport::new(vec![
        MockReply::ok_json(json_bytes(&p)),
        MockReply::ok_json(json_bytes(&p)),
        MockReply::ok_json(json_bytes(&p)),
    ]);

    let api = ApiClient::<Cx>::with_transport(Vars::new(), transport);

    let err = api
        .collect_all_items(endpoints::List::new())
        .detect_loops(true)
        .await
        .unwrap_err();

    match err {
        ApiClientError::Pagination(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }

    // max_pages hit
    let p2 = Page { items: vec![Item{ id: "1".into() }], next: Some("c".into()) };
    let (transport, _recorded) = MockTransport::new(vec![
        MockReply::ok_json(json_bytes(&p2)),
        MockReply::ok_json(json_bytes(&p2)),
        MockReply::ok_json(json_bytes(&p2)),
    ]);
    let api = ApiClient::<Cx>::with_transport(Vars::new(), transport);

    let err = api
        .collect_all_items(endpoints::List::new())
        .max_pages(2)
        .detect_loops(false)
        .await
        .unwrap_err();

    match err {
        ApiClientError::PaginationLimit(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }
}
