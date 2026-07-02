use bytes::Bytes;
use concord_core::prelude::PaginationTermination;
use concord_examples::custom_pagination::{CustomPaginationApi, Item as CustomPaginationItem};
use concord_test_support::{MockReply, assert_request, mock};

#[tokio::test]
async fn custom_pagination_collects_pages() {
    let (transport, handle) = mock()
        .reply(json_reply(r#"{"items":[{"id":1},{"id":2}]}"#))
        .reply(json_reply(r#"{"items":[{"id":3}]}"#))
        .build();
    let api = CustomPaginationApi::new_with_transport(transport);

    let items = api
        .list()
        .paginate(PaginationTermination::hard_page_cap(10))
        .collect()
        .await
        .expect("custom pagination collect succeeds");

    assert_eq!(
        items,
        vec![
            CustomPaginationItem { id: 1 },
            CustomPaginationItem { id: 2 },
            CustomPaginationItem { id: 3 },
        ]
    );
    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 2);
    assert_request(&recorded[0])
        .path("/")
        .header("x-page", "1")
        .header("x-count", "2");
    assert_request(&recorded[1])
        .path("/")
        .header("x-page", "2")
        .header("x-count", "2");
    handle.finish();
}

fn json_reply(body: &'static str) -> MockReply {
    MockReply::ok_json(Bytes::from_static(body.as_bytes()))
}
