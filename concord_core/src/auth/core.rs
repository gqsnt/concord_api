use super::future::AuthFuture;
use super::http::AuthHttpExecutor;
use super::ids::{AuthIdentity, AuthProvenance, AuthUsageId, CredentialId};
use crate::client::ClientContext;
use crate::endpoint::Endpoint;
use crate::error::{ApiClientError, ErrorContext};
use crate::transport::{BuiltRequest, RequestMeta};
use http::{HeaderMap, StatusCode};
use std::marker::PhantomData;

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

pub struct OneOfAuth<A, B>(PhantomData<(A, B)>);

impl<A, B> Default for OneOfAuth<A, B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A, B> OneOfAuth<A, B> {
    #[inline]
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

pub struct OneOfAuthController<A, B> {
    first: A,
    second: B,
}

enum OneOfActiveBranch {
    First,
    Second,
}

pub struct OneOfAuthState<A, B> {
    first: A,
    second: B,
    active: OneOfActiveBranch,
    fallback_used: bool,
}

impl<Cx, E, A, B> AuthPart<Cx, E> for OneOfAuth<A, B>
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    A: AuthPart<Cx, E>,
    B: AuthPart<Cx, E>,
{
    type Ctrl = OneOfAuthController<A::Ctrl, B::Ctrl>;

    fn controller(ctx: AuthBuildContext<'_, Cx>, ep: &E) -> Result<Self::Ctrl, ApiClientError> {
        Ok(OneOfAuthController {
            first: A::controller(ctx, ep)?,
            second: B::controller(ctx, ep)?,
        })
    }
}

impl<Cx, E, A, B> AuthController<Cx, E> for OneOfAuthController<A, B>
where
    Cx: ClientContext,
    E: Endpoint<Cx>,
    A: AuthController<Cx, E>,
    B: AuthController<Cx, E>,
{
    type State = OneOfAuthState<A::State, B::State>;

    fn init(&self, ep: &E) -> Result<Self::State, ApiClientError> {
        Ok(OneOfAuthState {
            first: self.first.init(ep)?,
            second: self.second.init(ep)?,
            active: OneOfActiveBranch::First,
            fallback_used: false,
        })
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

            match state.active {
                OneOfActiveBranch::First => {
                    self.first
                        .prepare(
                            &mut state.first,
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
                        .await
                }
                OneOfActiveBranch::Second => {
                    self.second
                        .prepare(
                            &mut state.second,
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
                        .await
                }
            }
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

            match state.active {
                OneOfActiveBranch::First => {
                    let action = self
                        .first
                        .on_response(
                            &mut state.first,
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
                    if let AuthResponseAction::Retry { reason } = action {
                        if !state.fallback_used {
                            state.fallback_used = true;
                            state.active = OneOfActiveBranch::Second;
                            return Ok(AuthResponseAction::Retry { reason });
                        }
                        return Ok(AuthResponseAction::Retry { reason });
                    }
                    Ok(action)
                }
                OneOfActiveBranch::Second => {
                    self.second
                        .on_response(
                            &mut state.second,
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
                        .await
                }
            }
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
    pub step_id: Option<String>,
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
    pub(crate) fn merge(a: Self, b: Self) -> Self {
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
