use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::header::{ACCEPT, USER_AGENT};

#[tokio::test]
async fn query_set_remove_push_override_and_tostring() {
    api! {
        client ApiQuery {
            scheme: https,
            host: "example.com",
            query {
                "sdk" = "concord",
                "dup" += "c0"
            }
        }

        scope x_scope {
            path["x"]
            query {
                -"sdk",
                "dup" += "p1",
                "n" = 12u64,
                "b" = false
            }

            GET One
            -> Json<()>
            {
                query { "dup" = "e1" }
            }
        }
    }

    use api_query::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

    let api = ApiQuery::new_with_transport(transport);
    let _ = api
        .request(endpoints::x_scope::One::new())
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    let r = &reqs[0];

    assert_request(r)
        .query_absent("sdk")
        .query_values("dup", &["e1"])
        .query_has("n", "12")
        .query_has("b", "false");

    h.finish();
}

#[tokio::test(flavor = "current_thread")]
async fn query_part_set_required_param() {
    api! {
        client ApiQueryFmtReq {
            scheme: https,
            host: "example.com",
        }

        GET One(v: String)
        -> Json<()>
        {
            path["x"]
            query { "q" = part["a:", v] }
        }
    }

    use api_query_fmt_req::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

    let api = ApiQueryFmtReq::new_with_transport(transport);
    let _ = api
        .request(endpoints::One::new("z".to_string()))
        .execute()
        .await
        .unwrap();

    h.assert_recorded_len(1);
    let reqs = h.recorded();
    assert_request(&reqs[0])
        .query_has("q", "a:z")
        .query_absent("v");

    h.finish();
}

#[tokio::test(flavor = "current_thread")]
async fn query_part_optional_removes_key_when_missing() {
    api! {
        client ApiQueryFmtOpt {
            scheme: https,
            host: "example.com",
        }

        GET One(v?: String)
        -> Json<()>
        {
            path["x"]
            query { "q" = part["a:", v] }
        }
    }

    use api_query_fmt_opt::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let api = ApiQueryFmtOpt::new_with_transport(transport);

    let _ = api.request(endpoints::One::new()).execute().await.unwrap();
    let _ = api
        .request(endpoints::One::new().v("z".to_string()))
        .execute()
        .await
        .unwrap();

    h.assert_recorded_len(2);
    let reqs = h.recorded();

    assert_request(&reqs[0]).query_absent("q");
    assert_request(&reqs[1]).query_has("q", "a:z");

    h.finish();
}

#[tokio::test(flavor = "current_thread")]
async fn query_part_push_appends_duplicate_keys_in_order() {
    api! {
        client ApiQueryFmtPush {
            scheme: https,
            host: "example.com",
        }

        GET One(v: String)
        -> Json<()>
        {
            path["x"]
            query {
                "dup" += part["p:", v],
                "dup" += "s"
            }
        }
    }

    use api_query_fmt_push::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

    let api = ApiQueryFmtPush::new_with_transport(transport);
    let _ = api
        .request(endpoints::One::new("z".to_string()))
        .execute()
        .await
        .unwrap();

    h.assert_recorded_len(1);
    let reqs = h.recorded();

    assert_request(&reqs[0])
        .query_values("dup", &["p:z", "s"])
        .query_absent("v");

    h.finish();
}

#[tokio::test]
async fn headers_kebab_string_bind_remove_override() {
    api! {
        client ApiHeaders {
            scheme: https,
            host: "example.com",
            vars {
                user_agent: String = "ua".to_string(),
                flag: bool = true
            }
            headers {
                user_agent = vars.user_agent,
                x_debug = "caribou",
                "x-static" = "s",
                "x-flag" = vars.flag
            }
        }

        scope p_scope {
            path["p"]
            headers {
                "x-debug" = "override",
                -"x-static"
            }

            GET One
            -> Json<()>
            {
                headers { -"x-flag" }
            }
        }
    }

    use api_headers::*;

    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

    let api = ApiHeaders::new_with_transport(transport);
    let _ = api
        .request(endpoints::p_scope::One::new())
        .execute()
        .await
        .unwrap();

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
async fn header_value_from_vars_to_string_and_invalid_header_value_error() {
    api! {
        client ApiHeaderInvalid {
            scheme: https,
            host: "example.com",
            vars {
                bad: String,
                trace: bool = false
            }
            headers {
                "x-bad" = vars.bad,
                "x-bool" = vars.trace
            }
        }

        GET One
        -> Json<()>
        {
        }
    }

    use api_header_invalid::*;

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiHeaderInvalid::new_with_transport("ok".to_string(), transport);
        let _ = api.request(endpoints::One::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).header("x-bool", "false");

        h.finish();
    }

    {
        let (transport, h) = mock().build();

        let api = ApiHeaderInvalid::new_with_transport("a\nb".to_string(), transport);
        let err = api
            .request(endpoints::One::new())
            .execute()
            .await
            .unwrap_err();

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
            headers { "accept" = "text/plain" }
        }

        GET A
        -> Json<()>
        {
        }

        GET B
        -> Json<()>
        {
            headers { "accept" = "text/plain" }
        }

        GET C
        -> Json<()>
        {
            headers { -"accept" }
        }
    }

    use api_accept::*;

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiAccept::new_with_transport(transport);
        let _ = api.request(endpoints::A::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).header(ACCEPT, "application/json");

        h.finish();
    }

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiAccept::new_with_transport(transport);
        let _ = api.request(endpoints::B::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).header(ACCEPT, "text/plain");

        h.finish();
    }

    {
        let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();

        let api = ApiAccept::new_with_transport(transport);
        let _ = api.request(endpoints::C::new()).execute().await.unwrap();

        let reqs = h.recorded();
        assert_request(&reqs[0]).header_absent(ACCEPT);

        h.finish();
    }
}
