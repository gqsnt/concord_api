use crate::client::ClientContext;
use crate::endpoint::Endpoint;
use crate::error::{ApiClientError, ErrorContext};
use crate::secret::SecretString;
use crate::transport::{BuiltRequest, RequestMeta};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use bytes::Bytes;
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue};
use http::{HeaderMap, Method, StatusCode};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{Mutex, Notify};
use url::Url;

#[cfg(feature = "json")]
use serde::Deserialize;

pub type AuthFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Default)]
pub struct NoAuthState;

pub struct AuthBuildContext<'a, Cx: ClientContext> {
    pub vars: &'a Cx::Vars,
    pub auth: &'a Cx::AuthVars,
    pub auth_state: &'a Cx::AuthState,
}

impl<Cx: ClientContext> Copy for AuthBuildContext<'_, Cx> {}

impl<Cx: ClientContext> Clone for AuthBuildContext<'_, Cx> {
    fn clone(&self) -> Self {
        *self
    }
}

pub struct AuthPrepareContext<'a, Cx: ClientContext, E: Endpoint<Cx>> {
    pub ep: &'a E,
    pub vars: &'a Cx::Vars,
    pub auth: &'a Cx::AuthVars,
    pub auth_state: &'a Cx::AuthState,
    pub executor: &'a dyn AuthHttpExecutor,
    pub meta: &'a RequestMeta,
    pub request: &'a mut BuiltRequest,
}

impl<Cx: ClientContext, E: Endpoint<Cx>> AuthPrepareContext<'_, Cx, E> {
    #[inline]
    pub fn error_context(&self) -> ErrorContext {
        ErrorContext {
            endpoint: self.meta.endpoint,
            method: self.meta.method.clone(),
        }
    }
}

pub struct AuthResponseContext<'a, Cx: ClientContext, E: Endpoint<Cx>> {
    pub ep: &'a E,
    pub vars: &'a Cx::Vars,
    pub auth: &'a Cx::AuthVars,
    pub auth_state: &'a Cx::AuthState,
    pub executor: &'a dyn AuthHttpExecutor,
    pub meta: &'a RequestMeta,
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
    pub attempt: &'a AuthAttempt,
}

impl<Cx: ClientContext, E: Endpoint<Cx>> AuthResponseContext<'_, Cx, E> {
    #[inline]
    pub fn error_context(&self) -> ErrorContext {
        ErrorContext {
            endpoint: self.meta.endpoint,
            method: self.meta.method.clone(),
        }
    }
}

pub trait AuthPart<Cx: ClientContext, E: Endpoint<Cx>>: Send + Sync + 'static {
    type Ctrl: AuthController<Cx, E>;

    fn controller(ctx: AuthBuildContext<'_, Cx>, ep: &E) -> Result<Self::Ctrl, ApiClientError>;
}

pub trait AuthController<Cx: ClientContext, E: Endpoint<Cx>>: Send + Sync + 'static {
    type State: Send + Sync + 'static;

    fn init(&self, ep: &E) -> Result<Self::State, ApiClientError>;

    fn prepare<'a>(
        &'a self,
        state: &'a mut Self::State,
        ctx: AuthPrepareContext<'a, Cx, E>,
    ) -> AuthFuture<'a, Result<AuthAttempt, ApiClientError>>;

    fn on_response<'a>(
        &'a self,
        state: &'a mut Self::State,
        ctx: AuthResponseContext<'a, Cx, E>,
    ) -> AuthFuture<'a, Result<AuthResponseAction, ApiClientError>>;
}

pub struct NoAuth;
pub struct NoAuthController;

impl<Cx: ClientContext, E: Endpoint<Cx>> AuthPart<Cx, E> for NoAuth {
    type Ctrl = NoAuthController;

    fn controller(_ctx: AuthBuildContext<'_, Cx>, _ep: &E) -> Result<Self::Ctrl, ApiClientError> {
        Ok(NoAuthController)
    }
}

impl<Cx: ClientContext, E: Endpoint<Cx>> AuthController<Cx, E> for NoAuthController {
    type State = ();

    fn init(&self, _ep: &E) -> Result<Self::State, ApiClientError> {
        Ok(())
    }

    fn prepare<'a>(
        &'a self,
        _state: &'a mut Self::State,
        _ctx: AuthPrepareContext<'a, Cx, E>,
    ) -> AuthFuture<'a, Result<AuthAttempt, ApiClientError>> {
        Box::pin(async { Ok(AuthAttempt::default()) })
    }

    fn on_response<'a>(
        &'a self,
        _state: &'a mut Self::State,
        _ctx: AuthResponseContext<'a, Cx, E>,
    ) -> AuthFuture<'a, Result<AuthResponseAction, ApiClientError>> {
        Box::pin(async { Ok(AuthResponseAction::Continue) })
    }
}

pub struct AuthChain<A, B>(PhantomData<(A, B)>);

