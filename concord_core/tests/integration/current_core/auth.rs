use super::common::*;
use crate::support::assert_error_chain_does_not_contain_any;
use base64::Engine as _;
use bytes::Bytes;
#[cfg(feature = "json")]
use concord_core::advanced::OAuth2ClientCredentialsProvider;
use concord_core::advanced::{
    AuthApplicationRequest, AuthAppliedCredential, AuthChallengePolicy, AuthDecision, AuthError,
    AuthErrorKind, AuthHttpRequest, AuthInternalPolicy, AuthMode, AuthPlacement, AuthRequirement,
    AuthStepPolicy, BufferedResponse, CodecError, DecodeContext, PreparedAuthCredential,
    RequestMeta, ResponseCodec, RetryDecision, TextContentType, auth_decision_for_status,
};
use concord_core::advanced::{
    CredentialContext, CredentialId, CredentialProvider, CredentialRefreshReason, CredentialSlot,
    InvalidateReason,
};
use concord_core::error::ErrorCategory;
use concord_core::internal::{ClientPlanContext, RequestPlan, ResponseEntity};
use concord_core::prelude::{
    AccessToken, ApiClientError, ApiKey, ClientContext, Endpoint, ReusableEndpoint,
};
use concord_core::transport::TransportErrorKind;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;

const AUTH_TRANSPORT_SENTINEL: &str = "PR17_AUTH_TRANSPORT_SENTINEL";
const AUTH_DECODE_SENTINEL: &str = "PR17_AUTH_DECODE_SENTINEL";
const AUTH_RETRY_SENTINEL: &str = "PR17_AUTH_RETRY_SENTINEL";
const AUTH_RESPONSE_SENTINEL: &str = "PR17_AUTH_RESPONSE_SENTINEL";

#[tokio::test]
async fn missing_credential_error_is_actionable() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let client = client(TestAuthVars::default(), transport);
    let endpoint = TextEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("missing token must fail before transport");

    assert_eq!(err.category(), ErrorCategory::MissingCredential);
    assert_eq!(err.context().endpoint, "Text");
    assert_eq!(err.context().method, Method::GET);
    let msg = err.to_string();
    assert!(msg.contains("missing credential"));
    assert!(msg.contains("test.token"));
    assert!(msg.contains("acquire or configure"));
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct SentinelError(&'static str);

impl fmt::Debug for SentinelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self.0;
        f.write_str("<redacted>")
    }
}

impl fmt::Display for SentinelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self.0;
        f.write_str("<redacted>")
    }
}

impl std::error::Error for SentinelError {}

#[derive(Clone, Copy, Debug, Default)]
struct FailingAuthDecodeCodec;

impl ResponseCodec for FailingAuthDecodeCodec {
    type Value = String;
    type Content = TextContentType;

    fn decode(_bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        Err(CodecError::with_source(
            "auth response decode failed",
            SentinelError(AUTH_DECODE_SENTINEL),
        ))
    }
}

#[tokio::test]
async fn auth_rejection_is_handled_before_normal_retry() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "unauthorized"),
            MockResponse::text(StatusCode::OK, "should-not-retry"),
        ],
    );
    let sent_transport = transport.clone();
    let mut client = concord_core::prelude::ApiClient::<RecordingAuthCx, _>::with_transport(
        (),
        RecordingAuthVars {
            token: Some("bad".to_string()),
            identity: "user-a",
            events: events.clone(),
        },
        transport,
    );
    client.set_runtime_hooks(Arc::new(RecordingRuntimeHooks::new(events.clone())));
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.retry = retry_policy_for_statuses(2, vec![StatusCode::UNAUTHORIZED]).retry;
            policy
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("auth rejection should not fall through to retry");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(sent_transport.sent_count().await, 1);
    let events = events.lock().await.clone();
    let transport = events
        .iter()
        .position(|event| event == "transport")
        .expect("transport sent");
    let classify = events
        .iter()
        .position(|event| event == "classify_response")
        .expect("response classified");
    let rejection = events
        .iter()
        .position(|event| event == "auth_rejection")
        .expect("auth rejection recorded");
    assert!(transport < classify);
    assert!(classify < rejection);
}

#[tokio::test]
async fn auth_rejection_errors_are_raw_secret_free() {
    use std::error::Error as _;

    fn rendered_chain(err: &ApiClientError) -> String {
        let mut rendered = format!("{err}\n{err:?}");
        let mut current = err.source();
        while let Some(source) = current {
            rendered.push('\n');
            rendered.push_str(&source.to_string());
            current = source.source();
        }
        rendered
    }

    let bearer_sentinel = "RAW_BEARER_SENTINEL";
    let query_sentinel = "RAW_QUERY_AUTH_SENTINEL";
    let header_sentinel = "RAW_HEADER_AUTH_SENTINEL";
    let basic_user_sentinel = "RAW_BASIC_USERNAME_SENTINEL";
    let basic_pass_sentinel = "RAW_BASIC_PASSWORD_SENTINEL";
    let response_body_sentinel = "RAW_AUTH_REJECTION_RESPONSE_BODY_SENTINEL";

    for placement in [
        AuthPlacement::Bearer,
        AuthPlacement::Query("api_key"),
        AuthPlacement::Header("X-Api-Key"),
    ] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let transport = MockTransport::new(
            events.clone(),
            vec![MockResponse::text(
                StatusCode::UNAUTHORIZED,
                response_body_sentinel,
            )],
        );
        let mut client = client(
            TestAuthVars {
                token: Some(match placement {
                    AuthPlacement::Bearer => bearer_sentinel.to_string(),
                    AuthPlacement::Query(_) => query_sentinel.to_string(),
                    AuthPlacement::Header(_) => header_sentinel.to_string(),
                    AuthPlacement::Basic | AuthPlacement::Certificate => unreachable!(),
                }),
                identity: "user-a",
            },
            transport,
        );
        client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
        configure_runtime(
            &mut client,
            Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
        );
        client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
            events.clone(),
            RetryDecision::Retry,
            8,
        )));

        let err = client
            .request(TextEndpoint {
                policy: auth_policy(placement),
                ..Default::default()
            })
            .execute_decoded_with::<concord_core::prelude::Text<String>>()
            .await
            .expect_err("auth rejection should fail");

        assert_eq!(err.context().endpoint, "Text");
        assert_eq!(err.context().method, Method::GET);
        let rendered = rendered_chain(&err);
        for sentinel in [
            bearer_sentinel,
            query_sentinel,
            header_sentinel,
            response_body_sentinel,
        ] {
            assert!(!rendered.contains(sentinel));
        }
        let event_rendered = events.lock().await.clone().join("\n");
        for sentinel in [
            bearer_sentinel,
            query_sentinel,
            header_sentinel,
            response_body_sentinel,
        ] {
            assert!(!event_rendered.contains(sentinel));
        }
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(
            StatusCode::FORBIDDEN,
            response_body_sentinel,
        )],
    );
    let mut client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::basic(
            basic_user_sentinel,
            basic_pass_sentinel,
            "user-a",
            events.clone(),
        ),
        transport,
    );
    client.set_runtime_hooks(Arc::new(ObservationRuntimeHooks::new(events.clone())));
    configure_runtime(
        &mut client,
        Some(Arc::new(ObservationRateLimiter::new(events.clone()))),
    );
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        events.clone(),
        RetryDecision::Retry,
        8,
    )));

    let err = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Basic),
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("basic auth rejection should fail");

    assert_eq!(err.context().endpoint, "Text");
    assert_eq!(err.context().method, Method::GET);
    let rendered = rendered_chain(&err);
    assert!(!rendered.contains(basic_user_sentinel));
    assert!(!rendered.contains(basic_pass_sentinel));
    assert!(!rendered.contains(response_body_sentinel));
    let event_rendered = events.lock().await.clone().join("\n");
    assert!(!event_rendered.contains(basic_user_sentinel));
    assert!(!event_rendered.contains(basic_pass_sentinel));
    assert!(!event_rendered.contains(response_body_sentinel));
}

