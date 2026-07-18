//! Unstable integration surface for Concord-generated Rust.

#[doc(hidden)]
pub const GENERATED_CONTRACT_VERSION: u32 = crate::generated_contract::VERSION;

/// Verifies the semantic generated-code contract during const evaluation.
#[doc(hidden)]
pub const fn assert_generated_contract(expected: u32) {
    assert!(
        GENERATED_CONTRACT_VERSION == expected,
        "concord macro/core contract mismatch"
    );
}

/// Produces a focused const-evaluation diagnostic for generated JSON syntax
/// when the runtime dependency omitted its `json` feature.
#[doc(hidden)]
pub const fn assert_json_feature() {
    #[cfg(not(feature = "json"))]
    panic!("Concord JSON DSL requires the `concord_core/json` feature");
}

/// Produces a focused const-evaluation diagnostic for generated multipart
/// syntax when the runtime dependency omitted its `multipart` feature.
#[doc(hidden)]
pub const fn assert_multipart_feature() {
    #[cfg(not(feature = "multipart"))]
    panic!("Concord multipart DSL requires the `concord_core/multipart` feature");
}

/// Produces a focused const-evaluation diagnostic for generated dangerous
/// custom retry syntax when the runtime dependency omitted its feature.
#[doc(hidden)]
pub const fn assert_custom_retry_feature() {
    #[cfg(not(feature = "dangerous-custom-retry"))]
    panic!(
        "Concord `retry custom PATH` requires the `concord_core/dangerous-custom-retry` feature"
    );
}

#[doc(hidden)]
pub use http::{HeaderName, Method};
#[doc(hidden)]
pub use url::Url;

#[doc(hidden)]
pub use crate::auth::Auth;
#[doc(hidden)]
pub use crate::body::StreamRequestBody;
#[doc(hidden)]
pub use crate::client::ClientCore;
#[doc(hidden)]
pub use crate::error::{ApiError, ClientBuildError};
#[doc(hidden)]
pub use crate::hooks::DebugLevel;
#[doc(hidden)]
pub use crate::request::{
    BodyMetadata, BodySpec, BufferedResponse, BytesResponse, CodecBody, EmptyBody, EndpointSpec,
    HeadBuilder, MetadataResponse, NoContentResponse, PreparedBody, PreparedHead, RequestMeta,
    RequestOptions, ResponseHandler, StreamResponseHandler, TimeoutOverride,
};

/// Generated-private owner for one credential source selection.
#[doc(hidden)]
pub struct GeneratedCredential<M>
where
    M: crate::auth::CredentialMaterial,
{
    selector: std::sync::Arc<crate::auth::SlotSelector<M>>,
}

impl<M> Clone for GeneratedCredential<M>
where
    M: crate::auth::CredentialMaterial,
{
    fn clone(&self) -> Self {
        Self {
            selector: std::sync::Arc::clone(&self.selector),
        }
    }
}

