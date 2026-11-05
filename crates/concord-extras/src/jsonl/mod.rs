//! JSON Lines / NDJSON, finite and incremental, and consecutive JSON values (feature `jsonl`).
//!
//! | Use | API |
//! |---|---|
//! | A finite body in an `api!` declaration | [`JsonLines`] / [`Ndjson`]: `Decode<Vec<T>>`, `Encode<Vec<T>>` |
//! | A lazily produced upload in an `api!` declaration | `Encode<StreamRecords<S>>` for [`JsonLines`] / [`Ndjson`] |
//! | Records read as a response arrives | [`JsonLinesReader`] → [`JsonLinesRecords`] |
//! | A runtime-configured upload (body and `Content-Type` together) | [`JsonLinesWriter::upload`] → [`Upload`](crate::stream::Upload) |
//! | A Kubernetes-style watch stream (values one after another) | [`JsonValuesReader`] → [`JsonValues`] |
//!
//! # Framing profile
//!
//! | | JSON Lines ([`JsonLinesOptions::json_lines`]) | NDJSON ([`JsonLinesOptions::ndjson`]) |
//! |---|---|---|
//! | encoding | UTF-8, validated per line whatever `T` reads | same |
//! | record | one JSON value of any kind (object, array, string, number, boolean, `null`) per line, surrounding spaces/tabs allowed | same |
//! | line end (reader) | LF or CRLF; a bare CR is not a line end | same |
//! | final line without LF | accepted at the true end of input (a chunk boundary is never an end) | same |
//! | blank line (empty, or only spaces, tabs, CR) | refused ([`BlankLines::Refuse`]) | ignored ([`BlankLines::Ignore`]) |
//! | byte order mark at the start | refused ([`BomPolicy::Refuse`]) | refused |
//! | malformed line | terminal error by default; [`RecordErrors::Yield`] reports a line ended by LF as an `Err` item and continues (a malformed final line without LF stays terminal) | same |
//! | writer output | compact JSON, LF after every record **including the last**, no BOM | same |
//! | response media types ([`MediaPolicy::standard`]) | `application/jsonl`, `application/jsonlines`, `application/x-jsonlines` | `application/x-ndjson`, `application/ndjson` |
//! | request `Content-Type` / `Accept` suggestion | `application/jsonl` | `application/x-ndjson` |
//!
//! The reader is lenient where the writer is strict: the writer always terminates the last
//! record, the reader also accepts a last line without LF at the true end of input. Input that
//! ends inside a line is that final line: a truncated record is therefore a malformed final
//! line (`Format`), not a silent end. Because a final line without LF may have been cut off,
//! its failure is terminal ([`ReadError::is_terminal`](crate::stream::ReadError::is_terminal))
//! even with [`RecordErrors::Yield`], which isolates only bad lines ended by LF.
//!
//! The media check (for [`Decode`] and the readers' `open`) also requires exactly one
//! `Content-Type` field and a `charset`, if any, of `utf-8`/`us-ascii` declared once. Use
//! [`MediaPolicy`] to accept a provider's actual type deliberately, or
//! `IgnoreContentType<JsonLines>` for the finite format.
//!
//! # Errors and diagnostics
//!
//! Failures carry static labels and numbers (`line`, `column`, `limit`), never body content; the
//! serde_json error (which quotes the input) is reachable only through
//! [`ReadError::source_unredacted`](crate::stream::ReadError::source_unredacted) /
//! `CodecError::source_unredacted`. Kinds: invalid UTF-8, invalid JSON, a blank or truncated line
//! and a byte order mark are [`Format`](crate::stream::ReadErrorKind::Format); valid JSON of the
//! wrong shape is [`Decode`](crate::stream::ReadErrorKind::Decode); a line over
//! `max_record_bytes` and the optional totals are [`Limit`](crate::stream::ReadErrorKind::Limit).
//!
//! # Resources
//!
//! A reader retains the current transport chunk (as delivered by the source; reqwest delivers
//! decompressed bytes, which is what the limits and positions count) and the unfinished line or
//! value, bounded by `max_record_bytes` / `max_value_bytes` (default 16 MiB) and enforced before
//! the buffer grows past it. It never queues decoded records: one `next()` produces at most one.
//! A `next()` frames at most 64 KiB of input between two yields to the executor, whatever the
//! chunk sizes, and yields after every 64 empty chunks from the source; parsing a completed record
//! (serde) is not divided. No task is spawned. Dropping the records value drops the response.
//!
//! The finite [`Decode`] for [`JsonLines`] / [`Ndjson`] applies the corresponding preset exactly
//! (including the 16 MiB line limit) to the body Concord has already collected: it is the same
//! framing code as the reader, over one chunk.

