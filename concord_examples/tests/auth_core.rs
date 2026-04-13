use bytes::Bytes;
use concord_core::prelude::*;
use concord_test_support::*;
use http::header::{AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Clone)]
struct TestCx;

#[derive(Clone)]
struct TestAuthState {
    token: Arc<CredentialSlot<TestCx, CountingBearerProvider>>,
}

impl ClientContext for TestCx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = TestAuthState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
        TestAuthState {
            token: Arc::new(CredentialSlot::new(CountingBearerProvider {
                calls: Arc::new(AtomicUsize::new(0)),
            })),
        }
    }
}

#[derive(Clone)]
struct CountingBearerProvider {
    calls: Arc<AtomicUsize>,
}

impl CredentialProvider<TestCx> for CountingBearerProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("test", "token")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, TestCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            let token = if n == 0 { "bad" } else { "good" };
            Ok(AccessToken::new(token))
        })
    }
}

struct TestBearerAuth;

impl AuthPart<TestCx, Ping> for TestBearerAuth {
    type Ctrl = UseCredential<TestCx, CountingBearerProvider, BearerAuth>;

    fn controller(
        ctx: AuthBuildContext<'_, TestCx>,
        _ep: &Ping,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(UseCredential::new(ctx.auth_state.token.clone(), BearerAuth))
    }
}

struct Ping;

impl Endpoint<TestCx> for Ping {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = TestBearerAuth;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;
}

#[tokio::test]
async fn bearer_auth_applies_authorization_header() {
    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let api = ApiClient::<TestCx, _>::with_transport((), (), transport);

    api.request(Ping).execute().await.unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).header(AUTHORIZATION, "Bearer bad");

    h.finish();
}

#[tokio::test]
async fn response_401_invalidates_and_retries_once() {
    let first = MockReply::status(http::StatusCode::UNAUTHORIZED).with_header(
        WWW_AUTHENTICATE,
        http::HeaderValue::from_static("Bearer error=\"invalid_token\""),
    );
    let (transport, h) = mock()
        .replies([first, MockReply::ok_json(json_bytes(&()))])
        .build();
    let api = ApiClient::<TestCx, _>::with_transport((), (), transport);

    api.request(Ping).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0]).header(AUTHORIZATION, "Bearer bad");
    assert_request(&reqs[1]).header(AUTHORIZATION, "Bearer good");

    h.finish();
}

#[derive(Clone)]
struct OAuthCx;

#[derive(Clone)]
struct OAuthAuthState {
    token: Arc<CredentialSlot<OAuthCx, OAuth2ClientCredentialsProvider>>,
}

impl ClientContext for OAuthCx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = OAuthAuthState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
        OAuthAuthState {
            token: Arc::new(CredentialSlot::new(
                OAuth2ClientCredentialsProvider::new(
                    CredentialId::new("test", "oauth_token"),
                    "https://auth.example.com/token".parse().unwrap(),
                    "client",
                    "secret",
                )
                .scope("read"),
            )),
        }
    }
}

struct OAuthBearerAuth;

impl AuthPart<OAuthCx, OAuthPing> for OAuthBearerAuth {
    type Ctrl = UseCredential<OAuthCx, OAuth2ClientCredentialsProvider, BearerAuth>;

    fn controller(
        ctx: AuthBuildContext<'_, OAuthCx>,
        _ep: &OAuthPing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(UseCredential::new(ctx.auth_state.token.clone(), BearerAuth))
    }
}

struct OAuthPing;

impl Endpoint<OAuthCx> for OAuthPing {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = OAuthBearerAuth;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;
}

#[tokio::test]
async fn oauth2_client_credentials_acquires_token_with_internal_request() {
    let token_body = json_bytes(&serde_json::json!({
        "access_token": "oauth-token",
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": "read"
    }));
    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(token_body),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();
    let api = ApiClient::<OAuthCx, _>::with_transport((), (), transport);

    api.request(OAuthPing).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0])
        .header(AUTHORIZATION, "Basic Y2xpZW50OnNlY3JldA==")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .host("auth.example.com")
        .path("/token");
    assert_eq!(
        std::str::from_utf8(reqs[0].body.as_ref().unwrap()).unwrap(),
        "grant_type=client_credentials&scope=read"
    );
    assert_request(&reqs[1]).header(AUTHORIZATION, "Bearer oauth-token");

    h.finish();
}

