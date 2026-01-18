mod common;

use common::*;
use concord_core::prelude::*;
use concord_macros::api;
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

    let (transport, recorded) = MockTransport::new(vec![
        MockReply::ok_json(json_bytes(&())),
        MockReply::ok_json(json_bytes(&())),
    ]);

    let mut api = ApiVarsDefault::new_with_transport(transport);

    let _ = api.request(endpoints::Ping::new()).await.unwrap();
    api.set_user_agent("ua2".to_string());
    let _ = api.request(endpoints::Ping::new()).await.unwrap();

    let reqs = recorded.lock().unwrap();
    assert_eq!(reqs.len(), 2);

    assert_eq!(
        reqs[0].headers.get("x-ua").unwrap().to_str().unwrap(),
        "ua1"
    );
    assert_eq!(
        reqs[1].headers.get("x-ua").unwrap().to_str().unwrap(),
        "ua2"
    );
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

    let (transport, recorded) = MockTransport::new(vec![
        MockReply::ok_json(json_bytes(&())),
        MockReply::ok_json(json_bytes(&())),
    ]);

    let mut api = ApiVarsReq::new_with_transport("t1".to_string(), transport);

    let _ = api.request(endpoints::Ping::new()).await.unwrap();
    api.set_tenant("t2".to_string());
    let _ = api.request(endpoints::Ping::new()).await.unwrap();

    let reqs = recorded.lock().unwrap();
    assert_eq!(reqs.len(), 2);

    assert_eq!(
        reqs[0].headers.get("x-tenant").unwrap().to_str().unwrap(),
        "t1"
    );
    assert_eq!(
        reqs[1].headers.get("x-tenant").unwrap().to_str().unwrap(),
        "t2"
    );
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

    let (transport, recorded) = MockTransport::new(vec![
        MockReply::ok_json(json_bytes(&())),
        MockReply::ok_json(json_bytes(&())),
    ]);

    // Pas de vars, 1 auth_var requise => new_with_transport(token, transport)
    let api = ApiAuthVars::new_with_transport("tok1".to_string(), transport);

    let _ = api.request(endpoints::Ping::new()).await.unwrap();
    api.set_token("tok2");
    let _ = api.request(endpoints::Ping::new()).await.unwrap();

    let reqs = recorded.lock().unwrap();
    assert_eq!(reqs.len(), 2);

    assert_eq!(
        reqs[0]
            .headers
            .get(AUTHORIZATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "tok1"
    );
    assert_eq!(
        reqs[1]
            .headers
            .get(AUTHORIZATION)
            .unwrap()
            .to_str()
            .unwrap(),
        "tok2"
    );
}

#[tokio::test]
async fn auth_vars_invalid_header_value_is_reported_as_invalid_param() {
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

    // Même si la requête ne doit pas partir, on met une reply pour éviter un panic
    // si jamais le build passe (le test échouerait via assert).
    let (transport, recorded) = MockTransport::new(vec![MockReply::ok_json(json_bytes(&()))]);

    let api = ApiAuthBad::new_with_transport("a\nb".to_string(), transport);
    let err = api.request(endpoints::Ping::new()).await.unwrap_err();

    // idéalement : erreur produite avant envoi
    assert_eq!(recorded.lock().unwrap().len(), 0);

    match err {
        ApiClientError::InEndpoint { source, .. } => match *source {
            ApiClientError::InvalidParam(s) => {
                assert!(s.contains("header"));
                assert!(s.contains("authorization"));
            }
            other => panic!("unexpected inner error: {other:?}"),
        },
        other => panic!("unexpected error: {other:?}"),
    }
}