impl<A, B> Default for AuthChain<A, B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A, B> AuthChain<A, B> {
    #[inline]
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

pub struct AuthChainController<A, B> {
    a: A,
    b: B,
}

impl<Cx, E, A, B> AuthPart<Cx, E> for AuthChain<A, B>
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    A: AuthPart<Cx, E>,
    B: AuthPart<Cx, E>,
{
    type Ctrl = AuthChainController<A::Ctrl, B::Ctrl>;

    fn controller(ctx: AuthBuildContext<'_, Cx>, ep: &E) -> Result<Self::Ctrl, ApiClientError> {
        Ok(AuthChainController {
            a: A::controller(ctx, ep)?,
            b: B::controller(ctx, ep)?,
        })
    }
}

impl<Cx, E, A, B> AuthController<Cx, E> for AuthChainController<A, B>
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    A: AuthController<Cx, E>,
    B: AuthController<Cx, E>,
{
    type State = (A::State, B::State);

    fn init(&self, ep: &E) -> Result<Self::State, ApiClientError> {
        Ok((self.a.init(ep)?, self.b.init(ep)?))
    }

    fn prepare<'a>(
        &'a self,
        state: &'a mut Self::State,
        ctx: AuthPrepareContext<'a, Cx, E>,
    ) -> AuthFuture<'a, Result<AuthAttempt, ApiClientError>> {
        Box::pin(async move {
            let AuthPrepareContext {
                ep,
                vars,
                auth,
                auth_state,
                executor,
                meta,
                request,
            } = ctx;

            let mut out = self
                .a
                .prepare(
                    &mut state.0,
                    AuthPrepareContext {
                        ep,
                        vars,
                        auth,
                        auth_state,
                        executor,
                        meta,
                        request,
                    },
                )
                .await?;
            let b = self
                .b
                .prepare(
                    &mut state.1,
                    AuthPrepareContext {
                        ep,
                        vars,
                        auth,
                        auth_state,
                        executor,
                        meta,
                        request,
                    },
                )
                .await?;
            out.merge(b);
            Ok(out)
        })
    }

    fn on_response<'a>(
        &'a self,
        state: &'a mut Self::State,
        ctx: AuthResponseContext<'a, Cx, E>,
    ) -> AuthFuture<'a, Result<AuthResponseAction, ApiClientError>> {
        Box::pin(async move {
            let AuthResponseContext {
                ep,
                vars,
                auth,
                auth_state,
                executor,
                meta,
                status,
                headers,
                attempt,
            } = ctx;
            let a = self
                .a
                .on_response(
                    &mut state.0,
                    AuthResponseContext {
                        ep,
                        vars,
                        auth,
                        auth_state,
                        executor,
                        meta,
                        status,
                        headers,
                        attempt,
                    },
                )
                .await?;
            let b = self
                .b
                .on_response(
                    &mut state.1,
                    AuthResponseContext {
                        ep,
                        vars,
                        auth,
                        auth_state,
                        executor,
                        meta,
                        status,
                        headers,
                        attempt,
                    },
                )
                .await?;
            Ok(AuthResponseAction::merge(a, b))
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct AuthAttempt {
    pub applied: Vec<AuthAppliedPart>,
}

impl AuthAttempt {
    #[inline]
    pub fn merge(&mut self, other: AuthAttempt) {
        self.applied.extend(other.applied);
    }
}

#[derive(Clone, Debug)]
pub struct AuthAppliedPart {
    pub credential_id: CredentialId,
    pub usage_id: AuthUsageId,
    pub generation: Option<u64>,
    pub identity: AuthIdentity,
    pub provenance: AuthProvenance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthResponseAction {
    Continue,
    Retry { reason: AuthRetryReason },
}

impl AuthResponseAction {
    #[inline]
    fn merge(a: Self, b: Self) -> Self {
        match (a, b) {
            (Self::Retry { reason }, _) | (_, Self::Retry { reason }) => Self::Retry { reason },
            (Self::Continue, Self::Continue) => Self::Continue,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthRetryReason {
    Unauthorized,
    Forbidden,
    ChallengeRejected,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CredentialId {
    namespace: &'static str,
    name: &'static str,
}

impl CredentialId {
    #[inline]
    pub const fn new(namespace: &'static str, name: &'static str) -> Self {
        Self { namespace, name }
    }

    #[inline]
    pub fn namespace(&self) -> &'static str {
        self.namespace
    }

    #[inline]
    pub fn name(&self) -> &'static str {
        self.name
    }

    #[inline]
    pub fn safe_fragment(&self) -> String {
        format!("{}:{}", self.namespace, self.name)
    }
}

impl fmt::Display for CredentialId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.namespace, self.name)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AuthUsageId(&'static str);

impl AuthUsageId {
    #[inline]
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    #[inline]
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

impl fmt::Display for AuthUsageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum AuthIdentity {
    Anonymous,
    Static(&'static str),
    User(String),
    Tenant(String),
    ScopeAudience {
        scope: Vec<String>,
        audience: Option<String>,
    },
    OpaqueHash(String),
}

impl AuthIdentity {
    #[inline]
    pub fn safe_fragment(&self) -> String {
        match self {
            Self::Anonymous => "anon".to_string(),
            Self::Static(v) => format!("static:{v}"),
            Self::User(v) => format!("user:{v}"),
            Self::Tenant(v) => format!("tenant:{v}"),
            Self::ScopeAudience { scope, audience } => {
                format!("scope:{};aud:{:?}", scope.join(","), audience)
            }
            Self::OpaqueHash(v) => format!("hash:{v}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AuthProvenance {
    pub layer: &'static str,
}

impl AuthProvenance {
    #[inline]
    pub const fn new(layer: &'static str) -> Self {
        Self { layer }
    }
}

impl Default for AuthProvenance {
    fn default() -> Self {
        Self::new("runtime")
    }
}

#[derive(Clone, Debug, Error)]
#[error("{kind:?}: {message}")]
pub struct AuthError {
    pub kind: AuthErrorKind,
    pub message: String,
}

impl AuthError {
    #[inline]
    pub fn new(kind: AuthErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthErrorKind {
    MissingCredential,
    AcquireFailed,
    RefreshFailed,
    RejectedCredential,
    UnsupportedScheme,
    RecursionDetected,
    ProviderRejected,
    StateUnavailable,
    CertificateUnavailable,
    InvalidConfiguration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialRefreshReason {
    Missing,
    Expired,
    ExpiringSoon,
    Rejected,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidateReason {
    Unauthorized,
    Forbidden,
    Manual,
    ProviderRejected,
}

pub trait CredentialMaterial: Clone + Send + Sync + 'static {
    fn expires_at(&self) -> Option<Instant> {
        None
    }

    fn safe_identity(&self) -> AuthIdentity {
        AuthIdentity::Anonymous
    }
}

pub trait SecretCredential: CredentialMaterial {
    fn secret_value(&self) -> &str;
}

pub struct CredentialContext<'a, Cx: ClientContext> {
    pub vars: &'a Cx::Vars,
    pub auth: &'a Cx::AuthVars,
    pub auth_state: &'a Cx::AuthState,
    pub executor: &'a dyn AuthHttpExecutor,
    pub credential_id: CredentialId,
    pub reason: CredentialRefreshReason,
}

impl<Cx: ClientContext> Clone for CredentialContext<'_, Cx> {
    fn clone(&self) -> Self {
        Self {
            vars: self.vars,
            auth: self.auth,
            auth_state: self.auth_state,
            executor: self.executor,
            credential_id: self.credential_id.clone(),
            reason: self.reason,
        }
    }
}

impl<'a, Cx: ClientContext> CredentialContext<'a, Cx> {
    #[inline]
    pub fn with_reason(&self, reason: CredentialRefreshReason) -> Self {
        Self {
            reason,
            ..self.clone()
        }
    }
}

pub trait CredentialProvider<Cx: ClientContext>: Send + Sync + 'static {
    type Credential: CredentialMaterial;

    fn id(&self) -> CredentialId;

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>>;

    fn refresh<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        current: &'a Self::Credential,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let _ = current;
            self.acquire(ctx).await
        })
    }

    fn invalidate<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        current: Option<&'a Self::Credential>,
        reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move {
            let _ = (ctx, current, reason);
            Ok(())
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AuthStepPolicy {
    pub refresh_skew: Duration,
    pub retry_on_unauthorized: bool,
    pub max_auth_retries: u8,
    pub retry_on_forbidden: bool,
}

impl Default for AuthStepPolicy {
    fn default() -> Self {
        Self {
            refresh_skew: Duration::from_secs(60),
            retry_on_unauthorized: true,
            max_auth_retries: 1,
            retry_on_forbidden: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CredentialLease<T> {
    pub value: T,
    pub generation: u64,
}

enum CredentialSlotState<T> {
    Empty,
    Valid {
        value: T,
        generation: u64,
    },
    Refreshing {
        notify: Arc<Notify>,
    },
    Failed {
        generation: u64,
        error: AuthError,
        retry_after: Option<Instant>,
    },
}

pub struct CredentialSlot<Cx: ClientContext, P: CredentialProvider<Cx>> {
    provider: P,
    state: Mutex<CredentialSlotState<P::Credential>>,
    _cx: PhantomData<Cx>,
}

enum SlotAction<T> {
    Wait(Arc<Notify>),
    Acquire {
        generation: u64,
        notify: Arc<Notify>,
    },
    Refresh {
        current: T,
        generation: u64,
        notify: Arc<Notify>,
        reason: CredentialRefreshReason,
    },
}

impl<Cx, P> CredentialSlot<Cx, P>
where
    Cx: ClientContext,
    P: CredentialProvider<Cx>,
{
    #[inline]
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            state: Mutex::new(CredentialSlotState::Empty),
            _cx: PhantomData,
        }
    }

    #[inline]
    pub fn provider(&self) -> &P {
        &self.provider
    }

    #[inline]
    pub fn id(&self) -> CredentialId {
        self.provider.id()
    }

    pub async fn get_or_refresh<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        policy: AuthStepPolicy,
    ) -> Result<CredentialLease<P::Credential>, AuthError> {
        loop {
            let action = {
                let mut state = self.state.lock().await;
                match &*state {
                    CredentialSlotState::Valid { value, generation }
                        if credential_refresh_reason(value, policy).is_none() =>
                    {
                        return Ok(CredentialLease {
                            value: value.clone(),
                            generation: *generation,
                        });
                    }
                    CredentialSlotState::Valid { value, generation } => {
                        let notify = Arc::new(Notify::new());
                        let current = value.clone();
                        let generation = *generation;
                        let reason = credential_refresh_reason(value, policy)
                            .unwrap_or(CredentialRefreshReason::ExpiringSoon);
                        *state = CredentialSlotState::Refreshing {
                            notify: notify.clone(),
                        };
                        SlotAction::Refresh {
                            current,
                            generation,
                            notify,
                            reason,
                        }
                    }
                    CredentialSlotState::Refreshing { notify, .. } => {
                        SlotAction::Wait(notify.clone())
                    }
                    CredentialSlotState::Failed {
                        generation,
                        error,
                        retry_after,
                    } => {
                        if retry_after.is_some_and(|retry_at| retry_at > Instant::now()) {
                            return Err(error.clone());
                        }
                        let notify = Arc::new(Notify::new());
                        let generation = *generation;
                        *state = CredentialSlotState::Refreshing {
                            notify: notify.clone(),
                        };
                        SlotAction::Acquire { generation, notify }
                    }
                    CredentialSlotState::Empty => {
                        let notify = Arc::new(Notify::new());
                        *state = CredentialSlotState::Refreshing {
                            notify: notify.clone(),
                        };
                        SlotAction::Acquire {
                            generation: 0,
                            notify,
                        }
                    }
                }
            };

            match action {
                SlotAction::Wait(notify) => {
                    notify.notified().await;
                }
                SlotAction::Acquire { generation, notify } => {
                    let result = self.provider.acquire(ctx.clone()).await;
                    return self.commit_slot_result(generation, notify, result).await;
                }
                SlotAction::Refresh {
                    current,
                    generation,
                    notify,
                    reason,
                } => {
                    let result = self
                        .provider
                        .refresh(ctx.with_reason(reason), &current)
                        .await;
                    return self.commit_slot_result(generation, notify, result).await;
                }
            }
        }
    }

    pub async fn invalidate_generation<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
        generation: Option<u64>,
        reason: InvalidateReason,
    ) -> Result<(), AuthError> {
        let current = {
            let mut state = self.state.lock().await;
            match &*state {
                CredentialSlotState::Valid {
                    value,
                    generation: current_generation,
                } if generation.is_none_or(|expected| expected == *current_generation) => {
                    let current = value.clone();
                    *state = CredentialSlotState::Empty;
                    Some(current)
                }
                _ => None,
            }
        };
        self.provider
            .invalidate(ctx, current.as_ref(), reason)
            .await
    }

    async fn commit_slot_result(
        &self,
        previous_generation: u64,
        notify: Arc<Notify>,
        result: Result<P::Credential, AuthError>,
    ) -> Result<CredentialLease<P::Credential>, AuthError> {
        let mut state = self.state.lock().await;
        match result {
            Ok(value) => {
                let generation = previous_generation.saturating_add(1);
                *state = CredentialSlotState::Valid {
                    value: value.clone(),
                    generation,
                };
                notify.notify_waiters();
                Ok(CredentialLease { value, generation })
            }
            Err(error) => {
                *state = CredentialSlotState::Failed {
                    generation: previous_generation,
                    error: error.clone(),
                    retry_after: None,
                };
                notify.notify_waiters();
                Err(error)
            }
        }
    }
}

fn credential_refresh_reason<T: CredentialMaterial>(
    value: &T,
    policy: AuthStepPolicy,
) -> Option<CredentialRefreshReason> {
    value.expires_at().and_then(|expires_at| {
        let now = Instant::now();
        if expires_at <= now {
            Some(CredentialRefreshReason::Expired)
        } else if expires_at <= now + policy.refresh_skew {
            Some(CredentialRefreshReason::ExpiringSoon)
        } else {
            None
        }
    })
}

#[derive(Clone, Debug)]
pub struct AccessToken {
    pub token: SecretString,
    pub expires_at: Option<Instant>,
    pub refresh_token: Option<SecretString>,
    pub scope: Vec<String>,
    pub audience: Option<String>,
    pub identity_hint: Option<String>,
}

impl AccessToken {
    #[inline]
    pub fn new(token: impl Into<SecretString>) -> Self {
        Self {
            token: token.into(),
            expires_at: None,
            refresh_token: None,
            scope: Vec::new(),
            audience: None,
            identity_hint: None,
        }
    }

    #[inline]
    pub fn expires_at(mut self, expires_at: Instant) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    #[inline]
    pub fn identity_hint(mut self, hint: impl Into<String>) -> Self {
        self.identity_hint = Some(hint.into());
        self
    }
}

impl CredentialMaterial for AccessToken {
    fn expires_at(&self) -> Option<Instant> {
        self.expires_at
    }

    fn safe_identity(&self) -> AuthIdentity {
        if let Some(hint) = &self.identity_hint {
            return AuthIdentity::User(hint.clone());
        }
        if !self.scope.is_empty() || self.audience.is_some() {
            return AuthIdentity::ScopeAudience {
                scope: self.scope.clone(),
                audience: self.audience.clone(),
            };
        }
        AuthIdentity::OpaqueHash(hash_secret(self.token.expose()))
    }
}

impl SecretCredential for AccessToken {
    fn secret_value(&self) -> &str {
        self.token.expose()
    }
}

#[derive(Clone, Debug)]
pub struct ApiKey {
    pub value: SecretString,
    pub identity_hint: Option<String>,
}

impl ApiKey {
    #[inline]
    pub fn new(value: impl Into<SecretString>) -> Self {
        Self {
            value: value.into(),
            identity_hint: None,
        }
    }

    #[inline]
    pub fn identity_hint(mut self, hint: impl Into<String>) -> Self {
        self.identity_hint = Some(hint.into());
        self
    }
}

impl CredentialMaterial for ApiKey {
    fn safe_identity(&self) -> AuthIdentity {
        if let Some(hint) = &self.identity_hint {
            AuthIdentity::Tenant(hint.clone())
        } else {
            AuthIdentity::OpaqueHash(hash_secret(self.value.expose()))
        }
    }
}

impl SecretCredential for ApiKey {
    fn secret_value(&self) -> &str {
        self.value.expose()
    }
}

#[derive(Clone, Debug)]
pub struct BasicCredential {
    pub username: String,
    pub password: SecretString,
}

impl BasicCredential {
    #[inline]
    pub fn new(username: impl Into<String>, password: impl Into<SecretString>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }
}

impl CredentialMaterial for BasicCredential {
    fn safe_identity(&self) -> AuthIdentity {
        AuthIdentity::User(self.username.clone())
    }
}

#[derive(Clone, Debug)]
pub struct ClientCertificate {
    pub identity_id: String,
}

impl ClientCertificate {
    #[inline]
    pub fn new(identity_id: impl Into<String>) -> Self {
        Self {
            identity_id: identity_id.into(),
        }
    }
}

impl CredentialMaterial for ClientCertificate {
    fn safe_identity(&self) -> AuthIdentity {
        AuthIdentity::OpaqueHash(hash_secret(&self.identity_id))
    }
}

#[derive(Clone)]
pub struct StaticBearerProvider {
    id: CredentialId,
    token: AccessToken,
}

impl StaticBearerProvider {
    #[inline]
    pub fn new(id: CredentialId, token: AccessToken) -> Self {
        Self { id, token }
    }
}

impl<Cx: ClientContext> CredentialProvider<Cx> for StaticBearerProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move { Ok(self.token.clone()) })
    }
}

#[derive(Clone)]
pub struct StaticApiKeyProvider {
    id: CredentialId,
    key: ApiKey,
}

impl StaticApiKeyProvider {
    #[inline]
    pub fn new(id: CredentialId, key: ApiKey) -> Self {
        Self { id, key }
    }
}

impl<Cx: ClientContext> CredentialProvider<Cx> for StaticApiKeyProvider {
    type Credential = ApiKey;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move { Ok(self.key.clone()) })
    }
}

#[derive(Clone)]
pub struct StaticBasicProvider {
    id: CredentialId,
    credential: BasicCredential,
}

impl StaticBasicProvider {
    #[inline]
    pub fn new(id: CredentialId, credential: BasicCredential) -> Self {
        Self { id, credential }
    }
}

impl<Cx: ClientContext> CredentialProvider<Cx> for StaticBasicProvider {
    type Credential = BasicCredential;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move { Ok(self.credential.clone()) })
    }
}

#[cfg(feature = "json")]
#[derive(Clone, Debug)]
pub struct OAuth2ClientCredentialsProvider {
    id: CredentialId,
    token_url: Url,
    client_id: SecretString,
    client_secret: SecretString,
    scope: Option<String>,
}

#[cfg(feature = "json")]
impl OAuth2ClientCredentialsProvider {
    #[inline]
    pub fn new(
        id: CredentialId,
        token_url: Url,
        client_id: impl Into<SecretString>,
        client_secret: impl Into<SecretString>,
    ) -> Self {
        Self {
            id,
            token_url,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            scope: None,
        }
    }

    #[inline]
    pub fn scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }
}