#[derive(Clone, Copy)]
struct FormattingBearerAuth {
    prefix: &'static str,
}

impl FormattingBearerAuth {
    fn new(prefix: &'static str) -> Self {
        Self { prefix }
    }
}

impl<Cx, E> AuthUsage<Cx, E, AccessToken> for FormattingBearerAuth
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
{
    fn name(&self) -> AuthUsageId {
        AuthUsageId::new("formatting-bearer")
    }

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        material: &AccessToken,
    ) -> Result<AuthIdentity, ApiClientError> {
        let value = format!("Bearer {}{}", self.prefix, material.token.expose());
        let value =
            http::HeaderValue::from_str(&value).map_err(|_| ApiClientError::InvalidParam {
                ctx: ctx.error_context(),
                param: "authorization formatted bearer token",
            })?;
        ctx.request.headers.insert(AUTHORIZATION, value);
        Ok(material.safe_identity())
    }
}

struct FormattingBearerPart;

impl AuthPart<TestCx, FormattingPing> for FormattingBearerPart {
    type Ctrl = UseCredential<TestCx, CountingBearerProvider, FormattingBearerAuth>;

    fn controller(
        ctx: AuthBuildContext<'_, TestCx>,
        _ep: &FormattingPing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(UseCredential::new(
            ctx.auth_state.token.clone(),
            FormattingBearerAuth::new("tenant-a:"),
        ))
    }
}

struct FormattingPing;

impl Endpoint<TestCx> for FormattingPing {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = FormattingBearerPart;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;
}

#[tokio::test]
async fn custom_bearer_usage_can_format_token_before_applying_it() {
    let (transport, h) = mock().reply(MockReply::ok_json(json_bytes(&()))).build();
    let api = ApiClient::<TestCx, _>::with_transport((), (), transport);

    api.request(FormattingPing).execute().await.unwrap();

    let reqs = h.recorded();
    assert_request(&reqs[0]).header(AUTHORIZATION, "Bearer tenant-a:bad");

    h.finish();
}

#[derive(Clone)]
struct LoginCx;

#[derive(Clone)]
struct LoginSecrets {
    username: String,
    password: SecretString,
}

#[derive(Clone)]
struct LoginAuthState {
    session: Arc<CredentialSlot<LoginCx, LoginProvider>>,
}

impl ClientContext for LoginCx {
    type Vars = ();
    type AuthVars = LoginSecrets;
    type AuthState = LoginAuthState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
        LoginAuthState {
            session: Arc::new(CredentialSlot::new(LoginProvider)),
        }
    }
}

#[derive(Clone)]
struct LoginProvider;

#[derive(serde::Deserialize)]
struct LoginTokenResponse {
    access_token: String,
}

impl CredentialProvider<LoginCx> for LoginProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("test", "login-session")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, LoginCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let mut headers = http::HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                http::HeaderValue::from_static("application/x-www-form-urlencoded"),
            );

            let body = {
                let mut form = url::form_urlencoded::Serializer::new(String::new());
                form.append_pair("username", &ctx.auth.username);
                form.append_pair("password", ctx.auth.password.expose());
                form.finish()
            };

            let url = format!("{}://{}/login", LoginCx::SCHEME, LoginCx::DOMAIN)
                .parse()
                .expect("valid login url");

            let resp = ctx
                .executor
                .send(AuthHttpRequest {
                    method: http::Method::POST,
                    url,
                    headers,
                    body: Some(bytes::Bytes::from(body)),
                    mode: AuthMode::SkipAuth,
                    policy: AuthInternalPolicy::default(),
                })
                .await?;

            if !resp.status.is_success() {
                return Err(AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    format!("login returned {}", resp.status),
                ));
            }

            let token: LoginTokenResponse = serde_json::from_slice(&resp.body).map_err(|e| {
                AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    format!("login response decode failed: {e}"),
                )
            })?;

            Ok(AccessToken::new(token.access_token))
        })
    }
}

struct LoginBearerAuth;

