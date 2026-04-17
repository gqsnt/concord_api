use super::core::{
    AuthAppliedPart, AuthAttempt, AuthController, AuthPrepareContext, AuthResponseAction,
    AuthResponseContext, AuthRetryReason,
};
use super::credentials::{
    AuthStepPolicy, CredentialContext, CredentialMaterial, CredentialProvider, CredentialSlot,
    SecretCredential,
};
use super::errors::{CredentialRefreshReason, InvalidateReason};
use super::future::AuthFuture;
use super::http::TransportAuth;
use super::ids::{AuthIdentity, AuthProvenance, AuthUsageId};
use super::materials::{AccessToken, BasicCredential, ClientCertificate};
use crate::client::ClientContext;
use crate::endpoint::Endpoint;
use crate::error::{ApiClientError, ErrorContext};
use crate::transport::{BuiltRequest, RequestMeta};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use http::header::{AUTHORIZATION, HeaderName, HeaderValue};
use http::{HeaderMap, StatusCode};
use std::sync::Arc;

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
    step_id: Option<String>,
}

impl<Cx: ClientContext, P: CredentialProvider<Cx>, U> UseCredential<Cx, P, U> {
    #[inline]
    pub fn new(slot: Arc<CredentialSlot<Cx, P>>, usage: U) -> Self {
        Self {
            slot,
            usage,
            policy: AuthStepPolicy::default(),
            provenance: AuthProvenance::default(),
            step_id: None,
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

    #[inline]
    pub fn with_step_id(mut self, step_id: impl Into<String>) -> Self {
        self.step_id = Some(step_id.into());
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
                step_id: self.step_id.clone(),
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
            let applied = if let Some(step_id) = self.step_id.as_deref() {
                ctx.attempt.applied.iter().find(|part| {
                    part.credential_id == credential_id && part.step_id.as_deref() == Some(step_id)
                })
            } else {
                ctx.attempt
                    .applied
                    .iter()
                    .find(|part| part.credential_id == credential_id && part.usage_id == usage_id)
            };

            let Some(applied) = applied else {
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

            enum ResponseSignal {
                ChallengeRejected,
                Unauthorized,
                Forbidden,
            }

            let signal = if challenge == AuthChallengeDecision::RejectCredential {
                Some(ResponseSignal::ChallengeRejected)
            } else if ctx.status == StatusCode::UNAUTHORIZED {
                Some(ResponseSignal::Unauthorized)
            } else if ctx.status == StatusCode::FORBIDDEN {
                Some(ResponseSignal::Forbidden)
            } else {
                None
            };

            let Some(signal) = signal else {
                return Ok(AuthResponseAction::Continue);
            };

            let (invalidate, retry_reason) = match signal {
                ResponseSignal::ChallengeRejected => (
                    self.policy.invalidate_on_challenge_rejection,
                    self.policy
                        .retry_on_challenge_rejection
                        .then_some(AuthRetryReason::ChallengeRejected),
                ),
                ResponseSignal::Unauthorized => (
                    self.policy.invalidate_on_unauthorized,
                    self.policy
                        .retry_on_unauthorized
                        .then_some(AuthRetryReason::Unauthorized),
                ),
                ResponseSignal::Forbidden => (
                    self.policy.invalidate_on_forbidden,
                    self.policy
                        .retry_on_forbidden
                        .then_some(AuthRetryReason::Forbidden),
                ),
            };

            if invalidate {
                let invalidate_reason = match signal {
                    ResponseSignal::ChallengeRejected | ResponseSignal::Unauthorized => {
                        InvalidateReason::Unauthorized
                    }
                    ResponseSignal::Forbidden => InvalidateReason::Forbidden,
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
            }

            let Some(reason) = retry_reason else {
                return Ok(AuthResponseAction::Continue);
            };

            if state.auth_retries >= self.policy.max_auth_retries {
                return Ok(AuthResponseAction::Continue);
            }

            state.auth_retries = state.auth_retries.saturating_add(1);
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
