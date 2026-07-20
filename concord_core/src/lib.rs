//! Core support for Concord-generated API clients.
//!
//! The crate contains safe error, redaction, managed HTTP, lifecycle, and
//! successful-response metadata, typed pending requests, and generated-private
//! integration for managed typed execution.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "dangerous-dev-tools")]
pub mod __development;
pub mod advanced;
#[cfg(any(feature = "dangerous-custom-retry", feature = "dangerous-raw-response"))]
pub mod dangerous;
pub mod prelude;
#[cfg(feature = "records")]
pub mod records;

mod auth;
mod body;
mod client;
mod codec;
mod error;
mod generated_contract;
mod hooks;
mod http;
mod operation;
mod pagination;
mod rate_limit;
mod redaction;
mod request;
mod response;
mod runtime;

pub use pagination::{PageLimit, PaginatedRequest};
pub use request::{Request, ResponseRequest};

mod execute;

#[doc(hidden)]
pub mod __private;

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_test_target_runs() {}
}