impl AuthPart<LoginCx, LoginPing> for LoginBearerAuth {
    type Ctrl = UseCredential<LoginCx, LoginProvider, BearerAuth>;

    fn controller(
        ctx: AuthBuildContext<'_, LoginCx>,
        _ep: &LoginPing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(UseCredential::new(
            ctx.auth_state.session.clone(),
            BearerAuth::new(),
        ))
    }
}

struct LoginPing;

impl Endpoint<LoginCx> for LoginPing {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = LoginBearerAuth;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;
}

#[tokio::test]
async fn custom_provider_can_login_with_internal_request_against_current_api() {
    let token_body = json_bytes(&serde_json::json!({
        "access_token": "login-token"
    }));
    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(token_body),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();
    let api = ApiClient::<LoginCx, _>::with_transport(
        (),
        LoginSecrets {
            username: "alice".to_string(),
            password: SecretString::new("pw"),
        },
        transport,
    );

    api.request(LoginPing).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0])
        .host("example.com")
        .path("/login")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded");
    assert_eq!(
        std::str::from_utf8(reqs[0].body.as_ref().unwrap()).unwrap(),
        "username=alice&password=pw"
    );
    assert_request(&reqs[1]).header(AUTHORIZATION, "Bearer login-token");

    h.finish();
}

#[derive(Clone)]
struct UnitCx;

impl ClientContext for UnitCx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = NoAuthState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
        NoAuthState
    }
}

#[derive(Clone)]
struct YieldingProvider {
    calls: Arc<AtomicUsize>,
}

impl CredentialProvider<UnitCx> for YieldingProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("test", "yielding")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, UnitCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok(AccessToken::new("shared"))
        })
    }
}

struct DummyAuthHttpExecutor;

impl AuthHttpExecutor for DummyAuthHttpExecutor {
    fn send<'a>(
        &'a self,
        _req: AuthHttpRequest,
    ) -> AuthFuture<'a, Result<AuthHttpResponse, AuthError>> {
        Box::pin(async {
            Err(AuthError::new(
                AuthErrorKind::UnsupportedScheme,
                "dummy executor",
            ))
        })
    }
}

#[tokio::test]
async fn credential_slot_single_flight_acquires_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let slot = CredentialSlot::<UnitCx, _>::new(YieldingProvider {
        calls: calls.clone(),
    });
    let auth_state = NoAuthState;
    let executor = DummyAuthHttpExecutor;
    let ctx = CredentialContext {
        vars: &(),
        auth: &(),
        auth_state: &auth_state,
        executor: &executor,
        credential_id: CredentialId::new("test", "yielding"),
        reason: CredentialRefreshReason::Missing,
    };

    let (a, b, c) = tokio::join!(
        slot.get_or_refresh(ctx.clone(), AuthStepPolicy::default()),
        slot.get_or_refresh(ctx.clone(), AuthStepPolicy::default()),
        slot.get_or_refresh(ctx, AuthStepPolicy::default()),
    );

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(a.unwrap().generation, 1);
    assert_eq!(b.unwrap().generation, 1);
    assert_eq!(c.unwrap().generation, 1);
}

#[derive(Clone)]
struct BackoffProvider {
    calls: Arc<AtomicUsize>,
}

impl CredentialProvider<UnitCx> for BackoffProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("test", "backoff")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, UnitCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Err(AuthError::new(AuthErrorKind::AcquireFailed, "temporary")
                    .with_retry_after(Duration::from_millis(120)))
            } else {
                Ok(AccessToken::new("ok"))
            }
        })
    }
}

