//! Typed failures and response metadata; diagnostic formatting omits provider payloads.

use concord::bytes::Bytes;
use serde::Deserialize;
use std::{fmt, ops::Deref, time::Duration};

/// Response metadata captured before consuming the body, on success and failure: Concord's
/// [`concord::ResponseMeta`] (actual status, never synthesized; HTTP version; all headers,
/// including request ID and raw Retry-After; the declared operation identity), reached through
/// `Deref`, plus Dropbox's header readings. `Debug` prints header names only.
#[derive(Clone)]
pub struct ResponseMeta(pub concord::ResponseMeta);

impl Deref for ResponseMeta {
    type Target = concord::ResponseMeta;

    fn deref(&self) -> &concord::ResponseMeta {
        &self.0
    }
}

impl ResponseMeta {
    /// Dropbox's request ID when present and valid header text.
    pub fn request_id(&self) -> Option<&str> {
        self.headers().get("x-dropbox-request-id")?.to_str().ok()
    }

    /// Parse a single Retry-After header as unsigned seconds. No retry is performed.
    pub fn retry_after(&self) -> RetryAfter {
        let mut values = self.headers().get_all("retry-after").iter();
        let Some(value) = values.next() else {
            return RetryAfter::Missing;
        };
        if values.next().is_some() {
            return RetryAfter::Invalid;
        }
        let Ok(value) = value.to_str() else {
            return RetryAfter::Invalid;
        };
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return RetryAfter::Invalid;
        }
        match value.parse::<u64>() {
            Ok(n) => RetryAfter::Seconds(Duration::from_secs(n)),
            Err(_) => RetryAfter::Invalid,
        }
    }
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

/// Interpretation of the optional Retry-After response header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryAfter {
    /// One valid unsigned integer, including zero.
    Seconds(Duration),
    /// Header absent.
    Missing,
    /// Malformed, overflowing, or repeated header.
    Invalid,
}

/// Lookup failures within a `path` error.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = ".tag", rename_all = "snake_case")]
pub enum LookupError {
    /// Invalid path syntax; optional provider explanation is in the raw document.
    MalformedPath,
    /// No entry exists at that path.
    NotFound,
    /// Expected a file.
    NotFile,
    /// Expected a folder.
    NotFolder,
    /// Access to the content is restricted.
    RestrictedContent,
    /// Operation unsupported for this content type.
    UnsupportedContentType,
    /// Path is locked.
    Locked,
    /// Unmodeled tag; the complete provider detail remains available.
    #[serde(other)]
    Other,
}

/// Root information supplied by an HTTP 422 `invalid_root` response.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RootInfo {
    /// `user`, `team`, or a future root type.
    #[serde(rename = ".tag")]
    pub kind: String,
    /// Updated root. The SDK never silently switches roots or retries.
    pub root_namespace_id: String,
    /// Home namespace, if supplied.
    pub home_namespace_id: Option<String>,
    /// Member's home path within the team root, if supplied.
    pub home_path: Option<String>,
}

/// Known tagged errors relevant to folder listing and context selection.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = ".tag", rename_all = "snake_case")]
pub enum TaggedError {
    /// Folder lookup failed.
    Path {
        /// Nested lookup reason.
        path: LookupError,
    },
    /// Continuation expired; restart the listing explicitly.
    Reset,
    /// Selected root no longer matches the member's actual root.
    InvalidRoot {
        /// Updated provider root information.
        invalid_root: RootInfo,
    },
    /// Selected namespace or resource is inaccessible.
    NoPermission,
    /// Token invalid.
    InvalidAccessToken,
    /// Token expired.
    ExpiredAccessToken,
    /// Selected team member invalid or inaccessible.
    InvalidSelectUser,
    /// Member suspended.
    UserSuspended,
    /// Token missing a required permission.
    MissingScope {
        /// Required OAuth scope.
        required_scope: String,
    },
    /// Route unavailable to this app.
    RouteAccessDenied,
    /// Team lacks access.
    TeamAccessDenied,
    /// An unmodeled tag, preserved in the raw provider detail.
    #[serde(other)]
    Other,
}

/// Rate limit reason returned in the body, distinct from the HTTP header.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = ".tag", rename_all = "snake_case")]
pub enum RateLimitReason {
    /// Too many calls.
    TooManyRequests,
    /// Too many writes against a namespace.
    TooManyWriteOperations,
    /// Unmodeled reason; raw detail is retained.
    #[serde(other)]
    Other,
}

