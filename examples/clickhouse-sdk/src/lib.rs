//! A bounded SDK for the ClickHouse HTTP interface, declared with `concord::api!`.
//!
//! - **Queries** ([`ClickHouse::select`]): SQL text with typed placeholders (`{name:Type}`) and
//!   values bound by name ([`Query`]), sent as `GET /?query=..&param_<name>=..` (read-only on the
//!   server); rows read incrementally as `CSVWithNames` or `JSONEachRow`, chosen at run time
//!   ([`OutputFormat`]), into any `serde::Deserialize` row type ([`Rows`]).
//! - **Inserts** ([`ClickHouse::insert`]): rows from a one-shot `Stream` serialized while the body
//!   is sent, as `JSONEachRow` or `CSVWithNames`, into a [`Table`].
//! - **Metadata** ([`ResponseMeta`]): query ID, summary and progress counters, format, time zone,
//!   exception code and tag — each `Option`al: never promised on every response.
//! - **Completion** ([`Outcome`]): a 2xx status is not a complete result. The SDK finds the
//!   exception block ClickHouse writes into a started body ([`Error::LateException`]), reports a
//!   broken body ([`Error::Read`]), and calls a clean end [`Outcome::Complete`] only when the
//!   response announced a nonempty exception tag. This outcome assumes the server uses the
//!   tagged-block contract; other exception formats are not detected.
//! - **Cancellation** ([`ClickHouse::cancel`]): `KILL QUERY` by query ID; dropping [`Rows`] only
//!   stops the client's read.
//!
//! Excluded: general SQL support, database management, sessions, transactions, compression,
//! every other format. See the crate README for the workflows and the detection contract.

mod client;
mod error;
mod format;
mod guard;
mod insert;
mod meta;
mod query;
mod rows;
mod wire;

pub use client::{CallContext, ClickHouse, Credentials, ERROR_BODY_LIMIT, KillStatus, Select};
pub use error::{
    Error, LateException, LateSource, ReadFailure, ResponseError, ResponseProblem, ServerException,
};
pub use format::OutputFormat;
pub use insert::{Insert, InsertReceipt};
pub use meta::{Counters, HeaderError, ResponseMeta};
pub use query::{Identifier, Param, Placeholder, Query, QueryId, RequestError, Table, escape};
pub use rows::{Collected, Outcome, Rows};

/// Concord's upload helper, re-exported: `stream::from_iter(rows)` turns an iterator of
/// `Result<T, E>` into the one-shot source an insert takes.
pub use concord::stream;

/// The README's examples, compiled (not run) as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
