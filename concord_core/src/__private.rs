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

#[doc(hidden)]
pub use http::Method;
#[doc(hidden)]
pub use url::Url;

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
    RequestOptions, ResponseHandler, TimeoutOverride,
};

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

/// Creates one side-effect-free stable pending request wrapper.
#[doc(hidden)]
#[must_use]
pub fn request<E>(core: &ClientCore, endpoint: E) -> crate::Request<E> {
    crate::Request::new(core.clone(), endpoint)
}
