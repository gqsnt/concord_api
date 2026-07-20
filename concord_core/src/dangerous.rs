//! Feature-gated APIs whose use reduces ordinary Concord safety guarantees.
//!
//! Cargo features are unified for the compiled crate, so enabling either
//! dangerous capability in one dependency enables its public surface for the
//! whole resolved binary. Neither capability is enabled by default.

/// Reqwest's native retry builder.
///
/// Its classifier may observe native materialized URLs, including query
/// authentication. Application-owned diagnostics inside that classifier are
/// outside Concord's redaction boundary.
#[cfg(feature = "dangerous-custom-retry")]
pub use reqwest::retry::Builder as ReqwestRetryBuilder;

#[cfg(feature = "dangerous-raw-response")]
pub use crate::request::RawResponseRequest;
#[cfg(feature = "dangerous-raw-response")]
pub use crate::response::RawResponse;
