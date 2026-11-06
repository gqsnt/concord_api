//! The SDK's errors.
//!
//! [`RegistryError`] says at which stage a call failed: an input refused before anything was
//! sent, Concord's own failure (build, transport, a 2xx head or body that does not decode), the
//! registry's non-2xx answer with its documented error body, the token flow, a 2xx answer that
//! breaks the protocol, or content that does not match its digest or size.
//!
//! `Display` never contains a token, a credential, a URL or a body text. `Debug` shows the
//! registry's error codes and messages (short server texts) but never a header value, a token or
//! a URL.

use std::fmt;

use concord::bytes::Bytes;
use concord::reqwest::StatusCode;
use concord::{ErrorKind, IgnoreContentType, Json, Operation, Received};
use serde::Deserialize;

use crate::auth::Challenge;
use crate::digest::Digest;
use crate::meta::ResponseMeta;

/// The most bytes of an error body the SDK reads (64 KiB).
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// Every failure of a call made through this SDK.
#[derive(Debug)]
#[non_exhaustive]
pub enum RegistryError {
    /// An input was refused; nothing was sent.
    Input(InputError),
    /// Concord could not build the request (nothing sent), send it or read the response, or a
    /// 2xx response head or body did not decode as declared (`error.kind()` says which). `meta` is
    /// `Some` once a response head arrived.
    Request {
        /// Concord's error.
        error: concord::Error,
        /// The response metadata, when a response arrived.
        meta: Option<Box<ResponseMeta>>,
    },
    /// The registry answered with a non-2xx status (its error document kept).
    Api(Box<ApiError>),
    /// The Bearer token flow failed: the challenge, the realm or the token service.
    Auth(Box<AuthError>),
    /// A 2xx answer broke the protocol (a missing or foreign `Location`, an unexpected `Range`,
    /// a bad `Link`, an unrequested media type ...), or a server-provided URL was refused before
    /// anything was sent to it.
    Protocol(Box<ProtocolError>),
    /// Content does not match its digest or size.
    Integrity(Box<IntegrityError>),
}

impl RegistryError {
    /// The HTTP status of an [`Api`](Self::Api) error.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Api(api) => Some(api.status),
            _ => None,
        }
    }

    /// The API error, if the registry answered with a non-2xx status.
    pub fn api(&self) -> Option<&ApiError> {
        match self {
            Self::Api(api) => Some(api),
            _ => None,
        }
    }

    /// Whether the registry's error document carries `code`.
    pub fn has_code(&self, code: &ErrorCode) -> bool {
        self.api().is_some_and(|api| api.has_code(code))
    }

    /// The protocol problem, if this is a [`Protocol`](Self::Protocol) error.
    pub fn protocol_problem(&self) -> Option<&ProtocolProblem> {
        match self {
            Self::Protocol(error) => Some(&error.problem),
            _ => None,
        }
    }

    /// The integrity problem, if this is an [`Integrity`](Self::Integrity) error.
    pub fn integrity_problem(&self) -> Option<&IntegrityProblem> {
        match self {
            Self::Integrity(error) => Some(&error.problem),
            _ => None,
        }
    }

    /// The token-flow problem, if this is an [`Auth`](Self::Auth) error.
    pub fn auth_problem(&self) -> Option<AuthProblem> {
        match self {
            Self::Auth(error) => Some(error.problem),
            _ => None,
        }
    }

    /// Concord's error kind of a [`Request`](Self::Request) error.
    pub fn request_kind(&self) -> Option<ErrorKind> {
        match self {
            Self::Request { error, .. } => Some(error.kind()),
            _ => None,
        }
    }

    /// The metadata of the response involved, if one arrived.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Input(_) => None,
            Self::Request { meta, .. } => meta.as_deref(),
            Self::Api(api) => Some(&api.meta),
            Self::Auth(error) => error.meta.as_ref(),
            Self::Protocol(error) => error.meta.as_ref(),
            Self::Integrity(error) => error.meta.as_ref(),
        }
    }

    pub(crate) fn protocol(problem: ProtocolProblem, meta: Option<ResponseMeta>) -> Self {
        Self::Protocol(Box::new(ProtocolError { problem, meta }))
    }

    pub(crate) fn integrity(problem: IntegrityProblem, meta: Option<ResponseMeta>) -> Self {
        Self::Integrity(Box::new(IntegrityError { problem, meta }))
    }

    pub(crate) fn request(error: concord::Error, meta: ResponseMeta) -> Self {
        Self::Request {
            error,
            meta: Some(Box::new(meta)),
        }
    }
}

