//! Authentication examples using only façade-owned semantic contracts.
//!
//! Required feature: `oauth2-client`; building the fixed HTTPS declaration also
//! requires `default-tls`. Mode: construction and deterministic local
//! provider/application execution. Documentation owner:
//! `docs/guide/authentication.md`. Proving tests: `tests/construction.rs` and
//! `tests/local_runtime.rs`.

pub mod api;
pub mod extensions;
pub mod usage;
