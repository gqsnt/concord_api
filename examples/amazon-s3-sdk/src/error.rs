//! The SDK's error type: where a call stopped, with the response metadata S3 sent.
//!
//! Every variant says whether anything was sent: [`S3Error::Input`], [`S3Error::Build`] and
//! [`S3Error::Signing`] stop before any I/O; [`S3Error::Transport`] means no response head
//! arrived; every other variant carries the [`ResponseMeta`] of the response that did.
//!
//! `Display` and `Debug` print the operation, the status, the S3 error code and request ids —
//! never credentials, the request URL, header values or S3's free-text `Message`/`Resource`
//! (which can echo object keys); those are reachable through accessors.

use std::fmt;

use concord::bytes::BytesMut;
use concord::reqwest::{self, StatusCode, header::HeaderMap};

use crate::types::{ETag, HttpDate};

/// The largest S3 error document the SDK reads (bytes); a larger body is not parsed.
pub const MAX_ERROR_BODY: usize = 64 * 1024;

/// A value refused before any request was built.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct InputError {
    what: &'static str,
    rule: &'static str,
}

impl InputError {
    pub(crate) const fn new(what: &'static str, rule: &'static str) -> Self {
        Self { what, rule }
    }

    /// What was refused (`object key`, `bucket name`, ...).
    pub fn what(&self) -> &'static str {
        self.what
    }

    /// The rule it broke.
    pub fn rule(&self) -> &'static str {
        self.rule
    }
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {}", self.what, self.rule)
    }
}

impl fmt::Debug for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputError")
            .field("what", &self.what)
            .field("rule", &self.rule)
            .finish()
    }
}

impl std::error::Error for InputError {}

/// The request could not be signed (nothing was sent).
pub struct SigningError {
    reason: &'static str,
    header: Option<String>,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl SigningError {
    pub(crate) fn new(reason: &'static str) -> Self {
        Self {
            reason,
            header: None,
            source: None,
        }
    }

    pub(crate) fn header(reason: &'static str, name: &str) -> Self {
        Self {
            header: Some(name.to_owned()),
            ..Self::new(reason)
        }
    }

    pub(crate) fn with_source(
        mut self,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Why signing failed.
    pub fn reason(&self) -> &'static str {
        self.reason
    }

    /// The header name involved, if any (never its value).
    pub fn header_name(&self) -> Option<&str> {
        self.header.as_deref()
    }

    /// The signer's own error, **unredacted** (it may quote the request).
    pub fn source_unredacted(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}

impl fmt::Display for SigningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "request not signed: {}", self.reason)?;
        if let Some(name) = &self.header {
            write!(f, " (header `{name}`)")?;
        }
        Ok(())
    }
}

impl fmt::Debug for SigningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningError")
            .field("reason", &self.reason)
            .field("header", &self.header)
            .field("source", &self.source.as_ref().map(|_| "<withheld>"))
            .finish()
    }
}

impl std::error::Error for SigningError {}

/// What every S3 response says about itself: status and request ids.
#[derive(Clone, PartialEq, Eq)]
pub struct ResponseMeta {
    status: StatusCode,
    request_id: Option<String>,
    extended_request_id: Option<String>,
    bucket_region: Option<String>,
}

impl ResponseMeta {
    pub(crate) fn from_headers(status: StatusCode, headers: &HeaderMap) -> Self {
        let text = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        Self {
            status,
            request_id: text("x-amz-request-id"),
            extended_request_id: text("x-amz-id-2"),
            bucket_region: text("x-amz-bucket-region"),
        }
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// `x-amz-request-id`.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// `x-amz-id-2` (the extended request id AWS support asks for).
    pub fn extended_request_id(&self) -> Option<&str> {
        self.extended_request_id.as_deref()
    }

    /// `x-amz-bucket-region`, when S3 says where the bucket lives (on redirects and some errors).
    pub fn bucket_region(&self) -> Option<&str> {
        self.bucket_region.as_deref()
    }
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseMeta")
            .field("status", &self.status.as_u16())
            .field("request_id", &self.request_id)
            .field("extended_request_id", &self.extended_request_id)
            .field("bucket_region", &self.bucket_region)
            .finish()
    }
}

/// What became of an error response's body.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum ErrorBody {
    /// An S3 `<Error>` document was read and parsed.
    Parsed,
    /// No body (every `HEAD` error; S3 documents that `HEAD` errors carry only the status).
    Empty,
    /// A body that is not an S3 `<Error>` document (a proxy's page, invalid XML).
    Unrecognized,
    /// More than [`MAX_ERROR_BODY`] bytes: not read to its end, not parsed.
    TooLarge,
    /// Reading the body failed after the head arrived.
    ReadFailed,
}

