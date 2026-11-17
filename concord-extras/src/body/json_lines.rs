//! Bounded request bodies containing one JSON value per line.

use concord_core::body::WriteError;
use concord_core::Body;
use std::{
    error::Error,
    fmt,
    io::{self, Write},
};

/// Serializes records as an `application/x-ndjson` body, with one compact JSON value per line.
///
/// Every record ends with LF, including the final record; an empty iterator
/// produces an empty body. The writer does not choose action and document pairs,
/// wrap records or infer whether a write is idempotent. For
/// Elasticsearch, the author supplies each action and document as separate
/// records and checks every item in the response.
///
/// `max_bytes` bounds the entire encoded body, including every LF. Zero accepts
/// only an empty iterator. Records are serialized directly into one bounded
/// output buffer; no per-record `Value` tree or second record buffer is built.
/// The writer stores at most `max_bytes` of output plus at most 128 nesting
/// frames. The iterator, the current input record, the serializer's own
/// allocations and values retained by the caller are additional. Each record
/// goes through [`concord_core::body::Bounded`].
///
/// This is synchronous, buffered serialization. It accepts an iterator, not an
/// asynchronous stream, and does no background work. The returned body can be
/// sent again under the engine's ordinary retry rules. It never sends or retries
/// by itself. For asynchronous streaming or provider-specific framing, supply
/// a custom writer. A description's `content-type` header takes precedence.
///
/// ```no_run
/// use concord::{json_lines, KIB};
/// use serde_json::{json, Value};
///
/// concord::api! {
///     client Search {
///         base "https://search.example.test";
///         POST bulk(records: Vec<Value>) {
///             path "_bulk";
///             body json_lines(records, 512 * KIB);
///         }
///     }
/// }
/// async fn bulk() -> Result<(), concord::Error> {
///     Search::new().bulk(vec![
///         json!({"index": {"_index": "books", "_id": "42"}}),
///         json!({"title": "Dune"}),
///     ]).await?;
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Returns [`JsonLinesError`] when the total byte bound would be exceeded or a
/// record fails serialization. No partial body is returned and nothing is sent.
/// Records are processed in iterator order: the first record that exceeds
/// `max_bytes` or fails serialization determines the error, and the iterator
/// is not advanced past that record.
pub fn json_lines<I>(records: I, max_bytes: usize) -> Result<Body, JsonLinesError>
where
    I: IntoIterator,
    I::Item: serde::Serialize,
{
    let mut output = Limited {
        bytes: Vec::new(),
        max: max_bytes,
        full: false,
    };
    for (index, record) in records.into_iter().enumerate() {
        let value = concord_core::body::Bounded::new(&record);
        if let Err(source) = serde_json::to_writer(&mut output, &value) {
            if output.full {
                return Err(JsonLinesError::TooLarge);
            }
            return Err(JsonLinesError::Serialize {
                record: index.saturating_add(1),
                source: value.error(source),
            });
        }
        output
            .write_all(b"\n")
            .map_err(|_| JsonLinesError::TooLarge)?;
    }
    Ok(Body::new(output.bytes, "application/x-ndjson"))
}

/// Why a JSON-lines request body could not be written: it is too large, or a record
/// failed to serialize.
///
/// `Display` uses a fixed sentence and `Debug` shows only the variant name.
/// Neither formats record bytes or a cause. A serialization failure retains
/// the original writer error and its format cause through `source()`. An
/// author's custom serializer remains responsible for its own error contents.
///
/// ```no_run
/// fn inspect(error: &concord::Error) {
///     if let Some(concord::JsonLinesError::Serialize { record, .. }) =
///         concord::JsonLinesError::find(error)
///     {
///         let one_based_record = *record;
///     }
/// }
/// ```
#[non_exhaustive]
pub enum JsonLinesError {
    /// The complete body, including LF delimiters, would exceed `max_bytes`.
    TooLarge,
    /// One record failed serialization, including its nesting-depth guard.
    #[non_exhaustive]
    Serialize {
        /// The record's one-based position in the input iterator.
        record: usize,
        /// The original writer failure, retaining its format cause.
        source: WriteError,
    },
}

impl JsonLinesError {
    /// Finds a `JsonLinesError` in this error or its `source()` chain.
    ///
    /// ```no_run
    /// fn inspect(error: &concord::Error) {
    ///     let _cause = concord::JsonLinesError::find(error);
    /// }
    /// ```
    pub fn find<'a>(error: &'a (dyn Error + 'static)) -> Option<&'a Self> {
        super::find_error(error)
    }
}

impl fmt::Debug for JsonLinesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => f.write_str("TooLarge"),
            Self::Serialize { record, .. } => {
                f.debug_struct("Serialize").field("record", record).finish()
            }
        }
    }
}

impl fmt::Display for JsonLinesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => f.write_str("the JSON lines body exceeds its byte limit"),
            Self::Serialize { record, .. } => {
                write!(f, "JSON record {record} could not be serialized")
            }
        }
    }
}

impl Error for JsonLinesError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Serialize { source, .. } => Some(source),
            Self::TooLarge => None,
        }
    }
}

struct Limited {
    bytes: Vec<u8>,
    max: usize,
    full: bool,
}
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
            self.full = true;
            return Err(io::Error::other(
                "the JSON lines body exceeds its byte limit",
            ));
        }
        let required = self.bytes.len().saturating_add(bytes.len());
        if required > self.bytes.capacity() {
            let capacity = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(required)
                .min(self.max);
            self.bytes
                .reserve_exact(capacity.saturating_sub(self.bytes.len()));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