impl From<concord::Error> for RegistryError {
    fn from(error: concord::Error) -> Self {
        let meta = error
            .head_meta()
            .map(ResponseMeta::from_concord)
            .map(Box::new);
        Self::Request { error, meta }
    }
}

impl From<InputError> for RegistryError {
    fn from(error: InputError) -> Self {
        Self::Input(error)
    }
}

impl From<AuthError> for RegistryError {
    fn from(error: AuthError) -> Self {
        Self::Auth(Box::new(error))
    }
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(error) => write!(f, "input refused: {error}"),
            // Concord's own `Display`: endpoint, stage, static labels; no value, URL or body.
            Self::Request { error, .. } => fmt::Display::fmt(error, f),
            Self::Api(api) => fmt::Display::fmt(api, f),
            Self::Auth(error) => fmt::Display::fmt(error, f),
            Self::Protocol(error) => fmt::Display::fmt(error, f),
            Self::Integrity(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for RegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request { error, .. } => Some(error),
            Self::Auth(error) => error
                .source
                .as_ref()
                .map(|e| e as &(dyn std::error::Error + 'static)),
            _ => None,
        }
    }
}

/// An input the SDK refused before sending anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InputError {
    /// Not a repository name (`[a-z0-9]+(?:[._-][a-z0-9]+)*` components joined by `/`, < 256).
    RepositoryName,
    /// Not a tag (`[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}`).
    Tag,
    /// Not a content digest (`algorithm:hex`).
    Digest,
    /// A page size of 0.
    PageSize,
    /// The registry URL is not an `http`/`https` URL with a host and without user information,
    /// query or fragment.
    RegistryUrl,
    /// A trusted realm is not an `http`/`https` URL with a host.
    RealmUrl,
    /// A Basic credential is unusable (a `:` in the user name, or a control character).
    Credentials,
    /// An empty list of accepted manifest media types.
    NoMediaType,
    /// Manifest bytes that are not an image manifest of their declared media type.
    Manifest,
    /// A manifest pushed by digest whose bytes have another digest.
    ReferenceDigest,
    /// An empty chunk.
    EmptyChunk,
    /// A first chunk of exactly one byte: the registry would acknowledge it as `0-0`, which
    /// also means "nothing received". Send at least two bytes, or the whole blob when completing.
    OneByteFirstChunk,
    /// A chunk size below 2 for a chunked push.
    ChunkSize,
    /// The upload's state is uncertain (an interrupted or refused request): query its status
    /// first.
    StatusRequired,
    /// The upload was already completed.
    UploadFinished,
    /// The blob reader already failed or finished.
    ReaderEnded,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RepositoryName => "not a repository name",
            Self::Tag => "not a tag",
            Self::Digest => "not a content digest",
            Self::PageSize => "a page size must be at least 1",
            Self::RegistryUrl => "the registry URL must be http(s) with a host, without user information, query or fragment",
            Self::RealmUrl => "a trusted realm must be an http(s) URL with a host",
            Self::Credentials => "the Basic credentials cannot be encoded",
            Self::NoMediaType => "at least one manifest media type must be accepted",
            Self::Manifest => "the bytes are not an image manifest of the declared media type",
            Self::ReferenceDigest => "the digest reference does not match the manifest bytes",
            Self::EmptyChunk => "a chunk must not be empty",
            Self::OneByteFirstChunk => "a first chunk of one byte cannot be acknowledged unambiguously",
            Self::ChunkSize => "a chunk size must be at least 2",
            Self::StatusRequired => "the upload state is uncertain: query its status first",
            Self::UploadFinished => "the upload is already complete",
            Self::ReaderEnded => "the blob reader already ended",
        })
    }
}