#[tokio::test]
async fn unauthorized_can_trigger_bounded_auth_refresh_before_retry() -> Result<(), ApiClientError>
{
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
            MockResponse::text(StatusCode::OK, "refreshed"),
        ],
    );
    let sent_transport = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some("refreshable".to_string()),
            identity: "refresh",
        },
        transport,
    );
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.retry = retry_policy_for_statuses(2, vec![StatusCode::UNAUTHORIZED]).retry;
            policy
        },
        ..Default::default()
    };

    let decoded = client
        .request(endpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;

    assert_eq!(decoded.value(), "refreshed");
    assert_eq!(sent_transport.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn auth_refresh_failure_is_terminal_auth_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ],
    );
    let sent_transport = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some("refreshable".to_string()),
            identity: "refresh-error",
        },
        transport,
    );
    let endpoint = TextEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("auth refresh failure is terminal");

    assert!(err.to_string().contains("auth refresh failed"));
    assert_eq!(sent_transport.sent_count().await, 1);
}

#[tokio::test]
async fn bearer_header_and_query_auth_are_applied() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "bearer"),
            MockResponse::text(StatusCode::OK, "query"),
        ],
    );
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some("token-1".to_string()),
            identity: "user-a",
        },
        transport,
    );

    client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;
    client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Query("api_key")),
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;

    let requests = sent.requests().await;
    assert_eq!(
        requests[0]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
        Some("Bearer token-1")
    );
    assert_eq!(
        requests[1]
            .url
            .query_pairs()
            .find(|(key, _)| key == "api_key")
            .map(|(_, value)| value.into_owned()),
        Some("token-1".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn header_auth_is_applied_to_the_configured_header() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "header-auth-ok")],
    );
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some(AUTH_TRANSPORT_SENTINEL.to_string()),
            identity: "user-a",
        },
        transport,
    );

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Header("X-Api-Key")),
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;

    assert_eq!(decoded.value(), "header-auth-ok");
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .headers
            .get(HeaderName::from_static("x-api-key"))
            .and_then(|value| value.to_str().ok()),
        Some(AUTH_TRANSPORT_SENTINEL)
    );
    Ok(())
}

#[tokio::test]
async fn basic_auth_is_applied_as_an_authorization_header() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "basic-auth-ok")],
    );
    let sent = transport.clone();
    let client = concord_core::prelude::ApiClient::<ObservationAuthCx, _>::with_transport(
        (),
        ObservationAuthVars::basic(
            "PR17_BASIC_USERNAME_SENTINEL",
            "PR17_BASIC_PASSWORD_SENTINEL",
            "basic",
            events.clone(),
        ),
        transport,
    );

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Basic),
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;

    assert_eq!(decoded.value(), "basic-auth-ok");
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    let expected = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD
            .encode("PR17_BASIC_USERNAME_SENTINEL:PR17_BASIC_PASSWORD_SENTINEL")
    );
    assert_eq!(
        requests[0]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some(expected.as_str())
    );
    Ok(())
}

#[tokio::test]
async fn auth_transport_failure_redacts_the_request_auth_sentinel() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::with_outcomes(
        events,
        vec![MockOutcome::TransportError(TransportErrorKind::Connect)],
    );
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some(AUTH_TRANSPORT_SENTINEL.to_string()),
            identity: "user-a",
        },
        transport,
    );

    let err = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Header("X-Api-Key")),
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("transport failure should surface");

    assert!(matches!(err, ApiClientError::Transport { .. }));
    assert_eq!(err.category(), ErrorCategory::Transport);
    assert_eq!(err.context().endpoint, "Text");
    assert_eq!(err.context().method, Method::GET);
    assert_error_chain_does_not_contain_any(&err, &[AUTH_TRANSPORT_SENTINEL]);
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .headers
            .get(HeaderName::from_static("x-api-key"))
            .and_then(|value| value.to_str().ok()),
        Some(AUTH_TRANSPORT_SENTINEL)
    );
}

#[tokio::test]
async fn auth_decode_failure_redacts_request_and_decode_sentinels() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(events, vec![MockResponse::text(StatusCode::OK, "unused")]);
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some(AUTH_TRANSPORT_SENTINEL.to_string()),
            identity: "user-a",
        },
        transport,
    );
    let plan = request_plan(
        "AuthDecode",
        Method::GET,
        "/auth/decode",
        auth_policy(AuthPlacement::Bearer),
        None,
    );

    let err = BufferedResponse::<FailingAuthDecodeCodec>::execute(&client, plan)
        .await
        .expect_err("decode failure should surface");

    assert!(matches!(err, ApiClientError::Decode { .. }));
    assert_eq!(err.category(), ErrorCategory::Decode);
    assert_eq!(err.context().endpoint, "AuthDecode");
    assert_eq!(err.context().method, Method::GET);
    assert_error_chain_does_not_contain_any(&err, &[AUTH_TRANSPORT_SENTINEL, AUTH_DECODE_SENTINEL]);
    assert_eq!(sent.sent_count().await, 1);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 1);
    let expected = format!("Bearer {AUTH_TRANSPORT_SENTINEL}");
    assert_eq!(
        requests[0]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some(expected.as_str())
    );
}

#[tokio::test]
async fn auth_retry_reuses_bearer_material_across_attempts() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-one"),
            MockResponse::text(StatusCode::OK, "retry-ok"),
        ],
    );
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some(AUTH_RETRY_SENTINEL.to_string()),
            identity: "user-a",
        },
        transport,
    );
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.retry =
                retry_policy_for_statuses(2, vec![StatusCode::INTERNAL_SERVER_ERROR]).retry;
            policy
        },
        ..Default::default()
    };

    let decoded = client
        .request(endpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;

    assert_eq!(decoded.value(), "retry-ok");
    assert_eq!(sent.sent_count().await, 2);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    let expected = format!("Bearer {AUTH_RETRY_SENTINEL}");
    for request in &requests {
        assert_eq!(
            request
                .headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some(expected.as_str())
        );
    }
    assert_eq!(requests[0].meta.attempt, 0);
    assert_eq!(requests[1].meta.attempt, 1);
    Ok(())
}

#[tokio::test]
async fn auth_retry_exhaustion_redacts_request_and_response_sentinels() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, AUTH_RESPONSE_SENTINEL),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, AUTH_RESPONSE_SENTINEL),
        ],
    );
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some(AUTH_RETRY_SENTINEL.to_string()),
            identity: "user-a",
        },
        transport,
    );
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.retry =
                retry_policy_for_statuses(2, vec![StatusCode::INTERNAL_SERVER_ERROR]).retry;
            policy
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("retry exhaustion should surface as status error");

    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "Text");
    assert_eq!(err.context().method, Method::GET);
    assert_error_chain_does_not_contain_any(&err, &[AUTH_RETRY_SENTINEL, AUTH_RESPONSE_SENTINEL]);
    assert_eq!(sent.sent_count().await, 2);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    let expected = format!("Bearer {AUTH_RETRY_SENTINEL}");
    for request in &requests {
        assert_eq!(
            request
                .headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some(expected.as_str())
        );
    }
}

