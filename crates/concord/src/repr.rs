//! Body and response representations.
//!
//! A representation is a type named in the declaration: `body item: Json<NewItem>` or
//! `-> Json<Item>`. Concord never chooses one by looking at a Rust type name.

use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;

use bytes::Bytes;

/// A request body representation.
pub trait Encode {
    /// The Rust value the generated endpoint method accepts.
    type Value;

    /// The `Content-Type` sent with the body, unless the declared headers already set one.
    const CONTENT_TYPE: Option<&'static str>;

    /// Encodes the value into a request body.
    fn encode(value: Self::Value) -> Result<reqwest::Body, BodyError>;
}

/// A response representation, applied to the complete body of a 2xx response.
pub trait Decode {
    /// The Rust value `send()` returns.
    type Output;

    /// When `true`, the body is drained chunk by chunk without being buffered, and `decode`
    /// receives an empty buffer. Used by `()`.
    const DISCARD: bool = false;

    /// Decodes a complete response body.
    fn decode(body: Bytes) -> Result<Self::Output, BodyError>;
}

/// A body could not be encoded or decoded.
///
/// `description` must not contain body content or request values: it is shown by
/// [`Error`](crate::Error)'s `Display`. The optional `source` is reachable only through
/// [`Error::body_source`](crate::Error::body_source), because decoder messages often quote the
/// input (for example serde's `invalid type: string "..."`).
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

/// JSON via `serde_json`: `application/json` request bodies; responses are decoded regardless of
/// their `Content-Type`.
#[cfg(feature = "json")]
pub struct Json<T>(std::marker::PhantomData<fn() -> T>);

#[cfg(feature = "json")]
impl<T: serde::Serialize> Encode for Json<T> {
    type Value = T;
    const CONTENT_TYPE: Option<&'static str> = Some("application/json");

    fn encode(value: T) -> Result<reqwest::Body, BodyError> {
        serde_json::to_vec(&value)
            .map(reqwest::Body::from)
            .map_err(|e| BodyError::new(json_description("encode", &e)).with_source(e))
    }
}

#[cfg(feature = "json")]
impl<T: serde::de::DeserializeOwned> Decode for Json<T> {
    type Output = T;

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
pub struct Text(());

impl Encode for Text {
    type Value = String;
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: String) -> Result<reqwest::Body, BodyError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Decode for Text {
    type Output = String;

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
pub struct Binary(());

impl Encode for Binary {
    type Value = Bytes;
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(value: Bytes) -> Result<reqwest::Body, BodyError> {
        Ok(reqwest::Body::from(value))
    }
}

impl Decode for Binary {
    type Output = Bytes;

    fn decode(body: Bytes) -> Result<Bytes, BodyError> {
        Ok(body)
    }
}

/// No typed output: any 2xx succeeds; the body (empty or not) is drained without being buffered.
impl Decode for () {
    type Output = ();
    const DISCARD: bool = true;

    fn decode(_body: Bytes) -> Result<(), BodyError> {
        Ok(())
    }
}