#[tokio::test]
async fn credential_slot_failed_state_honors_retry_after() {
    let calls = Arc::new(AtomicUsize::new(0));
    let slot = CredentialSlot::<UnitCx, _>::new(BackoffProvider {
        calls: calls.clone(),
    });
    let auth_state = NoAuthState;
    let executor = DummyAuthHttpExecutor;
    let ctx = CredentialContext {
        vars: &(),
        auth: &(),
        auth_state: &auth_state,
        executor: &executor,
        credential_id: CredentialId::new("test", "backoff"),
        reason: CredentialRefreshReason::Missing,
    };

    let first = slot
        .get_or_refresh(ctx.clone(), AuthStepPolicy::default())
        .await
        .expect_err("first acquire should fail");
    assert_eq!(first.kind, AuthErrorKind::AcquireFailed);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let second = slot
        .get_or_refresh(ctx.clone(), AuthStepPolicy::default())
        .await
        .expect_err("retry before retry_after should fail immediately");
    assert_eq!(second.kind, AuthErrorKind::AcquireFailed);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    tokio::time::sleep(Duration::from_millis(130)).await;
    let lease = slot
        .get_or_refresh(ctx, AuthStepPolicy::default())
        .await
        .expect("retry after backoff should succeed");
    assert_eq!(lease.generation, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[derive(Clone)]
struct AlwaysBadCx;

#[derive(Clone)]
struct AlwaysBadAuthState {
    token: Arc<CredentialSlot<AlwaysBadCx, AlwaysBadProvider>>,
}

impl ClientContext for AlwaysBadCx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = AlwaysBadAuthState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
        Self::AuthState {
            token: Arc::new(CredentialSlot::new(AlwaysBadProvider)),
        }
    }
}

#[derive(Clone)]
struct AlwaysBadProvider;

impl CredentialProvider<AlwaysBadCx> for AlwaysBadProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("test", "always-bad")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, AlwaysBadCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async { Ok(AccessToken::new("always-bad")) })
    }
}

struct AlwaysBadAuth;

impl AuthPart<AlwaysBadCx, AlwaysBadPing> for AlwaysBadAuth {
    type Ctrl = UseCredential<AlwaysBadCx, AlwaysBadProvider, BearerAuth>;

    fn controller(
        ctx: AuthBuildContext<'_, AlwaysBadCx>,
        _ep: &AlwaysBadPing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(
            UseCredential::new(ctx.auth_state.token.clone(), BearerAuth::new()).with_policy(
                AuthStepPolicy {
                    max_auth_retries: 8,
                    ..AuthStepPolicy::default()
                },
            ),
        )
    }
}

struct AlwaysBadPing;

impl Endpoint<AlwaysBadCx> for AlwaysBadPing {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = AlwaysBadAuth;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;
}

#[tokio::test]
async fn auth_retry_has_global_cap() {
    let unauthorized = MockReply::status(http::StatusCode::UNAUTHORIZED).with_header(
        WWW_AUTHENTICATE,
        http::HeaderValue::from_static("Bearer error=\"invalid_token\""),
    );
    let (transport, h) = mock()
        .replies([unauthorized.clone(), unauthorized.clone()])
        .build();
    let mut api = ApiClient::<AlwaysBadCx, _>::with_transport((), (), transport);
    api.set_max_auth_retries(1);

    let err = api
        .request(AlwaysBadPing)
        .execute()
        .await
        .expect_err("request should stop at global auth retry cap and return status error");
    match err {
        ApiClientError::HttpStatus { status, .. } => {
            assert_eq!(status, http::StatusCode::UNAUTHORIZED)
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2, "global cap=1 means at most 2 attempts");
    h.finish();
}

#[derive(Clone)]
struct UseAuthCtx;

#[derive(Clone)]
struct UseAuthState {
    upstream: Arc<CredentialSlot<UseAuthCtx, StaticBearerProvider>>,
    session: Arc<CredentialSlot<UseAuthCtx, UseAuthLoginProvider>>,
}

const USE_AUTH_UPSTREAM: AuthRequirementId = AuthRequirementId::new("use-auth", "upstream");

impl ClientContext for UseAuthCtx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = UseAuthState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
        Self::AuthState {
            upstream: Arc::new(CredentialSlot::new(StaticBearerProvider::new(
                CredentialId::new("use-auth", "upstream"),
                AccessToken::new("seed"),
            ))),
            session: Arc::new(CredentialSlot::new(UseAuthLoginProvider)),
        }
    }

    fn apply_internal_auth<'a>(
        requirement: &'a AuthRequirementId,
        request: &'a mut concord_core::transport::BuiltRequest,
        vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        auth_state: &'a Self::AuthState,
        executor: &'a dyn AuthHttpExecutor,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move {
            if requirement != &USE_AUTH_UPSTREAM {
                return Err(AuthError::new(
                    AuthErrorKind::UnsupportedScheme,
                    format!("unknown internal auth requirement `{requirement}`"),
                ));
            }
            let ctx = CredentialContext {
                vars,
                auth,
                auth_state,
                executor,
                credential_id: CredentialId::new("use-auth", "upstream"),
                reason: CredentialRefreshReason::Missing,
            };
            let lease = auth_state
                .upstream
                .get_or_refresh(ctx, AuthStepPolicy::default())
                .await?;
            let value = format!("Bearer {}", lease.value.token.expose());
            let header = http::HeaderValue::from_str(&value).map_err(|_| {
                AuthError::new(
                    AuthErrorKind::InvalidConfiguration,
                    "invalid upstream token",
                )
            })?;
            request.headers.insert(AUTHORIZATION, header);
            Ok(())
        })
    }
}

