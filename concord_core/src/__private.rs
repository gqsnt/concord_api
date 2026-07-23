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

/// Produces a focused const-evaluation diagnostic for generated multipart
/// syntax when the runtime dependency omitted its `multipart` feature.
#[doc(hidden)]
pub const fn assert_multipart_feature() {
    #[cfg(not(feature = "multipart"))]
    panic!("Concord multipart DSL requires the `concord_core/multipart` feature");
}

#[doc(hidden)]
pub use http::{HeaderName, Method};
#[doc(hidden)]
pub use url::Url;

#[doc(hidden)]
pub use crate::__concord_emit_generated_development_method as emit_generated_development_method;
#[doc(hidden)]
pub use crate::__concord_emit_generated_raw_response_method as emit_generated_raw_response_method;
#[doc(hidden)]
pub use crate::auth::Auth;
#[doc(hidden)]
pub use crate::body::{EmptyBody, RequestBodyContext, RequestBodyMetadata, RequestBodySpec};
#[doc(hidden)]
pub use crate::client::ClientCore;
#[doc(hidden)]
pub use crate::error::{ApiError, ClientBuildError, ClientInputKind};
#[doc(hidden)]
pub use crate::hooks::DebugLevel;
#[doc(hidden)]
pub use crate::pagination::PaginatedEndpoint;
#[doc(hidden)]
pub use crate::rate_limit::{
    RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitRule, RateLimitWindow,
};
#[doc(hidden)]
pub use crate::request::{
    EndpointSpec, HeadBuilder, PreparedHead, RawResponseEndpoint, RequestMeta, RequestOptions,
    TimeoutOverride,
};
#[doc(hidden)]
pub use crate::response::{ApiResponse, PageResponseSpec, ResponseBodySpec};

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

/// Creates the ordinary generated-client runtime configuration baseline.
#[doc(hidden)]
#[must_use]
pub fn default_runtime_config() -> crate::runtime::RuntimeConfigBuilder {
    crate::runtime::RuntimeConfigBuilder::new()
}

/// Installs a façade-composed generated default without overriding an explicit
/// application limiter.
#[doc(hidden)]
#[must_use]
pub fn install_generated_default_limiter(
    runtime: crate::runtime::RuntimeConfigBuilder,
    limiter: std::sync::Arc<dyn crate::rate_limit::RateLimiter>,
) -> crate::runtime::RuntimeConfigBuilder {
    runtime.install_generated_default_limiter(limiter)
}

/// Applies generated static rate-limit requirements without constructing a
/// custom observer.
#[doc(hidden)]
pub fn prepare_generated_rate_limits(
    runtime: crate::runtime::RuntimeConfigBuilder,
    requires_limiter: bool,
) -> Result<crate::runtime::RuntimeConfigBuilder, ClientBuildError> {
    runtime.prepare_generated_rate_limits(requires_limiter)
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

/// Produces a safe generated-builder error for one missing required input.
#[doc(hidden)]
pub const fn missing_input_error(
    field: &'static str,
    input_kind: ClientInputKind,
) -> ClientBuildError {
    ClientBuildError::missing_input(field, input_kind)
}

/// Creates one side-effect-free stable pending request wrapper.
#[doc(hidden)]
#[must_use]
pub fn request<E>(core: &ClientCore, endpoint: E) -> crate::Request<E> {
    crate::Request::new(core.clone(), endpoint)
}

/// Selects universal response metadata for one generated named request wrapper.
#[doc(hidden)]
#[must_use]
pub fn response_request<E>(request: crate::Request<E>) -> crate::ResponseRequest<E> {
    crate::ResponseRequest::from_generated(request)
}

/// Selects bounded raw acquisition for one generated named request wrapper.
#[cfg(feature = "dangerous-raw-response")]
#[doc(hidden)]
#[must_use]
pub fn raw_response_request<E>(
    request: crate::Request<E>,
) -> crate::dangerous::RawResponseRequest<E> {
    crate::dangerous::RawResponseRequest::from_generated(request)
}

/// Creates one side-effect-free stable pagination wrapper.
#[doc(hidden)]
#[must_use]
pub fn paginate_request<E, L>(request: crate::Request<E>, limit: L) -> crate::PaginatedRequest<E>
where
    L: Into<crate::PageLimit>,
{
    crate::PaginatedRequest::new(request, limit.into())
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