/// A non-2xx S3 response (other than `304 Not Modified`), with its `<Error>` document when one
/// was sent.
pub struct ApiError {
    operation: &'static str,
    meta: ResponseMeta,
    body: ErrorBody,
    code: Option<String>,
    message: Option<String>,
    resource: Option<String>,
    document_request_id: Option<String>,
    host_id: Option<String>,
}

impl ApiError {
    /// The operation that failed (`S3Api.bucket.get_object`).
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    /// Status and request ids from the response head.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.meta.status
    }

    /// The S3 error code (`NoSuchKey`, `PreconditionFailed`, `SignatureDoesNotMatch`); `None`
    /// without a parsed document (always for `HEAD`).
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    /// S3's English message, **unredacted** (it may echo the request).
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The bucket or object S3 names, **unredacted**.
    pub fn resource(&self) -> Option<&str> {
        self.resource.as_deref()
    }

    /// The document's `<RequestId>` (the head's `x-amz-request-id` is in [`meta`](Self::meta)).
    pub fn document_request_id(&self) -> Option<&str> {
        self.document_request_id.as_deref()
    }

    /// The document's `<HostId>`.
    pub fn host_id(&self) -> Option<&str> {
        self.host_id.as_deref()
    }

    /// What became of the body.
    pub fn body(&self) -> ErrorBody {
        self.body
    }

    /// Reads the (bounded) body of an error response and parses it as an S3 `<Error>` document.
    pub(crate) async fn read(
        operation: &'static str,
        mut response: reqwest::Response,
        meta: ResponseMeta,
    ) -> Self {
        let mut error = Self {
            operation,
            meta,
            body: ErrorBody::Empty,
            code: None,
            message: None,
            resource: None,
            document_request_id: None,
            host_id: None,
        };
        let mut buffer = BytesMut::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if buffer.len() + chunk.len() > MAX_ERROR_BODY {
                        error.body = ErrorBody::TooLarge;
                        return error;
                    }
                    buffer.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(_) => {
                    error.body = ErrorBody::ReadFailed;
                    return error;
                }
            }
        }
        if buffer.is_empty() {
            return error;
        }
        match crate::xml::parse_error_document(&buffer) {
            Some(doc) => {
                error.body = ErrorBody::Parsed;
                error.code = doc.code;
                error.message = doc.message;
                error.resource = doc.resource;
                error.document_request_id = doc.request_id;
                error.host_id = doc.host_id;
            }
            None => error.body = ErrorBody::Unrecognized,
        }
        error
    }
}

impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiError")
            .field("operation", &self.operation)
            .field("meta", &self.meta)
            .field("code", &self.code)
            .field("body", &self.body)
            .finish_non_exhaustive()
    }
}

/// `304 Not Modified`: an `If-None-Match` or `If-Modified-Since` condition said the copy the
/// caller holds is current. No body; the validators S3 returned are kept.
#[derive(Clone)]
pub struct NotModified {
    operation: &'static str,
    meta: ResponseMeta,
    etag: Option<ETag>,
    last_modified: Option<HttpDate>,
}

impl NotModified {
    pub(crate) fn from_headers(
        operation: &'static str,
        meta: ResponseMeta,
        headers: &HeaderMap,
    ) -> Self {
        let text = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
        Self {
            operation,
            meta,
            etag: text("etag").and_then(|v| ETag::parse(v).ok()),
            last_modified: text("last-modified").and_then(|v| HttpDate::parse(v).ok()),
        }
    }

    /// The operation (`S3Api.bucket.get_object`).
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    /// Status and request ids.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The current entity tag, when S3 sent it.
    pub fn etag(&self) -> Option<&ETag> {
        self.etag.as_ref()
    }

    /// The current `Last-Modified`, when S3 sent it.
    pub fn last_modified(&self) -> Option<HttpDate> {
        self.last_modified
    }
}

impl fmt::Debug for NotModified {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NotModified")
            .field("operation", &self.operation)
            .field("meta", &self.meta)
            .field("etag", &self.etag)
            .field("last_modified", &self.last_modified)
            .finish()
    }
}

