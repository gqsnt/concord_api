//! Riot support tree for the migrate-v3 compile-through test.
//!
//! `domain/` is a verbatim copy of the v2.1 baseline Riot domain types:
//! ordinary Rust that the migration leaves byte-for-byte unchanged.
//! `api/riot.rs`, `api/data_dragon.rs` and `usage.rs` are the exact migrate-v3
//! output of the captured v2.1 fixtures in `tools/xtask/fixtures/migrate_v3/`.

pub mod api;
pub mod domain;
pub mod usage;
