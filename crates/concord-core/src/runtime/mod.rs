//! Finite runtime configuration values.

mod config;
pub(crate) mod defaults;

pub(crate) use config::RuntimeConfig;
pub use config::RuntimeConfigBuilder;
