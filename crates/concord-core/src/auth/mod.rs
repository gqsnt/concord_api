//! Credential material, providers, placement, and private slot state.

mod error;
mod material;
mod placement;
mod provider;
mod slot;

pub(crate) use placement::{AuthPlan, PreparedAuthPlan};
pub(crate) use slot::SlotSelector;

pub use crate::limits::MAX_AUTH_PLACEMENTS;
pub use error::AuthError;
pub use material::{
    AccessToken, ApiKey, BasicCredential, CredentialMaterial, CredentialMaterialError,
    CredentialMaterialErrorKind, MAX_BASIC_PASSWORD_BYTES, MAX_BASIC_USERNAME_BYTES,
    MAX_CREDENTIAL_SECRET_BYTES, MAX_SECRET_STRING_BYTES, SecretCredential, SecretString,
};
pub use placement::{
    Auth, ChallengePolicy, MAX_AUTH_PLACEMENT_NAME_BYTES, MAX_AUTH_QUERY_VALUE_BYTES,
    MAX_AUTHORIZATION_SCHEME_BYTES, MAX_AUTHORIZATION_VALUE_BYTES,
};
pub use provider::{CredentialProvider, CredentialReason, CredentialRequest, RecoveryMode};
pub use slot::{CredentialHandle, CredentialRef};

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
