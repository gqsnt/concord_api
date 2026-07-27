//! Semantic runtime kernel and extension contracts for Concord.
//!
//! This implementation package owns bounded managed HTTP, lifecycle ordering,
//! redaction, transport isolation, and the semantic contracts used by
//! extensions. Applications should depend on the `concord` façade rather than
//! importing this crate directly.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod contracts;
#[cfg(feature = "dangerous-custom-retry")]
pub mod dangerous;
#[cfg(feature = "records")]
pub mod records;

mod auth;
mod body;
mod client;
pub mod codec;
mod construction;
mod endpoint;
mod error;
mod generated_contract;
mod hooks;
mod http;
mod limits;
mod operation;
pub mod pagination;
pub mod rate_limit;
mod redaction;
mod request;
mod response;
pub mod retry;
mod runtime;

pub use pagination::{PageLimit, PaginatedRequest};
pub use request::{Request, ResponseBytesRequest, ResponseRequest};

mod execute;

#[doc(hidden)]
pub mod __private;
