//! The SDK's error type and the provider's two error documents.
//!
//! `Display` is static text plus a status code: provider messages, token values and URLs are
//! never formatted. Reach the provider's document through the accessors.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use concord::ResponseMeta;
use concord_retry::http::RetryAfterError;
use serde::Deserialize;

/// The Web API's regular error object (`{"error": {"status", "message", "reason"?}}`).
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorObject {
    /// The HTTP status code the provider repeats in the body.
    pub status: u16,
    /// A short description of the cause.
    pub message: String,
    /// An optional enumeration, such as `QUOTA_EXCEEDED`.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ErrorEnvelope {
    pub(crate) error: ErrorObject,
}

/// The accounts service's error object (RFC 6749 section 5.2).
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthErrorObject {
    /// The error code (`invalid_grant`, `invalid_client`, ...).
    pub error: String,
    /// A human-readable description, when given.
    #[serde(default)]
    pub error_description: Option<String>,
}

/// What a non-2xx Web API status means for the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApiClass {
    /// 400: the request was malformed or a value was invalid.
    BadRequest,
    /// 401: the access token was refused (after one refresh and replay for reads; once for writes).
    Unauthorized,
    /// 403: refused. For an app in development mode this is what a user who is not on the app's
    /// allowlist receives; it can also be any other refusal. Never retried or refreshed.
    Forbidden,
    /// 404: no such resource.
    NotFound,
    /// 429 without `reason: QUOTA_EXCEEDED`: rate limiting; see [`Error::retry_after`].
    RateLimited,
    /// 429 with `reason: QUOTA_EXCEEDED`: a development-mode quota bucket is exhausted. Waiting a
    /// few seconds does not help, so it is never retried automatically.
    QuotaExceeded,
    /// 500, 502, 503 and other 5xx.
    Server,
    /// Any other non-2xx status (a 3xx the client did not follow, an undocumented 4xx).
    Other,
}

/// A non-2xx answer of the Web API, after the SDK's single refresh-and-replay and any configured
/// 429 retries.
pub struct ApiFailure {
    pub(crate) class: ApiClass,
    pub(crate) meta: ResponseMeta,
    pub(crate) document: Result<ErrorObject, concord::Error>,
    pub(crate) retry_after: Result<Option<Duration>, RetryAfterError>,
}

impl ApiFailure {
    /// The classification of the status (and, for 429, of the document's `reason`).
    pub fn class(&self) -> ApiClass {
        self.class
    }

    /// The HTTP status.
    pub fn status(&self) -> u16 {
        self.meta.status().as_u16()
    }

    /// The response head (`Debug` shows header names only).
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The provider's error object, or why it could not be read (absent, another media type,
    /// malformed, longer than 64 KiB). The status is authoritative either way.
    pub fn document(&self) -> Result<&ErrorObject, &concord::Error> {
        self.document.as_ref()
    }

    /// The `Retry-After` header: absent (`Ok(None)`), a delay, or why it was refused (repeated,
    /// invalid, overflowing). Read on every status, meaningful on 429.
    pub fn retry_after(&self) -> Result<Option<Duration>, RetryAfterError> {
        self.retry_after
    }
}

impl fmt::Debug for ApiFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiFailure")
            .field("class", &self.class)
            .field("status", &self.status())
            .field(
                "reason",
                &self
                    .document
                    .as_ref()
                    .ok()
                    .and_then(|d| d.reason.as_deref()),
            )
            .field(
                "document",
                &self.document.as_ref().map(|_| "..").map_err(|e| e.kind()),
            )
            .field("retry_after", &self.retry_after)
            .finish()
    }
}

/// A refusal of the accounts service's token endpoint.
pub struct TokenFailure {
    pub(crate) meta: ResponseMeta,
    pub(crate) document: Result<OAuthErrorObject, concord::Error>,
}

impl TokenFailure {
    /// The HTTP status.
    pub fn status(&self) -> u16 {
        self.meta.status().as_u16()
    }

    /// The response head.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }

    /// The RFC 6749 error object, or why it could not be read.
    pub fn document(&self) -> Result<&OAuthErrorObject, &concord::Error> {
        self.document.as_ref()
    }

    /// The RFC 6749 `error` code, when the document was read.
    pub fn code(&self) -> Option<&str> {
        self.document.as_ref().ok().map(|d| d.error.as_str())
    }
}

impl fmt::Debug for TokenFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenFailure")
            .field("status", &self.status())
            .field("code", &self.code())
            .finish_non_exhaustive()
    }
}

/// Why an access token could not be obtained.
#[non_exhaustive]
#[derive(concord::Debug)]
pub enum AuthError {
    /// The token endpoint answered `invalid_grant`: the refresh token (or authorization code) is
    /// expired, revoked or otherwise invalid. The session stops using that refresh token; send the
    /// user through the authorization flow again and call [`crate::Session::replace_grant`].
    InvalidGrant(Box<TokenFailure>),
    /// Any other refusal of the token endpoint (`invalid_client`, a 5xx, an unreadable document).
    Refused(Box<TokenFailure>),
    /// A previous refresh was answered `invalid_grant`; nothing was sent this time.
    ReauthorizationRequired,
    /// The access token expired and the session holds no refresh token.
    NoRefreshToken,
    /// The token endpoint answered 2xx with a document that breaks the contract (not `Bearer`,
    /// a lifetime not longer than the refresh margin, an access token that is not a header-safe
    /// token).
    Protocol(&'static str),
    /// Concord could not complete the token request (transport, wrong media type, malformed or
    /// oversized document).
    Concord(concord::Error),
    /// The token cache refused the acquisition (lifetime overflow or capacity).
    Cache(&'static str),
}

impl AuthError {
    /// The token endpoint's failure, when it answered with a non-2xx status.
    pub fn failure(&self) -> Option<&TokenFailure> {
        match self {
            Self::InvalidGrant(f) | Self::Refused(f) => Some(f),
            _ => None,
        }
    }

    /// `true` when only a new authorization by the user can restore access.
    pub fn requires_reauthorization(&self) -> bool {
        matches!(
            self,
            Self::InvalidGrant(_) | Self::ReauthorizationRequired | Self::NoRefreshToken
        )
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGrant(t) => write!(
                f,
                "the token endpoint answered invalid_grant ({}): reauthorization required",
                t.status()
            ),
            Self::Refused(t) => {
                write!(f, "the token endpoint refused the request ({})", t.status())
            }
            Self::ReauthorizationRequired => {
                f.write_str("the refresh token was rejected earlier: reauthorization required")
            }
            Self::NoRefreshToken => {
                f.write_str("the access token expired and no refresh token is held")
            }
            Self::Protocol(m) => write!(f, "token response breaks the contract: {m}"),
            Self::Concord(e) => write!(f, "token request failed: {e}"),
            Self::Cache(m) => write!(f, "token cache: {m}"),
        }
    }
}

impl std::error::Error for AuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Concord(e) => Some(e),
            _ => None,
        }
    }
}

