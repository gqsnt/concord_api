//! Product-scale Riot and Data Dragon contract narrative.
//!
//! Required features: `json` and `rate-limit`; the `riot-live`
//! feature additionally enables TLS, HTTP/2, and Tokio for the manual binary.
//! Mode: deterministic construction/planning in CI and explicitly manual live
//! execution. Documentation owner: `docs/spec.md`.
//! Proving test: `tests/riot_contract.rs`.

pub mod api;
pub mod domain;
pub mod usage;
