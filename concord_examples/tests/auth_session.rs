use bytes::Bytes;
use concord_core::prelude::{ApiClientError, ErrorCategory};
use concord_examples::auth_session::{
    SessionApi, SessionApiAcquireAsSessionExt, SessionLoginRequest,
};
use concord_test_support::{MockReply, assert_request, mock};

#[tokio::test]
async fn auth_endpoint_backed_session_flow_acquires_and_uses_credential() {
    let (transport, handle) = mock()
        .reply(json_reply(r#"{"access_token":"session-token"}"#))
        .reply(json_reply(r#"{"id":7,"username":"ada"}"#))
        .build();
    let api = SessionApi::new_with_transport("upstream-secret".to_string(), transport);

    let missing = api
        .protected()
        .me()
        .execute()
        .await
        .expect_err("protected request must require acquisition first");
    assert_eq!(missing.category(), ErrorCategory::MissingCredential);
    assert!(missing.to_string().contains("acquire_auth_session"));
    assert!(!rendered_error(&missing).contains("upstream-secret"));
    handle.assert_recorded_len(0);
    assert!(!api.auth_state().session().is_set().await);

    api.auth_api()
        .login_for_session(SessionLoginRequest {
            username: "ada".to_string(),
            password: "login-password".to_string(),
        })
        .acquire_as_session()
        .await
        .expect("session acquisition succeeds");
    assert!(api.auth_state().session().is_set().await);

    let user = api
        .protected()
        .me()
        .execute()
        .await
        .expect("protected request succeeds after acquire");
    assert_eq!(user.username, "ada");

    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 2);
    assert_request(&recorded[0])
        .host("example.com")
        .path("/login")
        .header("X-Upstream-Key", "upstream-secret")
        .body_present();
    assert_request(&recorded[1])
        .host("example.com")
        .path("/me")
        .header(http::header::AUTHORIZATION, "Bearer session-token")
        .body_absent();
    handle.finish();
}

#[tokio::test]
async fn auth_endpoint_errors_do_not_render_secret_values() {
    let (transport, handle) = mock().build();
    let api =
        SessionApi::new_with_transport("super-secret-upstream\ninvalid".to_string(), transport);

    let err = api
        .auth_api()
        .login_for_session(SessionLoginRequest {
            username: "ada".to_string(),
            password: "raw-password".to_string(),
        })
        .acquire_as_session()
        .await
        .expect_err("invalid upstream secret header fails acquisition");
    let rendered = rendered_error(&err);
    assert!(!rendered.contains("super-secret-upstream"));
    assert!(!rendered.contains("raw-password"));
    assert!(!rendered.contains("session-token"));

    handle.assert_recorded_len(0);
    handle.finish();
}

fn json_reply(body: &'static str) -> MockReply {
    MockReply::ok_json(Bytes::from_static(body.as_bytes()))
}

fn rendered_error(err: &ApiClientError) -> String {
    format!("{err}\n{err:?}")
}
