//! Optional first-party implementations of Concord's public semantic contracts.
//!
//! The crate intentionally depends only on `concord_core`. Implementations are
//! added in later extraction phases without access to native transport values.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod codec;
pub mod pagination;
#[cfg(any(feature = "csv", feature = "ndjson"))]
pub mod records;
