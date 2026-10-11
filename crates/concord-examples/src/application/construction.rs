//! Construction and explicit authentication (§23.1).
//!
//! The fake provider key is supplied by the application and never embedded
//! in source. A built client still needs a successful explicit credential
//! installation before its regional session calls can execute; nothing hides
//! that behind `regional()`.
//!
//! Applications must examine the [`StoreOutcome`]: `Superseded` is not a
//! transport failure and must not trigger an automatic new login that would
//! undo a logout or a newer sign-in.

use std::sync::Arc;
use std::time::Duration;

use concord::auth::{AuthError, CredentialVersion};
use concord::rate_limit::LocalRateLimiter;
use concord::{ApiError, ClientBuildError, SecretString, StoreOutcome};

use crate::contracts::catalog_v3::{CatalogApi, LoginInput, catalog_api};

/// 8 MiB response body limit.
const RESPONSE_BODY_LIMIT: usize = 8 * 1024 * 1024;

/// At most 100 pages per traversal.
const MAX_PAGES: u32 = 100;

// ANCHOR: build_catalog
/// Builds the canonical Catalog client for production use.
///
/// # Errors
///
/// One [`ClientBuildError`] accumulating every invalid or missing input.
pub fn build_catalog(
    tenant: String,
    bootstrap: SecretString,
) -> Result<CatalogApi, ClientBuildError> {
    catalog_builder(tenant, bootstrap).build()
}

/// The configured builder behind [`build_catalog`].
///
/// A deployment (or a loopback test) can layer further trusted transport
/// configuration, such as a proxy or private trust root, with one more
/// `configure_http` call before `build`; the closures apply in call order.
#[must_use]
pub fn catalog_builder(tenant: String, bootstrap: SecretString) -> catalog_api::CatalogApiBuilder {
    CatalogApi::builder()
        .set_tenant(tenant)
        .bootstrap_key(bootstrap)
        .configure_http(|http| http.require_https().connect_timeout(Duration::from_secs(3)))
        .configure_runtime(|runtime| {
            runtime
                .rate_limiter(Arc::new(LocalRateLimiter::new()))
                .response_body_limit(RESPONSE_BODY_LIMIT)
                .pagination_max_pages(MAX_PAGES)
        })
}
// ANCHOR_END: build_catalog

// ANCHOR: authenticate
/// Requests explicit local installation of a new session credential.
///
/// # Errors
///
/// The login exchange or local installation failure, with its truthful
/// contact facts.
pub async fn authenticate(client: &CatalogApi, user: String) -> Result<StoreOutcome, ApiError> {
    client
        .authenticate(LoginInput { user })
        .store_as_session()
        .await
}
// ANCHOR_END: authenticate

/// The application's view of one explicit sign-in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignIn {
    /// This sign-in installed a new session version.
    Installed {
        /// The installed credential version.
        version: CredentialVersion,
    },
    /// A newer explicit intent (a later sign-in, a logout or a manual
    /// replacement) superseded this one; its material was dropped.
    Superseded,
}

// ANCHOR: sign_in
/// Signs in once and reports the outcome without ever logging in again.
///
/// A `Superseded` outcome is returned as-is: automatically repeating the
/// login here would undo the logout or the newer sign-in that superseded it.
///
/// # Errors
///
/// The login exchange or local installation failure.
pub async fn sign_in(client: &CatalogApi, user: String) -> Result<SignIn, ApiError> {
    match authenticate(client, user).await? {
        StoreOutcome::Installed { version } => Ok(SignIn::Installed { version }),
        StoreOutcome::Superseded => Ok(SignIn::Superseded),
    }
}

/// Signs out locally: advances the session intent, removes ready material and
/// supersedes any in-flight explicit acquisition. No network work is done and
/// no remote session is revoked.
///
/// # Errors
///
/// A value-free [`AuthError`] when the local store fails closed.
pub fn sign_out(client: &CatalogApi) -> Result<(), AuthError> {
    client.credentials().clear_session()
}
// ANCHOR_END: sign_in
