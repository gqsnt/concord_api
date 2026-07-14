use super::native_harness::native_mock;
use concord_core::advanced::{
    AuthChallengeMode, AuthChallengePolicy, AuthFuture, AuthPreparationMode, AuthProviderBinding,
    CredentialContext, CredentialId, CredentialProvider, CredentialProviderState, InvalidateReason,
    PreparedBody, PreparedEndpoint, PreparedRequestEntity, RequestAuthentication, SafeProxy,
};
use concord_core::prelude::{ApiClient, ApiKey, ClientContext, RetryMode, Text};
use http::{HeaderValue, Method, StatusCode};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone)]
pub(super) struct PublicContext;

#[derive(Clone)]
pub(super) struct PublicAuthVars {
    provider: PublicProvider,
}

#[derive(Clone)]
pub(super) struct PublicAuthState {
    provider: Arc<CredentialProviderState<PublicContext, PublicProvider>>,
}

impl ClientContext for PublicContext {
    type Vars = ();
    type AuthVars = PublicAuthVars;
    type AuthState = PublicAuthState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "example.com";
    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        PublicAuthState {
            provider: Arc::new(CredentialProviderState::new(auth.provider.clone())),
        }
    }

    fn auth_provider_binding<'a>(
        credential: &CredentialId,
        auth_state: &'a Self::AuthState,
    ) -> Option<AuthProviderBinding<'a, Self>> {
        (credential == &CredentialId::new("test", "token")).then(|| {
            auth_state.provider.secret_binding(
                AuthPreparationMode::RequestLocal,
                AuthChallengeMode::Refresh,
            )
        })
    }
}

#[derive(Clone)]
struct PublicProvider {
    acquired: Arc<AtomicUsize>,
    invalidated: Arc<AtomicUsize>,
}

impl CredentialProvider<PublicContext> for PublicProvider {
    type Credential = ApiKey;

