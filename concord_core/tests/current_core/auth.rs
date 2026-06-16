use super::common::*;
use concord_core::advanced::{
    AuthAppliedCredential, AuthDecision, AuthError, AuthErrorKind, AuthIdentity, AuthPlacement,
    AuthRequirement, BuiltRequest, RequestMeta,
};
use concord_core::prelude::{ApiClientError, ClientContext, Endpoint};
use http::{HeaderMap, HeaderValue, StatusCode};
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
    configure_runtime(&mut client, Some(cache), None, false, None);
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
        request: &'a mut BuiltRequest,
        _vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> concord_core::advanced::AuthFuture<'a, Result<AuthAppliedCredential, AuthError>> {
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
            match requirement.placement {
                AuthPlacement::Bearer => {
                    request.headers.insert(
                        http::header::AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| {
                            AuthError::new(
                                AuthErrorKind::UnsupportedScheme,
                                "invalid bearer token for authorization header",
                            )
                        })?,
                    );
                }
                AuthPlacement::Header(name) => {
                    request.headers.insert(
                        http::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                            AuthError::new(AuthErrorKind::UnsupportedScheme, "invalid header name")
                        })?,
                        HeaderValue::from_str(token).map_err(|_| {
                            AuthError::new(AuthErrorKind::UnsupportedScheme, "invalid header value")
                        })?,
                    );
                }
                AuthPlacement::Query(name) => {
                    request.url.query_pairs_mut().append_pair(name, token);
                }
                AuthPlacement::Basic | AuthPlacement::Certificate => {
                    return Err(AuthError::new(
                        AuthErrorKind::UnsupportedScheme,
                        "test context supports bearer/header/query auth only",
                    ));
                }
            }
            Ok(AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(1),
                identity: AuthIdentity::User(auth.identity.to_string()),
                provenance: requirement.provenance.clone(),
            })
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
            if status == StatusCode::UNAUTHORIZED {
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
