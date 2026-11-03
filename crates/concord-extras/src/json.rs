//! `Json`: JSON via `serde_json`.

use concord_core::bytes::Bytes;
use concord_core::{BodyError, Decode, Encode, reqwest};

/// JSON via `serde_json` (feature `json`): `application/json` request bodies; responses are
/// decoded regardless of their `Content-Type`. `format Json;`, `request Json;`, `response Json;`
/// on a client, a scope or an endpoint.
pub struct Json(());

impl<T: serde::Serialize> Encode<T> for Json {
    const CONTENT_TYPE: Option<&'static str> = Some("application/json");

    fn encode(value: T) -> Result<reqwest::Body, BodyError> {
        serde_json::to_vec(&value)
            .map(reqwest::Body::from)
            .map_err(|e| BodyError::new(json_description("encode", &e)).with_source(e))
    }
}

impl<T: serde::de::DeserializeOwned> Decode<T> for Json {
    /// An empty or whitespace-only body is not JSON (RFC 8259 §2) and is an error, also for `()`
    /// and `Option<T>`. Two static hints are added to the description, which never quotes the
    /// body: one for an empty body, and one for a syntax error at the first byte (the body may
    /// not be JSON at all).
    fn decode(body: Bytes) -> Result<T, BodyError> {
        use serde_json::error::Category;
        serde_json::from_slice(&body).map_err(|e| {
            let blank = body
                .iter()
                .all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'));
            let description = if blank {
                "JSON decode error: the response body is empty or only whitespace, which is not \
                 JSON; omit `->` if this endpoint never returns a body, or use a format that \
                 accepts an empty one"
                    .into()
            } else if e.classify() == Category::Syntax && e.line() == 1 && e.column() == 1 {
                format!(
                    "{}; if the response is not JSON, the endpoint may need `response Text;` or \
                     `response Binary;`",
                    json_description("decode", &e)
                )
            } else {
                json_description("decode", &e)
            };
            BodyError::new(description).with_source(e)
        })
    }
}

fn json_description(action: &str, e: &serde_json::Error) -> String {
    use serde_json::error::Category;
    let category = match e.classify() {
        Category::Io => "I/O",
        Category::Syntax => "syntax",
        Category::Data => "data",
        Category::Eof => "unexpected end of input",
    };
    if e.line() == 0 {
        format!("JSON {action} error ({category})")
    } else {
        format!(
            "JSON {action} error ({category}) at line {}, column {}",
            e.line(),
            e.column()
        )
    }
}
