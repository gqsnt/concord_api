//! Core support for Concord-generated API clients.
//!
//! The crate contains safe error, redaction, managed HTTP, lifecycle, and
//! successful-response metadata, typed pending requests, and generated-private
//! integration for managed typed execution.

#![forbid(unsafe_code)]

#[cfg(feature = "dangerous-dev-tools")]
pub mod __development;
pub mod advanced;
#[cfg(feature = "dangerous-custom-retry")]
pub mod dangerous;
pub mod prelude;

mod auth;
mod body;
mod client;
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

pub use request::{Request, ResponseRequest};

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
