//! Incremental bodies: the infrastructure shared by the streaming formats (feature `stream`, implied
//! by `csv` and `jsonl`).
//!
//! The finite formats ([`Decode`](concord_core::Decode)) receive a body Concord has already
//! collected. The readers of this crate instead **open** a native `reqwest::Response` — obtained
//! with `Prepared::send_raw`, or by executing `Prepared::into_parts` yourself — and parse records
//! as chunks arrive:
//!
//! 1. **Opening** checks the status (2xx) and the format's head rules (`Content-Type`, a
//!    `header=` parameter) **before** the body is polled. A refusal is an [`OpenError`] that keeps
//!    the response unread ([`OpenError::into_response`]).
//! 2. **Reading** frames records from arbitrary chunk boundaries, retaining only the unfinished
//!    record (bounded by the format's limits), and yields owned records one at a time through an
//!    `async fn next()`. Nothing runs ahead of the caller: no task is spawned, no channel fills up;
//!    a caller that stops calling `next()` stops the reads. Dropping the reader drops the response.
//! 3. A failure after reading started is a [`ReadError`] with a [`Position`] (records delivered,
//!    input bytes consumed). Records already delivered stay delivered; the reader is then fused
//!    (every later `next()` returns `None`). A consumed body cannot be handed back unread.
//!
//! Uploads are the mirror image: a one-shot record source (a [`futures_core::Stream`]) is wrapped
//! into a lazily produced `reqwest::Body` that serializes records as the transport asks for
//! bytes. Preparing the request polls the source **zero** times; sending polls it as needed, in
//! one pass. A source or serialization failure after bytes were sent fails the transfer; nothing
//! is replayed or rolled back. [`Upload`] pairs a runtime-configured body with the `Content-Type`
//! that describes it.
//!
//! Diagnostics follow Concord's policy: `Display`/`Debug` show static labels, numeric positions,
//! the operation name the SDK supplied (never derived from a URL) and status codes; underlying
//! errors that may quote body content or URLs are reachable only through
//! [`ReadError::source_unredacted`].

// The internals below serve the formats (`csv`, `jsonl`); `stream` alone exposes only the public
// types, so they are unused there.
#[cfg_attr(not(any(feature = "csv", feature = "jsonl")), allow(dead_code))]
mod driver;
#[cfg_attr(not(any(feature = "csv", feature = "jsonl")), allow(dead_code))]
mod error;
mod source;
#[cfg_attr(not(any(feature = "csv", feature = "jsonl")), allow(dead_code))]
mod upload;

pub use error::{OpenError, OpenErrorKind, Position, ReadError, ReadErrorKind};
pub use source::{ByteSource, Chunks, SourceError};
pub use upload::{FromIter, Upload, UploadError, from_iter};

#[allow(unused_imports)]
pub(crate) use driver::{Driver, SKIP_BUDGET, yield_now};
#[allow(unused_imports)]
pub(crate) use error::open_response;
#[allow(unused_imports)]
pub(crate) use upload::{RecordBody, RecordEncoder};