impl<M> GeneratedCredential<M>
where
    M: crate::auth::CredentialMaterial,
{
    #[doc(hidden)]
    pub fn static_value(core: &ClientCore, value: M) -> Self {
        Self {
            selector: crate::auth::SlotSelector::static_value(
                value,
                core.auth_failure_cooldown_cap(),
            ),
        }
    }

    #[doc(hidden)]
    pub fn endpoint(core: &ClientCore) -> Self {
        Self {
            selector: crate::auth::SlotSelector::endpoint(core.auth_failure_cooldown_cap()),
        }
    }

    #[doc(hidden)]
    pub fn provider<P>(core: &ClientCore, provider: P) -> Self
    where
        P: crate::auth::CredentialProvider<Material = M>,
    {
        Self {
            selector: crate::auth::SlotSelector::provider(
                provider,
                core.auth_failure_cooldown_cap(),
            ),
        }
    }

    #[doc(hidden)]
    pub fn provider_arc(
        core: &ClientCore,
        provider: std::sync::Arc<dyn crate::auth::CredentialProvider<Material = M>>,
    ) -> Self {
        Self {
            selector: crate::auth::SlotSelector::provider_arc(
                provider,
                core.auth_failure_cooldown_cap(),
            ),
        }
    }

    #[doc(hidden)]
    pub fn credential(&self) -> crate::auth::CredentialRef<M> {
        crate::auth::CredentialRef::new(std::sync::Arc::clone(&self.selector))
    }

    #[doc(hidden)]
    pub fn endpoint_handle(&self) -> crate::auth::CredentialHandle<M> {
        crate::auth::CredentialHandle::new(std::sync::Arc::clone(&self.selector))
    }

    #[doc(hidden)]
    pub async fn replace_static(&self, value: M) -> Result<(), crate::auth::AuthError> {
        self.selector.replace_static(value)
    }

    #[doc(hidden)]
    pub async fn replace_provider<P>(&self, provider: P) -> Result<(), crate::auth::AuthError>
    where
        P: crate::auth::CredentialProvider<Material = M>,
    {
        self.selector.replace_provider(provider)
    }

    #[doc(hidden)]
    pub async fn set(&self, value: M) -> Result<(), crate::auth::AuthError> {
        self.selector.set(value)
    }

    #[doc(hidden)]
    pub async fn clear(&self) -> Result<(), crate::auth::AuthError> {
        self.selector.clear()
    }

    #[doc(hidden)]
    pub fn is_set(&self) -> bool {
        self.selector.is_set()
    }
}

#[cfg(feature = "json")]
#[doc(hidden)]
pub fn oauth2_client_credentials(
    token_url: &str,
    client_id: crate::auth::SecretString,
    client_secret: crate::auth::SecretString,
    scope: Option<&str>,
) -> Result<
    impl crate::auth::CredentialProvider<Material = crate::auth::AccessToken>,
    ClientBuildError,
> {
    crate::auth::OAuth2ClientCredentials::new(token_url, client_id, client_secret, scope)
}

#[cfg(feature = "json")]
#[doc(hidden)]
pub fn oauth2_client_credentials_for_replace(
    token_url: &str,
    client_id: crate::auth::SecretString,
    client_secret: crate::auth::SecretString,
    scope: Option<&str>,
) -> Result<
    impl crate::auth::CredentialProvider<Material = crate::auth::AccessToken>,
    crate::auth::AuthError,
> {
    crate::auth::OAuth2ClientCredentials::new(token_url, client_id, client_secret, scope)
        .map_err(|_| crate::auth::AuthError::new(crate::error::AuthErrorKind::InvalidConfiguration))
}

#[cfg(not(feature = "json"))]
#[doc(hidden)]
pub struct MissingJsonOAuthProvider;

#[cfg(not(feature = "json"))]
impl crate::auth::CredentialProvider for MissingJsonOAuthProvider {
    type Material = crate::auth::AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: crate::auth::CredentialRequest<'a>,
    ) -> crate::hooks::BoxFuture<'a, Result<Self::Material, crate::auth::AuthError>> {
        Box::pin(async {
            Err(crate::auth::AuthError::new(
                crate::error::AuthErrorKind::InvalidConfiguration,
            ))
        })
    }
}

#[cfg(not(feature = "json"))]
#[doc(hidden)]
pub fn oauth2_client_credentials(
    _token_url: &str,
    _client_id: crate::auth::SecretString,
    _client_secret: crate::auth::SecretString,
    _scope: Option<&str>,
) -> Result<MissingJsonOAuthProvider, ClientBuildError> {
    Ok(MissingJsonOAuthProvider)
}

#[cfg(not(feature = "json"))]
#[doc(hidden)]
pub fn oauth2_client_credentials_for_replace(
    _token_url: &str,
    _client_id: crate::auth::SecretString,
    _client_secret: crate::auth::SecretString,
    _scope: Option<&str>,
) -> Result<MissingJsonOAuthProvider, crate::auth::AuthError> {
    Ok(MissingJsonOAuthProvider)
}

