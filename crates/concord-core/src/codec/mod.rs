//! Stable buffered request and response codec contracts.
//!
//! Implement [`EncodeBody`] or [`DecodeBody`] for an ordinary complete
//! buffered codec. Blanket implementations provide
//! [`crate::body::RequestBodySpec`] or
//! [`crate::response::ResponseBodySpec`]. Implement those full lifecycle
//! specifications directly only when structural preparation or sealed-mode
//! response conversion is required.

use std::error::Error;
use std::fmt;

use bytes::Bytes;
use http::{Method, StatusCode};

/// Declares the one exact media type used by [`crate::body::Stream<M>`].
///
/// The marker is shared across concurrent operations and has no runtime callback or sensitive
/// input. Its constant must be non-sensitive and valid for every use; implementations must not
/// rely on mutable global state. Request content-type ownership and response acceptance remain
/// direction-specific responsibilities of the selected body and response specifications.
pub trait MediaType: Send + Sync + 'static {
    /// Valid HTTP media type owned by the marker.
    const MEDIA_TYPE: &'static str;
}

/// Marker selecting no request or response media assertion.
///
/// This marker intentionally does not implement [`MediaType`]. Bare
/// [`Stream`](crate::body::Stream) request bodies therefore insert no
/// `Content-Type`, and bare stream responses insert no `Accept` or validate
/// `Content-Type`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnyMedia;

/// Built-in `application/octet-stream` media marker.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OctetStream;

impl MediaType for OctetStream {
    const MEDIA_TYPE: &'static str = "application/octet-stream";
}

/// Safe request metadata supplied to a request encoder.
pub struct EncodeContext<'a> {
    /// Stable generated endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Positive finite request-body limit, when one applies.
    pub body_limit: Option<usize>,
}

/// Encodes one application input into a complete buffered request body.
///
/// Encoding is synchronous, runs at most once for a pending execution, and
/// begins only after `before_send` approves the request. Input may be
/// sensitive. Implementors are trusted application extensions: they must
/// bound their own CPU, memory, retries, maps, queues, spawned tasks, metric
/// cardinality, logging, storage, and external I/O. Calls for independent
/// operations may occur concurrently. Concord cannot preempt or cancel this
/// synchronous work; it checks the operation deadline immediately after return,
/// and cancellation prevents only later Concord work. Implementors own the
/// confidentiality of input and all application logging or storage.
pub trait EncodeBody: Send + Sync + 'static {
    /// Application value accepted by the generated endpoint.
    type Input: Send + 'static;

    /// Request `Content-Type` owned by this codec, when present.
    const CONTENT_TYPE: Option<&'static str>;

    /// Encodes one input without access to URL, headers, credentials, or native request state.
    fn encode(input: Self::Input, context: EncodeContext<'_>) -> Result<Bytes, CodecError>;
}

/// Stable finite category for one buffered codec failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecErrorKind {
    /// Request encoding failed.
    Encode,
    /// Response media validation or decoding failed.
    Decode,
    /// Encoding exceeded a configured finite limit.
    Limit,
}

/// A category-only safe failure reported by an application codec.
///
/// This value retains no codec-authored text or native source error. A codec
/// remains trusted for any application-owned side effects it performs before
/// returning this value.
pub struct CodecError {
    kind: CodecErrorKind,
}

impl CodecError {
    /// Creates a codec failure in one finite category.
    #[must_use]
    pub const fn new(kind: CodecErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable codec failure category.
    #[must_use]
    pub const fn kind(&self) -> CodecErrorKind {
        self.kind
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "codec operation failed ({:?})", self.kind)
    }
}

impl fmt::Debug for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodecError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl Error for CodecError {}

/// Safe response metadata supplied to a response decoder.
pub struct DecodeContext<'a> {
    /// Stable generated endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Final application response status.
    pub status: StatusCode,
    /// Parsed response content type, when one was available.
    pub content_type: Option<&'a str>,
}

/// Decodes a complete bounded response body.
///
/// Validation runs after known-length preflight and before body polling.
/// Decoding runs synchronously only after complete bounded collection. The
/// body may be sensitive. Implementors are trusted application extensions and
/// must bound their CPU, memory, retries, maps, queues, spawned tasks, metric
/// cardinality, logging, storage, and external I/O. Calls for independent
/// operations may occur concurrently. Concord cannot preempt or cancel these
/// synchronous callbacks; it checks the operation deadline immediately after
/// return, and cancellation prevents only later Concord work. Implementors own
/// the confidentiality of body and context data and all application logging or
/// storage.
pub trait DecodeBody: Send + Sync + 'static {
    /// Value produced by the decoder.
    type Output: Send + 'static;

    /// Default response media range requested by this decoder.
    const ACCEPT: Option<&'static str>;

    /// Validates response media metadata before decoding.
    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    /// Decodes one complete bounded body.
    fn decode(body: Bytes, context: DecodeContext<'_>) -> Result<Self::Output, CodecError>;
}

/// Built-in UTF-8 text request and response codec.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Text;

impl EncodeBody for Text {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for Text {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("text/plain");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|error| {
            drop(error);
            CodecError::new(CodecErrorKind::Decode)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_context() -> EncodeContext<'static> {
        EncodeContext {
            endpoint: "CodecApi::messages::create",
            method: &Method::POST,
            body_limit: Some(1024),
        }
    }

    fn decode_context(content_type: Option<&str>) -> DecodeContext<'_> {
        DecodeContext {
            endpoint: "CodecApi::messages::create",
            method: &Method::POST,
            status: StatusCode::OK,
            content_type,
        }
    }

    #[test]
    fn text_is_exact_utf8_and_does_not_require_response_media_metadata() {
        let encoded = Text::encode("hello ♥".to_owned(), encode_context()).expect("text encode");
        assert_eq!(encoded, "hello ♥".as_bytes());
        Text::validate_content_type(None).expect("missing media is accepted");
        Text::validate_content_type(Some("application/octet-stream"))
            .expect("other media is accepted");
        assert_eq!(
            Text::decode(encoded, decode_context(None)).expect("text decode"),
            "hello ♥"
        );
        let error = Text::decode(Bytes::from_static(&[0xff]), decode_context(None))
            .expect_err("invalid UTF-8 must fail");
        assert_eq!(error.kind(), CodecErrorKind::Decode);
        assert_eq!(error.to_string(), "codec operation failed (Decode)");
    }

    #[test]
    fn media_markers_have_the_normative_values() {
        assert_eq!(OctetStream::MEDIA_TYPE, "application/octet-stream");
        assert_eq!(Text::CONTENT_TYPE, Some("text/plain; charset=utf-8"));
        assert_eq!(Text::ACCEPT, Some("text/plain"));
    }
}