#[cfg(feature = "json")]
impl<Cx: ClientContext> CredentialProvider<Cx> for OAuth2ClientCredentialsProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let mut headers = HeaderMap::new();
            let raw = format!("{}:{}", self.client_id.expose(), self.client_secret.expose());
            let basic = format!("Basic {}", BASE64_STANDARD.encode(raw));
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&basic).map_err(|_| {
                    AuthError::new(AuthErrorKind::InvalidConfiguration, "invalid client secret")
                })?,
            );
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            );

            let body = {
                let mut form = url::form_urlencoded::Serializer::new(String::new());
                form.append_pair("grant_type", "client_credentials");
                if let Some(scope) = &self.scope {
                    form.append_pair("scope", scope);
                }
                form.finish()
            };

            let resp = ctx
                .executor
                .send(AuthHttpRequest {
                    method: Method::POST,
                    url: self.token_url.clone(),
                    headers,
                    body: Some(Bytes::from(body.into_bytes())),
                    mode: AuthMode::SkipAuth,
                    policy: AuthInternalPolicy::default(),
                })
                .await?;

            if !resp.status.is_success() {
                return Err(AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    format!("oauth2 token endpoint returned {}", resp.status),
                ));
            }

            let token: OAuth2TokenResponse = serde_json::from_slice(&resp.body).map_err(|e| {
                AuthError::new(
                    AuthErrorKind::AcquireFailed,
                    format!("oauth2 token response decode failed: {e}"),
                )
            })?;

            if let Some(token_type) = &token.token_type
                && !token_type.eq_ignore_ascii_case("bearer")
            {
                return Err(AuthError::new(
                    AuthErrorKind::UnsupportedScheme,
                    format!("unsupported oauth2 token_type {token_type}"),
                ));
            }

            let mut out = AccessToken::new(token.access_token);
            out.expires_at = token
                .expires_in
                .map(|seconds| Instant::now() + Duration::from_secs(seconds));
            out.refresh_token = token.refresh_token.map(SecretString::new);
            out.scope = token
                .scope
                .unwrap_or_default()
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect();
            Ok(out)
        })
    }
}