/// Borrows a generated endpoint through the opaque request wrapper without
/// starting execution.
#[doc(hidden)]
pub fn inspect_endpoint<E, T>(request: &crate::Request<E>, inspect: impl FnOnce(&E) -> T) -> T {
    inspect(&request.endpoint)
}

/// Reduces a generated endpoint-acquisition storage failure to one safe API error.
#[doc(hidden)]
pub fn credential_store_error(
    endpoint: &'static str,
    method: Method,
    error: crate::auth::AuthError,
) -> ApiError {
    ApiError::authentication(endpoint, method, error)
}

/// Creates the ordinary generated-client HTTP configuration baseline.
#[doc(hidden)]
#[must_use]
pub fn default_http_config() -> crate::http::HttpConfigBuilder {
    crate::http::HttpConfigBuilder::new()
}

/// Creates a generated-client HTTP baseline with a lazily selected dangerous
/// custom retry factory. The factory is invoked only if no later builder
/// operation replaces the selection.
#[cfg(feature = "dangerous-custom-retry")]
#[doc(hidden)]
#[must_use]
pub fn default_http_config_with_custom_retry(
    factory: fn() -> crate::dangerous::ReqwestRetryBuilder,
) -> crate::http::HttpConfigBuilder {
    crate::http::HttpConfigBuilder::new().generated_custom_retry_factory(factory)
}

/// Keeps missing-feature generated code structurally type-checkable so the
/// adjacent const feature assertion is the single focused diagnostic.
#[cfg(not(feature = "dangerous-custom-retry"))]
#[doc(hidden)]
#[must_use]
pub fn default_http_config_with_custom_retry<F>(_factory: F) -> crate::http::HttpConfigBuilder {
    crate::http::HttpConfigBuilder::new()
}

/// Creates the ordinary generated-client runtime configuration baseline.
#[doc(hidden)]
#[must_use]
pub fn default_runtime_config() -> crate::runtime::RuntimeConfigBuilder {
    crate::runtime::RuntimeConfigBuilder::new()
}

/// Builds one private shared core for generated client storage.
#[doc(hidden)]
pub fn build_client_core(
    fixed_base: &str,
    http: crate::http::HttpConfigBuilder,
    runtime: crate::runtime::RuntimeConfigBuilder,
) -> Result<ClientCore, ClientBuildError> {
    ClientCore::build(fixed_base, http, runtime)
}

/// Produces the structured generated-builder error for a repeated HTTP
/// configuration closure without invoking that duplicate closure.
#[doc(hidden)]
pub fn duplicate_http_configuration_error() -> ClientBuildError {
    ClientBuildError::new(crate::error::ClientBuildErrorKind::HttpConfiguration)
}

/// Produces the structured generated-builder error for a repeated runtime
/// configuration closure without invoking that duplicate closure.
#[doc(hidden)]
pub fn duplicate_runtime_configuration_error() -> ClientBuildError {
    ClientBuildError::new(crate::error::ClientBuildErrorKind::RuntimeConfiguration)
}

/// Produces the safe generated-builder classification for a missing required
/// value without retaining or formatting that value's DSL name.
#[doc(hidden)]
pub fn missing_input_error() -> ClientBuildError {
    ClientBuildError::new(crate::error::ClientBuildErrorKind::MissingInput)
}

/// Creates one side-effect-free stable pending request wrapper.
#[doc(hidden)]
#[must_use]
pub fn request<E>(core: &ClientCore, endpoint: E) -> crate::Request<E> {
    crate::Request::new(core.clone(), endpoint)
}

/// Consumes a pending request, maps its endpoint value, and preserves every
/// generic request-local and runtime field without starting execution.
#[doc(hidden)]
#[must_use]
pub fn map_endpoint<E, F>(request: crate::Request<E>, map: F) -> crate::Request<E>
where
    F: FnOnce(E) -> E,
{
    let crate::Request {
        core,
        endpoint,
        options,
    } = request;
    crate::Request {
        core,
        endpoint: map(endpoint),
        options,
    }
}
