//! JSON Lines output: compact JSON, one record per line, LF after every record.

use std::error::Error as StdError;
use std::fmt;

use concord_core::CodecError;
use concord_core::reqwest::header::HeaderValue;
use futures_core::Stream;
use serde::Serialize;

use super::policy::{Flavor, JsonLinesOptions};
use crate::stream::{RecordBody, RecordEncoder, Upload};

/// A one-shot record source for a lazily produced JSON Lines / NDJSON upload body: a
/// [`Stream`] of `Result<T, E>` (an async producer, or a synchronous iterator through
/// [`from_iter`](crate::stream::from_iter)).
///
/// Encoding it (`Encode<StreamRecords<S>>`, [`JsonLinesWriter::upload`]) polls the source
/// **zero** times; the transport polls it while sending, once, in one pass. A source error or a
/// record that cannot be serialized fails the transfer at that point, after the earlier records
/// were sent (reqwest reports a body error that wraps an
/// [`UploadError`](crate::stream::UploadError)); nothing is replayed or retried.
pub struct StreamRecords<S> {
    source: S,
}

impl<S> StreamRecords<S> {
    /// Wraps a one-shot source.
    pub fn new(source: S) -> Self {
        Self { source }
    }

    /// The source, unpolled.
    pub fn into_inner(self) -> S {
        self.source
    }

    pub(super) fn into_body<T, E>(self) -> concord_core::reqwest::Body
    where
        S: Stream<Item = Result<T, E>> + Send + 'static,
        T: Serialize + 'static,
        E: Into<Box<dyn StdError + Send + Sync>>,
    {
        RecordBody::new(self.source, LineEncoder { records: 0 }).into_body()
    }
}

impl<S> fmt::Debug for StreamRecords<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamRecords").finish_non_exhaustive()
    }
}

/// Appends one record as compact JSON and its LF. `number` is the record's 1-based position.
/// On failure `out` is left as it was.
pub(super) fn encode_line<T: Serialize + ?Sized>(
    record: &T,
    out: &mut Vec<u8>,
    number: u64,
) -> Result<(), CodecError> {
    let start = out.len();
    if let Err(e) = serde_json::to_writer(&mut *out, record) {
        out.truncate(start);
        return Err(CodecError::new("a record could not be encoded as JSON")
            .at("record", number)
            .with_source(e));
    }
    // Compact serde_json output escapes line breaks in strings; only a raw value
    // (`serde_json::value::RawValue`) can carry one, and it would split the record.
    if out[start..].contains(&b'\n') {
        out.truncate(start);
        return Err(CodecError::new(
            "a record encoded as JSON contains a line feed (a raw JSON value spanning lines)",
        )
        .at("record", number));
    }
    out.push(b'\n');
    Ok(())
}

/// Every record, encoded.
pub(super) fn encode_all<T: Serialize>(
    records: impl IntoIterator<Item = T>,
) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::new();
    for (index, record) in records.into_iter().enumerate() {
        encode_line(&record, &mut out, index as u64 + 1)?;
    }
    Ok(out)
}

struct LineEncoder {
    records: u64,
}

impl<T: Serialize> RecordEncoder<T> for LineEncoder {
    fn begin(&mut self, _: &mut Vec<u8>) -> Result<(), CodecError> {
        Ok(())
    }

    fn record(&mut self, record: &T, out: &mut Vec<u8>) -> Result<(), CodecError> {
        self.records += 1;
        encode_line(record, out, self.records)
    }

    fn finish(&mut self, _: &mut Vec<u8>, _: u64) -> Result<(), CodecError> {
        Ok(())
    }
}

/// A JSON Lines / NDJSON writer configured at run time. The output is the same for both
/// families (compact JSON, LF after every record including the last); the family chooses the
/// `Content-Type` that travels with an [`Upload`]. Reader-only options (blank lines, limits,
/// media policy) do not apply to writing.
#[derive(Clone, Debug)]
pub struct JsonLinesWriter {
    flavor: Flavor,
}

impl JsonLinesWriter {
    /// A writer for the options' family ([`JsonLinesOptions::json_lines`] or
    /// [`ndjson`](JsonLinesOptions::ndjson)).
    pub fn new(options: JsonLinesOptions) -> Self {
        Self {
            flavor: options.flavor,
        }
    }

    /// The `Content-Type` of this writer's output: `application/jsonl` or
    /// `application/x-ndjson`.
    pub fn content_type(&self) -> &'static str {
        match self.flavor {
            Flavor::JsonLines => super::JSON_LINES_MEDIA,
            Flavor::Ndjson => super::NDJSON_MEDIA,
        }
    }

    /// A lazily produced upload body over a one-shot source, with its `Content-Type`. Creating
    /// it polls the source zero times (see [`StreamRecords`]).
    pub fn upload<S, T, E>(&self, records: StreamRecords<S>) -> Upload
    where
        S: Stream<Item = Result<T, E>> + Send + 'static,
        T: Serialize + 'static,
        E: Into<Box<dyn StdError + Send + Sync>>,
    {
        Upload::new(
            records.into_body(),
            HeaderValue::from_static(self.content_type()),
        )
    }

    /// Every record, encoded in memory (a finite body).
    ///
    /// # Errors
    ///
    /// A [`CodecError`] naming the 1-based `record` that could not be encoded.
    pub fn to_vec<T: Serialize>(
        &self,
        records: impl IntoIterator<Item = T>,
    ) -> Result<Vec<u8>, CodecError> {
        encode_all(records)
    }
}
