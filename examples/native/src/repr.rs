//! U16 / U29: formats defined in this downstream crate.
//!
//! A format is a marker type implementing [`concord::Encode<T>`] (request bodies) and/or
//! [`concord::Decode<T>`] (2xx response bodies). It is chosen only by naming its path in a
//! format statement (`format Ndjson;`, `request Ndjson;`, `response Ndjson;`) on the
//! endpoint, a scope or the client; a payload (`body rows: Vec<StockRow>`, `-> Vec<StockRow>`)
//! names only its Rust type. A `CodecError` has a static label and numeric positions, shown by
//! Concord's `Display`; the underlying error (which may quote the body) goes to `with_source` and
//! is reachable only through `concord::Error::codec_error` and `CodecError::source_unredacted`.

use concord::bytes::Bytes;
use concord::{CodecError, Decode, Encode, ResponseHead, reqwest};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// `Encode::CONTENT_TYPE` sends `content-type: application/x-ndjson` with the body.
/// The separate `Decode` implementation uses the default `ACCEPT = None`.
///
/// Newline-delimited JSON (`application/x-ndjson`): a `Vec<T>` is one JSON object per line.
///
/// Use it for line-oriented bulk endpoints. Encoding writes a trailing `\n` after every record;
/// decoding ignores blank lines (including a final newline). The whole body is buffered, as
/// for every `Decode`; for unbounded feeds use the streaming recipe instead.
pub struct Ndjson(());

impl<T: Serialize> Encode<Vec<T>> for Ndjson {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-ndjson");

    fn encode(value: Vec<T>) -> Result<reqwest::Body, CodecError> {
        let mut out = Vec::new();
        for (index, record) in value.iter().enumerate() {
            serde_json::to_writer(&mut out, record).map_err(|e| {
                CodecError::new("an NDJSON record could not be encoded")
                    .at("record", index as u64 + 1)
                    .with_source(e)
            })?;
            out.push(b'\n');
        }
        Ok(reqwest::Body::from(out))
    }
}

impl<T: DeserializeOwned> Decode<Vec<T>> for Ndjson {
    type Context = ();
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<Vec<T>, CodecError> {
        let mut records = Vec::new();
        for (index, line) in body.split(|b| *b == b'\n').enumerate() {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let record = serde_json::from_slice(line).map_err(|e| {
                // Positions only: never the line's content (it is in the unredacted source).
                CodecError::new("an NDJSON line is not a valid record")
                    .at("record", records.len() as u64 + 1)
                    .at("line", index as u64 + 1)
                    .at("column", e.column() as u64)
                    .with_source(e)
            })?;
            records.push(record);
        }
        Ok(records)
    }
}

/// `Encode::CONTENT_TYPE` sends `content-type: application/x-www-form-urlencoded` with the body.
///
/// `application/x-www-form-urlencoded` request bodies from ordered `(name, value)` pairs.
///
/// Concord's `Form` (feature `form`, through serde) encodes the same pairs; this encoder shows the
/// same thing downstream, without serde. Declarations name it by path, `request repr::Form;`, a
/// different type from `concord::Form`.
///
/// Encoding is `url::form_urlencoded` (space as `+`, everything outside `*-._` and
/// alphanumerics percent-encoded), the same rules Concord and reqwest use for query strings.
/// Pair order and repeated names are preserved exactly.
pub struct Form(());

impl Encode<Vec<(String, String)>> for Form {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-www-form-urlencoded");

    fn encode(value: Vec<(String, String)>) -> Result<reqwest::Body, CodecError> {
        let encoded = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(value)
            .finish();
        Ok(reqwest::Body::from(encoded))
    }
}