impl std::error::Error for InputError {}

/// A non-2xx answer of the registry.
#[non_exhaustive]
pub struct ApiError {
    /// The operation (`GET Repository.manifest`).
    pub operation: Option<Operation>,
    /// The status.
    pub status: StatusCode,
    /// What the body held (a `HEAD` answer has none).
    pub body: ErrorBody,
    /// The Bearer challenge of a `401`, if one was sent.
    pub challenge: Option<Challenge>,
    /// The response metadata.
    pub meta: ResponseMeta,
}

impl ApiError {
    /// The registry's error entries (empty unless the body was an error document).
    pub fn errors(&self) -> &[ErrorEntry] {
        match &self.body {
            ErrorBody::Document(document) => &document.errors,
            _ => &[],
        }
    }

    /// Whether an entry carries `code`.
    pub fn has_code(&self, code: &ErrorCode) -> bool {
        self.errors().iter().any(|entry| entry.code == *code)
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(operation) = &self.operation {
            write!(f, "{operation}: ")?;
        }
        write!(f, "registry answered HTTP {}", self.status)?;
        match &self.body {
            ErrorBody::Document(document) => {
                let mut codes = document.errors.iter().map(|entry| entry.code.label());
                if let Some(first) = codes.next() {
                    write!(f, " ({first}")?;
                    for code in codes {
                        write!(f, ", {code}")?;
                    }
                    f.write_str(")")?;
                }
                Ok(())
            }
            ErrorBody::Empty => Ok(()),
            ErrorBody::Unrecognized(_) => f.write_str(" with a body that is not an error document"),
            ErrorBody::TooLarge { .. } => f.write_str(" with an error body over the SDK's limit"),
            ErrorBody::Unreadable(_) => f.write_str("; its body could not be read"),
        }
    }
}

/// `Debug`: the operation, the status, the error entries, the challenge and the metadata (header
/// names only); an unrecognized body by its length.
impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiError")
            .field("operation", &self.operation)
            .field("status", &self.status.as_u16())
            .field("body", &self.body)
            .field("challenge", &self.challenge)
            .field("meta", &self.meta)
            .finish()
    }
}

/// The body of a non-2xx answer, read up to [`ERROR_BODY_LIMIT`] bytes.
#[non_exhaustive]
pub enum ErrorBody {
    /// The documented error document, `{"errors": [...]}`.
    Document(ErrorDocument),
    /// No body (a `HEAD` answer, or an empty one).
    Empty,
    /// Something else (an HTML page from a proxy, other JSON), kept as bytes.
    Unrecognized(Bytes),
    /// Longer than the limit; the rest was not read.
    TooLarge {
        /// The limit, in bytes.
        limit: usize,
    },
    /// Reading the body failed (Concord's `Transport` error).
    Unreadable(concord::Error),
}

impl fmt::Debug for ErrorBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(document) => f.debug_tuple("Document").field(document).finish(),
            Self::Empty => f.write_str("Empty"),
            Self::Unrecognized(bytes) => write!(f, "Unrecognized({} bytes)", bytes.len()),
            Self::TooLarge { limit } => write!(f, "TooLarge {{ limit: {limit} }}"),
            Self::Unreadable(error) => f.debug_tuple("Unreadable").field(&error.kind()).finish(),
        }
    }
}

/// The registry's error document: `{"errors": [{"code", "message", "detail"}, ...]}`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct ErrorDocument {
    /// The entries, in order.
    pub errors: Vec<ErrorEntry>,
}

/// One entry of an error document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct ErrorEntry {
    /// The error code.
    pub code: ErrorCode,
    /// The registry's message, if any.
    #[serde(default)]
    pub message: Option<String>,
    /// The unstructured detail, if any (for `BLOB_UNKNOWN` on a manifest push, `{"digest": ..}`).
    #[serde(default)]
    pub detail: Option<serde_json::Value>,
}

