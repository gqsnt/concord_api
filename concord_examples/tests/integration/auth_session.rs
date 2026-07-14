use bytes::Bytes;
use concord_core::prelude::{ApiClientError, ErrorCategory};
use concord_examples::auth_session::{
    SessionApi, SessionApiAcquireAsSessionExt, SessionLoginRequest,
};
use concord_test_support::{ScriptedReply, assert_execution, deterministic_mock};

#[tokio::test]
async fn auth_endpoint_backed_session_flow_acquires_and_uses_credential() {
    let (transport, handle) = deterministic_mock()
        .reply(
            json_reply(r#"{"access_token":"session-token"}"#).expect_header(
                http::HeaderName::from_static("x-upstream-key"),
                "upstream-secret",
            ),
        )
        .reply(
            json_reply(r#"{"id":7,"username":"ada"}"#)
                .expect_header(http::header::AUTHORIZATION, "Bearer session-token"),
        )
        .build();
    let api = SessionApi::new_with_safe_reqwest_builder("upstream-secret".to_string(), |builder| {
        transport.configure_both(builder)
    })
    .expect("mock client");

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
    assert!(
        !api.auth_state()
            .session()
            .is_set()
            .await
            .expect("session state check succeeds")
    );

    api.auth_api()
        .login_for_session(SessionLoginRequest {
            username: "ada".to_string(),
            password: "login-password".to_string(),
        })
        .acquire_as_session()
        .await
        .expect("session acquisition succeeds");
    assert!(
        api.auth_state()
            .session()
            .is_set()
            .await
            .expect("session state check succeeds")
    );

    let user = api
        .protected()
        .me()
        .execute()
        .await
        .expect("protected request succeeds after acquire");
    assert_eq!(user.username, "ada");

    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 2);
    assert_execution(&recorded[0])
        .host("example.com")
        .path("/login")
        .protected_header("X-Upstream-Key")
        .body_present();
    assert_execution(&recorded[1])
        .host("example.com")
        .path("/me")
        .protected_header(http::header::AUTHORIZATION)
        .body_absent();
    handle.finish();
}

#[tokio::test]
async fn auth_endpoint_errors_do_not_render_secret_values() {
    let (transport, handle) = deterministic_mock().build();
    let api = SessionApi::new_with_safe_reqwest_builder(
        "super-secret-upstream\ninvalid".to_string(),
        |builder| transport.configure_both(builder),
    )
    .expect("mock client");

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

#[tokio::test]
async fn endpoint_backed_session_401_does_not_refresh_without_challenge_recovery() {
    let (transport, handle) = deterministic_mock()
        .reply(
            json_reply(r#"{"access_token":"session-token"}"#).expect_header(
                http::HeaderName::from_static("x-upstream-key"),
                "upstream-secret",
            ),
        )
        .reply(
            ScriptedReply::status(http::StatusCode::UNAUTHORIZED)
                .expect_header(http::header::AUTHORIZATION, "Bearer session-token")
                .with_body(Bytes::from_static(b"expired")),
        )
        .build();
    let api = SessionApi::new_with_safe_reqwest_builder("upstream-secret".to_string(), |builder| {
        transport.configure_both(builder)
    })
    .expect("mock client");

    api.auth_api()
        .login_for_session(SessionLoginRequest {
            username: "ada".to_string(),
            password: "login-password".to_string(),
        })
        .acquire_as_session()
        .await
        .expect("session acquisition succeeds");

    let err = api
        .protected()
        .me()
        .execute()
        .await
        .expect_err("401 should remain the protected response outcome");

    assert_eq!(err.category(), ErrorCategory::AuthRejected);
    assert!(matches!(err, ApiClientError::Auth { .. }));
    assert!(!err.to_string().contains("missing credential"));
    assert!(
        !api.auth_state()
            .session()
            .is_set()
            .await
            .expect("session state check succeeds")
    );

    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 2);
    assert_execution(&recorded[0])
        .host("example.com")
        .path("/login");
    assert_execution(&recorded[1])
        .host("example.com")
        .path("/me")
        .protected_header(http::header::AUTHORIZATION);
    let missing = api
        .protected()
        .me()
        .execute()
        .await
        .expect_err("rejected endpoint-backed session should require explicit reacquire");
    assert_eq!(missing.category(), ErrorCategory::MissingCredential);
    assert!(missing.to_string().contains("acquire_auth_session"));
    assert!(!rendered_error(&missing).contains("session-token"));
    handle.assert_recorded_len(2);
    handle.finish();
}

