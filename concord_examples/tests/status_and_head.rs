mod common;
use common::*;

use concord_core::prelude::*;
use concord_macros::api;
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
        let (transport, _recorded) = MockTransport::new(vec![MockReply::status(StatusCode::NO_CONTENT)]);
        let api = ApiNoContent::new_with_transport( transport);

        let err = api.execute(endpoints::A::new()).await.unwrap_err();
        match err {
            ApiClientError::InEndpoint { source, .. } => match *source {
                ApiClientError::NoContentStatusRequiresNoContent { status, .. } => {
                    assert_eq!(status, StatusCode::NO_CONTENT);
                }
                other => panic!("unexpected inner error: {other:?}"),
            },
            other => panic!("unexpected error: {other:?}"),
        }
    }

    // B with 204 => ok
    {
        let (transport, _recorded) = MockTransport::new(vec![MockReply::status(StatusCode::NO_CONTENT)]);
        let api = ApiNoContent::new_with_transport( transport);

        let _ = api.execute(endpoints::B::new()).await.unwrap();
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
        let (transport, _recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
        let api = ApiHead::new_with_transport( transport);

        let err = api.execute(endpoints::A::new()).await.unwrap_err();
        match err {
            ApiClientError::InEndpoint { source, .. } => match *source {
                ApiClientError::HeadRequiresNoContent { .. } => {}
                other => panic!("unexpected inner error: {other:?}"),
            },
            other => panic!("unexpected error: {other:?}"),
        }
    }

    // B => ok
    {
        let (transport, _recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);
        let api = ApiHead::new_with_transport( transport);

        let _ = api.execute(endpoints::B::new()).await.unwrap();
    }
}
