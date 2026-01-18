use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::header::{ACCEPT, USER_AGENT};

#[tokio::test]
async fn headers_kebab_string_bind_remove_override() {
    api! {
        client ApiHeaders {
            scheme: https,
            host: "example.com",
            headers {
                // ident => kebab-case
                user_agent as user_agent: String = "ua".to_string(),
                x_debug = "caribou",            // => "x-debug"
                "x-static" = "s",              // string key
                "x-flag" as flag: bool = true // bind + default => emitted
            }
        }

        // override x-debug and remove x-static at layer below
        path "p" {
            headers {
                "x-debug" = "override",
                -"x-static"
            }
            // endpoint removes x-flag
            GET One "" headers { -"x-flag" } -> Json<()>;
        }
    }

    use api_headers::*;

    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&())))
        .build();

    let api = ApiHeaders::new_with_transport(transport);
    let _ = api.request(endpoints::One::new()).execute().await.unwrap();

    let reqs = h.recorded();
    let req = &reqs[0];

    assert_request(req)
        .header(USER_AGENT, "ua")
        .header("x-debug", "override")
        .header_absent("x-static")
        .header_absent("x-flag");

    h.finish();
}

#[tokio::test]
async fn header_value_from_cx_to_string_and_invalid_header_value_error() {
    api! {
        client ApiHeaderInvalid {
            scheme: https,
            host: "example.com",
            headers {
                "x-bad" as bad: String,      // client var
                "x-bool" as trace: bool = false,
                "x-bad" = cx.bad,            // uses cx
                "x-bool" = cx.trace         // ToString => "false"
            }
        }
        GET One "" -> Json<()>;
    }

    use api_header_invalid::*;

    // OK: trace false emits "false"
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiHeaderInvalid::new_with_transport("ok".to_string(), transport);
        let _ = api.request(endpoints::One::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).header("x-bool", "false");

        h.finish();
    }

    // invalid header value (newline) => ApiClientError::InvalidParam
    {
        let (transport, h) = mock().build();

        let api = ApiHeaderInvalid::new_with_transport("a\nb".to_string(), transport);
        let err = api.request(endpoints::One::new()).execute().await.unwrap_err();

        h.assert_recorded_len(0);

        match err {
            ApiClientError::InvalidParam { param, .. } => {
                assert!(param.contains("header"));
                assert!(param.contains("x-bad"));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        h.finish();
    }
}

#[tokio::test]
async fn accept_injection_runtime_vs_endpoint_explicit_and_remove() {
    api! {
        client ApiAccept {
            scheme: https,
            host: "example.com",
            headers { "accept" = "text/plain" } // set at client
        }

        // runtime should override to application/json for Json response
        GET A "" -> Json<()>;
        // endpoint explicit set should block runtime override
        GET B "" headers { "accept" = "text/plain" } -> Json<()>;
        // endpoint remove should block runtime injection (Accept absent)
        GET C "" headers { -"accept" } -> Json<()>;
    }

    use api_accept::*;

    // A => accept application/json
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiAccept::new_with_transport(transport);
        let _ = api.request(endpoints::A::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).header(ACCEPT, "application/json");

        h.finish();
    }

    // B => accept text/plain
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiAccept::new_with_transport(transport);
        let _ = api.request(endpoints::B::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).header(ACCEPT, "text/plain");

        h.finish();
    }

    // C => no accept
    {
        let (transport, h) = mock()
            .reply(MockReply::ok_json(json_bytes(&())))
            .build();

        let api = ApiAccept::new_with_transport(transport);
        let _ = api.request(endpoints::C::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).header_absent(ACCEPT);

        h.finish();
    }
}