impl ErrorEntry {
    /// `detail.digest`, as `BLOB_UNKNOWN` names a missing blob on a manifest push.
    pub fn detail_digest(&self) -> Option<Digest> {
        let text = self.detail.as_ref()?.get("digest")?.as_str()?;
        Digest::parse(text).ok()
    }
}

/// The documented error codes. A code the SDK does not know is [`Unknown`](Self::Unknown), as
/// the spec tells clients to treat it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "String")]
#[non_exhaustive]
pub enum ErrorCode {
    /// `BLOB_UNKNOWN`: the blob is unknown to the registry in this repository.
    BlobUnknown,
    /// `BLOB_UPLOAD_INVALID`: the upload failed and cannot continue.
    BlobUploadInvalid,
    /// `BLOB_UPLOAD_UNKNOWN`: the upload was cancelled or never started.
    BlobUploadUnknown,
    /// `DIGEST_INVALID`: the content does not match the digest.
    DigestInvalid,
    /// `MANIFEST_BLOB_UNKNOWN`: a blob a manifest references is unknown.
    ManifestBlobUnknown,
    /// `MANIFEST_INVALID`: the manifest failed validation.
    ManifestInvalid,
    /// `MANIFEST_UNKNOWN`: the manifest is unknown to the repository.
    ManifestUnknown,
    /// `MANIFEST_UNVERIFIED`: the manifest failed signature verification.
    ManifestUnverified,
    /// `NAME_INVALID`: the repository name is invalid.
    NameInvalid,
    /// `NAME_UNKNOWN`: the repository is unknown to the registry.
    NameUnknown,
    /// `PAGINATION_NUMBER_INVALID`: `n` is not an integer, negative or too large.
    PaginationNumberInvalid,
    /// `RANGE_INVALID`: an upload chunk is out of order or malformed.
    RangeInvalid,
    /// `SIZE_INVALID`: a length does not match the content.
    SizeInvalid,
    /// `TAG_INVALID`: the manifest's tag does not match the URL's.
    TagInvalid,
    /// `UNAUTHORIZED`: authentication required.
    Unauthorized,
    /// `DENIED`: access denied.
    Denied,
    /// `UNSUPPORTED`: the operation is unsupported.
    Unsupported,
    /// `TOOMANYREQUESTS`: too many requests.
    TooManyRequests,
    /// Any other code, as sent.
    Unknown(String),
}

impl ErrorCode {
    /// The code as the registry spells it; `UNKNOWN` for an unknown one (whose own spelling is
    /// a server string, kept in the variant).
    pub fn label(&self) -> &'static str {
        match self {
            Self::BlobUnknown => "BLOB_UNKNOWN",
            Self::BlobUploadInvalid => "BLOB_UPLOAD_INVALID",
            Self::BlobUploadUnknown => "BLOB_UPLOAD_UNKNOWN",
            Self::DigestInvalid => "DIGEST_INVALID",
            Self::ManifestBlobUnknown => "MANIFEST_BLOB_UNKNOWN",
            Self::ManifestInvalid => "MANIFEST_INVALID",
            Self::ManifestUnknown => "MANIFEST_UNKNOWN",
            Self::ManifestUnverified => "MANIFEST_UNVERIFIED",
            Self::NameInvalid => "NAME_INVALID",
            Self::NameUnknown => "NAME_UNKNOWN",
            Self::PaginationNumberInvalid => "PAGINATION_NUMBER_INVALID",
            Self::RangeInvalid => "RANGE_INVALID",
            Self::SizeInvalid => "SIZE_INVALID",
            Self::TagInvalid => "TAG_INVALID",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Denied => "DENIED",
            Self::Unsupported => "UNSUPPORTED",
            Self::TooManyRequests => "TOOMANYREQUESTS",
            Self::Unknown(_) => "UNKNOWN",
        }
    }
}

