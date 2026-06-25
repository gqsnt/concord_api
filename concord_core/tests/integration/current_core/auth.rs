use super::common::*;
use concord_core::advanced::{
    AuthApplicationRequest, AuthAppliedCredential, AuthChallengePolicy, AuthDecision, AuthError,
    AuthErrorKind, AuthHttpRequest, AuthInternalPolicy, AuthMode, AuthPlacement, AuthRequirement,
    AuthStepPolicy, PreparedAuthCredential, RequestMeta, auth_decision_for_status,
};
#[cfg(feature = "json")]
use concord_core::advanced::{
    CredentialContext, CredentialId, CredentialProvider, CredentialRefreshReason,
    OAuth2ClientCredentialsProvider,
};
use concord_core::internal::{ClientPlanContext, RequestPlan};
use concord_core::prelude::{AccessToken, ApiClientError, ApiKey, ClientContext, Endpoint};
use http::{HeaderMap, Method, StatusCode};
use std::sync::Arc;
use tokio::sync::Mutex;

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
        .execute_decoded()
        .await
        .expect_err("missing token must fail before transport");

    let msg = err.to_string();
    assert!(msg.contains("missing credential"));
    assert!(msg.contains("test.token"));
    assert!(msg.contains("acquire or configure"));
}

#[tokio::test]
async fn auth_rejection_does_not_store_cache_entry() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::UNAUTHORIZED, "nope")],
    );
    let mut client = client(
        TestAuthVars {
            token: Some("bad".to_string()),
            identity: "user-a",
        },
        transport,
    );
    configure_runtime(&mut client, Some(cache), None);
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("401 auth rejection should fail");
    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(*after_response_count.lock().await, 0);
}

#[tokio::test]
async fn auth_rejection_does_not_store_cache_entry_forbidden() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::miss(events.clone()));
    let after_response_count = cache.after_response_count.clone();
    let transport = MockTransport::new(
        events,
        vec![MockResponse::text(StatusCode::FORBIDDEN, "forbidden")],
    );
    let mut client = client(
        TestAuthVars {
            token: Some("bad".to_string()),
            identity: "user-a",
        },
        transport,
    );
    configure_runtime(&mut client, Some(cache), None);
    let endpoint = TextEndpoint {
        policy: {
            let mut policy = auth_policy(AuthPlacement::Bearer);
            policy.cache = concord_core::internal::CacheSetting::Config(
                concord_core::advanced::CacheConfig::new(),
            );
            policy
        },
        ..Default::default()
    };

    let err = client
        .request(endpoint)
        .execute_decoded()
        .await
        .expect_err("403 auth rejection should fail");
    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(*after_response_count.lock().await, 0);
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
        .execute_decoded()
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

    let decoded = client.request(endpoint).execute_decoded().await?;

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
        .execute_decoded()
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
        .execute_decoded()
        .await?;
    client
        .request(TextEndpoint {
            policy: auth_policy(AuthPlacement::Query("api_key")),
            ..Default::default()
        })
        .execute_decoded()
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
        .execute_decoded()
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
async fn auth_401_retries_and_invalidates_by_default() -> Result<(), ApiClientError> {
    let harness = PolicyAuthHarness::new(AuthStepPolicy::default(), AuthChallengePolicy::Default);
    let client = harness.client(vec![
        MockResponse::text(StatusCode::UNAUTHORIZED, "expired"),
        MockResponse::text(StatusCode::OK, "refreshed"),
    ]);

    let decoded = client
        .request(harness.endpoint(StatusCode::UNAUTHORIZED))
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "refreshed");
    assert_eq!(harness.transport_attempts().await, 2);
    assert_eq!(harness.event_count("acquire").await, 2);
    assert_eq!(harness.event_count("invalidate:Unauthorized").await, 1);
    assert_eq!(harness.event_count("retry:Unauthorized").await, 1);
    Ok(())
}

