//! Body and response representations.
//!
//! A declaration names the payload type and its wire representation separately:
//! `body item: NewItem via Json`, `-> Item via Json`, `-> String via Text`. A representation is a
//! marker type implementing [`Encode<T>`] and/or [`Decode<T>`] for the payload types it supports;
//! Concord never chooses one by looking at a Rust type name. An endpoint without `->` has no typed
//! output: any 2xx succeeds and the body is drained without being buffered.

use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;

use bytes::Bytes;

/// Encodes a request body of type `T` (`body b: T via Self`).
///
/// Implement it on your own marker type for a custom format. The encoder runs when the request is
/// prepared; it receives the payload by value.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot encode a `{T}` request body",
    label = "`body ...: {T} via {Self}` needs `{Self}: Encode<{T}>`",
    note = "built in: `Json` (any `Serialize`, feature `json`), `Text` (`String`), `Binary` (`Bytes`, `Vec<u8>`); or implement `concord::Encode<{T}>` for your own marker type"
)]
pub trait Encode<T> {
    /// The `Content-Type` sent with the body, unless the declared headers already set one.
    const CONTENT_TYPE: Option<&'static str>;

    /// Encodes the payload into a request body.
    fn encode(value: T) -> Result<reqwest::Body, BodyError>;
}

/// Decodes the complete body of a 2xx response into `T` (`-> T via Self`).
///
/// Implement it on your own marker type for a custom format. It is also usable directly, for
/// example on the body of an error response: `<Json as Decode<Problem>>::decode(bytes)`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot decode a `{T}` response",
    label = "`-> {T} via {Self}` needs `{Self}: Decode<{T}>`",
    note = "built in: `Json` (any `DeserializeOwned`, feature `json`), `Text` (`String`), `Binary` (`Bytes`, `Vec<u8>`); or implement `concord::Decode<{T}>` for your own marker type"
)]
pub trait Decode<T> {
    /// Decodes a complete response body.
    fn decode(body: Bytes) -> Result<T, BodyError>;
}

/// A body could not be encoded or decoded.
///
/// `description` must not contain body content or request values: it is shown by
/// [`Error`](crate::Error)'s `Display`, and it is this type's own `Display`. The optional underlying
/// error is reachable only through [`Error::body_source`](crate::Error::body_source), never through
/// [`std::error::Error::source`], because decoder messages often quote the input (for example
/// serde's `invalid type: string "..."`).
pub struct BodyError {
    description: Cow<'static, str>,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl BodyError {
    /// Creates an error with a content-free description.
    pub fn new(description: impl Into<Cow<'static, str>>) -> Self {
        Self {
            description: description.into(),
            source: None,
        }
    }

    /// Attaches the underlying error (which may quote body content).
    pub fn with_source(mut self, source: impl Into<Box<dyn StdError + Send + Sync>>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// The content-free description.
    pub fn description(&self) -> &str {
        &self.description
    }

    pub(crate) fn source_ref(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}

impl fmt::Debug for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BodyError")
            .field("description", &self.description)
            .field("source", &self.source.as_ref().map(|_| "<withheld>"))
            .finish()
    }
}

impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.description)
    }
}

/// No `source()`: the underlying error may quote body content (see the type documentation).
impl StdError for BodyError {}

/// JSON via `serde_json`: `application/json` request bodies; responses are decoded regardless of
/// their `Content-Type`. `-> Item via Json`, `body item: NewItem via Json`.
#[cfg(feature = "json")]
pub struct Json(());

#[cfg(feature = "json")]
impl<T: serde::Serialize> Encode<T> for Json {
    const CONTENT_TYPE: Option<&'static str> = Some("application/json");

    fn encode(value: T) -> Result<reqwest::Body, BodyError> {
        serde_json::to_vec(&value)
            .map(reqwest::Body::from)
            .map_err(|e| BodyError::new(json_description("encode", &e)).with_source(e))
    }
}

#[cfg(feature = "json")]
impl<T: serde::de::DeserializeOwned> Decode<T> for Json {
    fn decode(body: Bytes) -> Result<T, BodyError> {
        serde_json::from_slice(&body)
            .map_err(|e| BodyError::new(json_description("decode", &e)).with_source(e))
    }
}

#[cfg(feature = "json")]
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

/// UTF-8 text: `text/plain; charset=utf-8` request bodies; responses must be valid UTF-8.
/// `-> String via Text`, `body note: String via Text`.
pub struct Text(());

impl Encode<String> for Text {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: String) -> Result<reqwest::Body, BodyError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Decode<String> for Text {
    fn decode(body: Bytes) -> Result<String, BodyError> {
        String::from_utf8(body.into()).map_err(|e| {
            BodyError::new(format!(
                "response body is not valid UTF-8 (valid up to byte {})",
                e.utf8_error().valid_up_to()
            ))
        })
    }
}

/// Raw bytes: `application/octet-stream` request bodies; responses are returned unchanged.
/// `-> Bytes via Binary`, `body data: Vec<u8> via Binary`.
pub struct Binary(());

impl Encode<Bytes> for Binary {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(value: Bytes) -> Result<reqwest::Body, BodyError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Encode<Vec<u8>> for Binary {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(value: Vec<u8>) -> Result<reqwest::Body, BodyError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Decode<Bytes> for Binary {
    fn decode(body: Bytes) -> Result<Bytes, BodyError> {
        Ok(body)
    }
}

impl Decode<Vec<u8>> for Binary {
    fn decode(body: Bytes) -> Result<Vec<u8>, BodyError> {
        Ok(body.into())
    }
}
