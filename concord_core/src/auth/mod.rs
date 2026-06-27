mod credentials;
mod errors;
mod future;
mod http;
mod ids;
mod materials;
mod plan;
mod providers;
mod util;

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
    AuthRequirementId, RequestExtensions,
};
pub use ids::{AuthProvenance, AuthUsageId, CredentialId};
pub use materials::{AccessToken, ApiKey, BasicCredential, ClientCertificate};
pub(crate) use plan::AuthTransportMaterial;
pub use plan::{
    AuthApplication, AuthApplicationRequest, AuthAppliedCredential, AuthAttemptSummary,
    AuthChallengePolicy, AuthDecision, AuthPlacement, AuthPlan, AuthRejectionDecision,
    AuthRequirement, AuthRetryReason, AuthSlotId, CredentialRef, PendingAuthPlacement,
    PendingAuthSlot, PreparedAuthCredential, PreparedInternalAuth, apply_basic_credential,
    apply_certificate_credential, apply_secret_credential, auth_decision_for_status,
    invalidate_rejected_credential,
};
#[cfg(feature = "json")]
pub use providers::OAuth2ClientCredentialsProvider;
pub use providers::{
    ManualCredentialProvider, StaticApiKeyProvider, StaticBasicProvider, StaticBearerProvider,
};

#[derive(Clone, Default)]
pub struct NoAuthState;
