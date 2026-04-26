use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;
use http::header::AUTHORIZATION;

#[tokio::test]
async fn vars_default_and_setter_affect_emitted_header() {
    api! {
        client ApiVarsDefault {
            base https "example.com"
            var user_agent: String = "ua1".to_string()
            headers {
                "x-ua" = vars.user_agent
            }
        }

        GET Ping
        -> Json<()>
        {
        }
    }

    use api_vars_default::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let mut api = ApiVarsDefault::new_with_transport(transport);
    api.request(endpoints::Ping::new()).execute().await.unwrap();
    api.set_user_agent("ua2".to_string());
    api.request(endpoints::Ping::new()).execute().await.unwrap();

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
            base https "example.com"
            var tenant: String
            headers {
                "x-tenant" = vars.tenant
            }
        }

        GET Ping
        -> Json<()>
        {
        }
    }

    use api_vars_req::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let mut api = ApiVarsReq::new_with_transport("t1".to_string(), transport);
    api.request(endpoints::Ping::new()).execute().await.unwrap();
    api.set_tenant("t2".to_string());
    api.request(endpoints::Ping::new()).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);

    assert_request(&reqs[0]).header("x-tenant", "t1");
    assert_request(&reqs[1]).header("x-tenant", "t2");

    h.finish();
}

#[tokio::test]
async fn secret_required_and_setter_affect_emitted_header() {
    api! {
        client ApiSecret {
            base https "example.com"
            secret token: String
            var token2: String = "default".to_string()
            headers {
                "authorization" = secret.token
            }
        }

        scope token2_scope {
            host [vars.token2]

            GET Ping
            -> Json<()>
            {
            }
        }
    }

    use api_secret::*;

    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(json_bytes(&())),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();

    let api = ApiSecret::new_with_transport("tok1".to_string(), transport);
    api.request(endpoints::token2_scope::Ping::new())
        .execute()
        .await
        .unwrap();
    api.set_token("tok2");
    api.request(endpoints::token2_scope::Ping::new())
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);

    assert_request(&reqs[0]).header(AUTHORIZATION, "tok1");
    assert_request(&reqs[1]).header(AUTHORIZATION, "tok2");

    h.finish();
}

#[tokio::test]
async fn secret_invalid_header_value_reported_as_invalid_param_and_request_not_sent() {
    api! {
        client ApiSecretBad {
            base https "example.com"
            secret token: String
            headers {
                "authorization" = secret.token
            }
        }

        GET Ping
        -> Json<()>
        {
        }
    }

    use api_secret_bad::*;

    let (transport, h) = mock().build();

    let api = ApiSecretBad::new_with_transport("a\nb".to_string(), transport);
    let err = api
        .request(endpoints::Ping::new())
        .execute()
        .await
        .unwrap_err();

    h.assert_recorded_len(0);

    match err {
        ApiClientError::InvalidParam { param, .. } => {
            assert!(param.contains("header"));
            assert!(param.contains("authorization"));
        }
        other => panic!("unexpected error: {other:?}"),
    }

    drop(h);
}
