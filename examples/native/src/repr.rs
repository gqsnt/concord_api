//! U16 / U29: formats defined in this downstream crate.
//!
//! A format is a marker type implementing [`concord::Encode<T>`] (request bodies) and/or
//! [`concord::Decode<T>`] (2xx response bodies). It is chosen only by naming its path where the
//! body or output is declared (`Ndjson rows: Vec<StockRow>`, `-> Ndjson Vec<StockRow>`) or in a
//! `request`/`response` default. Error descriptions must stay content-free: they are shown by Concord's `Display`;
//! the underlying error (which may quote the body) goes to `with_source` and is reachable only
//! through `concord::Error::body_source`.

use concord::bytes::Bytes;
use concord::{BodyError, Decode, Encode, reqwest};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Newline-delimited JSON (`application/x-ndjson`): a `Vec<T>` is one JSON object per line.
///
/// Use it for line-oriented bulk endpoints. Encoding writes a trailing `\n` after every record;
/// decoding ignores blank lines (including a final newline). The whole body is buffered, as
/// for every `Decode`; for unbounded feeds use the streaming recipe instead.
pub struct Ndjson(());

impl<T: Serialize> Encode<Vec<T>> for Ndjson {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-ndjson");

    fn encode(value: Vec<T>) -> Result<reqwest::Body, BodyError> {
        let mut out = Vec::new();
        for (index, record) in value.iter().enumerate() {
            serde_json::to_writer(&mut out, record).map_err(|e| {
                BodyError::new(format!("NDJSON record {} could not be encoded", index + 1))
                    .with_source(e)
            })?;
            out.push(b'\n');
        }
        Ok(reqwest::Body::from(out))
    }
}

impl<T: DeserializeOwned> Decode<Vec<T>> for Ndjson {
    fn decode(body: Bytes) -> Result<Vec<T>, BodyError> {
        let mut records = Vec::new();
        for (index, line) in body.split(|b| *b == b'\n').enumerate() {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let record = serde_json::from_slice(line).map_err(|e| {
                // Line number and column only: never the line's content.
                BodyError::new(format!(
                    "NDJSON line {} is not a valid record (column {})",
                    index + 1,
                    e.column()
                ))
                .with_source(e)
            })?;
            records.push(record);
        }
        Ok(records)
    }
}

/// `application/x-www-form-urlencoded` request bodies from ordered `(name, value)` pairs.
///
/// Concord's built-in `form` format (feature `form`, through serde) encodes the same pairs; this
/// encoder shows the same thing downstream, without serde. Declarations name it by path,
/// `repr::Form fields: Vec<(String, String)>`, so it is never confused with the word `form`.
///
/// Encoding is `url::form_urlencoded` (space as `+`, everything outside `*-._` and
/// alphanumerics percent-encoded), the same rules Concord and reqwest use for query strings.
/// Pair order and repeated names are preserved exactly.
pub struct Form(());

impl Encode<Vec<(String, String)>> for Form {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-www-form-urlencoded");

    fn encode(value: Vec<(String, String)>) -> Result<reqwest::Body, BodyError> {
        let encoded = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(value)
            .finish();
        Ok(reqwest::Body::from(encoded))
    }
}