#[derive(Clone)]
struct UseAuthLoginProvider;

#[derive(serde::Deserialize)]
struct UseAuthTokenBody {
    access_token: String,
}

impl CredentialProvider<UseAuthCtx> for UseAuthLoginProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("use-auth", "session")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, UseAuthCtx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let url = "https://example.com/login".parse().expect("valid url");
            let resp = ctx
                .executor
                .send(AuthHttpRequest {
                    method: http::Method::POST,
                    url,
                    headers: http::HeaderMap::new(),
                    body: None,
                    mode: AuthMode::UseAuth(USE_AUTH_UPSTREAM.clone()),
                    policy: AuthInternalPolicy::default(),
                })
                .await?;
            if !resp.status.is_success() {
                return Err(AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    format!("login returned {}", resp.status),
                ));
            }
            let parsed: UseAuthTokenBody = serde_json::from_slice(&resp.body).map_err(|e| {
                AuthError::new(AuthErrorKind::AcquireFailed, format!("decode failed: {e}"))
            })?;
            Ok(AccessToken::new(parsed.access_token))
        })
    }
}

struct UseAuthEndpointAuth;

impl AuthPart<UseAuthCtx, UseAuthPing> for UseAuthEndpointAuth {
    type Ctrl = UseCredential<UseAuthCtx, UseAuthLoginProvider, BearerAuth>;

    fn controller(
        ctx: AuthBuildContext<'_, UseAuthCtx>,
        _ep: &UseAuthPing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(UseCredential::new(
            ctx.auth_state.session.clone(),
            BearerAuth::new(),
        ))
    }
}

struct UseAuthPing;

impl Endpoint<UseAuthCtx> for UseAuthPing {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = UseAuthEndpointAuth;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;
}

#[tokio::test]
async fn internal_auth_useauth_applies_requirement() {
    let token_body = json_bytes(&serde_json::json!({
        "access_token": "login-issued"
    }));
    let (transport, h) = mock()
        .replies([
            MockReply::ok_json(token_body),
            MockReply::ok_json(json_bytes(&())),
        ])
        .build();
    let api = ApiClient::<UseAuthCtx, _>::with_transport((), (), transport);

    api.request(UseAuthPing).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0])
        .path("/login")
        .header(AUTHORIZATION, "Bearer seed");
    assert_request(&reqs[1]).header(AUTHORIZATION, "Bearer login-issued");
    h.finish();
}

#[derive(Clone)]
struct RecursiveCx;

#[derive(Clone)]
struct RecursiveState {
    token: Arc<CredentialSlot<RecursiveCx, RecursiveProvider>>,
}

impl ClientContext for RecursiveCx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = RecursiveState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
        RecursiveState {
            token: Arc::new(CredentialSlot::new(RecursiveProvider)),
        }
    }

    fn apply_internal_auth<'a>(
        requirement: &'a AuthRequirementId,
        _request: &'a mut concord_core::transport::BuiltRequest,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        executor: &'a dyn AuthHttpExecutor,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move {
            executor
                .send(AuthHttpRequest {
                    method: http::Method::GET,
                    url: "https://example.com/nested".parse().expect("valid url"),
                    headers: http::HeaderMap::new(),
                    body: None,
                    mode: AuthMode::UseAuth(requirement.clone()),
                    policy: AuthInternalPolicy::default(),
                })
                .await?;
            Ok(())
        })
    }
}