#[tokio::test]
async fn query_auth_key_collision_fails_before_transport_without_leaking_secret() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let client = client(
        TestAuthVars {
            token: Some("QUERY_AUTH_COLLISION_SECRET".to_string()),
            identity: "user-a",
        },
        transport,
    );
    let mut policy = auth_policy(AuthPlacement::Query("api_key"));
    policy
        .query
        .push(("api_key".to_string(), "public-value".to_string()));

    let err = client
        .request(TextEndpoint {
            policy,
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("query auth key collision should fail before transport");

    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::InvalidConfiguration);
            let msg = source.to_string();
            assert!(msg.contains("api_key"));
            assert!(!msg.contains("QUERY_AUTH_COLLISION_SECRET"));
        }
        other => panic!("expected auth error, got {other:?}"),
    }
    assert_eq!(sent.sent_count().await, 0);
}

#[tokio::test]
async fn query_auth_collision_fails_before_rate_limit() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some("QUERY_AUTH_COLLISION_SECRET".to_string()),
            identity: "user-a",
        },
        transport,
    );
    configure_runtime(&mut client, Some(rate_limiter));
    let mut policy = auth_policy(AuthPlacement::Query("api_key"));
    policy
        .query
        .push(("api_key".to_string(), "public-value".to_string()));

    let err = client
        .request(TextEndpoint {
            policy,
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("query auth collision should fail before rate limit");

    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::InvalidConfiguration);
            let msg = source.to_string();
            assert!(msg.contains("api_key"));
            assert!(!msg.contains("QUERY_AUTH_COLLISION_SECRET"));
        }
        other => panic!("expected auth error, got {other:?}"),
    }
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "rate_response"));
}

#[tokio::test]
async fn public_header_auth_collision_fails_before_rate_limit() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some("BEARER_HEADER_COLLISION_SECRET".to_string()),
            identity: "user-a",
        },
        transport,
    );
    configure_runtime(&mut client, Some(rate_limiter));
    let mut policy = auth_policy(AuthPlacement::Bearer);
    policy.headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_static("public"),
    );

    let err = client
        .request(TextEndpoint {
            policy,
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("bearer authorization collision should fail before rate limit");

    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::InvalidConfiguration);
            let msg = source.to_string();
            assert!(msg.contains("Authorization"));
            assert!(!msg.contains("BEARER_HEADER_COLLISION_SECRET"));
        }
        other => panic!("expected auth error, got {other:?}"),
    }
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "rate_response"));
}

#[tokio::test]
async fn custom_header_auth_collision_fails_before_rate_limit_case_insensitive() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let rate_limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let mut client = client(
        TestAuthVars {
            token: Some("HEADER_AUTH_COLLISION_SECRET".to_string()),
            identity: "user-a",
        },
        transport,
    );
    configure_runtime(&mut client, Some(rate_limiter));
    let mut policy = auth_policy(AuthPlacement::Header("X-Api-Key"));
    policy.headers.insert(
        HeaderName::from_static("x-api-key"),
        HeaderValue::from_static("public"),
    );

    let err = client
        .request(TextEndpoint {
            policy,
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("custom header collision should fail before rate limit");

    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::InvalidConfiguration);
            let msg = source.to_string();
            assert!(msg.contains("x-api-key"));
            assert!(!msg.contains("HEADER_AUTH_COLLISION_SECRET"));
        }
        other => panic!("expected auth error, got {other:?}"),
    }
    assert_eq!(sent.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(!events.iter().any(|event| event == "rate_acquire"));
    assert!(!events.iter().any(|event| event == "rate_response"));
}