#[cfg(feature = "json")]
#[derive(Deserialize)]
struct OAuth2TokenResponse {
    access_token: String,
    token_type: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

pub struct AuthApplyContext<'a, Cx: ClientContext, E: Endpoint<Cx>> {
    pub ep: &'a E,
    pub vars: &'a Cx::Vars,
    pub auth: &'a Cx::AuthVars,
    pub auth_state: &'a Cx::AuthState,
    pub meta: &'a RequestMeta,
    pub request: &'a mut BuiltRequest,
}

impl<Cx: ClientContext, E: Endpoint<Cx>> AuthApplyContext<'_, Cx, E> {
    #[inline]
    pub fn error_context(&self) -> ErrorContext {
        ErrorContext {
            endpoint: self.meta.endpoint,
            method: self.meta.method.clone(),
        }
    }
}

pub struct AuthChallengeContext<'a, Cx: ClientContext, E: Endpoint<Cx>> {
    pub ep: &'a E,
    pub vars: &'a Cx::Vars,
    pub auth: &'a Cx::AuthVars,
    pub auth_state: &'a Cx::AuthState,
    pub meta: &'a RequestMeta,
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
    pub applied: &'a AuthAppliedPart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthChallengeDecision {
    Ignore,
    RejectCredential,
}

pub trait AuthUsage<Cx: ClientContext, E: Endpoint<Cx>, M: CredentialMaterial>:
    Send + Sync + 'static
{
    fn name(&self) -> AuthUsageId;

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        material: &M,
    ) -> Result<AuthIdentity, ApiClientError>;

    fn challenge(&self, _ctx: AuthChallengeContext<'_, Cx, E>) -> AuthChallengeDecision {
        AuthChallengeDecision::Ignore
    }
}

