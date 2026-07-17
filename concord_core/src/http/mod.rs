//! Reviewed plain HTTP configuration values.

mod config;
mod proxy;

pub use config::{HttpConfigBuilder, ProviderRetry};
pub use proxy::SafeProxy;
