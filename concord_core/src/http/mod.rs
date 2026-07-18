//! Reviewed plain HTTP configuration values.

pub(crate) mod application;
#[allow(
    dead_code,
    reason = "owns both managed clients for the private ClientCore"
)]
pub(crate) mod build;
mod config;
#[cfg(any(test, feature = "dangerous-dev-tools"))]
mod development;
pub(crate) mod provider;
mod proxy;
#[allow(dead_code, reason = "called by the private managed response boundary")]
mod status;
#[cfg(all(test, feature = "http2"))]
pub(crate) mod test_support;

pub use config::{HttpConfigBuilder, ProviderRetry};
pub use proxy::SafeProxy;