#[tokio::test]
async fn auth_policy_default_401_invalidates_and_retries_runtime_reacquirable()
-> Result<(), ApiClientError> {
    let harness = RotatingPolicyAuthHarness::new(
        AuthStepPolicy::default(),
        AuthChallengePolicy::Default,
        vec!["old-401".to_string(), "new-401".to_string()],
    );
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let mut client = harness.client(vec![
        MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
        MockResponse::text(StatusCode::OK, "refreshed"),
    ]);
    configure_runtime(
        &mut client,
        Some(Arc::new(ObservationRateLimiter::new(
            harness.events.clone(),
        ))),
    );
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Retry,
        8,
    )));

    let decoded = client
        .request(harness.endpoint(StatusCode::UNAUTHORIZED))
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;

    assert_eq!(decoded.value(), "refreshed");
    assert_eq!(harness.transport_attempts().await, 2);
    assert_eq!(harness.event_count("invalidate:Unauthorized").await, 1);
    assert_eq!(harness.event_count("retry:Unauthorized").await, 1);
    assert_eq!(harness.event_count("prepare:old-401").await, 1);
    assert_eq!(harness.event_count("prepare:new-401").await, 1);
    assert!(retry_events.lock().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_policy_default_403_invalidates_and_retries_runtime_reacquirable()
-> Result<(), ApiClientError> {
    let harness = RotatingPolicyAuthHarness::new(
        AuthStepPolicy::default(),
        AuthChallengePolicy::Default,
        vec!["old-403".to_string(), "new-403".to_string()],
    );
    let retry_events = Arc::new(Mutex::new(Vec::new()));
    let mut client = harness.client(vec![
        MockResponse::text(StatusCode::FORBIDDEN, "forbidden"),
        MockResponse::text(StatusCode::OK, "refreshed"),
    ]);
    configure_runtime(
        &mut client,
        Some(Arc::new(ObservationRateLimiter::new(
            harness.events.clone(),
        ))),
    );
    client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
        retry_events.clone(),
        RetryDecision::Retry,
        8,
    )));

    let decoded = client
        .request(harness.endpoint(StatusCode::FORBIDDEN))
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;

    assert_eq!(decoded.value(), "refreshed");
    assert_eq!(harness.transport_attempts().await, 2);
    assert_eq!(harness.event_count("invalidate:Forbidden").await, 1);
    assert_eq!(harness.event_count("retry:Forbidden").await, 1);
    assert_eq!(harness.event_count("prepare:old-403").await, 1);
    assert_eq!(harness.event_count("prepare:new-403").await, 1);
    assert!(retry_events.lock().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn auth_403_retries_when_policy_enables_forbidden_retry() -> Result<(), ApiClientError> {
    let policy = AuthStepPolicy {
        retry_on_forbidden: true,
        invalidate_on_forbidden: false,
        ..AuthStepPolicy::default()
    };
    let harness = PolicyAuthHarness::new(policy, AuthChallengePolicy::Default);
    let client = harness.client(vec![
        MockResponse::text(StatusCode::FORBIDDEN, "forbidden"),
        MockResponse::text(StatusCode::OK, "refreshed"),
    ]);

    let decoded = client
        .request(harness.endpoint(StatusCode::FORBIDDEN))
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;

    assert_eq!(decoded.value(), "refreshed");
    assert_eq!(harness.transport_attempts().await, 2);
    assert_eq!(harness.event_count("acquire").await, 2);
    assert_eq!(harness.event_count("invalidate:Forbidden").await, 0);
    assert_eq!(harness.event_count("retry:Forbidden").await, 1);
    Ok(())
}

#[tokio::test]
async fn auth_403_invalidates_only_when_policy_enables_forbidden_invalidation() {
    let policy = AuthStepPolicy {
        retry_on_forbidden: false,
        invalidate_on_forbidden: true,
        ..AuthStepPolicy::default()
    };
    let harness = PolicyAuthHarness::new(policy, AuthChallengePolicy::Default);
    let client = harness.client(vec![MockResponse::text(StatusCode::FORBIDDEN, "forbidden")]);

    let err = client
        .request(harness.endpoint(StatusCode::FORBIDDEN))
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("403 should not retry when retry_on_forbidden is false");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(harness.transport_attempts().await, 1);
    assert_eq!(harness.event_count("acquire").await, 1);
    assert_eq!(harness.event_count("invalidate:Forbidden").await, 1);
    assert_eq!(harness.event_count("retry:Forbidden").await, 0);
}

#[tokio::test]
async fn auth_policy_retry_true_invalidate_false_behavior_characterized()
-> Result<(), ApiClientError> {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let policy = AuthStepPolicy {
            retry_on_unauthorized: status == StatusCode::UNAUTHORIZED,
            retry_on_forbidden: status == StatusCode::FORBIDDEN,
            invalidate_on_unauthorized: false,
            invalidate_on_forbidden: false,
            ..AuthStepPolicy::default()
        };
        let harness = PolicyAuthHarness::new(policy, AuthChallengePolicy::Default);
        let retry_events = Arc::new(Mutex::new(Vec::new()));
        let mut client = harness.client(vec![
            MockResponse::text(status, "rejected"),
            MockResponse::text(StatusCode::OK, "recovered"),
        ]);
        configure_runtime(
            &mut client,
            Some(Arc::new(ObservationRateLimiter::new(
                harness.events.clone(),
            ))),
        );
        client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
            retry_events.clone(),
            RetryDecision::Retry,
            8,
        )));

        let decoded = client
            .request(harness.endpoint(status))
            .execute_decoded_with::<concord_core::prelude::Text<String>>()
            .await?;

        assert_eq!(decoded.value(), "recovered");
        assert_eq!(harness.transport_attempts().await, 2);
        assert_eq!(harness.event_count("invalidate:Unauthorized").await, 0);
        assert_eq!(harness.event_count("invalidate:Forbidden").await, 0);
        assert_eq!(
            harness.event_count("retry:Unauthorized").await,
            if status == StatusCode::UNAUTHORIZED {
                1
            } else {
                0
            }
        );
        assert_eq!(
            harness.event_count("retry:Forbidden").await,
            if status == StatusCode::FORBIDDEN {
                1
            } else {
                0
            }
        );
        assert_eq!(harness.event_count("acquire").await, 2);
        assert!(retry_events.lock().await.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn auth_policy_invalidate_true_retry_false_behavior_characterized()
-> Result<(), ApiClientError> {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let policy = AuthStepPolicy {
            retry_on_unauthorized: false,
            retry_on_forbidden: false,
            invalidate_on_unauthorized: status == StatusCode::UNAUTHORIZED,
            invalidate_on_forbidden: status == StatusCode::FORBIDDEN,
            ..AuthStepPolicy::default()
        };
        let harness = RotatingPolicyAuthHarness::new(
            policy,
            AuthChallengePolicy::Default,
            vec!["terminal-token".to_string()],
        );
        let retry_events = Arc::new(Mutex::new(Vec::new()));
        let mut client = harness.client(vec![MockResponse::text(status, "rejected")]);
        configure_runtime(
            &mut client,
            Some(Arc::new(ObservationRateLimiter::new(
                harness.events.clone(),
            ))),
        );
        client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
            retry_events.clone(),
            RetryDecision::Retry,
            8,
        )));

        let err = client
            .request(harness.endpoint(status))
            .execute_decoded_with::<concord_core::prelude::Text<String>>()
            .await
            .expect_err("invalidate-only auth rejection should be terminal");

        assert_eq!(
            err.category(),
            concord_core::error::ErrorCategory::AuthRejected
        );
        assert!(err.to_string().contains("auth challenge rejected"));
        assert_eq!(harness.transport_attempts().await, 1);
        assert_eq!(
            harness.event_count("invalidate:Unauthorized").await,
            if status == StatusCode::UNAUTHORIZED {
                1
            } else {
                0
            }
        );
        assert_eq!(
            harness.event_count("invalidate:Forbidden").await,
            if status == StatusCode::FORBIDDEN {
                1
            } else {
                0
            }
        );
        assert_eq!(harness.event_count("retry:Unauthorized").await, 0);
        assert_eq!(harness.event_count("retry:Forbidden").await, 0);
        assert_eq!(harness.event_count("prepare:terminal-token").await, 1);
        assert!(retry_events.lock().await.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn auth_policy_no_retry_no_invalidate_terminal_rejection() -> Result<(), ApiClientError> {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let policy = AuthStepPolicy {
            retry_on_unauthorized: false,
            retry_on_forbidden: false,
            invalidate_on_unauthorized: false,
            invalidate_on_forbidden: false,
            ..AuthStepPolicy::default()
        };
        let harness = RotatingPolicyAuthHarness::new(
            policy,
            AuthChallengePolicy::Default,
            vec!["terminal-token".to_string()],
        );
        let retry_events = Arc::new(Mutex::new(Vec::new()));
        let mut client = harness.client(vec![MockResponse::text(status, "rejected")]);
        configure_runtime(
            &mut client,
            Some(Arc::new(ObservationRateLimiter::new(
                harness.events.clone(),
            ))),
        );
        client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
            retry_events.clone(),
            RetryDecision::Retry,
            8,
        )));

        let err = client
            .request(harness.endpoint(status))
            .execute_decoded_with::<concord_core::prelude::Text<String>>()
            .await
            .expect_err("non-refreshing auth rejection should be terminal");

        assert_eq!(
            err.category(),
            concord_core::error::ErrorCategory::AuthRejected
        );
        assert!(err.to_string().contains("auth challenge rejected"));
        assert_eq!(harness.transport_attempts().await, 1);
        assert_eq!(harness.event_count("invalidate:Unauthorized").await, 0);
        assert_eq!(harness.event_count("invalidate:Forbidden").await, 0);
        assert_eq!(harness.event_count("retry:Unauthorized").await, 0);
        assert_eq!(harness.event_count("retry:Forbidden").await, 0);
        assert_eq!(harness.event_count("prepare:terminal-token").await, 1);
        assert!(retry_events.lock().await.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn auth_never_refresh_does_not_retry_or_invalidate() {
    let harness =
        PolicyAuthHarness::new(AuthStepPolicy::default(), AuthChallengePolicy::NeverRefresh);
    let client = harness.client(vec![
        MockResponse::text(StatusCode::UNAUTHORIZED, "unauthorized"),
        MockResponse::text(StatusCode::OK, "should-not-send"),
    ]);

    let err = client
        .request(harness.endpoint(StatusCode::UNAUTHORIZED))
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("NeverRefresh should produce a terminal auth error");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(harness.transport_attempts().await, 1);
    assert_eq!(harness.event_count("acquire").await, 1);
    assert_eq!(harness.event_count("invalidate:Unauthorized").await, 0);
    assert_eq!(harness.event_count("retry:Unauthorized").await, 0);
}

#[tokio::test]
async fn auth_never_refresh_does_not_retry_or_invalidate_forbidden() {
    let harness =
        PolicyAuthHarness::new(AuthStepPolicy::default(), AuthChallengePolicy::NeverRefresh);
    let client = harness.client(vec![
        MockResponse::text(StatusCode::FORBIDDEN, "forbidden"),
        MockResponse::text(StatusCode::OK, "should-not-send"),
    ]);

    let err = client
        .request(harness.endpoint(StatusCode::FORBIDDEN))
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("NeverRefresh should leave 403 as a terminal auth error");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(harness.transport_attempts().await, 1);
    assert_eq!(harness.event_count("acquire").await, 1);
    assert_eq!(harness.event_count("invalidate:Forbidden").await, 0);
    assert_eq!(harness.event_count("retry:Forbidden").await, 0);
}

#[tokio::test]
async fn max_auth_retries_zero_behavior_characterized() {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let harness = RotatingPolicyAuthHarness::new(
            AuthStepPolicy::default(),
            AuthChallengePolicy::Default,
            vec!["token".to_string()],
        );
        let retry_events = Arc::new(Mutex::new(Vec::new()));
        let mut client = harness.client(vec![MockResponse::text(status, "rejected")]);
        client.set_max_auth_retries(0);
        client.set_retry_policy(Arc::new(RecordingRetryPolicy::new(
            retry_events.clone(),
            RetryDecision::Retry,
            8,
        )));

        let err = client
            .request(harness.endpoint(status))
            .execute_decoded_with::<concord_core::prelude::Text<String>>()
            .await
            .expect_err("zero auth retry budget should stop refresh attempts");

        assert_eq!(
            err.category(),
            concord_core::error::ErrorCategory::AuthRejected
        );
        assert_eq!(harness.transport_attempts().await, 1);
        assert_eq!(harness.event_count("prepare:token").await, 1);
        assert_eq!(
            harness.event_count("invalidate:Unauthorized").await,
            if status == StatusCode::UNAUTHORIZED {
                1
            } else {
                0
            }
        );
        assert_eq!(
            harness.event_count("invalidate:Forbidden").await,
            if status == StatusCode::FORBIDDEN {
                1
            } else {
                0
            }
        );
        assert_eq!(
            harness.event_count("retry:Unauthorized").await,
            if status == StatusCode::UNAUTHORIZED {
                1
            } else {
                0
            }
        );
        assert_eq!(
            harness.event_count("retry:Forbidden").await,
            if status == StatusCode::FORBIDDEN {
                1
            } else {
                0
            }
        );
        assert!(retry_events.lock().await.is_empty());
    }
}

