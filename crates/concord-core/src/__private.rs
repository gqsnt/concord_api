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

#[doc(hidden)]
pub use http::{HeaderName, Method};
#[doc(hidden)]
pub use url::Url;

#[doc(hidden)]
pub use crate::auth::Auth;
#[doc(hidden)]
pub use crate::body::{
    EmptyBody, PageRequestBodySpec, RequestBodyContext, RequestBodyMetadata, RequestBodySpec,
};
#[doc(hidden)]
pub use crate::client::ClientCore;
#[doc(hidden)]
pub use crate::endpoint::{
    EndpointCapabilities, EndpointDescription, EndpointId, RequestBodyCategory,
    ResponseAcquisition, SafeRequestPlan, TimeoutSelection,
};
#[doc(hidden)]
pub use crate::error::{ApiError, ClientBuildError, ClientInputKind};
#[doc(hidden)]
pub use crate::hooks::DebugLevel;
#[doc(hidden)]
pub use crate::limits::{
    MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES, MAX_REQUEST_HEADER_COUNT,
    MAX_REQUEST_URL_BYTES,
};
#[doc(hidden)]
pub use crate::pagination::PaginatedEndpoint;
#[doc(hidden)]
pub use crate::rate_limit::{
    RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitRule, RateLimitWindow,
};
#[doc(hidden)]
pub use crate::request::{
    EndpointSpec, HeadBuilder, PreparedHead, RequestMeta, RequestOptions, TimeoutOverride,
};
#[doc(hidden)]
pub use crate::response::{ApiResponse, PageResponseSpec, ResponseBodySpec};
#[doc(hidden)]
pub use crate::runtime::defaults::{
    DEFAULT_AUTH_FAILURE_COOLDOWN_SECONDS, DEFAULT_PAGINATION_IDENTITY_BYTES,
    DEFAULT_PAGINATION_MAX_ITEMS, DEFAULT_PAGINATION_MAX_PAGES, DEFAULT_PROVIDER_REQUEST_BYTES,
    DEFAULT_PROVIDER_RESPONSE_BYTES, DEFAULT_RATE_LIMIT_COOLDOWN_SECONDS,
    DEFAULT_RATE_LIMIT_KEY_BYTES, DEFAULT_REQUEST_BODY_BYTES, DEFAULT_RESPONSE_BODY_BYTES,
    DEFAULT_STATUS_BODY_BYTES, DEFAULT_STREAM_RESPONSE_BYTES,
};

/// Creates one allocation-free, compile-time-validated generated endpoint ID.
#[doc(hidden)]
#[must_use]
pub const fn endpoint_id_from_static(value: &'static str) -> EndpointId {
    EndpointId::from_static(value)
}

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
    pub async fn replace_static(
        &self,
        value: Result<M, crate::auth::CredentialMaterialError>,
    ) -> Result<(), crate::auth::AuthError> {
        self.selector.replace_static_result(value)
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
    ApiError::authentication(EndpointId::from_static(endpoint), method, error)
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

/// Produces a safe generated-builder error for one invalid single-string credential input.
#[doc(hidden)]
pub fn credential_input_error(
    field: &'static str,
    _error: crate::auth::CredentialMaterialError,
) -> ClientBuildError {
    ClientBuildError::invalid_credential(field)
}

/// Produces a safe generated-builder error for one invalid Basic credential input.
#[doc(hidden)]
pub fn basic_credential_input_error(
    username_field: &'static str,
    password_field: &'static str,
    error: crate::auth::CredentialMaterialError,
) -> ClientBuildError {
    let field = match error.kind() {
        crate::auth::CredentialMaterialErrorKind::BasicPasswordTooLarge => password_field,
        crate::auth::CredentialMaterialErrorKind::SecretStringTooLarge
        | crate::auth::CredentialMaterialErrorKind::CredentialSecretTooLarge
        | crate::auth::CredentialMaterialErrorKind::BasicUsernameTooLarge => username_field,
    };
    ClientBuildError::invalid_credential(field)
}

/// Creates one side-effect-free stable pending request wrapper.
#[doc(hidden)]
#[must_use]
pub fn request<E>(core: &ClientCore, endpoint: E) -> crate::Request<E> {
    crate::Request::new(core.clone(), endpoint)
}

/// Creates one no-effect sanitized plan through real public-head preparation.
#[doc(hidden)]
pub fn safe_request_plan(
    core: &ClientCore,
    description: &EndpointDescription,
    response_accept: Option<&'static str>,
    prepare_head: impl FnOnce(HeadBuilder) -> Result<PreparedHead, ApiError>,
) -> Result<SafeRequestPlan, ApiError> {
    let meta = RequestMeta::from_description(description);
    let builder = HeadBuilder::new(
        core.base_url(),
        meta,
        &RequestOptions::new(),
        core.default_debug(),
        core.default_runtime_endpoint_timeout(),
    );
    let mut head = prepare_head(builder)?;
    if head.endpoint_id() != description.endpoint_id() || head.method() != description.method() {
        return Err(ApiError::classified(
            description.endpoint_id().clone(),
            description.method().clone(),
            crate::error::ErrorCategory::Internal,
        ));
    }
    head.validate_rate_limit_key_limit(core.rate_limit_key_limit())?;
    if let Some(value) = response_accept {
        crate::response::validate_accept(value).map_err(|error| {
            drop(error);
            ApiError::classified(
                description.endpoint_id().clone(),
                description.method().clone(),
                crate::error::ErrorCategory::ResponseContract,
            )
        })?;
    }
    // Safe planning cannot call application-owned body metadata, but the
    // response specification's static Accept value is part of every finalized
    // head and must consume the same aggregate budget as execution.
    head.apply_media_headers(response_accept, None, false)?;
    head.validate_auth()?;
    if head.auth().is_empty() {
        core.preflight_url(head.endpoint_id(), head.method(), head.logical_url())?;
    } else {
        core.preflight_protected_url(head.endpoint_id(), head.method(), head.logical_url())?;
    }
    let public_headers = crate::redaction::SanitizedHeaderMap::from_headers_with_sensitive(
        head.public_headers(),
        Some(head.pagination_sensitive_headers()),
    );
    Ok(SafeRequestPlan::new(
        description.clone(),
        std::sync::Arc::from(head.safe_url()),
        public_headers,
        head.timeout(),
        0,
        head.rate_limit_plan().rules().len(),
    ))
}

/// Selects universal response metadata for one generated named request wrapper.
#[doc(hidden)]
#[must_use]
pub fn response_request<E>(request: crate::Request<E>) -> crate::ResponseRequest<E> {
    crate::ResponseRequest::from_generated(request)
}

/// Selects complete bounded bytes for one generated named request wrapper.
#[doc(hidden)]
#[must_use]
pub fn response_bytes_request<E>(request: crate::Request<E>) -> crate::ResponseBytesRequest<E> {
    crate::ResponseBytesRequest::from_generated(request)
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
