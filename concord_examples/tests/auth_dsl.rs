use concord_core::prelude::*;
use concord_examples::auth_session::{SessionLoginRequest, SessionLoginResponse, SessionUser};
use concord_macros::api;
use concord_test_support::*;
use http::header::AUTHORIZATION;

#[test]
fn auth_session_request_type_is_public() {
    let req = SessionLoginRequest {
        username: "u".to_string(),
        password: "p".to_string(),
    };
    assert_eq!(req.username, "u");
}

api! {
    client AuthPlacementApi {
        base https "example.com"
        secret api_key: String
        secret username: String
        secret password: String

        credential key = api_key(secret.api_key)
        credential basic_auth = basic(secret.username, secret.password)
    }

    scope protected {
        path ["me"]
        auth header "X-Api-Key" = key
        auth query "api_key" = key
        auth basic basic_auth

        GET Me -> Json<()>;
    }
}

#[tokio::test]
async fn auth_plan_applies_header_query_basic_and_multiple_requirements() {
    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let api = auth_placement_api::AuthPlacementApi::new_with_transport(
        "secret-key".to_string(),
        "pass".to_string(),
        "user".to_string(),
        transport,
    );

    api.protected().me().await.unwrap();

    h.assert_recorded_len(1);
    let req = &h.recorded()[0];
    assert_eq!(req.headers.get("x-api-key").unwrap(), "secret-key");
    assert_eq!(
        req.url
            .query_pairs()
            .find(|(k, _)| k == "api_key")
            .unwrap()
            .1,
        "secret-key"
    );
    assert_eq!(
        req.headers.get(AUTHORIZATION).unwrap(),
        "Basic dXNlcjpwYXNz"
    );
}

#[tokio::test]
async fn missing_manual_session_credential_errors_before_transport() {
    let (transport, h) = mock().build();
    let api = concord_examples::auth_session::SessionApi::new_with_transport(
        "upstream".to_string(),
        transport,
    );

    let err = api.protected().me().await.unwrap_err();
    assert!(
        err.to_string().contains("missing credential `session`"),
        "{err}"
    );
    h.assert_recorded_len(0);
}

#[tokio::test]
async fn endpoint_acquired_session_auth_flow_uses_stored_bearer() {
    let login = SessionLoginResponse {
        access_token: "session-token".to_string(),
    };
    let me = SessionUser {
        id: 7,
        username: "alice".to_string(),
    };
    let (transport, h) = mock()
        .reply(MockReply::ok_json(json_bytes(&login)))
        .reply(MockReply::ok_json(json_bytes(&me)))
        .build();
    let api = concord_examples::auth_session::SessionApi::new_with_transport(
        "upstream".to_string(),
        transport,
    );

    api.auth_state()
        .session()
        .acquire(api.auth_api().login_for_session(SessionLoginRequest {
            username: "alice".to_string(),
            password: "secret".to_string(),
        }))
        .await
        .unwrap();
    let out = api.protected().me().await.unwrap();

    assert_eq!(out.username, "alice");
    h.assert_recorded_len(2);
    let reqs = h.recorded();
    assert_eq!(reqs[0].headers.get("x-upstream-key").unwrap(), "upstream");
    assert_eq!(
        reqs[1].headers.get(AUTHORIZATION).unwrap(),
        "Bearer session-token"
    );
}
