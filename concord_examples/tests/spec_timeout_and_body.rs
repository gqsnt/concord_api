use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::header::CONTENT_TYPE;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct NewObj {
    id: String,
}

#[tokio::test]
async fn timeout_layering_client_path_endpoint() {
    api! {
        client ApiTimeout {
            scheme: https,
            host: "example.com",
            timeout: core::time::Duration::from_secs(30)
        }

        path "x" {
            timeout: core::time::Duration::from_secs(10)
            GET A "" timeout: core::time::Duration::from_secs(2), -> Json<()>;
        }
    }

    use api_timeout::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&())))
        .build();

    let api = ApiTimeout::new_with_transport(transport);
    let _ = api.request(endpoints::A::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).timeout(Some(core::time::Duration::from_secs(2)));

    h.finish();
}

#[tokio::test]
async fn content_type_injection_only_when_missing_and_body_present() {
    api! {
        client ApiBody {
            scheme: https,
            host: "example.com",
        }

        POST A "x" body Json<NewObj> -> Json<()>;

        // Explicit content-type should not be overridden
        POST B "y"
        headers { "content-type" = "text/plain" }
        body Json<NewObj>
        -> Json<()>;

        // GET (no body) must not inject Content-Type
        GET C "z" -> Json<()>;
    }

    use api_body::*;

    // A => inject application/json
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiBody::new_with_transport(transport);
        let _ = api
            .request(endpoints::A::new(NewObj { id: "1".into() }))
            .execute()
            .await
            .unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0])
            .header(CONTENT_TYPE, "application/json")
            .body_present();

        h.finish();
    }

    // B => keep text/plain
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiBody::new_with_transport(transport);
        let _ = api
            .request(endpoints::B::new(NewObj { id: "1".into() }))
            .execute()
            .await
            .unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0])
            .header(CONTENT_TYPE, "text/plain")
            .body_present();

        h.finish();
    }

    // C => no Content-Type injected
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiBody::new_with_transport(transport);
        let _ = api.request(endpoints::C::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).header_absent(CONTENT_TYPE).body_absent();

        h.finish();
    }
}

#[tokio::test]
async fn timeout_endpoint_allows_no_comma_before_arrow() {
    api! {
        client ApiTimeoutNoComma {
            scheme: https,
            host: "example.com",
            timeout: core::time::Duration::from_secs(30)
        }

        path "x" {
            timeout: core::time::Duration::from_secs(10)
            // NOTE: no comma before `->`
            GET A "" timeout: core::time::Duration::from_secs(2) -> Json<()>;
        }
    }

    use api_timeout_no_comma::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&())))
        .build();

    let api = ApiTimeoutNoComma::new_with_transport(transport);
    let _ = api.request(endpoints::A::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).timeout(Some(core::time::Duration::from_secs(2)));

    h.finish();
}
