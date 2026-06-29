use base64::Engine;
use bytes::Bytes;
use http::HeaderValue;
use std::fmt;

#[cfg(feature = "json")]
pub(crate) mod json;

pub(crate) mod text;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Binary,
    Text,
}

pub trait FormatType {
    const FORMAT_TYPE: Format;

    fn into_encoded_string(bytes: Bytes) -> String {
        match Self::FORMAT_TYPE {
            Format::Binary => base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes),
            Format::Text => String::from_utf8_lossy(bytes.as_ref()).to_string(),
        }
    }
}

pub trait ContentType: Send + Sync + 'static {
    const CONTENT_TYPE: &'static str;

    fn header_value() -> Result<HeaderValue, http::header::InvalidHeaderValue> {
        HeaderValue::from_str(Self::CONTENT_TYPE)
    }

    fn try_header_value() -> Result<HeaderValue, http::header::InvalidHeaderValue> {
        Self::header_value()
    }
}

pub trait Decodes<T>: FormatType {
    type Error: std::error::Error + Send + Sync + 'static;
    fn decode_owned(bytes: Bytes) -> Result<T, Self::Error>;
}

pub trait Encodes<T>: FormatType {
    type Error: std::error::Error + Send + Sync + 'static;
    fn encode(output: &T) -> Result<Bytes, Self::Error>;
}

/// Context provided to a custom request body codec.
///
/// This type is part of the stable advanced API and intentionally contains
/// only request metadata, not internal request plans.
#[derive(Clone, Copy, Debug)]
pub struct EncodeContext<'a> {
    endpoint: &'a str,
    method: &'a http::Method,
}

impl<'a> EncodeContext<'a> {
    pub fn new(endpoint: &'a str, method: &'a http::Method) -> Self {
        Self { endpoint, method }
    }

    pub fn endpoint(&self) -> &'a str {
        self.endpoint
    }

    pub fn method(&self) -> &'a http::Method {
        self.method
    }
}

/// Context provided to a custom response codec.
///
/// This type is part of the stable advanced API and intentionally contains
/// only response metadata, not internal response plans.
#[derive(Clone, Copy, Debug)]
pub struct DecodeContext<'a> {
    endpoint: &'a str,
    method: &'a http::Method,
    status: http::StatusCode,
    content_type: Option<&'a str>,
}

impl<'a> DecodeContext<'a> {
    pub fn new(
        endpoint: &'a str,
        method: &'a http::Method,
        status: http::StatusCode,
        content_type: Option<&'a str>,
    ) -> Self {
        Self {
            endpoint,
            method,
            status,
            content_type,
        }
    }

    pub fn endpoint(&self) -> &'a str {
        self.endpoint
    }

    pub fn method(&self) -> &'a http::Method {
        self.method
    }

    pub fn status(&self) -> http::StatusCode {
        self.status
    }

    pub fn content_type(&self) -> Option<&'a str> {
        self.content_type
    }
}

/// Encoded request body returned by a [`BodyCodec`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedBody {
    bytes: Bytes,
    format: Format,
}

impl EncodedBody {
    /// Create a binary encoded body.
    #[inline]
    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        Self {
            bytes: bytes.into(),
            format: Format::Binary,
        }
    }

    /// Create an empty body without a content type.
    #[inline]
    pub fn empty() -> Self {
        Self {
            bytes: Bytes::new(),
            format: Format::Text,
        }
    }

    /// Mark this body as text for debug previews.
    #[inline]
    pub fn text(mut self) -> Self {
        self.format = Format::Text;
        self
    }

    /// Known number of encoded bytes.
    #[inline]
    pub fn content_len(&self) -> Option<u64> {
        Some(self.bytes.len() as u64)
    }

    /// Split into bytes and debug format.
    #[inline]
    pub fn into_parts(self) -> (Bytes, Format) {
        (self.bytes, self.format)
    }
}

/// Error returned by custom codecs.
#[derive(Debug)]
pub struct CodecError {
    message: String,
    source: Option<crate::error::FxError>,
}

impl CodecError {
    /// Create a codec error from a message.
    #[inline]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// Create a codec error with an underlying source error.
    #[inline]
    pub fn with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}

/// Stable advanced trait for request body codecs.
///
/// Implement this on a marker type such as `Cbor<CreateUser>`. The associated
/// `Value` is the value users pass to generated endpoint methods.
pub trait BodyCodec: Send + Sync + 'static {
    type Value: Send + 'static;
    type Content: ContentType;

    /// Fallible content type applied to requests encoded by this codec.
    fn try_content_type() -> Result<Option<HeaderValue>, http::header::InvalidHeaderValue> {
        Ok(Some(<Self::Content as ContentType>::try_header_value()?))
    }

    /// Content type applied to requests encoded by this codec.
    fn content_type() -> Option<HeaderValue> {
        Self::try_content_type().ok().flatten()
    }

    /// Debug formatting mode for encoded request bytes.
    fn format() -> Format {
        Format::Binary
    }

    /// Encode a request body value.
    fn encode(value: Self::Value, ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError>;
}

/// Stable advanced trait for response codecs.
///
/// Implement this on a marker type such as `Cbor<User>`. The associated
/// `Value` is the value returned by generated endpoint methods.
pub trait ResponseCodec: Send + Sync + 'static {
    type Value: Send + 'static;
    type Content: ContentType;

    /// Fallible accept header value for responses decoded by this codec.
    fn try_accept() -> Result<Option<HeaderValue>, http::header::InvalidHeaderValue> {
        Ok(Some(<Self::Content as ContentType>::try_header_value()?))
    }

    /// Accept header value for responses decoded by this codec.
    fn accept() -> Option<HeaderValue> {
        Self::try_accept().ok().flatten()
    }

    /// Whether this codec expects no response body.
    fn is_no_content() -> bool {
        false
    }

    /// Debug formatting mode for response bytes.
    fn format() -> Format {
        Format::Binary
    }

    /// Decode response bytes.
    fn decode(bytes: Bytes, ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError>;
}

pub struct NoContent;

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoDeclaredContentType;

impl ContentType for NoDeclaredContentType {
    const CONTENT_TYPE: &'static str = "application/octet-stream";
}

impl FormatType for NoContent {
    const FORMAT_TYPE: Format = Format::Text;
}

impl Encodes<()> for NoContent {
    type Error = std::convert::Infallible;
    fn encode(_output: &()) -> Result<Bytes, Self::Error> {
        Ok(Bytes::new())
    }
}

impl Decodes<()> for NoContent {
    type Error = std::convert::Infallible;
    fn decode_owned(_bytes: Bytes) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl BodyCodec for NoContent {
    type Value = ();
    type Content = NoDeclaredContentType;

    fn try_content_type() -> Result<Option<HeaderValue>, http::header::InvalidHeaderValue> {
        Ok(None)
    }

    fn content_type() -> Option<HeaderValue> {
        None
    }

    fn format() -> Format {
        <Self as FormatType>::FORMAT_TYPE
    }

    fn encode(_value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::empty())
    }
}

impl ResponseCodec for NoContent {
    type Value = ();
    type Content = NoDeclaredContentType;

    fn try_accept() -> Result<Option<HeaderValue>, http::header::InvalidHeaderValue> {
        Ok(None)
    }

    fn accept() -> Option<HeaderValue> {
        None
    }

    fn is_no_content() -> bool {
        true
    }

    fn format() -> Format {
        <Self as FormatType>::FORMAT_TYPE
    }

    fn decode(_bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        Ok(())
    }
}
