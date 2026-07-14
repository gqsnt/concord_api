// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

pub trait ClientContext: Sized + Send + Sync + 'static {
    type Vars: Clone + Send + Sync + 'static;
    type AuthVars: Clone + Send + Sync + 'static;
    type AuthState: Clone + Send + Sync + 'static;
    const SCHEME: Scheme;
    const DOMAIN: &'static str;

    fn init_auth_state(_vars: &Self::Vars, _auth: &Self::AuthVars) -> Self::AuthState;

    /// Resolves an opaque provider binding for one credential.
    /// Core owns every lifecycle operation performed through the binding.
    fn auth_provider_binding<'a>(
        _credential: &crate::auth::CredentialId,
        _auth_state: &'a Self::AuthState,
    ) -> Option<crate::auth::AuthProviderBinding<'a, Self>> {
        None
    }

    fn base_route(_vars: &Self::Vars, _auth: &Self::AuthVars) -> RouteBuilder {
        RouteBuilder::new()
    }

    fn base_policy(
        _vars: &Self::Vars,
        _auth: &Self::AuthVars,
        _ctx: &ErrorContext,
    ) -> Result<crate::policy::ClientPolicyBuilder, ApiClientError> {
        Ok(crate::policy::ClientPolicyBuilder::new())
    }
}

#[derive(Clone, Copy)]
pub(super) struct SendClassifyCtx<'a> {
    pub(super) dbg: DebugLevel,
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
    #[cfg(feature = "dangerous-dev-tools")]
    pub(super) lifecycle_observation_targets: Vec<AuthLifecycleObservationTarget>,
}

#[cfg(feature = "dangerous-dev-tools")]
#[derive(Clone)]
pub(super) struct AuthLifecycleObservationTarget {
    pub(super) credential_id: crate::auth::CredentialId,
    pub(super) usage_id: crate::auth::AuthUsageId,
    pub(super) step_id: Option<&'static str>,
    pub(super) target: crate::auth::CredentialLifecycleObservationTarget,
}

#[cfg(feature = "dangerous-dev-tools")]
impl AuthLifecycleObservationTarget {
    pub(super) fn matches(&self, action: &crate::auth::AuthRejectionAction) -> bool {
        action.matches_use_identity(&self.credential_id, &self.usage_id, self.step_id)
    }
}

pub(super) struct AuthRejectionCtx<'a, Cx: ClientContext> {
    pub(super) plan: &'a crate::endpoint::RequestPlanView,
    pub(super) auth_state: &'a Cx::AuthState,
    pub(super) meta: &'a RequestExecutionMeta,
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
    pub(super) page_index: u32,
    pub(super) idempotent: bool,
    pub(super) plan: &'a RateLimitPlan,
    pub(super) status: StatusCode,
    pub(super) headers: &'a http::HeaderMap,
}
