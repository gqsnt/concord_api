//! Explicitly gated capabilities with narrower ordinary safety guarantees.
//!
//! Child modules state their exact feature gates and proving tests.

#[cfg(feature = "dangerous-custom-retry")]
pub mod custom_retry;

#[cfg(all(feature = "dangerous-raw-response", feature = "json"))]
pub mod raw_response;
