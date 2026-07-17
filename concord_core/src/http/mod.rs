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
#[allow(dead_code, reason = "assembled by the later private ClientCore")]
mod provider;
mod proxy;
#[allow(dead_code, reason = "called by the private managed response boundary")]
mod status;

pub use config::{HttpConfigBuilder, ProviderRetry};
pub use proxy::SafeProxy;
