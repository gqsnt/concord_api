//! Core support for Concord-generated API clients.
//!
//! Phase 0 exposes no request execution surface. The only implementation is
//! the generated-private construction contract used by `concord_macros`.

#![forbid(unsafe_code)]

mod generated_contract;
mod request;

#[cfg(test)]
mod auth;

#[doc(hidden)]
pub mod __private;

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_test_target_runs() {}
}