impl From<String> for ErrorCode {
    fn from(code: String) -> Self {
        match code.as_str() {
            "BLOB_UNKNOWN" => Self::BlobUnknown,
            "BLOB_UPLOAD_INVALID" => Self::BlobUploadInvalid,
            "BLOB_UPLOAD_UNKNOWN" => Self::BlobUploadUnknown,
            "DIGEST_INVALID" => Self::DigestInvalid,
            "MANIFEST_BLOB_UNKNOWN" => Self::ManifestBlobUnknown,
            "MANIFEST_INVALID" => Self::ManifestInvalid,
            "MANIFEST_UNKNOWN" => Self::ManifestUnknown,
            "MANIFEST_UNVERIFIED" => Self::ManifestUnverified,
            "NAME_INVALID" => Self::NameInvalid,
            "NAME_UNKNOWN" => Self::NameUnknown,
            "PAGINATION_NUMBER_INVALID" => Self::PaginationNumberInvalid,
            "RANGE_INVALID" => Self::RangeInvalid,
            "SIZE_INVALID" => Self::SizeInvalid,
            "TAG_INVALID" => Self::TagInvalid,
            "UNAUTHORIZED" => Self::Unauthorized,
            "DENIED" => Self::Denied,
            "UNSUPPORTED" => Self::Unsupported,
            "TOOMANYREQUESTS" => Self::TooManyRequests,
            _ => Self::Unknown(code),
        }
    }
}

/// A failure of the Bearer token flow.
#[non_exhaustive]
pub struct AuthError {
    /// What failed.
    pub problem: AuthProblem,
    /// The token service's status, for [`AuthProblem::TokenRefused`].
    pub status: Option<StatusCode>,
    /// The metadata of the registry's `401`, or of the token service's answer.
    pub meta: Option<ResponseMeta>,
    pub(crate) source: Option<concord::Error>,
}

impl AuthError {
    pub(crate) fn new(problem: AuthProblem) -> Self {
        Self {
            problem,
            status: None,
            meta: None,
            source: None,
        }
    }

    pub(crate) fn with_meta(mut self, meta: ResponseMeta) -> Self {
        self.meta = Some(meta);
        self
    }

    pub(crate) fn with_source(mut self, source: concord::Error) -> Self {
        if self.meta.is_none() {
            self.meta = source.head_meta().map(ResponseMeta::from_concord);
        }
        self.source = Some(source);
        self
    }

    /// Concord's error behind a [`TokenTransport`](AuthProblem::TokenTransport) or
    /// [`TokenResponse`](AuthProblem::TokenResponse) problem.
    pub fn source_error(&self) -> Option<&concord::Error> {
        self.source.as_ref()
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "registry token flow: {}", self.problem)?;
        if let Some(status) = self.status {
            write!(f, " (HTTP {status})")?;
        }
        Ok(())
    }
}

impl fmt::Debug for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthError")
            .field("problem", &self.problem)
            .field("status", &self.status.map(|s| s.as_u16()))
            .field("meta", &self.meta)
            .field("source", &self.source.as_ref().map(concord::Error::kind))
            .finish()
    }
}

/// What failed in the Bearer token flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthProblem {
    /// The `WWW-Authenticate` header of a `401` could not be read as a Bearer challenge
    /// (a missing `realm`, an unterminated quoted string, a repeated parameter ...).
    MalformedChallenge,
    /// The challenge's `realm` is on an origin the client does not trust: no request was sent
    /// to it, so no credential crossed to it.
    UntrustedRealm,
    /// The `realm` is not an `http`/`https` URL usable as the token endpoint (user information,
    /// a query or a fragment).
    InvalidRealm,
    /// The token request could not be sent or its answer not read.
    TokenTransport,
    /// The token service answered with a non-2xx status.
    TokenRefused,
    /// The token service's answer is not a token document with `token` or `access_token`, the
    /// token cannot be sent in a header, or its `expires_in` is a lifetime the client cannot
    /// represent.
    TokenResponse,
}

impl fmt::Display for AuthProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MalformedChallenge => "the Bearer challenge could not be read",
            Self::UntrustedRealm => "the challenge's realm is not a trusted origin",
            Self::InvalidRealm => "the challenge's realm is not a usable token endpoint",
            Self::TokenTransport => "the token request failed",
            Self::TokenRefused => "the token service refused the request",
            Self::TokenResponse => "the token service's answer holds no usable token",
        })
    }
}

