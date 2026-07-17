//! Reviewed plain HTTP configuration values.

#[allow(dead_code, reason = "assembled by the later private ClientCore")]
mod application;
#[allow(dead_code, reason = "assembled by the later private ClientCore")]
mod build;
mod config;
#[allow(dead_code, reason = "assembled by the later private ClientCore")]
mod provider;
mod proxy;
#[allow(dead_code, reason = "called by the private managed response boundary")]
mod status;

pub use config::{HttpConfigBuilder, ProviderRetry};
pub use proxy::SafeProxy;