/// Structured rate-limit document (`error` is a struct here, not a tagged union).
#[derive(Clone, Debug, Deserialize)]
pub struct RateLimitError {
    /// Reason supplied by Dropbox.
    pub reason: RateLimitReason,
    /// Body delay in seconds, default 1 per the endpoint schema. Does not override the header.
    #[serde(default = "default_retry")]
    pub retry_after: u64,
}
fn default_retry() -> u64 {
    1
}

/// Parsed interpretation of the provider's `error` field.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum ProviderErrorKind {
    /// Known tagged shape (possibly an unmodeled tag).
    Tagged(TaggedError),
    /// Rate-limit struct.
    RateLimited(RateLimitError),
    /// Future or malformed shape; still an HTTP failure, with its exact detail retained.
    Other(serde_json::Value),
}

/// Optional localized user-facing error text supplied by Dropbox.
#[derive(Clone, Debug, Deserialize)]
pub struct UserMessage {
    /// Message text, when supplied.
    pub text: Option<String>,
    /// Locale, when supplied.
    pub locale: Option<String>,
}

/// Provider error envelope. Use typed tags, not exact equality on the summary.
#[derive(Clone, Debug)]
pub struct ProviderError {
    /// Human-readable summary whose suffix is not stable.
    pub summary: String,
    /// Typed interpretation of `error`.
    pub kind: ProviderErrorKind,
    /// Original `error` JSON, including future tags and unmodeled fields.
    pub detail: serde_json::Value,
    /// Optional localized message.
    pub user_message: Option<UserMessage>,
}

#[derive(Deserialize)]
struct Envelope {
    error_summary: String,
    error: serde_json::Value,
    user_message: Option<UserMessage>,
}

/// A bounded HTTP error body. Access raw bytes deliberately; Debug prints no body data.
pub struct ErrorBody {
    read: Result<Bytes, concord::Error>,
    provider: Option<ProviderError>,
}

impl ErrorBody {
    pub(crate) fn new(read: Result<Bytes, concord::Error>) -> Self {
        let provider = read.as_ref().ok().and_then(|bytes| {
            let envelope: Envelope = serde_json::from_slice(bytes).ok()?;
            let kind = serde_json::from_value(envelope.error.clone()).ok()?;
            Some(ProviderError {
                summary: envelope.error_summary,
                kind,
                detail: envelope.error,
                user_message: envelope.user_message,
            })
        });
        Self { read, provider }
    }

    /// Complete raw bytes when the body was read successfully within the byte limit.
    pub fn bytes(&self) -> Option<&[u8]> {
        self.read.as_deref().ok()
    }

    /// Body read/size failure, carrying Concord's operation identity.
    pub fn read_error(&self) -> Option<&concord::Error> {
        self.read.as_ref().err()
    }

    /// Typed error envelope when the body contained one.
    pub fn provider(&self) -> Option<&ProviderError> {
        self.provider.as_ref()
    }
}

impl fmt::Debug for ErrorBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorBody")
            .field("byte_count", &self.bytes().map(<[u8]>::len))
            .field("read_error", &self.read_error())
            .finish_non_exhaustive()
    }
}

/// An HTTP failure even if its body is plaintext, malformed, oversized, or interrupted.
#[derive(Debug)]
pub struct HttpError {
    /// Status, headers and operation identity.
    pub meta: ResponseMeta,
    /// Bounded error document and/or body read failure.
    pub body: ErrorBody,
}

/// SDK failure. Metadata survives every failure after a response head was received.
#[derive(Debug)]
pub enum Error {
    /// Request construction, transport, content-type, JSON, or success-body size failure.
    Concord {
        /// Original Concord failure with declared operation identity.
        error: concord::Error,
        /// Present if response headers arrived.
        meta: Option<Box<ResponseMeta>>,
    },
    /// Non-2xx status, including context (422) and rate-limit (429) failures.
    Http(Box<HttpError>),
    /// A page omitted the required nonempty cursor. Never interpreted as exhaustion.
    EmptyCursor {
        /// Metadata of the invalid page.
        meta: Box<ResponseMeta>,
    },
}

impl Error {
    /// Response metadata, if a head was received.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Concord { meta, .. } => meta.as_deref(),
            Self::Http(e) => Some(&e.meta),
            Self::EmptyCursor { meta } => Some(meta),
        }
    }

    /// Provider envelope, when the HTTP failure body is a valid envelope.
    pub fn provider(&self) -> Option<&ProviderError> {
        match self {
            Self::Http(e) => e.body.provider(),
            _ => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord { error, meta: None }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord { error, .. } => error.fmt(f),
            Self::Http(e) => write!(f, "Dropbox HTTP failure ({})", e.meta.status()),
            Self::EmptyCursor { .. } => f.write_str("Dropbox returned an empty cursor"),
        }
    }
}

impl std::error::Error for Error {}
