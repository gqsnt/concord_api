//! Credential material, providers, placement, and private slot state.

mod error;
mod material;
mod origin;
mod placement;
mod provider;
mod recovery;
mod slot;
mod version;

pub(crate) use origin::OriginSetError;
pub(crate) use placement::{AuthPlan, PreparedAuthPlan};
pub(crate) use slot::{CredentialContext, IntentTicket, ReleasedMaterial, SlotSelector};

pub use crate::limits::MAX_AUTH_PLACEMENTS;
pub use error::AuthError;
pub use material::{
    AccessToken, ApiKey, BasicCredential, CredentialMaterial, CredentialMaterialError,
    CredentialMaterialErrorKind, MAX_BASIC_PASSWORD_BYTES, MAX_BASIC_USERNAME_BYTES,
    MAX_CREDENTIAL_SECRET_BYTES, MAX_SECRET_STRING_BYTES, SecretCredential, SecretString,
};
pub use origin::{
    MAX_PROVIDER_ORIGIN_BYTES, ProviderOrigin, ProviderOriginError, ProviderOriginErrorKind,
};
pub use placement::{
    Auth, AuthTarget, ChallengePolicy, MAX_AUTH_PLACEMENT_NAME_BYTES, MAX_AUTH_QUERY_VALUE_BYTES,
    MAX_AUTHORIZATION_SCHEME_BYTES, MAX_AUTHORIZATION_VALUE_BYTES,
};
pub use provider::{CredentialProvider, CredentialReason, CredentialRequest, RecoveryMode};
pub use recovery::ChallengeRecovery;
pub use slot::{CredentialHandle, CredentialRef, MAX_CREDENTIAL_FLIGHT_WAITERS};
pub use version::{CredentialVersion, StoreOutcome};

// The retained bounded model remains proof machinery. It deliberately models
// identities and transitions rather than production material or public APIs.
#[cfg(test)]
mod challenge;
#[cfg(test)]
mod model;
#[cfg(test)]
mod systematic;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod transition;
