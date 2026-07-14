mod credentials;
mod errors;
mod future;
mod http;
mod ids;
mod materials;
mod orchestrator;
mod plan;
mod providers;

#[cfg(feature = "dangerous-dev-tools")]
pub(crate) use credentials::CredentialLifecycleObservationTarget;
pub use credentials::{
    AuthStepPolicy, CredentialContext, CredentialLease, CredentialMaterial, CredentialProvider,
    CredentialSlot, SecretCredential,
};
#[cfg(feature = "dangerous-dev-tools")]
pub use credentials::{CredentialGenerationSnapshot, CredentialLifecycleEvent};
pub use errors::{
    AuthError, AuthErrorKind, CredentialRefreshReason, InvalidateReason, write_auth_lock,
};
pub use future::AuthFuture;
pub use http::{
    AuthHttpExecutor, AuthHttpRequest, AuthHttpResponse, AuthInternalPolicy, AuthMode,
    AuthRequirementId,
};
pub use ids::{AuthProvenance, AuthUsageId, CredentialId};
pub use materials::{AccessToken, ApiKey, BasicCredential};
pub use orchestrator::{
    AuthChallengeMode, AuthPreparationMode, AuthProviderBinding, CredentialProviderState,
};
pub(crate) use orchestrator::{
    apply_rejection, apply_rejection_invalidation_only, plan_rejection, prepare,
};
pub(crate) use plan::AuthRejectionPlan;
pub(crate) use plan::AuthTransportMaterial;
pub use plan::{
    AuthApplication, AuthApplicationRequest, AuthAppliedCredential, AuthAttemptSummary,
    AuthChallengePolicy, AuthPlacement, AuthPlacementPlan, AuthPlan, AuthPreparationReuse,
    AuthRecoveryReason, AuthRejectionAction, AuthRejectionDecision, AuthRequirement, CredentialRef,
    PlannedAuthPlacement, PreparedAuthCredential, apply_basic_credential, apply_secret_credential,
    auth_decision_for_status,
};
#[cfg(feature = "json")]
pub use providers::OAuth2ClientCredentialsProvider;
pub use providers::{
    ManualCredentialProvider, StaticApiKeyProvider, StaticBasicProvider, StaticBearerProvider,
};

#[derive(Clone, Default)]
pub struct NoAuthState;
