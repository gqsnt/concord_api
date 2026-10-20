//! Ecosystem-backed buffered request and response codecs and adapters.

#[cfg(feature = "json")]
mod json;
mod not_found;

#[cfg(feature = "json")]
pub use json::Json;
pub use not_found::NotFound;
