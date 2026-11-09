//! Typed failures. `Display` and `Debug` name the stage, the HTTP status and the provider's error
//! `code`; they never contain an application key, an authorization token, an issued URL, file
//! content or the provider's free-text `message` (read it deliberately with
//! [`ProviderError::message`]).

use std::fmt;

use concord::ResponseMeta;
use concord::reqwest::StatusCode;
use serde::Deserialize;

/// A local refusal: nothing was sent. The refused value is never included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InputError {
    /// The application key ID or key is empty, the ID contains a colon, or either contains a
    /// control character (the Basic credential cannot carry it).
    ApplicationKey,
    /// A bucket ID is empty or contains a control character.
    BucketId,
    /// A file ID is empty or contains a control character.
    FileId,
    /// A file name is empty.
    FileName,
    /// A media type is not `type/subtype` (optionally with parameters) in header-safe ASCII.
    ContentType,
    /// A SHA-1 is not 40 hexadecimal digits.
    Sha1,
    /// The base URL given to [`Client::with_base_url`](crate::Client::with_base_url) is not an
    /// `http`/`https` URL without credentials, query or fragment.
    BaseUrl,
    /// A one-shot stream declared zero bytes: upload an empty file with
    /// [`UploadFile::bytes`](crate::UploadFile::bytes).
    EmptyStream,
    /// A one-shot source was already handed to the transport; it cannot be sent again.
    SourceConsumed,
    /// `maxFileCount` exceeds B2's maximum of 10,000 (zero selects B2's default).
    MaxFileCount,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self {
            Self::ApplicationKey => "application key",
            Self::BucketId => "bucket ID",
            Self::FileId => "file ID",
            Self::FileName => "file name",
            Self::ContentType => "content type",
            Self::Sha1 => "SHA-1",
            Self::BaseUrl => "base URL",
            Self::EmptyStream => "one-shot stream of zero bytes",
            Self::SourceConsumed => "one-shot source already consumed",
            Self::MaxFileCount => "maximum file count (over 10000)",
        };
        write!(f, "invalid B2 input: {what}")
    }
}

impl std::error::Error for InputError {}

/// The `code` of a B2 error document: every code the implemented operations document, and
/// [`Other`](Self::Other) for any other identifier (kept as sent).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// `bad_request` (400): wrong fields or illegal values.
    BadRequest,
    /// `bad_bucket_id` (400): the bucket ID matches no bucket.
    BadBucketId,
    /// `invalid_bucket_id` (400, list).
    InvalidBucketId,
    /// `out_of_range` (400, list): `maxFileCount` out of range.
    OutOfRange,
    /// `file_not_present` (400, delete): the file name and ID do not identify a version.
    FileNotPresent,
    /// `too_many_requests` (429, list).
    TooManyRequests,
    /// `auth_token_limit` (400, upload): the upload token is already in use.
    AuthTokenLimit,
    /// `custom_timestamp_invalid` (400, upload).
    CustomTimestampInvalid,
    /// `custom_timestamp_not_allowed` (400, upload).
    CustomTimestampNotAllowed,
    /// `unauthorized` (401): wrong key, or a valid token that does not authorize this call.
    Unauthorized,
    /// `unsupported` (401, authorize): the key cannot be used with this API version.
    Unsupported,
    /// `bad_auth_token` (401): the token is not valid.
    BadAuthToken,
    /// `expired_auth_token` (401): the token expired.
    ExpiredAuthToken,
    /// `transaction_cap_exceeded` (403).
    TransactionCapExceeded,
    /// `storage_cap_exceeded` (403, get upload URL).
    StorageCapExceeded,
    /// `cap_exceeded` (403, upload).
    CapExceeded,
    /// `download_cap_exceeded` (403, download).
    DownloadCapExceeded,
    /// `access_denied` (403, download; 401, delete): encryption key or Object Lock refusal.
    AccessDenied,
    /// `not_found` (404, download).
    NotFound,
    /// `method_not_allowed` (405, upload).
    MethodNotAllowed,
    /// `request_timeout` (408, upload): the service timed out reading the file.
    RequestTimeout,
    /// `length_required` (411, upload): `Content-Length` missing.
    LengthRequired,
    /// `unsupported_media_type` (415, upload).
    UnsupportedMediaType,
    /// `range_not_satisfiable` (416, download).
    RangeNotSatisfiable,
    /// `service_unavailable` (503).
    ServiceUnavailable,
    /// An identifier this SDK does not model.
    Other(String),
}