#[tokio::test]
async fn auth_403_retries_and_invalidates_by_default() -> Result<(), ApiClientError> {
    let harness = PolicyAuthHarness::new(AuthStepPolicy::default(), AuthChallengePolicy::Default);
    let client = harness.client(vec![
        MockResponse::text(StatusCode::FORBIDDEN, "forbidden"),
        MockResponse::text(StatusCode::OK, "refreshed"),
    ]);

    let decoded = client
        .request(harness.endpoint(StatusCode::FORBIDDEN))
        .execute_decoded()
        .await?;

    assert_eq!(decoded.value(), "refreshed");
    assert_eq!(harness.transport_attempts().await, 2);
    assert_eq!(harness.event_count("acquire").await, 2);
    assert_eq!(harness.event_count("invalidate:Forbidden").await, 1);
    assert_eq!(harness.event_count("retry:Forbidden").await, 1);
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
        .execute_decoded()
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
        .execute_decoded()
        .await
        .expect_err("403 should not retry when retry_on_forbidden is false");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(harness.transport_attempts().await, 1);
    assert_eq!(harness.event_count("acquire").await, 1);
    assert_eq!(harness.event_count("invalidate:Forbidden").await, 1);
    assert_eq!(harness.event_count("retry:Forbidden").await, 0);
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
        .execute_decoded()
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
        .execute_decoded()
        .await
        .expect_err("NeverRefresh should leave 403 as a terminal auth error");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(harness.transport_attempts().await, 1);
    assert_eq!(harness.event_count("acquire").await, 1);
    assert_eq!(harness.event_count("invalidate:Forbidden").await, 0);
    assert_eq!(harness.event_count("retry:Forbidden").await, 0);
}

#[tokio::test]
async fn auth_retry_respects_max_auth_retries() {
    let harness = PolicyAuthHarness::new(AuthStepPolicy::default(), AuthChallengePolicy::Default);
    let mut client = harness.client(vec![
        MockResponse::text(StatusCode::UNAUTHORIZED, "expired-1"),
        MockResponse::text(StatusCode::UNAUTHORIZED, "expired-2"),
        MockResponse::text(StatusCode::OK, "should-not-send"),
    ]);
    client.set_max_auth_retries(1);

    let err = client
        .request(harness.endpoint(StatusCode::UNAUTHORIZED))
        .execute_decoded()
        .await
        .expect_err("auth retry budget should stop repeated refresh attempts with an auth error");

    assert!(err.to_string().contains("auth challenge rejected"));
    assert_eq!(harness.transport_attempts().await, 2);
    assert_eq!(harness.event_count("acquire").await, 2);
    assert_eq!(harness.event_count("invalidate:Unauthorized").await, 2);
    assert_eq!(harness.event_count("retry:Unauthorized").await, 2);
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
        .execute_decoded()
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
        .execute_decoded()
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
        .execute_decoded()
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
        .execute_decoded()
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
        .execute_decoded()
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
}

impl AuthHttpLimitVars {
    fn plain(max_body_bytes: usize) -> Self {
        Self {
            max_body_bytes,
            use_oauth: false,
        }
    }

    #[cfg(feature = "json")]
    fn oauth() -> Self {
        Self {
            max_body_bytes: AuthInternalPolicy::DEFAULT_MAX_BODY_BYTES,
            use_oauth: true,
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
                    let provider = OAuth2ClientCredentialsProvider::new(
                        CredentialId::new("test", "oauth"),
                        "https://auth.example.com/token".parse().expect("token url"),
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
                        body: None,
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
                identity: application.identity().clone(),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }
}

struct AuthHttpLimitEndpoint;

impl Endpoint<AuthHttpLimitCx> for AuthHttpLimitEndpoint {
    type Response = String;

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
            decode_string,
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
                identity: application.identity().clone(),
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

    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, RecordingAuthCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination.clone(),
            decode_string,
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
                identity: application.identity().clone(),
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

    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, PolicyAuthCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination.clone(),
            decode_string,
        ))
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