#[tokio::test]
async fn auth_retry_respects_max_auth_retries() {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let harness = RotatingPolicyAuthHarness::new(
            AuthStepPolicy::default(),
            AuthChallengePolicy::Default,
            vec!["token-1".to_string(), "token-2".to_string()],
        );
        let mut client = harness.client(vec![
            MockResponse::text(status, "expired-1"),
            MockResponse::text(status, "expired-2"),
            MockResponse::text(StatusCode::OK, "should-not-send"),
        ]);
        client.set_max_auth_retries(1);

        let err = client
            .request(harness.endpoint(status))
            .execute_decoded_with::<concord_core::prelude::Text<String>>()
            .await
            .expect_err(
                "auth retry budget should stop repeated refresh attempts with an auth error",
            );

        assert_eq!(
            err.category(),
            concord_core::error::ErrorCategory::AuthRejected
        );
        assert_eq!(harness.transport_attempts().await, 2);
        assert_eq!(harness.event_count("prepare:token-1").await, 1);
        assert_eq!(harness.event_count("prepare:token-2").await, 1);
        assert_eq!(
            harness.event_count("invalidate:Unauthorized").await,
            if status == StatusCode::UNAUTHORIZED {
                2
            } else {
                0
            }
        );
        assert_eq!(
            harness.event_count("invalidate:Forbidden").await,
            if status == StatusCode::FORBIDDEN {
                2
            } else {
                0
            }
        );
        assert_eq!(
            harness.event_count("retry:Unauthorized").await,
            if status == StatusCode::UNAUTHORIZED {
                2
            } else {
                0
            }
        );
        assert_eq!(
            harness.event_count("retry:Forbidden").await,
            if status == StatusCode::FORBIDDEN {
                2
            } else {
                0
            }
        );
    }
}

#[tokio::test]
async fn auth_http_content_length_above_limit_fails() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "token").with_content_length(Some(5))],
    );
    let client = auth_http_client(transport, AuthHttpLimitVars::plain(4));

    let err = client
        .request(AuthHttpLimitEndpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("auth HTTP content length above limit should fail");

    assert_auth_response_too_large(err, 4);
}

#[tokio::test]
async fn auth_http_unknown_length_above_limit_fails() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, bytes::Bytes::new())
                .with_content_length(None)
                .with_chunks(vec![
                    bytes::Bytes::from_static(b"abcd"),
                    bytes::Bytes::from_static(b"e"),
                ]),
        ],
    );
    let client = auth_http_client(transport, AuthHttpLimitVars::plain(4));

    let err = client
        .request(AuthHttpLimitEndpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("auth HTTP chunked body above limit should fail");

    assert_auth_response_too_large(err, 4);
}

#[tokio::test]
async fn auth_http_body_at_limit_succeeds() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "tokn").with_content_length(Some(4)),
            MockResponse::text(StatusCode::OK, "protected"),
        ],
    );
    let client = auth_http_client(transport, AuthHttpLimitVars::plain(4));

    let decoded = client
        .request(AuthHttpLimitEndpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;

    assert_eq!(decoded.value(), "protected");
    Ok(())
}

#[cfg(feature = "json")]
#[tokio::test]
async fn oauth_client_credentials_token_response_above_limit_fails() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "{}").with_content_length(Some(
                (AuthInternalPolicy::DEFAULT_MAX_BODY_BYTES + 1) as u64,
            )),
        ],
    );
    let client = auth_http_client(transport, AuthHttpLimitVars::oauth());

    let err = client
        .request(AuthHttpLimitEndpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("oversized OAuth token response should fail");

    assert_auth_response_too_large(err, AuthInternalPolicy::DEFAULT_MAX_BODY_BYTES);
}

#[cfg(feature = "json")]
#[tokio::test]
async fn oauth_client_credentials_expires_in_overflow_returns_typed_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(
            StatusCode::OK,
            r#"{"access_token":"token","token_type":"Bearer","expires_in":18446744073709551615}"#,
        )],
    );
    let sent = transport.clone();
    let client = auth_http_client(transport, AuthHttpLimitVars::oauth());

    let err = client
        .request(AuthHttpLimitEndpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("overflowing OAuth expires_in should fail");

    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::InvalidConfiguration);
            assert!(source.to_string().contains("oauth2 expires_in overflowed"));
        }
        other => panic!("expected auth configuration error, got {other:?}"),
    }
    assert_eq!(sent.sent_count().await, 1);
}

#[cfg(feature = "json")]
#[tokio::test]
async fn oauth_client_credentials_invalid_token_url_returns_typed_error() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::OK, "should-not-send")],
    );
    let sent = transport.clone();
    let client = auth_http_client(transport, AuthHttpLimitVars::oauth_invalid_token_url());

    let err = client
        .request(AuthHttpLimitEndpoint)
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("invalid OAuth token URL should fail before transport");

    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::InvalidConfiguration);
            assert!(source.to_string().contains("invalid oauth2 token URL"));
        }
        other => panic!("expected auth configuration error, got {other:?}"),
    }
    assert_eq!(sent.sent_count().await, 0);
}

