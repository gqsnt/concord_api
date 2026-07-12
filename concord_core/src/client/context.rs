// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

pub trait ClientContext: Sized + Send + Sync + 'static {
    type Vars: Clone + Send + Sync + 'static;
    type AuthVars: Clone + Send + Sync + 'static;
    type AuthState: Clone + Send + Sync + 'static;
    const SCHEME: Scheme;
    const DOMAIN: &'static str;

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState;

    fn apply_internal_auth<'a>(
        _requirement: &'a AuthRequirementId,
        _request: &'a mut crate::auth::AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn AuthHttpExecutor,
    ) -> crate::auth::AuthFuture<'a, Result<crate::auth::PreparedInternalAuth, AuthError>> {
        Box::pin(async {
            Err(AuthError::new(
                AuthErrorKind::UnsupportedScheme,
                "internal auth requirement is not supported by this client context",
            ))
        })
    }

    fn prepare_auth_requirement<'a>(
        _requirement: &'a crate::auth::AuthRequirement,
        _request: &'a mut crate::auth::AuthApplicationRequest<'_>,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> crate::auth::AuthFuture<'a, Result<crate::auth::PreparedAuthCredential, AuthError>> {
        Box::pin(async {
            Err(AuthError::new(
                AuthErrorKind::UnsupportedScheme,
                "auth requirement is not supported by this client context",
            ))
        })
    }

    /// Derives one authentication rejection action for this exact requirement
    /// and applied credential pair without performing any credential,
    /// provider, or network operation. Core validates and aggregates it.
    fn plan_auth_response(
        _requirement: &crate::auth::AuthRequirement,
        applied: &crate::auth::AuthAppliedCredential,
        _vars: &Self::Vars,
        _auth: &Self::AuthVars,
        _meta: &RequestMeta,
        _status: http::StatusCode,
        _headers: &http::HeaderMap,
    ) -> Result<crate::auth::AuthRejectionAction, AuthError> {
        Ok(crate::auth::AuthRejectionAction::terminal(
            _requirement,
            applied,
            None,
        ))
    }

    /// Applies one terminal action using only local credential state. This
    /// hook has no executor and cannot acquire or refresh credentials.
    #[allow(clippy::too_many_arguments)]
    fn apply_terminal_auth_action<'a>(
        _action: &'a crate::auth::AuthRejectionAction,
        _requirement: &'a crate::auth::AuthRequirement,
        _applied: &'a crate::auth::AuthAppliedCredential,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _meta: &'a RequestMeta,
        _status: http::StatusCode,
    ) -> crate::auth::AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async { Ok(()) })
    }

    /// Applies the selected refresh action. Core calls this only after a
    /// retry-admission permit has been reserved.
    #[allow(clippy::too_many_arguments)]
    fn apply_refresh_auth_action<'a>(
        _action: &'a crate::auth::AuthRejectionAction,
        _requirement: &'a crate::auth::AuthRequirement,
        _applied: &'a crate::auth::AuthAppliedCredential,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn AuthHttpExecutor,
        _meta: &'a RequestMeta,
        _status: http::StatusCode,
    ) -> crate::auth::AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async { Ok(()) })
    }

    fn base_route(_vars: &Self::Vars, _auth: &Self::AuthVars) -> RouteBuilder {
        RouteBuilder::new()
    }

    fn base_policy(
        _vars: &Self::Vars,
        _auth: &Self::AuthVars,
        _ctx: &ErrorContext,
    ) -> Result<Policy, ApiClientError> {
        Ok(Policy::new())
    }
}

#[derive(Clone, Copy)]
pub(super) struct SendClassifyCtx<'a> {
    pub(super) dbg: DebugLevel,
    pub(super) dbg_verbose: bool,
    pub(super) dbg_vv: bool,
    pub(super) url_str: &'a str,
    pub(super) error_ctx: &'a ErrorContext,
    pub(super) auth_materials: &'a [crate::auth::AuthTransportMaterial],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AuthPreparationCachePolicy {
    Never,
    RequestLocalReusable,
}

impl AuthPreparationCachePolicy {
    #[inline]
    pub(super) fn allows_request_local_reuse(self) -> bool {
        matches!(self, Self::RequestLocalReusable)
    }
}

#[derive(Clone)]
pub(super) struct AuthPreparation {
    pub(super) summary: crate::auth::AuthAttemptSummary,
    pub(super) materials: Vec<crate::auth::AuthTransportMaterial>,
    pub(super) cache_policy: AuthPreparationCachePolicy,
}

pub(super) struct AuthRejectionCtx<'a> {
    pub(super) plan: &'a crate::endpoint::RequestPlanView,
    pub(super) meta: &'a RequestMeta,
    pub(super) status: StatusCode,
    pub(super) headers: &'a http::HeaderMap,
    pub(super) auth_attempt: &'a crate::auth::AuthAttemptSummary,
}

#[derive(Clone, Copy)]
pub(super) struct ResponseObservationCtx<'a> {
    pub(super) endpoint: &'static str,
    pub(super) method: &'a http::Method,
    pub(super) url: &'a str,
    pub(super) url_host: Option<&'a str>,
    pub(super) attempt: u32,
    pub(super) page_index: u32,
    pub(super) idempotent: bool,
    pub(super) plan: &'a RateLimitPlan,
    pub(super) status: StatusCode,
    pub(super) headers: &'a http::HeaderMap,
}
