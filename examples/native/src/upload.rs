//! W1 (S01–S04): a one-shot request stream with a fixed media type, through a custom `Encode`.
//!
//! [`Encode::encode`] receives the payload **by value** when the request is prepared, so a
//! non-`Clone`, read-once source can be moved into a streaming `reqwest::Body`
//! (`Body::wrap_stream`, reqwest's `stream` feature, enabled in this crate's `Cargo.toml`, never by
//! Concord). Preparing wraps the source without polling it; executing the request reads it once,
//! as a `Transfer-Encoding: chunked` body (its length is unknown). The media type is the format's
//! constant [`Encode::CONTENT_TYPE`]; a `request Streamed "type/subtype";` statement or a
//! `content-type` header on the endpoint replaces it (author guide 5.3).
//!
//! The operation cannot be copied because the source cannot (a request is `Clone` only when its
//! values and body are), and a [`concord::Prepared`] is never `Clone`: a second upload needs a
//! fresh source, and nothing replays the first one behind your back.
//!
//! ```compile_fail,E0277
//! # use concord_example_native::{api::Inventory, upload::OneShot};
//! fn needs_clone<T: Clone>(_: &T) {}
//! fn copy_the_operation(api: &Inventory) {
//!     let (source, _tally) = OneShot::new(Vec::new());
//!     let operation = api.upload(1, source);
//!     needs_clone(&operation); // the operation owns a non-`Clone` source
//! }
//! ```
//!
//! ```compile_fail,E0277
//! # use concord_example_native::{api::Inventory, upload::OneShot};
//! fn needs_clone<T: Clone>(_: &T) {}
//! fn copy_the_prepared(api: &Inventory) -> Result<(), concord::Error> {
//!     let (source, _tally) = OneShot::new(Vec::new());
//!     let prepared = api.upload(1, source).prepare()?;
//!     needs_clone(&prepared); // `Prepared` is never `Clone`
//!     Ok(())
//! }
//! ```
//!
//! The same code without the `needs_clone` calls compiles (the control for the two cases above):
//!
//! ```
//! # use concord_example_native::{api::Inventory, upload::OneShot};
//! fn prepare_once(api: &Inventory) -> Result<(), concord::Error> {
//!     let (source, _tally) = OneShot::new(Vec::new());
//!     let operation = api.upload(1, source);
//!     let _prepared = operation.prepare()?;
//!     Ok(())
//! }
//! ```

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::task::{Context, Poll};

use concord::bytes::Bytes;
use concord::{CodecError, Encode};
use futures_core::Stream;
use serde::Deserialize;

use crate::api::Inventory;

/// What a [`OneShot`] source was asked for and handed out, shared with whoever created it.
///
/// The counters are the source's own observations: how often the HTTP stack polled it, how many
/// chunks and bytes it yielded, and whether it reported its end.
#[derive(Debug, Default)]
pub struct Tally {
    polls: AtomicUsize,
    chunks: AtomicUsize,
    bytes: AtomicU64,
    ended: AtomicBool,
}

impl Tally {
    /// Times the source was polled.
    pub fn polls(&self) -> usize {
        self.polls.load(Ordering::SeqCst)
    }

    /// Chunks yielded.
    pub fn chunks(&self) -> usize {
        self.chunks.load(Ordering::SeqCst)
    }

    /// Bytes yielded.
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::SeqCst)
    }

    /// Whether the source reported its end (it was drained).
    pub fn ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }
}

/// A finite, read-once byte source (a stand-in for a file, a socket or a channel), deliberately
/// **not** `Clone`: it can be consumed once, by moving it.
///
/// It implements `futures_core::Stream<Item = Result<Bytes, std::io::Error>>`, the shape
/// `reqwest::Body::wrap_stream` takes, and counts what it yields in a shared [`Tally`].
pub struct OneShot {
    chunks: std::vec::IntoIter<Bytes>,
    tally: Arc<Tally>,
}

impl OneShot {
    /// A source yielding `chunks` in order, and the tally it reports to.
    pub fn new(chunks: Vec<Bytes>) -> (Self, Arc<Tally>) {
        let tally = Arc::new(Tally::default());
        let source = Self {
            chunks: chunks.into_iter(),
            tally: Arc::clone(&tally),
        };
        (source, tally)
    }
}

impl Stream for OneShot {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.tally.polls.fetch_add(1, Ordering::SeqCst);
        match this.chunks.next() {
            Some(chunk) => {
                this.tally.chunks.fetch_add(1, Ordering::SeqCst);
                this.tally
                    .bytes
                    .fetch_add(chunk.len() as u64, Ordering::SeqCst);
                Poll::Ready(Some(Ok(chunk)))
            }
            None => {
                this.tally.ended.store(true, Ordering::SeqCst);
                Poll::Ready(None)
            }
        }
    }
}

/// `Encode::CONTENT_TYPE` sends `content-type: application/octet-stream` with the body.
///
/// `application/octet-stream`, streamed: moves a [`OneShot`] into the request body unread.
///
/// Name it like any format (`request upload::Streamed;`). The media type is fixed; the body
/// length is not known in advance, so reqwest sends it chunked.
pub struct Streamed(());

impl Encode<OneShot> for Streamed {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(source: OneShot) -> Result<reqwest::Body, CodecError> {
        // Wraps, does not poll: the first poll happens when the request is executed.
        Ok(reqwest::Body::wrap_stream(source))
    }
}

/// The server's answer to an upload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadReceipt {
    /// Bytes the server received.
    pub bytes: u64,
}

/// Uploads a one-shot source as an item's attachment: one `send()`, one execution, the source
/// read once.
///
/// Use it for a body that is produced while it is sent and has one fixed media type. A failed
/// upload cannot be retried with the same source (it was moved and possibly partly read); build
/// a fresh source for another attempt, and decide yourself whether the operation may be
/// repeated.
pub async fn upload_attachment(
    api: &Inventory,
    id: u64,
    source: OneShot,
) -> Result<UploadReceipt, concord::Error> {
    api.upload(id, source).send().await
}