/// A 2xx answer that breaks the protocol, or a server-provided URL refused before use.
#[derive(Debug)]
#[non_exhaustive]
pub struct ProtocolError {
    /// What was wrong.
    pub problem: ProtocolProblem,
    /// The response that carried it.
    pub meta: Option<ResponseMeta>,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(operation) = self.meta.as_ref().and_then(ResponseMeta::operation) {
            write!(f, "{operation}: ")?;
        }
        write!(f, "registry protocol: {}", self.problem)
    }
}

/// How a response broke the protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProtocolProblem {
    /// A 2xx status other than the documented one.
    UnexpectedStatus(StatusCode),
    /// No `Location` where the protocol requires one.
    MissingLocation,
    /// A `Location` that is repeated, not text or not a URL reference.
    MalformedLocation,
    /// A `Location` on another origin than the registry, or with user information: it is not
    /// requested, so the token never goes there.
    ForeignLocation,
    /// No `Range` where the protocol requires one.
    MissingRange,
    /// A `Range` that is not `[bytes=]0-<end>`.
    MalformedRange,
    /// The acknowledged range does not match what was sent (`end` is the inclusive last offset
    /// the SDK expected, `reported` what the registry said; `None` when it reported none).
    UnexpectedRange {
        /// The expected inclusive end offset (`None` when nothing was expected).
        expected: Option<u64>,
        /// The registry's acknowledged byte count.
        reported: u64,
    },
    /// The `Link` header could not be read.
    MalformedLink,
    /// `rel="next"` is on another origin: it is not followed.
    ForeignLink,
    /// `rel="next"` names another collection than the listing.
    OtherCollection,
    /// `rel="next"` repeats the page just requested.
    NotAdvancing,
    /// A continuation from another listing was passed to resume this one.
    ContinuationMismatch,
    /// The manifest's `Content-Type` is not one of the media types the call accepted.
    UnexpectedMediaType,
    /// The manifest's `mediaType` field contradicts its `Content-Type`.
    MediaTypeMismatch,
    /// The manifest body is not an image manifest (schema version 2, `config`, `layers`).
    MalformedManifest,
    /// A `Docker-Content-Digest` header that is not a digest.
    MalformedDigestHeader,
    /// A tag list for another repository than the one requested.
    NameMismatch,
    /// A `Content-Length` header that is repeated or not a number.
    MalformedLength,
}

impl fmt::Display for ProtocolProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedStatus(status) => write!(f, "unexpected success status {status}"),
            Self::MissingLocation => f.write_str("the Location header is missing"),
            Self::MalformedLocation => f.write_str("the Location header is not a URL reference"),
            Self::ForeignLocation => f.write_str("the Location leaves the registry origin"),
            Self::MissingRange => f.write_str("the Range header is missing"),
            Self::MalformedRange => f.write_str("the Range header is not `0-<end>`"),
            Self::UnexpectedRange { .. } => {
                f.write_str("the acknowledged range does not match the bytes sent")
            }
            Self::MalformedLink => f.write_str("the Link header could not be read"),
            Self::ForeignLink => f.write_str("the next link leaves the registry origin"),
            Self::OtherCollection => f.write_str("the next link names another collection"),
            Self::NotAdvancing => f.write_str("the next link repeats the page just requested"),
            Self::ContinuationMismatch => {
                f.write_str("the continuation belongs to another listing")
            }
            Self::UnexpectedMediaType => {
                f.write_str("the manifest media type is not one the call accepted")
            }
            Self::MediaTypeMismatch => {
                f.write_str("the manifest mediaType field contradicts its Content-Type")
            }
            Self::MalformedManifest => f.write_str("the body is not an image manifest"),
            Self::MalformedDigestHeader => {
                f.write_str("the Docker-Content-Digest header is not a digest")
            }
            Self::NameMismatch => f.write_str("the tag list names another repository"),
            Self::MalformedLength => f.write_str("the Content-Length header is not a length"),
        }
    }
}

