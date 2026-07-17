//! Core support for Concord-generated API clients.
//!
//! The crate contains safe error, redaction, managed HTTP, lifecycle, and
//! successful-response metadata foundations plus generated-private integration.
//! Executable typed pending requests are assembled by the Phase 2 runtime work.

#![forbid(unsafe_code)]

#[cfg(feature = "dangerous-dev-tools")]
pub mod __development;
pub mod advanced;
#[cfg(feature = "dangerous-custom-retry")]
pub mod dangerous;
pub mod prelude;

mod codec;
mod error;
mod generated_contract;
mod hooks;
mod http;
mod operation;
mod redaction;
mod request;
mod response;
mod runtime;

#[cfg(test)]
mod auth;

#[cfg(test)]
mod execute;

#[cfg(test)]
mod pagination;

#[doc(hidden)]
pub mod __private;

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_test_target_runs() {}
}
