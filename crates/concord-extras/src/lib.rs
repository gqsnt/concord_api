//! Optional first-party implementations of Concord's public semantic contracts.
//!
//! This implementation package intentionally depends only on `concord_core`
//! and has no access to native transport values. Applications select these
//! implementations through features and domain modules on the `concord`
//! façade.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod atom;
#[cfg(feature = "oauth2-client")]
pub mod auth;
pub mod codec;
pub mod pagination;
#[cfg(feature = "rate-limit")]
pub mod rate_limit;
#[cfg(any(feature = "csv", feature = "ndjson"))]
pub mod records;