/// Content that does not match its digest or size.
#[derive(Debug)]
#[non_exhaustive]
pub struct IntegrityError {
    /// What did not match.
    pub problem: IntegrityProblem,
    /// The response that carried the content.
    pub meta: Option<ResponseMeta>,
}

impl fmt::Display for IntegrityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(operation) = self.meta.as_ref().and_then(ResponseMeta::operation) {
            write!(f, "{operation}: ")?;
        }
        write!(f, "content integrity: {}", self.problem)
    }
}

/// What did not match. Digests are content identifiers, shown in `Debug`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum IntegrityProblem {
    /// The content's digest is not the expected one (the one it was fetched by, or the one the
    /// caller gave).
    DigestMismatch {
        /// The digest the content should have.
        expected: Digest,
        /// The digest it has.
        actual: Digest,
    },
    /// The registry's `Docker-Content-Digest` header names other content (same algorithm).
    HeaderDigestMismatch {
        /// The digest the SDK expected or computed.
        expected: Digest,
        /// The header's digest.
        header: Digest,
    },
    /// The content's length is not the expected one (from a descriptor or `Content-Length`).
    SizeMismatch {
        /// The expected length.
        expected: u64,
        /// The length read so far (reading stops at the first byte too many).
        actual: u64,
    },
    /// The digest's algorithm cannot be computed by this SDK, so the content cannot be
    /// verified; it is not fetched.
    UnsupportedAlgorithm,
}

impl fmt::Display for IntegrityProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DigestMismatch { .. } => "the content does not match its digest",
            Self::HeaderDigestMismatch { .. } => {
                "the Docker-Content-Digest header names other content"
            }
            Self::SizeMismatch { .. } => "the content length is not the expected one",
            Self::UnsupportedAlgorithm => "the digest algorithm cannot be verified",
        })
    }
}

/// A non-2xx response as [`RegistryError::Api`], its body read up to the limit.
pub(crate) async fn api_error<T>(received: Received<T>) -> RegistryError {
    let operation = received.operation();
    let status = received.status();
    let challenge = if status == StatusCode::UNAUTHORIZED {
        Challenge::bearer(received.head().headers()).ok().flatten()
    } else {
        None
    };
    // Registries and proxies are decoded regardless of their media label; malformed bytes
    // remain available as Unrecognized, including the distinct empty-body case.
    let (meta, document) = received
        .document::<IgnoreContentType<Json>, ErrorDocument>(ERROR_BODY_LIMIT)
        .await;
    let meta = ResponseMeta::from_concord(meta);
    let body = match document {
        Ok(document) => ErrorBody::Document(document),
        Err(error) if error.kind() == ErrorKind::Decode => {
            let bytes = error.body_unredacted().cloned().unwrap_or_default();
            if bytes.is_empty() {
                ErrorBody::Empty
            } else {
                ErrorBody::Unrecognized(bytes)
            }
        }
        Err(error) if error.kind() == ErrorKind::Limit => ErrorBody::TooLarge {
            limit: ERROR_BODY_LIMIT,
        },
        Err(error) => ErrorBody::Unreadable(error),
    };
    RegistryError::Api(Box::new(ApiError {
        operation,
        status,
        body,
        challenge,
        meta,
    }))
}

/// Requires `expected`: a non-2xx answer is [`RegistryError::Api`], another 2xx is
/// [`ProtocolProblem::UnexpectedStatus`] (its body drained).
pub(crate) async fn expect_status<T>(
    received: Received<T>,
    expected: StatusCode,
) -> Result<(Received<T>, ResponseMeta), RegistryError> {
    let status = received.status();
    if status == expected {
        let meta = ResponseMeta::capture(&received);
        return Ok((received, meta));
    }
    if !status.is_success() {
        return Err(api_error(received).await);
    }
    let meta = ResponseMeta::capture(&received);
    let _ = received.bytes_limited(ERROR_BODY_LIMIT).await;
    Err(RegistryError::protocol(
        ProtocolProblem::UnexpectedStatus(status),
        Some(meta),
    ))
}
