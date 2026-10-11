//! Stable buffered request and response codec contracts.
//!
//! Implement [`EncodeBody`] or [`DecodeBody`] for an ordinary complete
//! buffered codec. Blanket implementations provide
//! [`crate::body::RequestRepresentation`] or
//! [`crate::response::ResponseRepresentation`]. Implement those full lifecycle
//! specifications directly only when structural preparation or sealed-mode
//! response conversion is required.

use std::error::Error;
use std::fmt;

use bytes::Bytes;
use http::{Method, StatusCode};

/// Built-in binary request and response representation.
///
/// `Binary` supports complete [`bytes::Bytes`] and `Vec<u8>` values, lazy
/// [`crate::response::ByteStream`] responses, and
/// [`crate::body::RequestStream<Bytes>`] request bodies. It owns the
/// `application/octet-stream` media type in both directions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Binary;

/// Safe request metadata supplied to a request encoder.
pub struct EncodeContext<'a> {
    /// Operational endpoint ID (an explicit `id` pin or navigation-derived).
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Positive finite request-body limit, when one applies.
    pub body_limit: Option<usize>,
}

/// Encodes one application input into a complete buffered request body.
///
/// A blanket implementation bridges every `EncodeBody` codec to
/// [`RequestRepresentation<Self::Input>`](crate::body::RequestRepresentation):
/// the bridge calls `encode` exactly once per direct operation or page, during
/// body preparation (after public head selection, before credentials and rate
/// admission), rejects output above the body limit, wraps the bytes in a
/// mechanically replayable [`PreparedBody`](crate::body::PreparedBody) and
/// applies the one declared [`EncodeBody::CONTENT_TYPE`]. There is no second
/// metadata method; automatic retries replay the prepared bytes and never call
/// `encode` again. A codec chooses either this convenience role or a full
/// `RequestRepresentation` for the same pair, never both.
///
/// # Extension contract
///
/// * **Ownership.** `encode` consumes the input; the returned bytes are owned
///   by core and replayed for automatic attempts. Codecs are marker types
///   that Concord never constructs.
/// * **Concurrency.** Calls for independent operations may occur
///   concurrently.
/// * **Cancellation.** Encoding is synchronous; Concord cannot preempt or
///   cancel it. It checks the operation deadline immediately after return,
///   and cancellation prevents only later Concord work.
/// * **Input sensitivity.** Input may be sensitive. Implementors own the
///   confidentiality of input and all application logging or storage.
/// * **Errors.** Errors are category-only: [`CodecErrorKind::Limit`] becomes a
///   request-body limit failure and other categories a request-encoding
///   failure.
/// * **Resource obligations.** Implementors are trusted application
///   extensions: they must bound their own CPU, memory, retries, maps,
///   queues, spawned tasks, metric cardinality, logging, storage and external
///   I/O, and should write into a buffer bounded by
///   [`EncodeContext::body_limit`] and stop at the first excess (returning
///   `Limit`) rather than serialize without bound.
/// * **Negative capabilities.** No access to credentials, headers, the URL or
///   the native request, and no way to declare a second, conflicting media
///   type.
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
    /// Operational endpoint ID (an explicit `id` pin or navigation-derived).
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
/// A blanket implementation bridges every `DecodeBody` codec to
/// `ResponseRepresentation<Self::Output, Driver = Buffered>`, forwarding the
/// static [`DecodeBody::ACCEPT`] constant (read without running codec code),
/// media validation and decoding with the correct complete-acquisition phase.
/// Validation runs after known-length preflight and before body polling.
/// Decoding runs synchronously only after complete bounded collection.
///
/// # Extension contract
///
/// * **Ownership.** `decode` takes ownership of the complete bounded body;
///   the output is moved to the caller. Codecs are marker types that Concord
///   never constructs.
/// * **Concurrency.** Calls for independent operations may occur
///   concurrently.
/// * **Cancellation.** Concord cannot preempt or cancel these synchronous
///   callbacks; it checks the operation deadline immediately after return,
///   and cancellation prevents only later Concord work.
/// * **Input sensitivity.** The body may be sensitive. Implementors own the
///   confidentiality of body and context data and all application logging or
///   storage.
/// * **Errors.** Errors are category-only: a validation failure becomes an
///   invalid-media-type failure, a `Decode` failure a conversion failure; no
///   codec text or source chain is retained. Conversion failures are
///   terminal, never retried.
/// * **Resource obligations.** Implementors are trusted application
///   extensions and must bound their CPU, memory, retries, maps, queues,
///   spawned tasks, metric cardinality, logging, storage and external I/O.
///   Core bounds the collected wire bytes, not the heap size of the decoded
///   output.
/// * **Negative capabilities.** A decoder cannot change the accepted status,
///   the absence rule or the acquisition mode, and receives no native
///   response or transport handle.
pub trait DecodeBody: Send + Sync + 'static {
    /// Value produced by the decoder.
    type Output: Send + 'static;

    /// Static response media range negotiated for this decoder.
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
        assert_eq!(Text::CONTENT_TYPE, Some("text/plain; charset=utf-8"));
        assert_eq!(Text::ACCEPT, Some("text/plain"));
    }
}
