//! Incremental bodies: the infrastructure shared by the streaming formats (feature `stream`, implied
//! by `csv`, `jsonl` and `sse`).
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
//! **Reading a source the caller checked.** Every reader also has `check_head(&head)` (its own
//! media policy, exactly what `open` applies) and `read_for(source, operation)`: it reads any
//! [`ByteSource`] — an [`Incoming`](concord_core::Incoming) body (status already 2xx), a filter of
//! yours wrapped around it — without status or head checks, naming `operation` in every later
//! error as `open` does. Read through the `Incoming` itself, a broken body is a `Transport`
//! [`ReadError`] whose source is Concord's own `concord_core::Error` (naming the operation, URL
//! removed), taken back by value with [`ReadError::into_source_unredacted`]; so is any error type
//! a source of yours attaches to its [`SourceError`].
//!
//! Uploads are the mirror image: a one-shot record source (a [`futures_core::Stream`]) is wrapped
//! into a lazily produced `reqwest::Body` that serializes records as the transport asks for
//! bytes. Preparing the request polls the source **zero** times; sending polls it as needed, in
//! one pass. A source or serialization failure after bytes were sent fails the transfer; nothing
//! is replayed or rolled back. Dropping the send before the response arrives stops the polling:
//! once the drop returns, no new poll reaches the source. A body poll's result checked after
//! cancellation is discarded in favour of an error; results accepted before cancellation cannot
//! be recalled, and a whole body already accepted can still complete. The drop waits for an
//! active poll on another thread; a blocking source can hold it indefinitely or deadlock if it
//! waits on the dropping thread. A drop from inside the poll itself returns immediately; the
//! rest of that poll can run, but its result is discarded
//! ([`Decoder::execute`](concord_core::Decoder::execute), "Cancellation"). [`Upload`] pairs a
//! runtime-configured body with the `Content-Type` that
//! describes it.
//!
//! Diagnostics follow Concord's policy: `Display`/`Debug` show static labels, numeric positions,
//! the operation name the SDK supplied (never derived from a URL) and status codes; underlying
//! errors that may quote body content or URLs are reachable only through
//! [`ReadError::source_unredacted`] (by reference) and [`ReadError::into_source_unredacted`] (the
//! byte source's error, by value).

// The internals below serve the formats (`csv`, `jsonl`, `sse`; uploads only the first two);
// `stream` alone exposes only the public types, so they are unused there.
#[cfg_attr(
    not(any(feature = "csv", feature = "jsonl", feature = "sse")),
    allow(dead_code)
)]
mod driver;
#[cfg_attr(
    not(any(feature = "csv", feature = "jsonl", feature = "sse")),
    allow(dead_code)
)]
mod error;
#[cfg_attr(
    not(any(feature = "csv", feature = "jsonl", feature = "sse")),
    allow(dead_code)
)]
mod open;
#[cfg_attr(
    not(any(feature = "csv", feature = "jsonl", feature = "sse")),
    allow(dead_code)
)]
mod records;
mod source;
#[cfg_attr(not(any(feature = "csv", feature = "jsonl")), allow(dead_code))]
mod upload;

pub use error::{OpenError, OpenErrorKind, Position, ReadError, ReadErrorKind};
pub use open::Openable;
#[allow(unused_imports)]
pub(crate) use records::sealed::Sealed as RecordSealed;
pub use records::{Completion, RecordReader, RecordStream, StreamWatch};
pub use source::{ByteSource, Chunks, SourceError};
pub use upload::{FromIter, Upload, UploadError, from_iter};

#[allow(unused_imports)]
pub(crate) use driver::{Driver, SKIP_BUDGET, yield_now};
#[allow(unused_imports)]
pub(crate) use error::open_response;
#[allow(unused_imports)]
pub(crate) use open::open_input;
#[allow(unused_imports)]
pub(crate) use upload::{RecordBody, RecordEncoder};
