//! `Json`: JSON via `serde_json`, with a strict response media type.

use concord_core::bytes::Bytes;
use concord_core::{CodecError, Decode, Encode, ResponseHead, reqwest};

use crate::media;

/// JSON via `serde_json` (feature `json`).
///
/// - **Requests:** any `T: Serialize`, sent as `application/json`.
/// - **Responses:** any `T: DeserializeOwned`, after a strict check of the response head, made
///   before the body is read. The response needs exactly one `Content-Type` field, parsed as a
///   media type (type, subtype and parameter names are case-insensitive), that is
///   `application/json` or an `application/*+json` structured-syntax type (RFC 6839:
///   `application/problem+json`, `application/vnd.github+json`). A `charset` parameter, if any,
///   must be `utf-8` (or `us-ascii`): serde_json reads UTF-8 and nothing is transcoded. A missing,
///   repeated (even with equal values), comma-joined or malformed field, a media range
///   (`application/*`) and any other type (`text/html`, `text/json`, `application/json-seq`)
///   are an [`ErrorKind::Metadata`](concord_core::ErrorKind::Metadata) error that keeps the
///   response unread. A JSON media type does not make the body valid JSON or of type `T`: that
///   is checked when the body is decoded ([`ErrorKind::Decode`](concord_core::ErrorKind::Decode)).
///   To accept JSON under another `Content-Type`, select
///   [`IgnoreContentType<Json>`](crate::IgnoreContentType) deliberately where it is needed.
/// - **`Accept`:** suggests `application/json` (only as a fallback: an authored `Accept`, such as
///   a vendor `application/vnd.acme.v2+json`, wins and is not checked against the response).
/// - An empty or whitespace-only body is not JSON (RFC 8259) and is a decode error, also for `()`
///   and `Option<T>`; JSON `null` decodes into `()` and `None`. An endpoint that returns no body
///   declares no `->`.
///
/// Statements name it by its path: `format Json;`, `request Json;`, `response Json;`.
pub struct Json(());

impl<T: serde::Serialize> Encode<T> for Json {
    const CONTENT_TYPE: Option<&'static str> = Some("application/json");

    fn encode(value: T) -> Result<reqwest::Body, CodecError> {
        serde_json::to_vec(&value)
            .map(reqwest::Body::from)
            .map_err(|e| CodecError::new("the value cannot be encoded as JSON").with_source(e))
    }
}

impl<T: serde::de::DeserializeOwned> Decode<T> for Json {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("application/json");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        let Some((media, text)) = media::single(head)? else {
            return Err(CodecError::new("the response has no Content-Type"));
        };
        if media.type_() == mime::STAR || media.subtype() == mime::STAR {
            return Err(CodecError::new(
                "the response Content-Type is a media range, not a media type",
            ));
        }
        // `mime` splits a structured suffix off the subtype: `json+xml` is subtype `json` with
        // suffix `xml`, which is XML, not JSON.
        let json = media.type_() == mime::APPLICATION
            && match media.suffix() {
                None => media.subtype() == mime::JSON,
                Some(suffix) => suffix == mime::JSON,
            };
        if !json {
            return Err(CodecError::new(
                "the response Content-Type is not application/json or an application/*+json type",
            ));
        }
        media::check_charset(
            text,
            "the JSON response declares a charset other than utf-8",
        )
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<T, CodecError> {
        serde_json::from_slice(&body).map_err(|e| decode_error(e, &body))
    }
}

/// A static label for a serde_json error, with its line and column; the error itself (which
/// quotes the input) is attached as the unredacted source.
fn decode_error(e: serde_json::Error, body: &[u8]) -> CodecError {
    use serde_json::error::Category;
    let blank = body
        .iter()
        .all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'));
    let label = if blank {
        "the body is empty or only whitespace, which is not JSON; omit `->` if this endpoint returns no body, or use a format that accepts an empty one"
    } else {
        match e.classify() {
            Category::Syntax => "the body is not valid JSON",
            Category::Data => "the JSON does not match the expected type",
            Category::Eof => "the JSON ends early",
            Category::Io => "the JSON could not be read",
        }
    };
    let error = if blank || e.line() == 0 {
        CodecError::new(label)
    } else {
        CodecError::new(label)
            .at("line", e.line() as u64)
            .at("column", e.column() as u64)
    };
    error.with_source(e)
}
