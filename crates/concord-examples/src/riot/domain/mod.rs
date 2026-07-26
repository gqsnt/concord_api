//! Domain-only Riot routing, request, and response types.
//!
//! Required features: `json` and `rate-limit` through the Riot bundle.
//! Mode: compile/serde support for deterministic planning and manual live use.
//! Documentation owner: `docs/recipes/riot.md`.
//! Proving test: `tests/riot_contract.rs`.

pub mod account;
pub mod data_dragon;
pub mod league;
pub mod match_api;
pub mod routing;
pub mod status;
pub mod tournament;

pub use account::*;
pub use data_dragon::*;
pub use league::*;
pub use match_api::*;
pub use routing::*;
pub use status::*;
pub use tournament::*;
