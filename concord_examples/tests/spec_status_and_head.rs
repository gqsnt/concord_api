#![allow(unused_imports)]
use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::StatusCode;

#[tokio::test]
async fn status_204_requires_no_content_response_spec() {
    api! {
        client ApiNoContent {
            base https "example.com"
        }

        GET A
            path ["a"]
        -> Json<()>

        GET B
            path ["b"]
        -> NoContent<()>
    }

    use api_no_content::*;

    {
        let (transport, h) = mock()
            .reply(MockReply::status(StatusCode::NO_CONTENT))
            .build();

        let api = ApiNoContent::new_with_transport(transport);
        let err = api
            .request(endpoints::A::new())
            .execute()
            .await
            .unwrap_err();

        match err {
            ApiClientError::NoContentStatusRequiresNoContent { status, .. } => {
                assert_eq!(status, StatusCode::NO_CONTENT);
            }
            other => panic!("unexpected error: {other:?}"),
        }

        h.finish();
    }

    {
        let (transport, h) = mock()
            .reply(MockReply::status(StatusCode::NO_CONTENT))
            .build();

        let api = ApiNoContent::new_with_transport(transport);
        api.request(endpoints::B::new()).execute().await.unwrap();

        h.finish();
    }
}

#[tokio::test]
async fn head_requires_no_content_response_spec() {
    api! {
        client ApiHead {
            base https "example.com"
        }

        HEAD A
            path ["a"]
        -> Json<()>

        HEAD B
            path ["b"]
        -> NoContent<()>
    }

    use api_head::*;

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiHead::new_with_transport(transport);
        let err = api
            .request(endpoints::A::new())
            .execute()
            .await
            .unwrap_err();

        match err {
            ApiClientError::HeadRequiresNoContent { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }

        h.finish();
    }

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiHead::new_with_transport(transport);
        api.request(endpoints::B::new()).execute().await.unwrap();

        h.finish();
    }
}
