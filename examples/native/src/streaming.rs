//! U28 (R08): reading a chunked response incrementally and stopping early.

use std::fmt;
use std::time::Duration;

use concord::bytes::Bytes;
use concord::reqwest;

use crate::api::Inventory;

/// Why [`first_event_chunks`] stopped without its chunks.
#[derive(Debug)]
pub enum StreamError {
    /// The request could not be built or sent (Concord's error, redacted).
    Concord(concord::Error),
    /// The server answered with a non-2xx status; `send_raw` does not check it, so the recipe
    /// does. The body was not read.
    Status(reqwest::StatusCode),
    /// Reading a chunk failed (native error, URL removed).
    Read(reqwest::Error),
    /// No chunk arrived within the application's per-chunk deadline.
    Stalled {
        /// Chunks received before the stall.
        received: usize,
    },
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord(e) => e.fmt(f),
            Self::Status(s) => write!(f, "event stream answered {s}"),
            Self::Read(_) => f.write_str("event stream read failed"),
            Self::Stalled { received } => {
                write!(f, "event stream stalled after {received} chunks")
            }
        }
    }
}

impl std::error::Error for StreamError {}

/// Reads at most `max_chunks` chunks of the event export, waiting at most `per_chunk` for each,
/// then drops the response.
///
/// Use it for large or unbounded bodies: the declared `-> Bytes via Binary` buffers the whole
/// body, while [`concord::Prepared::send_raw`] returns the unread native response. Dropping it
/// before the end abandons the body; reqwest/hyper then close that connection instead of
/// returning it to the pool (the server observes the close). The deadline is the
/// application's: `tokio::time::timeout` around each `chunk()` call. A body that ends earlier
/// simply yields fewer chunks.
pub async fn first_event_chunks(
    api: &Inventory,
    max_chunks: usize,
    per_chunk: Duration,
) -> Result<Vec<Bytes>, StreamError> {
    let mut response = api
        .events()
        .prepare()
        .map_err(StreamError::Concord)?
        .send_raw()
        .await
        .map_err(StreamError::Concord)?;
    if !response.status().is_success() {
        return Err(StreamError::Status(response.status()));
    }
    let mut chunks = Vec::new();
    while chunks.len() < max_chunks {
        match tokio::time::timeout(per_chunk, response.chunk()).await {
            Ok(Ok(Some(chunk))) => chunks.push(chunk),
            Ok(Ok(None)) => break,
            Ok(Err(error)) => return Err(StreamError::Read(error.without_url())),
            Err(_elapsed) => {
                return Err(StreamError::Stalled {
                    received: chunks.len(),
                });
            }
        }
    }
    drop(response); // early stop: the rest of the body is never read
    Ok(chunks)
}