#[derive(Clone)]
struct RecursiveProvider;

impl CredentialProvider<RecursiveCx> for RecursiveProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("recursive", "token")
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, RecursiveCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let _ = ctx
                .executor
                .send(AuthHttpRequest {
                    method: http::Method::POST,
                    url: "https://example.com/login".parse().expect("valid url"),
                    headers: http::HeaderMap::new(),
                    body: None,
                    mode: AuthMode::UseAuth(AuthRequirementId::new("recursive", "loop")),
                    policy: AuthInternalPolicy::default(),
                })
                .await?;
            Ok(AccessToken::new("never"))
        })
    }
}

struct RecursiveAuthPart;

impl AuthPart<RecursiveCx, RecursivePing> for RecursiveAuthPart {
    type Ctrl = UseCredential<RecursiveCx, RecursiveProvider, BearerAuth>;

    fn controller(
        ctx: AuthBuildContext<'_, RecursiveCx>,
        _ep: &RecursivePing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(UseCredential::new(
            ctx.auth_state.token.clone(),
            BearerAuth::new(),
        ))
    }
}

struct RecursivePing;

impl Endpoint<RecursiveCx> for RecursivePing {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = RecursiveAuthPart;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;
}

#[tokio::test]
async fn internal_auth_recursion_is_detected() {
    let (transport, h) = mock().build();
    let api = ApiClient::<RecursiveCx, _>::with_transport((), (), transport);
    let err = api
        .request(RecursivePing)
        .execute()
        .await
        .expect_err("recursive internal auth must fail");
    match err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::RecursionDetected);
        }
        other => panic!("unexpected error: {other:?}"),
    }
    h.finish();
}

#[derive(Clone)]
struct CertificateCx;

#[derive(Clone)]
struct CertificateState {
    cert: Arc<CredentialSlot<CertificateCx, StaticCertificateProvider>>,
}

impl ClientContext for CertificateCx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = CertificateState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
        CertificateState {
            cert: Arc::new(CredentialSlot::new(StaticCertificateProvider)),
        }
    }
}

#[derive(Clone)]
struct StaticCertificateProvider;

impl CredentialProvider<CertificateCx> for StaticCertificateProvider {
    type Credential = ClientCertificate;

    fn id(&self) -> CredentialId {
        CredentialId::new("certificate", "client")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, CertificateCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async { Ok(ClientCertificate::new("mtls-identity")) })
    }
}

struct CertificateAuthPart;

impl AuthPart<CertificateCx, CertificatePing> for CertificateAuthPart {
    type Ctrl = UseCredential<CertificateCx, StaticCertificateProvider, CertificateAuth>;

    fn controller(
        ctx: AuthBuildContext<'_, CertificateCx>,
        _ep: &CertificatePing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(UseCredential::new(
            ctx.auth_state.cert.clone(),
            CertificateAuth::new(),
        ))
    }
}

struct CertificatePing;

impl Endpoint<CertificateCx> for CertificatePing {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = CertificateAuthPart;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;
}

#[derive(Clone, Default)]
struct CapturingTransport {
    seen_transport_auth: Arc<Mutex<Vec<Option<TransportAuth>>>>,
}

struct StaticBody {
    chunk: Option<Bytes>,
}

impl concord_core::transport::TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Option<Bytes>, concord_core::transport::TransportError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move { Ok(self.chunk.take()) })
    }
}

impl concord_core::prelude::Transport for CapturingTransport {
    fn send(
        &self,
        req: concord_core::transport::BuiltRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        concord_core::transport::TransportResponse,
                        concord_core::transport::TransportError,
                    >,
                > + Send,
        >,
    > {
        let seen = self.seen_transport_auth.clone();
        Box::pin(async move {
            seen.lock()
                .expect("capture lock")
                .push(req.extensions.transport_auth.clone());
            let body = json_bytes(&());
            Ok(concord_core::transport::TransportResponse {
                meta: req.meta,
                url: req.url,
                status: http::StatusCode::OK,
                headers: {
                    let mut h = http::HeaderMap::new();
                    h.insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/json"),
                    );
                    h
                },
                content_length: Some(body.len() as u64),
                body: Box::new(StaticBody { chunk: Some(body) }),
            })
        })
    }
}

