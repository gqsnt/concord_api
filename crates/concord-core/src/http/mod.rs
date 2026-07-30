//! Reviewed plain HTTP configuration values.

pub(crate) mod application;
pub(crate) mod build;
mod config;
#[cfg(test)]
mod development;
mod limits;
pub(crate) mod provider;
mod proxy;
pub(crate) mod response_progress;
mod status;
#[cfg(all(test, feature = "http2"))]
pub(crate) mod test_support;

pub use config::{HttpConfigBuilder, ProviderRetry};
pub use limits::{
    MAX_IDENTITY_PEM_BYTES, MAX_PROXY_URL_BYTES, MAX_ROOT_CERTIFICATE_BYTES, MAX_ROOT_CERTIFICATES,
    MAX_ROOT_CERTIFICATES_TOTAL_BYTES,
};
pub use proxy::SafeProxy;
