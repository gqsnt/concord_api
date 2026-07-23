//! Ecosystem-backed buffered request and response codecs.

#[cfg(feature = "json")]
mod json;

#[cfg(feature = "json")]
pub use json::Json;
