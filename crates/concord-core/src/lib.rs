//! Semantic runtime kernel and extension contracts for Concord.
//!
//! This implementation package owns bounded managed HTTP, lifecycle ordering,
//! redaction, transport isolation, and the semantic contracts used by
//! extensions. Applications should depend on the `concord` façade rather than
//! importing this crate directly.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod contracts;
#[cfg(feature = "records")]
pub mod records;

mod atom;
mod auth;
pub mod body;
mod client;
pub mod codec;
mod construction;
mod controls;
mod endpoint;
mod error;
mod generated_contract;
mod hooks;
mod http;
pub mod inspection;
mod limits;
mod operation;
pub mod pagination;
pub mod rate_limit;
mod redaction;
mod request;
mod response;
pub mod retry;
mod runtime;
mod value;

pub use request::{AcceptedBytesRequest, Request, ResponseRequest};

mod execute;

pub use execute::StoreRequest;

#[doc(hidden)]
pub mod __private;