pub struct UseCredential<Cx: ClientContext, P: CredentialProvider<Cx>, U> {
    slot: Arc<CredentialSlot<Cx, P>>,
    usage: U,
    policy: AuthStepPolicy,
    provenance: AuthProvenance,
}

impl<Cx: ClientContext, P: CredentialProvider<Cx>, U> UseCredential<Cx, P, U> {
    #[inline]
    pub fn new(slot: Arc<CredentialSlot<Cx, P>>, usage: U) -> Self {
        Self {
            slot,
            usage,
            policy: AuthStepPolicy::default(),
            provenance: AuthProvenance::default(),
        }
    }

    #[inline]
    pub fn with_policy(mut self, policy: AuthStepPolicy) -> Self {
        self.policy = policy;
        self
    }

    #[inline]
    pub fn with_provenance(mut self, provenance: AuthProvenance) -> Self {
        self.provenance = provenance;
        self
    }
}

pub struct UseCredentialState {
    auth_retries: u8,
}

impl<Cx, E, P, U> AuthController<Cx, E> for UseCredential<Cx, P, U>
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    P: CredentialProvider<Cx>,
    U: AuthUsage<Cx, E, P::Credential>,
{
    type State = UseCredentialState;

    fn init(&self, _ep: &E) -> Result<Self::State, ApiClientError> {
        Ok(UseCredentialState { auth_retries: 0 })
    }

    fn prepare<'a>(
        &'a self,
        _state: &'a mut Self::State,
        ctx: AuthPrepareContext<'a, Cx, E>,
    ) -> AuthFuture<'a, Result<AuthAttempt, ApiClientError>> {
        Box::pin(async move {
            let credential_id = self.slot.id();
            let credential_ctx = CredentialContext {
                vars: ctx.vars,
                auth: ctx.auth,
                auth_state: ctx.auth_state,
                executor: ctx.executor,
                credential_id: credential_id.clone(),
                reason: CredentialRefreshReason::Missing,
            };
            let lease = self
                .slot
                .get_or_refresh(credential_ctx, self.policy)
                .await
                .map_err(|source| ApiClientError::Auth {
                    ctx: ctx.error_context(),
                    source,
                })?;
            let identity = self.usage.apply(
                AuthApplyContext {
                    ep: ctx.ep,
                    vars: ctx.vars,
                    auth: ctx.auth,
                    auth_state: ctx.auth_state,
                    meta: ctx.meta,
                    request: ctx.request,
                },
                &lease.value,
            )?;
            ctx.request
                .extensions
                .auth_identities
                .push(identity.safe_fragment());
            let applied = AuthAppliedPart {
                credential_id,
                usage_id: self.usage.name(),
                generation: Some(lease.generation),
                identity,
                provenance: self.provenance.clone(),
            };
            Ok(AuthAttempt {
                applied: vec![applied],
            })
        })
    }

    fn on_response<'a>(
        &'a self,
        state: &'a mut Self::State,
        ctx: AuthResponseContext<'a, Cx, E>,
    ) -> AuthFuture<'a, Result<AuthResponseAction, ApiClientError>> {
        Box::pin(async move {
            let usage_id = self.usage.name();
            let credential_id = self.slot.id();
            let Some(applied) = ctx
                .attempt
                .applied
                .iter()
                .find(|part| part.credential_id == credential_id && part.usage_id == usage_id)
            else {
                return Ok(AuthResponseAction::Continue);
            };

            let challenge = self.usage.challenge(AuthChallengeContext {
                ep: ctx.ep,
                vars: ctx.vars,
                auth: ctx.auth,
                auth_state: ctx.auth_state,
                meta: ctx.meta,
                status: ctx.status,
                headers: ctx.headers,
                applied,
            });

            let retry_reason = if challenge == AuthChallengeDecision::RejectCredential {
                Some(AuthRetryReason::ChallengeRejected)
            } else if ctx.status == StatusCode::UNAUTHORIZED && self.policy.retry_on_unauthorized {
                Some(AuthRetryReason::Unauthorized)
            } else if ctx.status == StatusCode::FORBIDDEN && self.policy.retry_on_forbidden {
                Some(AuthRetryReason::Forbidden)
            } else {
                None
            };

            let Some(reason) = retry_reason else {
                return Ok(AuthResponseAction::Continue);
            };

            if state.auth_retries >= self.policy.max_auth_retries {
                return Ok(AuthResponseAction::Continue);
            }

            state.auth_retries = state.auth_retries.saturating_add(1);
            let invalidate_reason = match reason {
                AuthRetryReason::Unauthorized | AuthRetryReason::ChallengeRejected => {
                    InvalidateReason::Unauthorized
                }
                AuthRetryReason::Forbidden => InvalidateReason::Forbidden,
            };
            let credential_ctx = CredentialContext {
                vars: ctx.vars,
                auth: ctx.auth,
                auth_state: ctx.auth_state,
                executor: ctx.executor,
                credential_id: credential_id.clone(),
                reason: CredentialRefreshReason::Rejected,
            };
            self.slot
                .invalidate_generation(credential_ctx, applied.generation, invalidate_reason)
                .await
                .map_err(|source| ApiClientError::Auth {
                    ctx: ctx.error_context(),
                    source,
                })?;
            Ok(AuthResponseAction::Retry { reason })
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BearerAuth;

impl BearerAuth {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl<Cx, E> AuthUsage<Cx, E, AccessToken> for BearerAuth
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
{
    fn name(&self) -> AuthUsageId {
        AuthUsageId::new("bearer")
    }

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        material: &AccessToken,
    ) -> Result<AuthIdentity, ApiClientError> {
        let value = format!("Bearer {}", material.token.expose());
        let value = HeaderValue::from_str(&value).map_err(|_| ApiClientError::InvalidParam {
            ctx: ctx.error_context(),
            param: "authorization bearer token",
        })?;
        ctx.request.headers.insert(AUTHORIZATION, value);
        Ok(material.safe_identity())
    }

    fn challenge(&self, ctx: AuthChallengeContext<'_, Cx, E>) -> AuthChallengeDecision {
        if ctx.status != StatusCode::UNAUTHORIZED {
            return AuthChallengeDecision::Ignore;
        }
        let Some(value) = ctx.headers.get(http::header::WWW_AUTHENTICATE) else {
            return AuthChallengeDecision::RejectCredential;
        };
        let value = value.to_str().unwrap_or_default().to_ascii_lowercase();
        if value.contains("bearer") && value.contains("invalid_token") {
            AuthChallengeDecision::RejectCredential
        } else {
            AuthChallengeDecision::Ignore
        }
    }
}

#[derive(Clone, Debug)]
pub struct HeaderAuth {
    header: HeaderName,
}

impl HeaderAuth {
    #[inline]
    pub fn new(header: HeaderName) -> Self {
        Self { header }
    }

    #[inline]
    pub fn from_static(header: &'static str) -> Self {
        Self {
            header: HeaderName::from_static(header),
        }
    }
}

impl<Cx, E, M> AuthUsage<Cx, E, M> for HeaderAuth
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    M: SecretCredential,
{
    fn name(&self) -> AuthUsageId {
        AuthUsageId::new("header")
    }

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        material: &M,
    ) -> Result<AuthIdentity, ApiClientError> {
        let value = HeaderValue::from_str(material.secret_value()).map_err(|_| {
            ApiClientError::InvalidParam {
                ctx: ctx.error_context(),
                param: "auth header value",
            }
        })?;
        ctx.request.headers.insert(self.header.clone(), value);
        Ok(material.safe_identity())
    }
}

#[derive(Clone, Debug)]
pub struct QueryAuth {
    key: String,
}

impl QueryAuth {
    #[inline]
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }
}

