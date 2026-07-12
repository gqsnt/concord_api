mod config;

#[allow(deprecated)]
pub use config::{DebugConfig, RuntimeConfig};

#[cfg(feature = "dangerous-dev-tools")]
#[allow(deprecated)]
pub use config::DevBodyCaptureConfig;
