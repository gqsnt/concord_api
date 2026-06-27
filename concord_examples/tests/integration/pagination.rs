use bytes::Bytes;
use concord_core::prelude::PaginationTermination;
use concord_examples::pagination::{Item, PaginationApi, PaginationAuthApi};
use concord_test_support::{MockReply, assert_request, mock};
use http::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn offset_pagination_collects_items_and_preserves_query_shape() {
    let (transport, handle) = mock()
        .reply(json_reply(r#"[{"id":1},{"id":2}]"#))
        .reply(json_reply(r#"[{"id":3}]"#))
        .build();
    let api = PaginationApi::new_with_transport(transport);

    let items = api
        .list_offset()
        .paginate(PaginationTermination::hard_page_cap(10))
        .collect()
        .await
        .expect("offset pagination collect succeeds");

    assert_eq!(ids(&items), vec![1, 2, 3]);
    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 2);
    assert_request(&recorded[0])
        .path("/offset-items")
        .query_has("start", "0")
        .query_has("count", "2");
    assert_request(&recorded[1])
        .path("/offset-items")
        .query_has("start", "2")
        .query_has("count", "2");
    handle.finish();
}

#[tokio::test]
async fn cursor_pagination_for_each_page_uses_next_cursor() {
    let (transport, handle) = mock()
        .reply(json_reply(
            r#"{"items":[{"id":10},{"id":11}],"next_cursor":"next-page"}"#,
        ))
        .reply(json_reply(r#"{"items":[{"id":12}],"next_cursor":null}"#))
        .build();
    let api = PaginationApi::new_with_transport(transport);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen_for_callback = seen.clone();

    api.list_cursor()
        .paginate(PaginationTermination::hard_page_cap(10))
        .for_each_page(move |page| {
            let seen = seen_for_callback.clone();
            async move {
                seen.lock()
                    .await
                    .extend(page.value.items.into_iter().map(|item| item.id));
                Ok(())
            }
        })
        .await
        .expect("cursor pagination for_each_page succeeds");

    assert_eq!(*seen.lock().await, vec![10, 11, 12]);
    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 2);
    assert_request(&recorded[0])
        .path("/cursor-items")
        .query_absent("cursor")
        .query_has("count", "2");
    assert_request(&recorded[1])
        .path("/cursor-items")
        .query_has("cursor", "next-page")
        .query_has("count", "2");
    handle.finish();
}

#[tokio::test]
async fn session_header_pagination_preserves_offset_and_items() {
    let (transport, handle) = mock()
        .reply(json_reply(r#"[{"id":1},{"id":2}]"#))
        .reply(MockReply::status(StatusCode::UNAUTHORIZED))
        .reply(json_reply(r#"[{"id":3}]"#))
        .build();
    let api = PaginationAuthApi::new_with_transport("page-token".to_string(), transport);

    let items = api
        .protected()
        .list_protected()
        .paginate(PaginationTermination::hard_page_cap(10))
        .collect()
        .await
        .expect("auth retry on page N succeeds");

    assert_eq!(ids(&items), vec![1, 2, 3]);
    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 3);
    assert_request(&recorded[0])
        .path("/protected-items")
        .query_has("start", "0")
        .header(http::header::AUTHORIZATION, "Bearer page-token");
    assert_request(&recorded[1])
        .path("/protected-items")
        .query_has("start", "2")
        .header(http::header::AUTHORIZATION, "Bearer page-token");
    assert_request(&recorded[2])
        .path("/protected-items")
        .query_has("start", "2")
        .header(http::header::AUTHORIZATION, "Bearer page-token");
    assert_eq!(recorded[1].meta.page_index, 1);
    assert_eq!(recorded[2].meta.page_index, 1);
    handle.finish();
}

fn ids(items: &[Item]) -> Vec<u64> {
    items.iter().map(|item| item.id).collect()
}

fn json_reply(body: &'static str) -> MockReply {
    MockReply::ok_json(Bytes::from_static(body.as_bytes()))
}
