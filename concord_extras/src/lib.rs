//! Optional first-party implementations of Concord's public semantic contracts.
//!
//! The crate intentionally depends only on `concord_core`. Implementations are
//! added in later extraction phases without access to native transport values.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "oauth2-client")]
pub mod auth;
pub mod codec;
pub mod pagination;
#[cfg(feature = "rate-limit-governor")]
pub mod rate_limit;
#[cfg(any(feature = "csv", feature = "ndjson"))]
pub mod records;
