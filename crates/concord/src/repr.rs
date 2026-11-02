//! Body and response formats.
//!
//! Every body and every typed output has a format. A declaration writes it in place —
//! `json item: NewItem`, `-> text String`, `Csv rows: Vec<Row>` — or inherits it from the
//! nearest client or scope that declares a default (`request json;`, `response json;`) with
//! `body item: NewItem` and a bare `-> Item`. The words `json`, `text`, `bytes` and `form` name the
//! built-in markers `Json`, `Text`, `Binary` and `Form`; any other format is a type implementing
//! `Encode<T>` and/or `Decode<T>`, named by its path. Concord never chooses a
//! format by looking at a Rust type name. An endpoint without `->` has no typed output: any 2xx
//! succeeds and the body is drained without being buffered, whatever the defaults.

use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;

use bytes::Bytes;

/// Encodes a request body of type `T` with the format `Self`.
///
/// Implement it on your own marker type for a custom format, then name the type in place
/// (`Csv rows: Vec<Row>`) or in a default (`request Csv;`). The encoder runs when the request is
/// prepared; it receives the payload by value.
///
/// `CONTENT_TYPE` is the media type sent as `Content-Type` with the body, unless the body declares
/// its own (`json("application/merge-patch+json") patch: Patch`, `body("…") p: T`, or a
/// `request json("…");` default reached through `body`) or the endpoint sends a `content-type`
/// header. It is validated when the declaration is compiled (visible ASCII, like
/// `HeaderValue::from_static`): an invalid value is a compile error wherever it may be sent, and
/// is not used at all where a declared media type replaces it. A media type that varies per body
/// (a multipart boundary) is set on the native request after
/// [`Prepared::into_parts`](crate::Prepared::into_parts).
#[diagnostic::on_unimplemented(
    message = "the format `{Self}` cannot encode a `{T}` request body",
    label = "this body's format is `{Self}` (written here or inherited from a `request` default); it needs `{Self}: Encode<{T}>`",
    note = "built in: `json` (any `Serialize`, feature `json`), `text` (`String`), `bytes` (`Bytes`, `Vec<u8>`), `form` (any `Serialize`, feature `form`); the words name `Json`, `Text`, `Binary` and `Form`. Another format is a type implementing `concord::Encode<{T}>`, written by its path in place or in a `request` default"
)]
pub trait Encode<T> {
    /// The media type sent as `Content-Type` with the body (see the trait documentation).
    const CONTENT_TYPE: Option<&'static str>;

    /// Encodes the payload into a request body.
    fn encode(value: T) -> Result<reqwest::Body, BodyError>;
}

/// Decodes the complete body of a 2xx response into `T` with the format `Self`.
///
/// Implement it on your own marker type for a custom format, then name the type in place
/// (`-> Csv Vec<Row>`) or in a default (`response Csv;`). It is also usable directly, for example
/// on the body of an error response: `<Json as Decode<Problem>>::decode(bytes)`. Concord never
/// checks the response `Content-Type` and sends no `Accept`.
#[diagnostic::on_unimplemented(
    message = "the format `{Self}` cannot decode a `{T}` response",
    label = "this output's format is `{Self}` (written here or inherited from a `response` default); it needs `{Self}: Decode<{T}>`",
    note = "built in: `json` (any `DeserializeOwned`, feature `json`), `text` (`String`), `bytes` (`Bytes`, `Vec<u8>`), `form` (any `DeserializeOwned`, feature `form`); the words name `Json`, `Text`, `Binary` and `Form`. Another format is a type implementing `concord::Decode<{T}>`, written by its path in place or in a `response` default; an endpoint without typed output omits `->`"
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

/// JSON via `serde_json`, the format word `json` (feature `json`): `application/json` request
/// bodies; responses are decoded regardless of their `Content-Type`. `json item: NewItem`,
/// `-> json Item`, or `request json;` / `response json;` on a client or scope.
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

/// UTF-8 text, the format word `text`: `text/plain; charset=utf-8` request bodies; responses must
/// be valid UTF-8. `text note: String`, `-> text String`.
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

/// Raw bytes, the format word `bytes`: `application/octet-stream` request bodies; responses are
/// returned unchanged. `bytes data: Vec<u8>`, `bytes("image/png") png: Bytes`, `-> bytes Bytes`.
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

/// URL-encoded forms via `serde_urlencoded`, the format word `form` (feature `form`):
/// `application/x-www-form-urlencoded` request bodies from any `Serialize` struct, map or
/// sequence of pairs (flat values only: nested structures and sequences are an encoding error);
/// responses decode into any `DeserializeOwned`. `form credentials: Login`, `-> form Token`.
#[cfg(feature = "form")]
pub struct Form(());

#[cfg(feature = "form")]
impl<T: serde::Serialize> Encode<T> for Form {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-www-form-urlencoded");

    fn encode(value: T) -> Result<reqwest::Body, BodyError> {
        serde_urlencoded::to_string(&value)
            .map(reqwest::Body::from)
            .map_err(|e| BodyError::new("form encode error").with_source(e))
    }
}

#[cfg(feature = "form")]
impl<T: serde::de::DeserializeOwned> Decode<T> for Form {
    fn decode(body: Bytes) -> Result<T, BodyError> {
        serde_urlencoded::from_bytes(&body)
            .map_err(|e| BodyError::new("form decode error").with_source(e))
    }
}
