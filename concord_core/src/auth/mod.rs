mod credentials;
mod errors;
mod future;
mod http;
mod ids;
mod materials;
mod orchestrator;
mod plan;
mod providers;

pub use credentials::{
    AuthStepPolicy, CredentialContext, CredentialLease, CredentialMaterial, CredentialProvider,
    CredentialSlot, SecretCredential,
};
pub use errors::{
    AuthError, AuthErrorKind, CredentialRefreshReason, InvalidateReason, read_auth_lock,
    write_auth_lock,
};
pub use future::AuthFuture;
pub use http::{
    AuthHttpExecutor, AuthHttpRequest, AuthHttpResponse, AuthInternalPolicy, AuthMode,
    AuthRequirementId,
};
pub use ids::{AuthProvenance, AuthUsageId, CredentialId};
pub use materials::{AccessToken, ApiKey, BasicCredential};
#[doc(hidden)]
pub use orchestrator::{AuthChallengeMode, AuthPreparationMode, AuthProviderBinding};
pub(crate) use orchestrator::{apply_rejection, plan_rejection, prepare};
pub(crate) use plan::AuthRejectionPlan;
pub(crate) use plan::AuthTransportMaterial;
pub use plan::{
    AuthApplication, AuthApplicationRequest, AuthAppliedCredential, AuthAttemptSummary,
    AuthChallengePolicy, AuthDecision, AuthPlacement, AuthPlacementPlan, AuthPlan,
    AuthPreparationReuse, AuthRejectionAction, AuthRejectionDecision, AuthRequirement,
    AuthRetryReason, AuthSlotId, CredentialRef, PlannedAuthPlacement, PlannedAuthSlot,
    PreparedAuthCredential, PreparedInternalAuth, apply_basic_credential, apply_secret_credential,
    auth_decision_for_status, invalidate_rejected_credential, invalidate_rejected_credential_local,
};
#[cfg(feature = "json")]
pub use providers::OAuth2ClientCredentialsProvider;
pub use providers::{
    ManualCredentialProvider, StaticApiKeyProvider, StaticBasicProvider, StaticBearerProvider,
};

#[derive(Clone, Default)]
pub struct NoAuthState;
