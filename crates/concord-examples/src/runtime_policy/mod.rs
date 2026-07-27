//! Runtime observation and production HTTP configuration.
//!
//! These examples configure the generated builder without changing the remote
//! contract declaration.

pub mod hooks;
pub mod retries;

#[cfg(feature = "default-tls")]
pub mod production_http;