/// The SDK's error.
#[non_exhaustive]
#[derive(concord::Debug)]
pub enum Error {
    /// A value the SDK refused before building any request.
    InvalidInput(&'static str),
    /// The Web API answered with a non-2xx status.
    Api(Box<ApiFailure>),
    /// No access token could be obtained. Shared by every request that waited on the same
    /// refresh, hence the `Arc`.
    Auth(Arc<AuthError>),
    /// A 2xx response breaks the provider contract the SDK relies on (a page that does not
    /// start at the requested offset, a next page without progress).
    Protocol(&'static str),
    /// Concord: a request that could not be built, a transport failure, a 2xx response with
    /// another media type, a malformed, truncated or oversized body.
    Concord(concord::Error),
    /// The session's total deadline expired before any attempt completed.
    Deadline,
    /// A write was dispatched but its result could not be established (transport failure,
    /// deadline, or an unreadable success document). The items may already have been added.
    /// Never resend automatically: re-read the playlist and reconcile it first. The boxed
    /// error preserves the cause; a timeout during token acquisition alone is not uncertain.
    UncertainWrite(Box<Error>),
}

impl Error {
    /// The Web API failure, when the provider answered non-2xx.
    pub fn api(&self) -> Option<&ApiFailure> {
        match self {
            Self::Api(f) => Some(f),
            _ => None,
        }
    }

    /// The class of a Web API failure.
    pub fn api_class(&self) -> Option<ApiClass> {
        self.api().map(ApiFailure::class)
    }

    /// The server's minimum delay of a 429 (`Retry-After`), when present and valid.
    pub fn retry_after(&self) -> Option<Duration> {
        self.api()
            .filter(|f| matches!(f.class, ApiClass::RateLimited | ApiClass::QuotaExceeded))
            .and_then(|f| f.retry_after.ok().flatten())
    }

    /// `true` for a 403: for an app in development mode, the user may not be on the app's
    /// allowlist (or the app owner lacks the required subscription). The SDK cannot tell this
    /// apart from any other refusal; it never refreshes or retries a 403.
    pub fn may_be_development_mode(&self) -> bool {
        self.api_class() == Some(ApiClass::Forbidden)
    }

    /// The authentication failure, when no access token could be obtained.
    pub fn auth(&self) -> Option<&AuthError> {
        match self {
            Self::Auth(e) => Some(e),
            _ => None,
        }
    }

    /// The underlying Concord error (also inside an authentication failure).
    pub fn concord(&self) -> Option<&concord::Error> {
        match self {
            Self::Concord(e) => Some(e),
            Self::UncertainWrite(cause) => cause.concord(),
            Self::Auth(a) => match &**a {
                AuthError::Concord(e) => Some(e),
                _ => None,
            },
            _ => None,
        }
    }

    /// Marks an incomplete dispatched write without obscuring an explicit API refusal.
    pub(crate) fn after_write(self) -> Self {
        if matches!(&self, Self::Deadline)
            || matches!(&self, Self::Concord(cause) if cause.delivery() != concord::Delivery::NotSent)
        {
            Self::UncertainWrite(Box::new(self))
        } else {
            self
        }
    }
}

impl From<concord::Error> for Error {
    fn from(e: concord::Error) -> Self {
        Self::Concord(e)
    }
}

impl From<AuthError> for Error {
    fn from(e: AuthError) -> Self {
        Self::Auth(Arc::new(e))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(m) => write!(f, "invalid input: {m}"),
            Self::Api(a) => {
                write!(f, "the Web API answered {}", a.status())?;
                match a.class {
                    ApiClass::Forbidden => f.write_str(
                        " (forbidden; for a development-mode app the user may not be allowlisted)",
                    ),
                    ApiClass::QuotaExceeded => f.write_str(" (quota exceeded)"),
                    ApiClass::RateLimited => f.write_str(" (rate limited)"),
                    _ => Ok(()),
                }
            }
            Self::Auth(a) => write!(f, "authentication failed: {a}"),
            Self::Protocol(m) => write!(f, "response breaks the contract: {m}"),
            Self::Concord(e) => fmt::Display::fmt(e, f),
            Self::Deadline => f.write_str("the request deadline expired"),
            Self::UncertainWrite(_) => f.write_str(
                "the playlist addition may have succeeded; re-read the playlist before another addition",
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Concord(e) => Some(e),
            Self::Auth(a) => Some(&**a),
            Self::UncertainWrite(cause) => Some(&**cause),
            _ => None,
        }
    }
}

/// Classifies a status (and, for 429, the document's reason).
pub(crate) fn classify(status: u16, document: Option<&ErrorObject>) -> ApiClass {
    match status {
        400 => ApiClass::BadRequest,
        401 => ApiClass::Unauthorized,
        403 => ApiClass::Forbidden,
        404 => ApiClass::NotFound,
        429 if document.and_then(|d| d.reason.as_deref()) == Some("QUOTA_EXCEEDED") => {
            ApiClass::QuotaExceeded
        }
        429 => ApiClass::RateLimited,
        500..=599 => ApiClass::Server,
        _ => ApiClass::Other,
    }
}
