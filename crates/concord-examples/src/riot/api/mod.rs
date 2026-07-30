//! Importable Riot and Data Dragon contract declarations.
//!
//! Required features: `json` and `rate-limit`. Mode: deterministic
//! construction/planning. Documentation owner: `docs/spec.md`.
//! Proving test: `tests/riot_contract.rs`.

pub mod data_dragon;
pub mod riot;

pub use data_dragon::{DDragonApi, d_dragon_api};
pub use riot::{RiotApi, riot_api};
