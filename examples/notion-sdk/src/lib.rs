//! A small, read-only Notion SDK built with Concord.
//!
//! [`NotionClient`] pins [`NOTION_VERSION`] and supplies typed operations, bounded JSON
//! responses, provider errors, and lazy cursor traversals. See the crate README for both
//! workflows and the limits of property and block completeness. Fixtures are hand-authored,
//! not a vendor conformance suite.

#![doc = include_str!("../README.md")]

pub mod api;
mod client;
mod error;
pub mod models;
pub mod params;
pub mod render;

pub use client::{BlockPages, NotionClient, QueryPages, Response};
pub use error::{EnvelopeProblem, Error, ErrorCode, ProviderError};
pub use params::{PageSize, PageSizeError, Query, SelectEquals};

/// The transport types accepted by [`NotionClient`], re-exported through Concord.
pub use concord::reqwest;

/// API version checked against the official references on 2026-11-06.
pub const NOTION_VERSION: &str = "2026-03-11";
/// Maximum decoded bytes in one successful response (8 MiB).
pub const RESPONSE_LIMIT: usize = 8 * 1024 * 1024;
/// Maximum decoded bytes in a provider error response (64 KiB).
pub const ERROR_LIMIT: usize = 64 * 1024;

/// Public pagination vocabulary for caps, checkpoints, streams and partial outcomes.
pub use concord_pagination as pagination;
