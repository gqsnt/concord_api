use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::StatusCode;

#[tokio::test]
async fn status_204_requires_no_content_response_spec() {
    api! {
        client ApiNoContent {
            scheme: https,
            host: "example.com",
        }

        // Json response: must error on 204
        GET A "a" -> Json<()>;

        // NoContent response: ok on 204
        GET B "b" -> NoContent<()>;
    }

    use api_no_content::*;

    // A with 204 => error
    {
        let (transport, h) = mock()
            .reply(MockReply::status(StatusCode::NO_CONTENT))
            .build();

        let api = ApiNoContent::new_with_transport(transport);
        let err = api.request(endpoints::A::new()).execute().await.unwrap_err();

        match err {
            ApiClientError::NoContentStatusRequiresNoContent { status, .. } => {
                assert_eq!(status, StatusCode::NO_CONTENT);
            }
            other => panic!("unexpected error: {other:?}"),
        }

        h.finish();
    }

    // B with 204 => ok
    {
        let (transport, h) = mock()
            .reply(MockReply::status(StatusCode::NO_CONTENT))
            .build();

        let api = ApiNoContent::new_with_transport(transport);
        let _ = api.request(endpoints::B::new()).execute().await.unwrap();

        h.finish();
    }
}

#[tokio::test]
async fn head_requires_no_content_response_spec() {
    api! {
        client ApiHead {
            scheme: https,
            host: "example.com",
        }

        HEAD A "a" -> Json<()>;
        HEAD B "b" -> NoContent<()>;
    }

    use api_head::*;

    // A => error HeadRequiresNoContent
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiHead::new_with_transport(transport);
        let err = api.request(endpoints::A::new()).execute().await.unwrap_err();

        match err {
            ApiClientError::HeadRequiresNoContent { .. } => {}
            other => panic!("unexpected error: {other:?}"),
        }

        h.finish();
    }

    // B => ok
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiHead::new_with_transport(transport);
        let _ = api.request(endpoints::B::new()).execute().await.unwrap();

        h.finish();
    }
}