mod lines;
mod policy;
mod values;
mod writer;

use std::error::Error as StdError;

use concord_core::bytes::Bytes;
use concord_core::{CodecError, Decode, Encode, ResponseHead, reqwest};
use futures_core::Stream;
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use lines::{JsonLinesReader, JsonLinesRecords};
pub use policy::{
    BlankLines, BomPolicy, DEFAULT_MAX_RECORD_BYTES, JsonLinesOptions, MediaPolicy, RecordErrors,
};
pub use values::{JsonValues, JsonValuesReader};
pub use writer::{JsonLinesWriter, StreamRecords};

const JSON_LINES_MEDIA: &str = "application/jsonl";
const NDJSON_MEDIA: &str = "application/x-ndjson";

/// Strict JSON Lines (feature `jsonl`): blank lines and a byte order mark are refused.
///
/// - **Requests:** `Vec<T>` (`T: Serialize`) or a lazily produced [`StreamRecords<S>`], as compact
///   JSON with an LF after every record, sent as `application/jsonl`. A record that cannot be
///   encoded is a `Build` error for `Vec<T>`, and a failed transfer for `StreamRecords` (it is
///   serialized while sending).
/// - **Responses:** `Vec<T>` (`T: DeserializeOwned`) after a head check made before the body is
///   read: exactly one `Content-Type`, `application/jsonl`, `application/jsonlines` or
///   `application/x-jsonlines`, charset absent or `utf-8`. The body is parsed with
///   [`JsonLinesOptions::json_lines`] (see the [module](self) framing profile).
/// - **`Accept`:** suggests `application/jsonl`.
///
/// For records read as the response arrives, use [`JsonLinesReader`].
pub struct JsonLines(());

/// NDJSON (feature `jsonl`): JSON Lines whose blank lines are ignored.
///
/// Identical to [`JsonLines`] except: blank lines are skipped when reading
/// ([`JsonLinesOptions::ndjson`]), the response must be `application/x-ndjson` or
/// `application/ndjson`, and requests are sent (and `Accept` suggests) `application/x-ndjson`.
/// The output bytes are the same.
pub struct Ndjson(());

impl<T: Serialize> Encode<Vec<T>> for JsonLines {
    const CONTENT_TYPE: Option<&'static str> = Some(JSON_LINES_MEDIA);

    fn encode(value: Vec<T>) -> Result<reqwest::Body, CodecError> {
        writer::encode_all(value).map(reqwest::Body::from)
    }
}

impl<T: Serialize> Encode<Vec<T>> for Ndjson {
    const CONTENT_TYPE: Option<&'static str> = Some(NDJSON_MEDIA);

    fn encode(value: Vec<T>) -> Result<reqwest::Body, CodecError> {
        writer::encode_all(value).map(reqwest::Body::from)
    }
}

impl<S, T, E> Encode<StreamRecords<S>> for JsonLines
where
    S: Stream<Item = Result<T, E>> + Send + 'static,
    T: Serialize + 'static,
    E: Into<Box<dyn StdError + Send + Sync>>,
{
    const CONTENT_TYPE: Option<&'static str> = Some(JSON_LINES_MEDIA);

    /// Wraps the source into a lazy body; polls it zero times.
    fn encode(value: StreamRecords<S>) -> Result<reqwest::Body, CodecError> {
        Ok(value.into_body())
    }
}

impl<S, T, E> Encode<StreamRecords<S>> for Ndjson
where
    S: Stream<Item = Result<T, E>> + Send + 'static,
    T: Serialize + 'static,
    E: Into<Box<dyn StdError + Send + Sync>>,
{
    const CONTENT_TYPE: Option<&'static str> = Some(NDJSON_MEDIA);

    /// Wraps the source into a lazy body; polls it zero times.
    fn encode(value: StreamRecords<S>) -> Result<reqwest::Body, CodecError> {
        Ok(value.into_body())
    }
}

impl<T: DeserializeOwned> Decode<Vec<T>> for JsonLines {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some(JSON_LINES_MEDIA);

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        policy::check_media(MediaPolicy::standard(), &lines::JSON_LINES, head)
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<Vec<T>, CodecError> {
        lines::decode_all(&JsonLinesOptions::json_lines(), &body)
    }
}

impl<T: DeserializeOwned> Decode<Vec<T>> for Ndjson {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some(NDJSON_MEDIA);

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        policy::check_media(MediaPolicy::standard(), &lines::NDJSON, head)
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<Vec<T>, CodecError> {
        lines::decode_all(&JsonLinesOptions::ndjson(), &body)
    }
}
