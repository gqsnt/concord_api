//! Credential material, providers, placement, and private slot state.

#![cfg_attr(test, allow(dead_code))]

mod error;
mod material;
#[cfg(feature = "oauth2-client")]
mod oauth2;
mod placement;
mod provider;
mod slot;

pub(crate) use placement::{AuthPlan, PreparedAuthPlan};
pub(crate) use slot::SlotSelector;

pub use error::AuthError;
pub use material::{
    AccessToken, ApiKey, BasicCredential, CredentialMaterial, SecretCredential, SecretString,
};
#[cfg(feature = "oauth2-client")]
pub(crate) use oauth2::OAuth2ClientCredentials;
pub use placement::{Auth, ChallengePolicy};
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