#[tokio::test]
async fn endpoint_backed_session_403_is_terminal_without_recovery() {
    let (transport, handle) = deterministic_mock()
        .reply(
            json_reply(r#"{"access_token":"session-token"}"#).expect_header(
                http::HeaderName::from_static("x-upstream-key"),
                "upstream-secret",
            ),
        )
        .reply(
            ScriptedReply::status(http::StatusCode::FORBIDDEN)
                .expect_header(http::header::AUTHORIZATION, "Bearer session-token")
                .with_body(Bytes::from_static(b"denied")),
        )
        .build();
    let api = SessionApi::new_with_safe_reqwest_builder("upstream-secret".to_string(), |builder| {
        transport.configure_both(builder)
    })
    .expect("mock client");

    api.auth_api()
        .login_for_session(SessionLoginRequest {
            username: "ada".to_string(),
            password: "login-password".to_string(),
        })
        .acquire_as_session()
        .await
        .expect("session acquisition succeeds");

    let err = api
        .protected()
        .me()
        .execute()
        .await
        .expect_err("403 should remain the protected response outcome");

    assert_eq!(err.category(), ErrorCategory::AuthRejected);
    assert!(matches!(err, ApiClientError::Auth { .. }));
    assert!(!err.to_string().contains("missing credential"));
    assert!(
        api.auth_state()
            .session()
            .is_set()
            .await
            .expect("session state check succeeds")
    );

    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 2);
    assert_execution(&recorded[0])
        .host("example.com")
        .path("/login");
    assert_execution(&recorded[1])
        .host("example.com")
        .path("/me")
        .protected_header(http::header::AUTHORIZATION);
    handle.assert_recorded_len(2);
    handle.finish();
}

#[tokio::test]
async fn rotating_static_secret_preserves_endpoint_backed_session() {
    let (transport, handle) = deterministic_mock()
        .reply(
            json_reply(r#"{"access_token":"session-token"}"#).expect_header(
                http::HeaderName::from_static("x-upstream-key"),
                "upstream-secret",
            ),
        )
        .reply(
            json_reply(r#"{"id":7,"username":"ada"}"#)
                .expect_header(http::header::AUTHORIZATION, "Bearer session-token"),
        )
        .build();
    let mut api =
        SessionApi::new_with_safe_reqwest_builder("upstream-secret".to_string(), |builder| {
            transport.configure_both(builder)
        })
        .expect("mock client");

    api.auth_api()
        .login_for_session(SessionLoginRequest {
            username: "ada".to_string(),
            password: "login-password".to_string(),
        })
        .acquire_as_session()
        .await
        .expect("session acquisition succeeds");
    assert!(
        api.auth_state()
            .session()
            .is_set()
            .await
            .expect("session state check succeeds")
    );

    api.set_upstream_key("rotated-upstream-secret")
        .expect("static secret rotation should rebuild providers");
    assert!(
        api.auth_state()
            .session()
            .is_set()
            .await
            .expect("session state check succeeds after static secret rotation")
    );

    let user = api
        .protected()
        .me()
        .execute()
        .await
        .expect("protected request should use preserved session");
    assert_eq!(user.username, "ada");

    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 2);
    assert_execution(&recorded[0])
        .host("example.com")
        .path("/login")
        .protected_header("X-Upstream-Key");
    assert_execution(&recorded[1])
        .host("example.com")
        .path("/me")
        .protected_header(http::header::AUTHORIZATION);
    handle.finish();
}

fn json_reply(body: &'static str) -> ScriptedReply {
    ScriptedReply::ok_json(Bytes::from_static(body.as_bytes()))
}

fn rendered_error(err: &ApiClientError) -> String {
    format!("{err}\n{err:?}")
}