fn assert_auth_response_too_large(err: ApiClientError, limit: usize) {
    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::ResponseTooLarge);
            assert!(source.to_string().contains(&limit.to_string()));
        }
        other => panic!("expected auth response-too-large error, got {other:?}"),
    }
}

#[derive(Clone)]
struct AuthHttpLimitVars {
    max_body_bytes: usize,
    use_oauth: bool,
    #[cfg(feature = "json")]
    invalid_oauth_token_url: bool,
}

impl AuthHttpLimitVars {
    fn plain(max_body_bytes: usize) -> Self {
        Self {
            max_body_bytes,
            use_oauth: false,
            #[cfg(feature = "json")]
            invalid_oauth_token_url: false,
        }
    }

    #[cfg(feature = "json")]
    fn oauth() -> Self {
        Self {
            max_body_bytes: AuthInternalPolicy::DEFAULT_MAX_BODY_BYTES,
            use_oauth: true,
            #[cfg(feature = "json")]
            invalid_oauth_token_url: false,
        }
    }

    #[cfg(feature = "json")]
    fn oauth_invalid_token_url() -> Self {
        Self {
            max_body_bytes: AuthInternalPolicy::DEFAULT_MAX_BODY_BYTES,
            use_oauth: true,
            invalid_oauth_token_url: true,
        }
    }
}

#[derive(Clone)]
struct AuthHttpLimitCx;

impl ClientContext for AuthHttpLimitCx {
    type Vars = ();
    type AuthVars = AuthHttpLimitVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            let material = if auth.use_oauth {
                #[cfg(feature = "json")]
                {
                    let token_url = if auth.invalid_oauth_token_url {
                        "https://[invalid"
                    } else {
                        "https://auth.example.com/token"
                    };
                    let provider = OAuth2ClientCredentialsProvider::from_validated_token_url(
                        CredentialId::new("test", "oauth"),
                        token_url,
                        "client-id",
                        "client-secret",
                    );
                    provider
                        .acquire(CredentialContext::<AuthHttpLimitCx> {
                            vars: _vars,
                            auth,
                            auth_state: _auth_state,
                            executor,
                            credential_id:
                                <OAuth2ClientCredentialsProvider as CredentialProvider<
                                    AuthHttpLimitCx,
                                >>::id(&provider),
                            reason: CredentialRefreshReason::Missing,
                        })
                        .await?
                }
                #[cfg(not(feature = "json"))]
                {
                    return Err(AuthError::new(
                        AuthErrorKind::UnsupportedScheme,
                        "json feature is required for oauth2 provider tests",
                    ));
                }
            } else {
                let resp = executor
                    .send(AuthHttpRequest {
                        method: Method::POST,
                        url: "https://auth.example.com/token".parse().expect("auth url"),
                        headers: HeaderMap::new(),
                        body: concord_core::advanced::TransportRequestBody::Empty,
                        mode: AuthMode::SkipAuth,
                        policy: AuthInternalPolicy {
                            max_body_bytes: auth.max_body_bytes,
                            ..Default::default()
                        },
                    })
                    .await?;
                AccessToken::new(String::from_utf8_lossy(&resp.body).to_string())
            };
            let application =
                concord_core::advanced::apply_secret_credential(request, requirement, &material)?;
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }
}

struct AuthHttpLimitEndpoint;

impl Endpoint<AuthHttpLimitCx> for AuthHttpLimitEndpoint {
    type Response = String;

    buffered_endpoint_execute!(AuthHttpLimitCx, concord_core::prelude::Text<String>);
}

impl ReusableEndpoint<AuthHttpLimitCx> for AuthHttpLimitEndpoint {
    fn plan(
        &self,
        _ctx: &ClientPlanContext<'_, AuthHttpLimitCx>,
    ) -> Result<RequestPlan, ApiClientError> {
        Ok(request_plan(
            "AuthHttpLimit",
            Method::GET,
            "/auth-http-limit",
            auth_policy(AuthPlacement::Bearer),
            None,
        ))
    }
}

fn auth_http_client(
    transport: MockTransport,
    auth: AuthHttpLimitVars,
) -> concord_core::prelude::ApiClient<AuthHttpLimitCx, MockTransport> {
    concord_core::prelude::ApiClient::with_transport((), auth, transport)
}

#[derive(Clone)]
struct RecordingAuthVars {
    token: Option<String>,
    identity: &'static str,
    events: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
struct RecordingAuthCx;

impl ClientContext for RecordingAuthCx {
    type Vars = ();
    type AuthVars = RecordingAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            let token = auth.token.as_deref().ok_or_else(|| {
                AuthError::new(
                    AuthErrorKind::MissingCredential,
                    format!(
                        "missing credential `{}`; acquire or configure it before sending request",
                        requirement.credential.id
                    ),
                )
            })?;
            let application = match requirement.placement {
                AuthPlacement::Bearer | AuthPlacement::Header(_) | AuthPlacement::Query(_) => {
                    let material = ApiKey::new(token.to_string());
                    concord_core::advanced::apply_secret_credential(
                        request,
                        requirement,
                        &material,
                    )?
                }
                AuthPlacement::Basic | AuthPlacement::Certificate => {
                    return Err(AuthError::new(
                        AuthErrorKind::UnsupportedScheme,
                        "test context supports bearer/header/query auth only",
                    ));
                }
            };
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }

    fn handle_auth_response<'a>(
        requirement: &'a AuthRequirement,
        applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
        status: StatusCode,
        _headers: &'a HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        let events = auth.events.clone();
        Box::pin(async move {
            if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                events.lock().await.push("auth_rejection".to_string());
                if auth.identity == "refresh" {
                    events.lock().await.push("auth_retry".to_string());
                    return Ok(AuthDecision::RetryAfterRefresh {
                        credential: requirement.credential.clone(),
                        generation: applied.generation,
                        reason: concord_core::advanced::AuthRetryReason::Unauthorized,
                    });
                }
                Ok(AuthDecision::Fail)
            } else {
                Ok(AuthDecision::Continue)
            }
        })
    }
}

impl Endpoint<RecordingAuthCx> for TextEndpoint {
    type Response = String;

    buffered_endpoint_execute!(RecordingAuthCx, concord_core::prelude::Text<String>);
}

impl ReusableEndpoint<RecordingAuthCx> for TextEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, RecordingAuthCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination
                .as_ref()
                .map(|_| concord_core::internal::PaginationMarker),
        ))
    }
}

#[derive(Clone)]
struct PolicyAuthVars {
    token: String,
    policy: AuthStepPolicy,
    events: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
struct PolicyAuthCx;

impl ClientContext for PolicyAuthCx {
    type Vars = ();
    type AuthVars = PolicyAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            auth.events.lock().await.push("acquire".to_string());
            let material = ApiKey::new(auth.token.clone());
            let application =
                concord_core::advanced::apply_secret_credential(request, requirement, &material)?;
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }

    fn handle_auth_response<'a>(
        requirement: &'a AuthRequirement,
        applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
        status: StatusCode,
        _headers: &'a HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        Box::pin(async move {
            let Some(decision) =
                auth_decision_for_status(status, requirement, applied, auth.policy)
            else {
                return Ok(AuthDecision::Continue);
            };

            if let Some(reason) = decision.invalidate_reason {
                auth.events
                    .lock()
                    .await
                    .push(format!("invalidate:{reason:?}"));
            }
            if let Some(reason) = decision.retry_reason {
                auth.events.lock().await.push(format!("retry:{reason:?}"));
                return Ok(AuthDecision::RetryAfterRefresh {
                    credential: requirement.credential.clone(),
                    generation: applied.generation,
                    reason,
                });
            }

