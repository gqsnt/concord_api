use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::header::AUTHORIZATION;

#[tokio::test]
async fn vars_default_and_setter_affect_emitted_header() {
    api! {
        client ApiVarsDefault {
            scheme: https,
            host: "example.com",
            headers {
                "x-ua" as user_agent: String = "ua1".to_string()
            }
        }
        GET Ping "" -> Json<()>;
    }

    use api_vars_default::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let mut api = ApiVarsDefault::new_with_transport(transport);
    let _ = api.request(endpoints::Ping::new()).execute().await.unwrap();
    api.set_user_agent("ua2".to_string());
    let _ = api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);

    assert_request(&reqs[0]).header("x-ua", "ua1");
    assert_request(&reqs[1]).header("x-ua", "ua2");

    h.finish();
}

#[tokio::test]
async fn vars_required_ctor_arg_and_setter_affect_emitted_header() {
    api! {
        client ApiVarsReq {
            scheme: https,
            host: "example.com",
            headers {
                "x-tenant" as tenant: String
            }
        }
        GET Ping "" -> Json<()>;
    }

    use api_vars_req::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let mut api = ApiVarsReq::new_with_transport("t1".to_string(), transport);
    let _ = api.request(endpoints::Ping::new()).execute().await.unwrap();
    api.set_tenant("t2".to_string());
    let _ = api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);

    assert_request(&reqs[0]).header("x-tenant", "t1");
    assert_request(&reqs[1]).header("x-tenant", "t2");

    h.finish();
}

#[tokio::test]
async fn auth_vars_required_secret_and_setter_affect_emitted_header() {
    api! {
        client ApiAuthVars {
            scheme: https,
            host: "example.com",
            auth_vars {
                token: String
            }
            vars {
                token2: String = "default".to_string()
            }
            headers {
                "authorization" = auth.token
            }
        }

        prefix {cx.token2}{
            GET Ping "" -> Json<()>;
        }
    }

    use api_auth_vars::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let api = ApiAuthVars::new_with_transport("tok1".to_string(), transport);
    let _ = api.request(endpoints::Ping::new()).execute().await.unwrap();
    api.set_token("tok2");
    let _ = api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);

    assert_request(&reqs[0]).header(AUTHORIZATION, "tok1");
    assert_request(&reqs[1]).header(AUTHORIZATION, "tok2");

    h.finish();
}

#[tokio::test]
async fn auth_vars_invalid_header_value_reported_as_invalid_param_and_request_not_sent() {
    api! {
        client ApiAuthBad {
            scheme: https,
            host: "example.com",
            auth_vars {
                token: String
            }
            headers {
                "authorization" = auth.token
            }
        }
        GET Ping "" -> Json<()>;
    }

    use api_auth_bad::*;

    let (transport, h) = mock()

        .build();

    let api = ApiAuthBad::new_with_transport("a\nb".to_string(), transport);
    let err = api.request(endpoints::Ping::new()).execute().await.unwrap_err();

    h.assert_recorded_len(0);

    match err {
        ApiClientError::InvalidParam { param, .. } => {
            assert!(param.contains("header"));
            assert!(param.contains("authorization"));
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // reply remains unused because request never sent: this is intentional, so do not finish().
    // Consume the handle without triggering drop-panic:
    drop(h);
}
