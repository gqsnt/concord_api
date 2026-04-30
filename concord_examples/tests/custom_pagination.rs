use bytes::Bytes;
use concord_examples::custom_pagination::{CustomPaginationApi, Item};
use concord_test_support::{MockReply, assert_request, mock};

#[tokio::test]
async fn custom_pagination_controller_collects_pages() {
    let (transport, handle) = mock()
        .reply(json_reply(r#"{"items":[{"id":1},{"id":2}]}"#))
        .reply(json_reply(r#"{"items":[{"id":3}]}"#))
        .build();
    let api = CustomPaginationApi::new_with_transport(transport);

    let items = api.list_items().paginate().collect().await.unwrap();

    assert_eq!(items, vec![Item { id: 1 }, Item { id: 2 }, Item { id: 3 }]);
    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 2);
    assert_request(&recorded[0])
        .path("/items")
        .query_has("page", "0")
        .header("x-page-cursor", "0");
    assert_request(&recorded[1])
        .path("/items")
        .query_has("page", "1")
        .header("x-page-cursor", "1");
    handle.finish();
}

fn json_reply(body: &'static str) -> MockReply {
    MockReply::ok_json(Bytes::from_static(body.as_bytes()))
}