            Ok(AuthDecision::Continue)
        })
    }
}

impl Endpoint<PolicyAuthCx> for TextEndpoint {
    type Response = String;

    buffered_endpoint_execute!(PolicyAuthCx, concord_core::prelude::Text<String>);
}

impl ReusableEndpoint<PolicyAuthCx> for TextEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, PolicyAuthCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination
                .as_ref()
                .map(|_| concord_core::internal::PaginationMarker),
        ))
    }
}

#[derive(Clone)]
struct RotatingPolicyAuthVars {
    tokens: Arc<Mutex<VecDeque<String>>>,
    policy: AuthStepPolicy,
    events: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
struct RotatingPolicyAuthCx;

impl ClientContext for RotatingPolicyAuthCx {
    type Vars = ();
    type AuthVars = RotatingPolicyAuthVars;
    type AuthState = ();
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {}

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            let token = {
                let mut tokens = auth.tokens.lock().await;
                tokens.pop_front().ok_or_else(|| {
                    AuthError::new(
                        AuthErrorKind::MissingCredential,
                        format!(
                            "missing credential `{}`; acquire or configure it before sending request",
                            requirement.credential.id
                        ),
                    )
                })?
            };
            auth.events.lock().await.push(format!("prepare:{token}"));
            let material = ApiKey::new(token);
            let application =
                concord_core::advanced::apply_secret_credential(request, requirement, &material)?;
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }

    fn handle_auth_response<'a>(
        requirement: &'a AuthRequirement,
        applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
        status: StatusCode,
        _headers: &'a HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        Box::pin(async move {
            let Some(decision) =
                auth_decision_for_status(status, requirement, applied, auth.policy)
            else {
                return Ok(AuthDecision::Continue);
            };

            if let Some(reason) = decision.invalidate_reason {
                auth.events
                    .lock()
                    .await
                    .push(format!("invalidate:{reason:?}"));
            }
            if let Some(reason) = decision.retry_reason {
                auth.events.lock().await.push(format!("retry:{reason:?}"));
                return Ok(AuthDecision::RetryAfterRefresh {
                    credential: requirement.credential.clone(),
                    generation: applied.generation,
                    reason,
                });
            }

            Ok(AuthDecision::Continue)
        })
    }
}

impl Endpoint<RotatingPolicyAuthCx> for TextEndpoint {
    type Response = String;

    buffered_endpoint_execute!(RotatingPolicyAuthCx, concord_core::prelude::Text<String>);
}

impl ReusableEndpoint<RotatingPolicyAuthCx> for TextEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, RotatingPolicyAuthCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination
                .as_ref()
                .map(|_| concord_core::internal::PaginationMarker),
        ))
    }
}

#[derive(Clone)]
struct RotatingPolicyAuthHarness {
    events: Arc<Mutex<Vec<String>>>,
    policy: AuthStepPolicy,
    challenge: AuthChallengePolicy,
    tokens: Vec<String>,
}

impl RotatingPolicyAuthHarness {
    fn new(policy: AuthStepPolicy, challenge: AuthChallengePolicy, tokens: Vec<String>) -> Self {
        let events = Arc::new(Mutex::new(Vec::new()));
        Self {
            events,
            policy,
            challenge,
            tokens,
        }
    }

    fn client(
        &self,
        responses: Vec<MockResponse>,
    ) -> concord_core::prelude::ApiClient<RotatingPolicyAuthCx, MockTransport> {
        let transport = MockTransport::new(self.events.clone(), responses);
        let mut client =
            concord_core::prelude::ApiClient::<RotatingPolicyAuthCx, _>::with_transport(
                (),
                RotatingPolicyAuthVars {
                    tokens: Arc::new(Mutex::new(VecDeque::from(self.tokens.clone()))),
                    policy: self.policy,
                    events: self.events.clone(),
                },
                transport.clone(),
            );
        client.set_max_auth_retries(8);
        client
    }

    fn endpoint(&self, retry_status: StatusCode) -> TextEndpoint {
        let mut policy = auth_policy(AuthPlacement::Bearer);
        policy.auth.requirements[0].challenge = self.challenge;
        policy.retry = retry_policy_for_statuses(1, vec![retry_status]).retry;
        TextEndpoint {
            policy,
            ..Default::default()
        }
    }

    async fn event_count(&self, event: &str) -> usize {
        self.events
            .lock()
            .await
            .iter()
            .filter(|seen| seen.as_str() == event)
            .count()
    }

    async fn transport_attempts(&self) -> usize {
        self.events
            .lock()
            .await
            .iter()
            .filter(|seen| seen.as_str() == "transport")
            .count()
    }
}

struct PolicyAuthHarness {
    events: Arc<Mutex<Vec<String>>>,
    policy: AuthStepPolicy,
    challenge: AuthChallengePolicy,
}

impl PolicyAuthHarness {
    fn new(policy: AuthStepPolicy, challenge: AuthChallengePolicy) -> Self {
        let events = Arc::new(Mutex::new(Vec::new()));
        Self {
            events,
            policy,
            challenge,
        }
    }

    fn client(
        &self,
        responses: Vec<MockResponse>,
    ) -> concord_core::prelude::ApiClient<PolicyAuthCx, MockTransport> {
        let transport = MockTransport::new(self.events.clone(), responses);
        let mut client = concord_core::prelude::ApiClient::<PolicyAuthCx, _>::with_transport(
            (),
            PolicyAuthVars {
                token: "token".to_string(),
                policy: self.policy,
                events: self.events.clone(),
            },
            transport.clone(),
        );
        client.set_max_auth_retries(8);
        client
    }

    fn endpoint(&self, retry_status: StatusCode) -> TextEndpoint {
        let mut policy = auth_policy(AuthPlacement::Bearer);
        policy.auth.requirements[0].challenge = self.challenge;
        policy.retry = retry_policy_for_statuses(1, vec![retry_status]).retry;
        TextEndpoint {
            policy,
            ..Default::default()
        }
    }

    async fn event_count(&self, event: &str) -> usize {
        self.events
            .lock()
            .await
            .iter()
            .filter(|seen| seen.as_str() == event)
            .count()
    }

    async fn transport_attempts(&self) -> usize {
        self.events
            .lock()
            .await
            .iter()
            .filter(|seen| seen.as_str() == "transport")
            .count()
    }
}

#[derive(Clone)]
struct SlotAuthVars {
    slot: Arc<CredentialSlot<SlotAuthCx, SlotTokenProvider>>,
    events: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
struct SlotAuthState {
    slot: Arc<CredentialSlot<SlotAuthCx, SlotTokenProvider>>,
}

#[derive(Clone)]
struct SlotAuthCx;

#[derive(Clone)]
struct SlotTokenProvider {
    events: Arc<Mutex<Vec<String>>>,
    fail_acquire: bool,
}

impl CredentialProvider<SlotAuthCx> for SlotTokenProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("test", "slot-token")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, SlotAuthCx>,
    ) -> concord_core::advanced::AuthFuture<'a, Result<Self::Credential, AuthError>> {
        let events = self.events.clone();
        let fail_acquire = self.fail_acquire;
        Box::pin(async move {
            events
                .lock()
                .await
                .push(format!("slot_acquire:{:?}", ctx.reason));
            if fail_acquire {
                return Err(AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    "slot acquire failed",
                ));
            }
            Ok(AccessToken::new("token-1".to_string()))
        })
    }

    fn invalidate<'a>(
        &'a self,
        _ctx: CredentialContext<'a, SlotAuthCx>,
        current: Option<&'a Self::Credential>,
        reason: InvalidateReason,
    ) -> concord_core::advanced::AuthFuture<'a, Result<(), AuthError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .await
                .push(format!("slot_invalidate:{reason:?}:{}", current.is_some()));
            Ok(())
        })
    }
}