/// A response that breaks the S3 contract this SDK relies on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum ProtocolErrorKind {
    /// A listing page with `IsTruncated` true and no `NextContinuationToken`: no way to continue.
    TruncatedWithoutToken,
    /// A listing page returned the continuation token it was requested with: following it would
    /// loop.
    RepeatedToken,
    /// A `206 Partial Content` without a valid single `Content-Range`.
    MissingContentRange,
    /// A `206` whose `Content-Range` does not answer the requested range.
    RangeMismatch,
    /// A `multipart/byteranges` response: this SDK requests one range and reads one.
    MultipleRanges,
    /// `Content-Length` disagrees with the `Content-Range` of a `206`.
    LengthMismatch,
}

impl ProtocolErrorKind {
    fn label(self) -> &'static str {
        match self {
            Self::TruncatedWithoutToken => "a truncated listing page has no NextContinuationToken",
            Self::RepeatedToken => {
                "a listing page repeated the continuation token it was requested with"
            }
            Self::MissingContentRange => "a 206 response has no valid single Content-Range",
            Self::RangeMismatch => "the Content-Range does not answer the requested range",
            Self::MultipleRanges => "a multipart/byteranges response (one range was requested)",
            Self::LengthMismatch => "Content-Length disagrees with Content-Range",
        }
    }
}

/// How reading a streamed object body ended without success.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum BodyErrorKind {
    /// The transport failed while reading (connection reset, timeout).
    Transport,
    /// The body ended before `Content-Length` bytes arrived.
    Truncated,
    /// More bytes than the caller's limit (declared by `Content-Length`, or received).
    LimitExceeded,
}

/// A failure while reading an object body, after a successful response head.
pub struct BodyError {
    operation: &'static str,
    kind: BodyErrorKind,
    received: u64,
    expected: Option<u64>,
    meta: ResponseMeta,
    transport: Option<concord::Error>,
}

impl BodyError {
    pub(crate) fn new(
        operation: &'static str,
        kind: BodyErrorKind,
        received: u64,
        expected: Option<u64>,
        meta: ResponseMeta,
        transport: Option<concord::Error>,
    ) -> Self {
        Self {
            operation,
            kind,
            received,
            expected,
            meta,
            transport,
        }
    }

    /// What happened.
    pub fn kind(&self) -> BodyErrorKind {
        self.kind
    }

    /// Bytes received before the failure (a partial body is never returned as complete).
    pub fn received(&self) -> u64 {
        self.received
    }

    /// The `Content-Length` S3 declared.
    pub fn expected(&self) -> Option<u64> {
        self.expected
    }

    /// The response head's metadata.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The Concord transport error (URL removed), for [`BodyErrorKind::Transport`].
    pub fn transport(&self) -> Option<&concord::Error> {
        self.transport.as_ref()
    }
}

impl fmt::Debug for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BodyError")
            .field("operation", &self.operation)
            .field("kind", &self.kind)
            .field("received", &self.received)
            .field("expected", &self.expected)
            .field("meta", &self.meta)
            .finish()
    }
}

/// An S3 call that did not produce its result.
#[non_exhaustive]
pub enum S3Error {
    /// An SDK input was refused; nothing was sent.
    Input(InputError),
    /// Concord could not build the request (`ErrorKind::Build`); nothing was sent.
    Build(concord::Error),
    /// The request could not be signed; nothing was sent.
    Signing(SigningError),
    /// No response head arrived (connect failure, timeout): Concord's `Transport` error, URL
    /// removed.
    Transport(concord::Error),
    /// S3 answered a non-2xx status other than `304`.
    Api(Box<ApiError>),
    /// `304 Not Modified` (a read condition held the caller's copy current).
    NotModified(Box<NotModified>),
    /// A 2xx response whose head the format refused (`Metadata`) or whose body did not decode
    /// (`Decode`), or whose body could not be read (`Transport`).
    Response {
        /// Concord's error (`Metadata` keeps the unread response).
        error: concord::Error,
        /// The response head's metadata.
        meta: Box<ResponseMeta>,
    },
    /// The response broke the S3 contract the SDK relies on.
    Protocol {
        /// The operation.
        operation: &'static str,
        /// What was wrong.
        kind: ProtocolErrorKind,
        /// The response head's metadata.
        meta: Box<ResponseMeta>,
    },
    /// Reading a streamed object body failed or was refused.
    Body(Box<BodyError>),
}

impl S3Error {
    pub(crate) fn protocol(
        operation: &'static str,
        kind: ProtocolErrorKind,
        meta: ResponseMeta,
    ) -> Self {
        Self::Protocol {
            operation,
            kind,
            meta: Box::new(meta),
        }
    }