impl<Cx, E, M> AuthUsage<Cx, E, M> for QueryAuth
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    M: SecretCredential,
{
    fn name(&self) -> AuthUsageId {
        AuthUsageId::new("query")
    }

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        material: &M,
    ) -> Result<AuthIdentity, ApiClientError> {
        ctx.request
            .url
            .query_pairs_mut()
            .append_pair(&self.key, material.secret_value());
        Ok(material.safe_identity())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BasicAuth;

impl BasicAuth {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl<Cx, E> AuthUsage<Cx, E, BasicCredential> for BasicAuth
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
{
    fn name(&self) -> AuthUsageId {
        AuthUsageId::new("basic")
    }

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        material: &BasicCredential,
    ) -> Result<AuthIdentity, ApiClientError> {
        let raw = format!("{}:{}", material.username, material.password.expose());
        let value = format!("Basic {}", BASE64_STANDARD.encode(raw));
        let value = HeaderValue::from_str(&value).map_err(|_| ApiClientError::InvalidParam {
            ctx: ctx.error_context(),
            param: "authorization basic credential",
        })?;
        ctx.request.headers.insert(AUTHORIZATION, value);
        Ok(material.safe_identity())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CertificateAuth;

impl CertificateAuth {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl<Cx, E> AuthUsage<Cx, E, ClientCertificate> for CertificateAuth
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
{
    fn name(&self) -> AuthUsageId {
        AuthUsageId::new("certificate")
    }

    fn apply(
        &self,
        ctx: AuthApplyContext<'_, Cx, E>,
        material: &ClientCertificate,
    ) -> Result<AuthIdentity, ApiClientError> {
        ctx.request.extensions.transport_auth = Some(TransportAuth::ClientCertificate {
            identity_id: material.identity_id.clone(),
        });
        Ok(material.safe_identity())
    }
}

#[derive(Clone, Debug)]
pub struct AuthHttpRequest {
    pub method: Method,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
    pub mode: AuthMode,
    pub policy: AuthInternalPolicy,
}

#[derive(Clone, Debug)]
pub struct AuthHttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthMode {
    SkipAuth,
    UseAuth(AuthRequirementId),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AuthRequirementId {
    namespace: &'static str,
    name: &'static str,
}

impl AuthRequirementId {
    #[inline]
    pub const fn new(namespace: &'static str, name: &'static str) -> Self {
        Self { namespace, name }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AuthInternalPolicy {
    pub timeout: Option<Duration>,
}

pub trait AuthHttpExecutor: Send + Sync {
    fn send<'a>(
        &'a self,
        req: AuthHttpRequest,
    ) -> AuthFuture<'a, Result<AuthHttpResponse, AuthError>>;
}

#[derive(Clone, Debug, Default)]
pub struct RequestExtensions {
    pub auth_identities: Vec<String>,
    pub transport_auth: Option<TransportAuth>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportAuth {
    ClientCertificate { identity_id: String },
}

fn hash_secret(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