#[tokio::test]
async fn certificate_auth_reaches_transport_extension() {
    let transport = CapturingTransport::default();
    let seen = transport.seen_transport_auth.clone();
    let api = ApiClient::<CertificateCx, _>::with_transport((), (), transport);

    api.request(CertificatePing).execute().await.unwrap();

    let captured = seen.lock().expect("capture lock").clone();
    assert_eq!(captured.len(), 1);
    match captured[0].clone() {
        Some(TransportAuth::ClientCertificate { identity_id }) => {
            assert_eq!(identity_id, "mtls-identity");
        }
        other => panic!("expected client-certificate transport auth, got {other:?}"),
    }
}

#[derive(Clone)]
struct OneOfCtx;

#[derive(Clone)]
struct OneOfState {
    bearer: Arc<CredentialSlot<OneOfCtx, StaticBearerProvider>>,
    fallback: Arc<CredentialSlot<OneOfCtx, StaticApiKeyProvider>>,
}

impl ClientContext for OneOfCtx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = OneOfState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
        Self::AuthState {
            bearer: Arc::new(CredentialSlot::new(StaticBearerProvider::new(
                CredentialId::new("one-of", "bearer"),
                AccessToken::new("bad-token"),
            ))),
            fallback: Arc::new(CredentialSlot::new(StaticApiKeyProvider::new(
                CredentialId::new("one-of", "fallback"),
                ApiKey::new("fallback-key"),
            ))),
        }
    }
}

struct OneOfBearerPart;
struct OneOfHeaderPart;

impl AuthPart<OneOfCtx, OneOfPing> for OneOfBearerPart {
    type Ctrl = UseCredential<OneOfCtx, StaticBearerProvider, BearerAuth>;

    fn controller(
        ctx: AuthBuildContext<'_, OneOfCtx>,
        _ep: &OneOfPing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(UseCredential::new(
            ctx.auth_state.bearer.clone(),
            BearerAuth::new(),
        ))
    }
}

impl AuthPart<OneOfCtx, OneOfPing> for OneOfHeaderPart {
    type Ctrl = UseCredential<OneOfCtx, StaticApiKeyProvider, HeaderAuth>;

    fn controller(
        ctx: AuthBuildContext<'_, OneOfCtx>,
        _ep: &OneOfPing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(UseCredential::new(
            ctx.auth_state.fallback.clone(),
            HeaderAuth::from_static("x-fallback-key"),
        ))
    }
}

struct OneOfPing;

impl Endpoint<OneOfCtx> for OneOfPing {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = concord_core::internal::OneOfAuth<OneOfBearerPart, OneOfHeaderPart>;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;
}

#[tokio::test]
async fn one_of_auth_falls_back_to_next_branch_after_rejection() {
    let unauthorized = MockReply::status(http::StatusCode::UNAUTHORIZED).with_header(
        WWW_AUTHENTICATE,
        http::HeaderValue::from_static("Bearer error=\"invalid_token\""),
    );
    let (transport, h) = mock()
        .replies([unauthorized, MockReply::ok_json(json_bytes(&()))])
        .build();
    let api = ApiClient::<OneOfCtx, _>::with_transport((), (), transport);

    api.request(OneOfPing).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0])
        .header(AUTHORIZATION, "Bearer bad-token")
        .header_absent("x-fallback-key");
    assert_request(&reqs[1])
        .header("x-fallback-key", "fallback-key")
        .header_absent(AUTHORIZATION);
    h.finish();
}

#[derive(Clone)]
struct DuplicateUsageCx;

#[derive(Clone)]
struct DuplicateUsageState {
    token: Arc<CredentialSlot<DuplicateUsageCx, RotatingTokenProvider>>,
}

impl ClientContext for DuplicateUsageCx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = DuplicateUsageState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState {
        DuplicateUsageState {
            token: Arc::new(CredentialSlot::new(RotatingTokenProvider {
                calls: Arc::new(AtomicUsize::new(0)),
            })),
        }
    }
}

