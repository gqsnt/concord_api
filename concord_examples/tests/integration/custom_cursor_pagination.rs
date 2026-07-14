use bytes::Bytes;
use concord_core::prelude::PaginationTermination;
use concord_examples::custom_cursor_pagination::{CustomCursorPaginationApi, Item};
use concord_test_support::{ScriptedReply, assert_execution, deterministic_mock};

#[tokio::test]
async fn custom_cursor_pagination_collects_pages() {
    let (transport, handle) = deterministic_mock()
        .reply(json_reply(r#"{"items":[{"id":1},{"id":2}]}"#))
        .reply(json_reply(r#"{"items":[{"id":3}]}"#))
        .build();
    let api = CustomCursorPaginationApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_both(builder)
    })
    .expect("mock client");

    let items = api
        .list_items()
        .paginate(PaginationTermination::hard_page_cap(10))
        .collect()
        .await
        .unwrap();

    assert_eq!(items, vec![Item { id: 1 }, Item { id: 2 }, Item { id: 3 }]);
    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 2);
    assert_execution(&recorded[0])
        .path("/items")
        .header("x-page-cursor", "0");
    assert_execution(&recorded[1])
        .path("/items")
        .header("x-page-cursor", "1");
    handle.finish();
}

fn json_reply(body: &'static str) -> ScriptedReply {
    ScriptedReply::ok_json(Bytes::from_static(body.as_bytes()))
}