    /// The metadata of the response, when one arrived.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Api(e) => Some(e.meta()),
            Self::NotModified(e) => Some(e.meta()),
            Self::Response { meta, .. } | Self::Protocol { meta, .. } => Some(meta),
            Self::Body(e) => Some(e.meta()),
            Self::Input(_) | Self::Build(_) | Self::Signing(_) | Self::Transport(_) => None,
        }
    }

    /// The response status, when one arrived.
    pub fn status(&self) -> Option<StatusCode> {
        self.meta().map(ResponseMeta::status)
    }

    /// The S3 error code of an [`S3Error::Api`] with a parsed document.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Api(e) => e.code(),
            _ => None,
        }
    }

    /// `x-amz-request-id` of the response, when one arrived.
    pub fn request_id(&self) -> Option<&str> {
        self.meta().and_then(ResponseMeta::request_id)
    }

    /// `404` (`NoSuchKey`, `NoSuchBucket`, or a `HEAD` 404 without a document).
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Api(e) if e.status() == StatusCode::NOT_FOUND)
    }

    /// `412 Precondition Failed`: an `If-Match` / `If-Unmodified-Since` (read) or an
    /// `If-Match` / `If-None-Match: *` (write) condition did not hold.
    pub fn is_precondition_failed(&self) -> bool {
        matches!(self, Self::Api(e) if e.status() == StatusCode::PRECONDITION_FAILED)
    }

    /// `304 Not Modified`.
    pub fn is_not_modified(&self) -> bool {
        matches!(self, Self::NotModified(_))
    }

    /// Whether the request was never sent.
    pub fn is_before_send(&self) -> bool {
        matches!(self, Self::Input(_) | Self::Build(_) | Self::Signing(_))
    }
}

impl fmt::Display for S3Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(e) => fmt::Display::fmt(e, f),
            Self::Build(e) | Self::Transport(e) => fmt::Display::fmt(e, f),
            Self::Signing(e) => fmt::Display::fmt(e, f),
            Self::Api(e) => {
                write!(f, "{}: HTTP {}", e.operation, e.status().as_u16())?;
                match e.code() {
                    Some(code) => write!(f, " {code}")?,
                    None => write!(f, " (no S3 error document)")?,
                }
                if let Some(id) = e.meta.request_id() {
                    write!(f, " (request id {id})")?;
                }
                Ok(())
            }
            Self::NotModified(e) => write!(f, "{}: 304 Not Modified", e.operation),
            Self::Response { error, .. } => fmt::Display::fmt(error, f),
            Self::Protocol {
                operation, kind, ..
            } => write!(f, "{operation}: {}", kind.label()),
            Self::Body(e) => {
                let what = match e.kind {
                    BodyErrorKind::Transport => "reading the object body failed",
                    BodyErrorKind::Truncated => "the object body ended early",
                    BodyErrorKind::LimitExceeded => "the object body exceeds the caller's limit",
                };
                write!(f, "{}: {what} ({} bytes received", e.operation, e.received)?;
                if let Some(expected) = e.expected {
                    write!(f, " of {expected}")?;
                }
                f.write_str(")")
            }
        }
    }
}

impl fmt::Debug for S3Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(e) => f.debug_tuple("Input").field(e).finish(),
            Self::Build(e) => f.debug_tuple("Build").field(e).finish(),
            Self::Signing(e) => f.debug_tuple("Signing").field(e).finish(),
            Self::Transport(e) => f.debug_tuple("Transport").field(e).finish(),
            Self::Api(e) => f.debug_tuple("Api").field(e).finish(),
            Self::NotModified(e) => f.debug_tuple("NotModified").field(e).finish(),
            Self::Response { error, meta } => f
                .debug_struct("Response")
                .field("error", error)
                .field("meta", meta)
                .finish(),
            Self::Protocol {
                operation,
                kind,
                meta,
            } => f
                .debug_struct("Protocol")
                .field("operation", operation)
                .field("kind", kind)
                .field("meta", meta)
                .finish(),
            Self::Body(e) => f.debug_tuple("Body").field(e).finish(),
        }
    }
}

impl std::error::Error for S3Error {}

impl From<InputError> for S3Error {
    fn from(error: InputError) -> Self {
        Self::Input(error)
    }
}

impl From<SigningError> for S3Error {
    fn from(error: SigningError) -> Self {
        Self::Signing(error)
    }
}
