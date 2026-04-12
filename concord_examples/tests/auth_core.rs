use concord_core::prelude::*;
use concord_test_support::*;
use http::header::{AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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
        let value = http::HeaderValue::from_str(&value).map_err(|_| {
            ApiClientError::InvalidParam {
                ctx: ctx.error_context(),
                param: "authorization formatted bearer token",
            }
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
