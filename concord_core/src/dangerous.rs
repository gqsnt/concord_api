//! Feature-gated APIs whose use reduces ordinary Concord safety guarantees.

/// Reqwest's native retry builder.
///
/// Its classifier may observe native materialized URLs, including query
/// authentication. Application-owned diagnostics inside that classifier are
/// outside Concord's redaction boundary.
pub use reqwest::retry::Builder as ReqwestRetryBuilder;
