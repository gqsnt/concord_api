//! First-party credential-provider implementations.

mod oauth2_client;

pub use oauth2_client::{
    OAuth2ClientCredentials, OAuth2ClientCredentialsBuildError,
    OAuth2ClientCredentialsBuildErrorKind, OAuth2ClientCredentialsBuilder,
};
