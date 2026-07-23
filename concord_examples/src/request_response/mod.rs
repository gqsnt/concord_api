//! Request and response representations from ordinary codecs to bounded streams.
//!
//! Child modules list their required features, execution mode, documentation
//! owner, and proving acceptance test.

pub mod specifications;

#[cfg(all(feature = "json", feature = "multipart"))]
pub mod bodies;

#[cfg(all(feature = "records", feature = "csv", feature = "ndjson"))]
pub mod records;
