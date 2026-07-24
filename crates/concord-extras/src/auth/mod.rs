//! First-party credential-provider implementations.

mod oauth2_client;

pub use oauth2_client::{
    MAX_OAUTH2_AUTHORIZATION_HEADER_BYTES, MAX_OAUTH2_CLIENT_ID_BYTES,
    MAX_OAUTH2_CLIENT_SECRET_BYTES, MAX_OAUTH2_SCOPE_BYTES, MAX_OAUTH2_TOKEN_URL_BYTES,
    OAuth2ClientCredentials, OAuth2ClientCredentialsBuildError,
    OAuth2ClientCredentialsBuildErrorKind, OAuth2ClientCredentialsBuilder,
};