impl ClientContext for SlotAuthCx {
    type Vars = ();
    type AuthVars = SlotAuthVars;
    type AuthState = SlotAuthState;
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        SlotAuthState {
            slot: auth.slot.clone(),
        }
    }

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        auth_state: &'a Self::AuthState,
        executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            let lease = auth
                .slot
                .get_or_refresh(
                    CredentialContext {
                        vars: _vars,
                        auth,
                        auth_state,
                        executor,
                        credential_id: requirement.credential.id.clone(),
                        reason: CredentialRefreshReason::Missing,
                    },
                    AuthStepPolicy::default(),
                )
                .await?;
            let application = concord_core::advanced::apply_secret_credential(
                request,
                requirement,
                &lease.value,
            )?;
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(lease.generation),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }

    fn handle_auth_response<'a>(
        requirement: &'a AuthRequirement,
        applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        auth_state: &'a Self::AuthState,
        executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        meta: &'a RequestMeta,
        status: StatusCode,
        headers: &'a HeaderMap,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthDecision, AuthError>> {
        let events = auth.events.clone();
        Box::pin(async move {
            let Some(decision) =
                auth_decision_for_status(status, requirement, applied, AuthStepPolicy::default())
            else {
                return Ok(AuthDecision::Continue);
            };

            if let Some(reason) = decision.invalidate_reason {
                events
                    .lock()
                    .await
                    .push(format!("slot_invalidate:{reason:?}"));
                auth_state
                    .slot
                    .invalidate_generation(
                        CredentialContext {
                            vars: _vars,
                            auth,
                            auth_state,
                            executor,
                            credential_id: requirement.credential.id.clone(),
                            reason: CredentialRefreshReason::Rejected,
                        },
                        applied.generation,
                        match status {
                            StatusCode::UNAUTHORIZED => InvalidateReason::Unauthorized,
                            StatusCode::FORBIDDEN => InvalidateReason::Forbidden,
                            _ => InvalidateReason::Manual,
                        },
                    )
                    .await?;
            }
            if let Some(reason) = decision.retry_reason {
                events.lock().await.push(format!("slot_retry:{reason:?}"));
                return Ok(AuthDecision::RetryAfterRefresh {
                    credential: requirement.credential.clone(),
                    generation: applied.generation,
                    reason,
                });
            }

            let _ = (meta, headers);
            Ok(AuthDecision::Continue)
        })
    }
}

impl Endpoint<SlotAuthCx> for TextEndpoint {
    type Response = String;

    buffered_endpoint_execute!(SlotAuthCx, concord_core::prelude::Text<String>);
}

impl ReusableEndpoint<SlotAuthCx> for TextEndpoint {
    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, SlotAuthCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination
                .as_ref()
                .map(|_| concord_core::internal::PaginationMarker),
        ))
    }
}

#[tokio::test]
async fn endpoint_backed_auth_slot_acquires_and_applies_the_credential()
-> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let slot = Arc::new(CredentialSlot::new(SlotTokenProvider {
        events: events.clone(),
        fail_acquire: false,
    }));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "slot-ok")],
    );
    let sent_transport = transport.clone();
    let mut client = concord_core::prelude::ApiClient::<SlotAuthCx, _>::with_transport(
        (),
        SlotAuthVars {
            slot: slot.clone(),
            events: events.clone(),
        },
        transport,
    );
    client.set_max_auth_retries(8);

    let decoded = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await?;

    assert_eq!(decoded.value(), "slot-ok");
    assert_eq!(sent_transport.sent_count().await, 1);
    let requests = sent_transport.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer token-1")
    );
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "slot_acquire:Missing"));
    Ok(())
}

#[tokio::test]
async fn endpoint_backed_auth_slot_acquire_failure_is_typed_and_contextual() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let slot = Arc::new(CredentialSlot::new(SlotTokenProvider {
        events: events.clone(),
        fail_acquire: true,
    }));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "unused")],
    );
    let sent_transport = transport.clone();
    let mut client = concord_core::prelude::ApiClient::<SlotAuthCx, _>::with_transport(
        (),
        SlotAuthVars {
            slot: slot.clone(),
            events: events.clone(),
        },
        transport,
    );
    client.set_max_auth_retries(8);

    let err = client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("slot acquisition failure should surface as auth error");

    match &err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::AcquireFailed);
            assert!(source.to_string().contains("slot acquire failed"));
        }
        other => panic!("expected auth error, got {other:?}"),
    }
    assert_eq!(err.category(), ErrorCategory::AuthRejected);
    assert_eq!(err.context().endpoint, "Text");
    assert_eq!(err.context().method, Method::GET);
    assert_eq!(sent_transport.sent_count().await, 0);
    let events = events.lock().await.clone();
    assert!(events.iter().any(|event| event == "slot_acquire:Missing"));
}

#[tokio::test]
async fn auth_rejection_stale_invalidation_cannot_clear_newer_generation()
-> Result<(), ApiClientError> {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let slot = Arc::new(CredentialSlot::new(SlotTokenProvider {
            events: events.clone(),
            fail_acquire: false,
        }));
        let transport = GateTransport::new(
            events.clone(),
            vec![
                MockResponse::text(status, "expired"),
                MockResponse::text(StatusCode::OK, "refreshed"),
            ],
        );
        let sent_transport = transport.clone();
        let mut client = concord_core::prelude::ApiClient::<SlotAuthCx, _>::with_transport(
            (),
            SlotAuthVars {
                slot: slot.clone(),
                events: events.clone(),
            },
            transport,
        );
        client.set_max_auth_retries(8);
        let client = Arc::new(client);

        let request = TextEndpoint {
            policy: auth_policy(AuthPlacement::Bearer),
            ..Default::default()
        };
        let handle = {
            let client = client.clone();
            let request = request.clone();
            tokio::spawn(async move {
                client
                    .request(request)
                    .execute_decoded_with::<concord_core::prelude::Text<String>>()
                    .await
                    .map(|response| response.into_value())
            })
        };

        sent_transport.wait_for_sends(1).await;
        slot.set_manual(AccessToken::new("token-2".to_string()))
            .await
            .expect("manual replacement should advance the generation");
        sent_transport.release_all();

        let decoded = handle
            .await
            .expect("request task should complete successfully")?;
        assert_eq!(decoded, "refreshed");
        assert_eq!(sent_transport.sent_count().await, 2);
        let requests = sent_transport.requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0]
                .headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer token-1")
        );
        assert_eq!(
            requests[1]
                .headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer token-2")
        );
        let cached = client
            .auth_state()
            .slot
            .get_cached()
            .await
            .expect("newer generation should remain valid");
        assert_eq!(cached.generation, 2);
        let events = events.lock().await.clone();
        assert!(events.iter().any(|event| event == "slot_acquire:Missing"));
        assert!(
            events
                .iter()
                .any(|event| event.starts_with("slot_invalidate:"))
        );
    }
    Ok(())
}