#[derive(Clone)]
struct RotatingTokenProvider {
    calls: Arc<AtomicUsize>,
}

impl CredentialProvider<DuplicateUsageCx> for RotatingTokenProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        CredentialId::new("dup", "token")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, DuplicateUsageCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            let token = if n == 0 { "bad" } else { "good" };
            Ok(AccessToken::new(token))
        })
    }
}

#[derive(Clone, Copy)]
struct DuplicateUsage {
    marker: &'static str,
}

impl AuthUsage<DuplicateUsageCx, DuplicateUsagePing, AccessToken> for DuplicateUsage {
    fn name(&self) -> AuthUsageId {
        AuthUsageId::new("dup-usage")
    }

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, DuplicateUsageCx, DuplicateUsagePing>,
        material: &AccessToken,
    ) -> Result<AuthIdentity, ApiClientError> {
        let value = format!("Bearer {}:{}", self.marker, material.token.expose());
        let value =
            http::HeaderValue::from_str(&value).map_err(|_| ApiClientError::InvalidParam {
                ctx: ctx.error_context(),
                param: "duplicate usage authorization",
            })?;
        ctx.request.headers.insert(AUTHORIZATION, value);
        Ok(AuthIdentity::Static(self.marker))
    }

    fn challenge(
        &self,
        ctx: AuthChallengeContext<'_, DuplicateUsageCx, DuplicateUsagePing>,
    ) -> AuthChallengeDecision {
        if ctx.status != http::StatusCode::UNAUTHORIZED {
            return AuthChallengeDecision::Ignore;
        }
        if ctx.applied.identity == AuthIdentity::Static("b") {
            AuthChallengeDecision::RejectCredential
        } else {
            AuthChallengeDecision::Ignore
        }
    }
}

struct DuplicatePartA;
struct DuplicatePartB;

impl AuthPart<DuplicateUsageCx, DuplicateUsagePing> for DuplicatePartA {
    type Ctrl = UseCredential<DuplicateUsageCx, RotatingTokenProvider, DuplicateUsage>;

    fn controller(
        ctx: AuthBuildContext<'_, DuplicateUsageCx>,
        _ep: &DuplicateUsagePing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(
            UseCredential::new(ctx.auth_state.token.clone(), DuplicateUsage { marker: "a" })
                .with_step_id("dup-step-a"),
        )
    }
}

impl AuthPart<DuplicateUsageCx, DuplicateUsagePing> for DuplicatePartB {
    type Ctrl = UseCredential<DuplicateUsageCx, RotatingTokenProvider, DuplicateUsage>;

    fn controller(
        ctx: AuthBuildContext<'_, DuplicateUsageCx>,
        _ep: &DuplicateUsagePing,
    ) -> Result<Self::Ctrl, ApiClientError> {
        Ok(
            UseCredential::new(ctx.auth_state.token.clone(), DuplicateUsage { marker: "b" })
                .with_step_id("dup-step-b"),
        )
    }
}

struct DuplicateUsagePing;

impl Endpoint<DuplicateUsageCx> for DuplicateUsagePing {
    const METHOD: http::Method = http::Method::GET;

    type Route = concord_core::internal::NoRoute;
    type Policy = concord_core::internal::NoPolicy;
    type Auth = concord_core::internal::AuthChain<DuplicatePartA, DuplicatePartB>;
    type Pagination = concord_core::internal::NoPagination;
    type Body = concord_core::internal::NoBody;
    type Response = concord_core::internal::Decoded<Json, ()>;
}

#[tokio::test]
async fn duplicate_usage_ids_do_not_misattribute_applied_part() {
    let first = MockReply::status(http::StatusCode::UNAUTHORIZED);
    let (transport, h) = mock()
        .replies([first, MockReply::ok_json(json_bytes(&()))])
        .build();
    let api = ApiClient::<DuplicateUsageCx, _>::with_transport((), (), transport);

    api.request(DuplicateUsagePing).execute().await.unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_request(&reqs[0]).header(AUTHORIZATION, "Bearer b:bad");
    assert_request(&reqs[1]).header(AUTHORIZATION, "Bearer b:good");
    h.finish();
}