impl ErrorCode {
    fn parse(code: &str) -> Self {
        match code {
            "bad_request" => Self::BadRequest,
            "bad_bucket_id" => Self::BadBucketId,
            "invalid_bucket_id" => Self::InvalidBucketId,
            "out_of_range" => Self::OutOfRange,
            "file_not_present" => Self::FileNotPresent,
            "too_many_requests" => Self::TooManyRequests,
            "auth_token_limit" => Self::AuthTokenLimit,
            "custom_timestamp_invalid" => Self::CustomTimestampInvalid,
            "custom_timestamp_not_allowed" => Self::CustomTimestampNotAllowed,
            "unauthorized" => Self::Unauthorized,
            "unsupported" => Self::Unsupported,
            "bad_auth_token" => Self::BadAuthToken,
            "expired_auth_token" => Self::ExpiredAuthToken,
            "transaction_cap_exceeded" => Self::TransactionCapExceeded,
            "storage_cap_exceeded" => Self::StorageCapExceeded,
            "cap_exceeded" => Self::CapExceeded,
            "download_cap_exceeded" => Self::DownloadCapExceeded,
            "access_denied" => Self::AccessDenied,
            "not_found" => Self::NotFound,
            "method_not_allowed" => Self::MethodNotAllowed,
            "request_timeout" => Self::RequestTimeout,
            "length_required" => Self::LengthRequired,
            "unsupported_media_type" => Self::UnsupportedMediaType,
            "range_not_satisfiable" => Self::RangeNotSatisfiable,
            "service_unavailable" => Self::ServiceUnavailable,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The identifier as B2 spells it.
    pub fn as_str(&self) -> &str {
        match self {
            Self::BadRequest => "bad_request",
            Self::BadBucketId => "bad_bucket_id",
            Self::InvalidBucketId => "invalid_bucket_id",
            Self::OutOfRange => "out_of_range",
            Self::FileNotPresent => "file_not_present",
            Self::TooManyRequests => "too_many_requests",
            Self::AuthTokenLimit => "auth_token_limit",
            Self::CustomTimestampInvalid => "custom_timestamp_invalid",
            Self::CustomTimestampNotAllowed => "custom_timestamp_not_allowed",
            Self::Unauthorized => "unauthorized",
            Self::Unsupported => "unsupported",
            Self::BadAuthToken => "bad_auth_token",
            Self::ExpiredAuthToken => "expired_auth_token",
            Self::TransactionCapExceeded => "transaction_cap_exceeded",
            Self::StorageCapExceeded => "storage_cap_exceeded",
            Self::CapExceeded => "cap_exceeded",
            Self::DownloadCapExceeded => "download_cap_exceeded",
            Self::AccessDenied => "access_denied",
            Self::NotFound => "not_found",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::RequestTimeout => "request_timeout",
            Self::LengthRequired => "length_required",
            Self::UnsupportedMediaType => "unsupported_media_type",
            Self::RangeNotSatisfiable => "range_not_satisfiable",
            Self::ServiceUnavailable => "service_unavailable",
            Self::Other(code) => code,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// B2's error document (`status`, `code`, `message`), as the endpoint pages document it.
#[derive(Clone, concord::Debug)]
pub struct ErrorDocument {
    status: u16,
    code: ErrorCode,
    #[sensitive]
    message: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct WireErrorDocument {
    status: u16,
    code: String,
    #[serde(default)]
    message: Option<String>,
}

impl ErrorDocument {
    /// The numeric status the document states (B2: "always matches the status in the HTTP
    /// response"; a document that disagrees is not treated as B2's, see [`ProviderError`]).
    pub fn status(&self) -> u16 {
        self.status
    }

    /// The error identifier.
    pub fn code(&self) -> &ErrorCode {
        &self.code
    }

    /// The provider's English message, **unredacted** (it may quote request values). Never
    /// printed by this SDK.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

/// What the body of a non-2xx response turned out to be.
enum Body {
    /// A B2 error document whose `status` matches the HTTP status.
    Document(ErrorDocument),
    /// A well-formed document whose `status` contradicts the HTTP status.
    Inconsistent(ErrorDocument),
    /// Not a JSON document of that shape (a proxy's HTML page, an empty or oversized body, a
    /// body that broke off): the status stays the failure.
    Unreadable(concord::Error),
}

/// A non-2xx answer from B2. The HTTP status is authoritative whatever the body turned out to
/// be; the typed document is available when the body was a consistent B2 error document.
pub struct ProviderError {
    meta: ResponseMeta,
    body: Body,
}

impl ProviderError {
    pub(crate) fn new(
        meta: ResponseMeta,
        document: Result<WireErrorDocument, concord::Error>,
    ) -> Self {
        let body = match document {
            Ok(wire) => {
                let document = ErrorDocument {
                    status: wire.status,
                    code: ErrorCode::parse(&wire.code),
                    message: wire.message,
                };
                if document.status == meta.status().as_u16() {
                    Body::Document(document)
                } else {
                    Body::Inconsistent(document)
                }
            }
            Err(error) => Body::Unreadable(error),
        };
        Self { meta, body }
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.meta.status()
    }

    /// The error code of a consistent B2 error document; `None` when the body was not one.
    pub fn code(&self) -> Option<&ErrorCode> {
        self.document().map(ErrorDocument::code)
    }

    /// The consistent B2 error document, if the body was one.
    pub fn document(&self) -> Option<&ErrorDocument> {
        match &self.body {
            Body::Document(document) => Some(document),
            _ => None,
        }
    }

    /// A well-formed document whose `status` contradicts the HTTP status (kept for inspection;
    /// [`code`](Self::code) ignores it).
    pub fn inconsistent_document(&self) -> Option<&ErrorDocument> {
        match &self.body {
            Body::Inconsistent(document) => Some(document),
            _ => None,
        }
    }

    /// Why the body was not a B2 error document (wrong media type, malformed, oversized,
    /// interrupted): Concord's error, which names the operation and keeps the head.
    pub fn unreadable(&self) -> Option<&concord::Error> {
        match &self.body {
            Body::Unreadable(error) => Some(error),
            _ => None,
        }
    }

    /// The provider's message of a consistent document, **unredacted**.
    pub fn message(&self) -> Option<&str> {
        self.document().and_then(ErrorDocument::message)
    }

    /// Status, version, headers and the declared operation of the response.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("ProviderError");
        s.field("meta", &self.meta);
        match &self.body {
            Body::Document(document) => s.field("document", document),
            Body::Inconsistent(document) => s.field("inconsistent_document", document),
            Body::Unreadable(error) => s.field("unreadable", error),
        };
        s.finish()
    }
}

/// A documented rule that a 2xx response broke. The response was answered, so for an upload the
/// file may well be stored (see [`Evidence::StoredUnverified`](crate::Evidence::StoredUnverified)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Violation {
    /// An issued `apiUrl`, `downloadUrl` or `uploadUrl` is not an `http`/`https` URL without
    /// credentials and fragment, carries a query where a base is expected, or downgrades
    /// `https` to `http`.
    IssuedUrl,
    /// An issued authorization token is empty or not valid header text.
    IssuedToken,
    /// An `allowed.buckets[].id` of `b2_authorize_account` is empty or has a control character.
    AllowedBucket,
    /// `b2_get_upload_url` answered for another bucket than the one requested.
    UploadUrlBucket,
    /// The uploaded file's version disagrees with what was sent: its `action`, `fileName`,
    /// `bucketId`, `contentLength` or `contentSha1`, or it has no `fileId`.
    UploadedFileVersion,
    /// A download answered with a status other than `200` (this SDK sends no `Range`).
    DownloadStatus,
    /// A download's `X-Bz-File-Id` is missing, repeated or another file's.
    DownloadFileId,
    /// A listed entry has an invalid ID/length or names another bucket.
    ListedFile,
    /// A delete's success document does not name the requested file version.
    DeletedFileVersion,
}

/// A documented response rule that was broken, with the response head.
#[derive(concord::Debug)]
pub struct ContractError {
    violation: Violation,
    meta: Option<ResponseMeta>,
}

impl ContractError {
    pub(crate) fn new(violation: Violation, meta: Option<ResponseMeta>) -> Self {
        Self { violation, meta }
    }

    /// Which rule.
    pub fn violation(&self) -> Violation {
        self.violation
    }

    /// The head of the response that broke it.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        self.meta.as_ref()
    }
}

/// Why a downloaded body could not be verified against `X-Bz-Content-Sha1`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChecksumFailure {
    /// The body's SHA-1 differs from the header's.
    Mismatch {
        /// The header's digest.
        expected: String,
        /// The digest of the bytes received.
        actual: String,
    },
    /// No `X-Bz-Content-Sha1`, or more than one.
    Missing,
    /// `none`: B2 has no SHA-1 for this file (a large file); nothing to verify against.
    Unavailable,
    /// A value that is not 40 hexadecimal digits.
    Malformed,
}

/// A download whose bytes were received but not verified. The bytes are withheld from the
/// caller.
#[derive(concord::Debug)]
pub struct ChecksumError {
    failure: ChecksumFailure,
    meta: ResponseMeta,
}

impl ChecksumError {
    pub(crate) fn new(failure: ChecksumFailure, meta: ResponseMeta) -> Self {
        Self { failure, meta }
    }

    /// What went wrong.
    pub fn failure(&self) -> &ChecksumFailure {
        &self.failure
    }

    /// The download's response head.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }
}

/// An SDK failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Refused locally; nothing was sent.
    Input(InputError),
    /// Concord's failure: building (nothing sent), transport (no answer, or a body that broke
    /// off), a refused response head, an undecodable or oversized 2xx body. It names the
    /// operation and keeps the head when one arrived ([`concord::Error::head`]).
    Concord(concord::Error),
    /// A non-2xx answer.
    Provider(Box<ProviderError>),
    /// A 2xx answer that broke a documented rule.
    Contract(Box<ContractError>),
    /// A download that does not verify.
    Checksum(Box<ChecksumError>),
    /// An upload policy's total deadline expired while an attempt was in flight; that attempt
    /// was abandoned and may have stored the file.
    Deadline,
}

impl Error {
    pub(crate) fn contract(violation: Violation, meta: Option<ResponseMeta>) -> Self {
        Self::Contract(Box::new(ContractError::new(violation, meta)))
    }

    /// The B2 error code of a consistent provider error document.
    pub fn code(&self) -> Option<&ErrorCode> {
        match self {
            Self::Provider(e) => e.code(),
            _ => None,
        }
    }

    /// The provider failure, if this is one.
    pub fn provider(&self) -> Option<&ProviderError> {
        match self {
            Self::Provider(e) => Some(e),
            _ => None,
        }
    }

    /// The HTTP status of any failure after a response head arrived.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Concord(e) => e.head().map(|head| head.status()),
            Self::Provider(e) => Some(e.status()),
            Self::Contract(e) => e.meta().map(ResponseMeta::status),
            Self::Checksum(e) => Some(e.meta().status()),
            Self::Input(_) | Self::Deadline => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord(error)
    }
}

impl From<InputError> for Error {
    fn from(error: InputError) -> Self {
        Self::Input(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(e) => e.fmt(f),
            Self::Concord(e) => e.fmt(f),
            Self::Provider(e) => {
                match e.meta().operation() {
                    Some(op) => write!(f, "{op}: ")?,
                    None => f.write_str("B2: ")?,
                }
                write!(f, "HTTP status {}", e.status().as_u16())?;
                match &e.body {
                    Body::Document(d) => write!(f, " ({})", d.code),
                    Body::Inconsistent(_) => f.write_str(" (error document with another status)"),
                    Body::Unreadable(_) => f.write_str(" (no B2 error document)"),
                }
            }
            Self::Contract(e) => {
                write!(f, "B2 response broke a documented rule: {:?}", e.violation)
            }
            Self::Checksum(e) => match &e.failure {
                ChecksumFailure::Mismatch { .. } => {
                    f.write_str("downloaded bytes do not match X-Bz-Content-Sha1")
                }
                ChecksumFailure::Missing => f.write_str("download has no single X-Bz-Content-Sha1"),
                ChecksumFailure::Unavailable => {
                    f.write_str("B2 has no SHA-1 for this file (X-Bz-Content-Sha1: none)")
                }
                ChecksumFailure::Malformed => f.write_str("X-Bz-Content-Sha1 is not a SHA-1"),
            },
            Self::Deadline => f.write_str("the upload deadline expired during an attempt"),
        }
    }
}

impl std::error::Error for Error {}