    fn id(&self) -> CredentialId {
        CredentialId::new("test", "token")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, PublicContext>,
    ) -> AuthFuture<'a, Result<Self::Credential, concord_core::advanced::AuthError>> {
        Box::pin(async move {
            let generation = self.acquired.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(ApiKey::new(format!("public-token-{generation}")))
        })
    }

    fn invalidate<'a>(
        &'a self,
        _ctx: CredentialContext<'a, PublicContext>,
        _current: Option<&'a Self::Credential>,
        _reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), concord_core::advanced::AuthError>> {
        Box::pin(async move {
            self.invalidated.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

#[test]
fn hand_written_context_rejects_status_retry_mode() {
    let auth = PublicAuthVars {
        provider: PublicProvider {
            acquired: Arc::new(AtomicUsize::new(0)),
            invalidated: Arc::new(AtomicUsize::new(0)),
        },
    };
    let status = RetryMode::status(1, [StatusCode::SERVICE_UNAVAILABLE]).unwrap();
    let error = match ApiClient::<PublicContext>::with_retry_mode((), auth, status) {
        Ok(_) => panic!("hand-written contexts cannot install status retry"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        concord_core::prelude::RetryModeError::NotFixedOrigin
    ));

    let auth = PublicAuthVars {
        provider: PublicProvider {
            acquired: Arc::new(AtomicUsize::new(0)),
            invalidated: Arc::new(AtomicUsize::new(0)),
        },
    };
    ApiClient::<PublicContext>::with_retry_mode((), auth.clone(), RetryMode::ProtocolRecovery)
        .expect("hand-written contexts retain protocol recovery");
    ApiClient::<PublicContext>::with_retry_mode((), auth, RetryMode::Disabled)
        .expect("hand-written contexts retain disabled mode");
}

#[tokio::test]
async fn authenticated_fixed_origin_context_uses_only_supported_public_modules() {
    let (server, handle) = native_mock::mock()
        .replies([
            native_mock::MockReply::status(StatusCode::UNAUTHORIZED),
            native_mock::MockReply::ok_text(bytes::Bytes::from_static(b"authenticated")),
        ])
        .build();
    let acquired = Arc::new(AtomicUsize::new(0));
    let invalidated = Arc::new(AtomicUsize::new(0));
    let auth = PublicAuthVars {
        provider: PublicProvider {
            acquired: acquired.clone(),
            invalidated: invalidated.clone(),
        },
    };
    let proxy = SafeProxy::all(server.base_url().as_str()).expect("loopback proxy");
    let client = ApiClient::<PublicContext>::with_safe_reqwest_builder_and_retry_mode(
        (),
        auth,
        RetryMode::ProtocolRecovery,
        |builder| Ok(builder.proxy(proxy)),
    )
    .expect("public fixed-origin context");
    let value = PreparedEndpoint::<Text<String>>::new(
        "PublicContextRequest",
        Method::GET,
        "/auth",
        PreparedRequestEntity {
            body: PreparedBody::empty(),
        },
    )
    .authentication(RequestAuthentication::bearer(CredentialId::new(
        "test", "token",
    )))
    .execute(&client)
    .await
    .expect("one challenge is recovered");
    assert_eq!(value, "authenticated");
    assert_eq!(acquired.load(Ordering::SeqCst), 2);
    assert_eq!(invalidated.load(Ordering::SeqCst), 1);

    let requests = handle.recorded();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].headers.get(http::header::AUTHORIZATION),
        Some(&HeaderValue::from_static("Bearer public-token-1"))
    );
    assert_eq!(
        requests[1].headers.get(http::header::AUTHORIZATION),
        Some(&HeaderValue::from_static("Bearer public-token-2"))
    );
}

#[tokio::test]
async fn prepared_endpoint_explicit_policy_recovers_forbidden_once() {
    let (server, handle) = native_mock::mock()
        .replies([
            native_mock::MockReply::status(StatusCode::FORBIDDEN),
            native_mock::MockReply::ok_text(bytes::Bytes::from_static(b"recovered")),
        ])
        .build();
    let acquired = Arc::new(AtomicUsize::new(0));
    let invalidated = Arc::new(AtomicUsize::new(0));
    let auth = PublicAuthVars {
        provider: PublicProvider {
            acquired: acquired.clone(),
            invalidated: invalidated.clone(),
        },
    };
    let proxy = SafeProxy::all(server.base_url().as_str()).expect("loopback proxy");
    let client = ApiClient::<PublicContext>::with_safe_reqwest_builder_and_retry_mode(
        (),
        auth,
        RetryMode::ProtocolRecovery,
        |builder| Ok(builder.proxy(proxy)),
    )
    .expect("client");

    let value = PreparedEndpoint::<Text<String>>::new(
        "ExplicitForbidden",
        Method::GET,
        "/forbidden",
        PreparedRequestEntity {
            body: PreparedBody::empty(),
        },
    )
    .authentication(
        RequestAuthentication::bearer(CredentialId::new("test", "token"))
            .challenge_policy(AuthChallengePolicy::UnauthorizedOrForbidden),
    )
    .execute(&client)
    .await
    .expect("explicit forbidden recovery");

    assert_eq!(value, "recovered");
    assert_eq!(acquired.load(Ordering::SeqCst), 2);
    assert_eq!(invalidated.load(Ordering::SeqCst), 1);
    assert_eq!(handle.recorded().len(), 2);
}

#[tokio::test]
async fn prepared_endpoint_default_policy_keeps_forbidden_terminal() {
    let (server, handle) = native_mock::mock()
        .replies([native_mock::MockReply::status(StatusCode::FORBIDDEN)])
        .build();
    let acquired = Arc::new(AtomicUsize::new(0));
    let invalidated = Arc::new(AtomicUsize::new(0));
    let auth = PublicAuthVars {
        provider: PublicProvider {
            acquired: acquired.clone(),
            invalidated: invalidated.clone(),
        },
    };
    let proxy = SafeProxy::all(server.base_url().as_str()).expect("loopback proxy");
    let client = ApiClient::<PublicContext>::with_safe_reqwest_builder_and_retry_mode(
        (),
        auth,
        RetryMode::ProtocolRecovery,
        |builder| Ok(builder.proxy(proxy)),
    )
    .expect("client");

    let error = PreparedEndpoint::<Text<String>>::new(
        "DefaultForbidden",
        Method::GET,
        "/forbidden",
        PreparedRequestEntity {
            body: PreparedBody::empty(),
        },
    )
    .authentication(RequestAuthentication::bearer(CredentialId::new(
        "test", "token",
    )))
    .execute(&client)
    .await
    .expect_err("default 403 is terminal");

    assert!(matches!(
        error,
        concord_core::prelude::ApiClientError::Auth { .. }
    ));
    assert_eq!(acquired.load(Ordering::SeqCst), 1);
    assert_eq!(invalidated.load(Ordering::SeqCst), 0);
    assert_eq!(handle.recorded().len(), 1);
}
