//! Core support for Concord-generated API clients.
//!
//! No request-execution surface exists yet. The crate contains the Phase 0
//! generated-private construction contract used by `concord_macros` and the
//! Phase 1 safe error, redaction, and sanitized-metadata foundations.

#![forbid(unsafe_code)]

pub mod advanced;
#[cfg(feature = "dangerous-custom-retry")]
pub mod dangerous;
pub mod prelude;

mod codec;
mod error;
mod generated_contract;
mod http;
mod redaction;
mod request;
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
