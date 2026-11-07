//! The SDK's error type: what every generated `send()` returns on failure.
//!
//! Every variant names the declared operation and the route's top-level resource (the channel or
//! guild), so two channels that share a rate-limit bucket hash are never confused. `Display`
//! and `Debug` print the operation, the resource ID, the status, Discord's numeric error code and
//! delays; never the bot token, a URL, a provider message or a body.

use std::fmt;
use std::time::Duration;

use concord::reqwest::StatusCode;
use concord::{ErrorKind, Operation, ResponseMeta};
use serde::Deserialize;

use crate::limiter::{BucketHash, Route};
use crate::model::{ChannelId, GuildId};

/// The top-level resource of a route (Discord's "major parameter"): rate limits for routes with
/// different top-level resources are counted independently, even when they share a bucket hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Resource {
    /// A `/channels/{channel.id}/..` route.
    Channel(ChannelId),
    /// A `/guilds/{guild.id}/..` route.
    Guild(GuildId),
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Channel(id) => write!(f, "channel {id}"),
            Self::Guild(id) => write!(f, "guild {id}"),
        }
    }
}

/// A numeric Discord JSON error code (the `code` of an error document). Unknown codes are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct JsonErrorCode(pub u32);

impl JsonErrorCode {
    /// `0`: general error (such as a malformed request body).
    pub const GENERAL: Self = Self(0);
    /// `10003`: unknown channel.
    pub const UNKNOWN_CHANNEL: Self = Self(10003);
    /// `10004`: unknown guild.
    pub const UNKNOWN_GUILD: Self = Self(10004);
    /// `10008`: unknown message.
    pub const UNKNOWN_MESSAGE: Self = Self(10008);
    /// `20016`: slowmode rate limit.
    pub const SLOWMODE_RATE_LIMIT: Self = Self(20016);
    /// `20028`: the channel's write rate limit.
    pub const CHANNEL_WRITE_RATE_LIMIT: Self = Self(20028);
    /// `40001`: unauthorized.
    pub const UNAUTHORIZED: Self = Self(40001);
    /// `50001`: missing access.
    pub const MISSING_ACCESS: Self = Self(50001);
    /// `50006`: cannot send an empty message.
    pub const EMPTY_MESSAGE: Self = Self(50006);
    /// `50008`: cannot send messages in a non-text channel.
    pub const NON_TEXT_CHANNEL: Self = Self(50008);
    /// `50013`: missing permissions.
    pub const MISSING_PERMISSIONS: Self = Self(50013);
    /// `50014`: invalid authentication token.
    pub const INVALID_TOKEN: Self = Self(50014);
    /// `50035`: invalid form body (or an invalid `Content-Type`).
    pub const INVALID_FORM_BODY: Self = Self(50035);
}

/// A Discord error document: `{"code": .., "message": ..}`, with nested form `errors` for 50035.
///
/// `Debug` prints the code only; the message and the nested errors are read through the fields.
#[derive(Clone, Deserialize)]
pub struct ErrorDocument {
    /// Discord's numeric code.
    pub code: JsonErrorCode,
    /// Discord's human-readable message (provider text; not printed by this SDK).
    pub message: String,
    /// Nested form errors (`{"field": {"_errors": [{"code", "message"}]}}`), when sent.
    #[serde(default)]
    pub errors: Option<serde_json::Value>,
}

impl fmt::Debug for ErrorDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorDocument")
            .field("code", &self.code.0)
            .field("errors", &self.errors.is_some())
            .finish_non_exhaustive()
    }
}

/// The documented HTTP failure classes of Discord's API (any other status is `Other`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorClass {
    /// 400: malformed request (form errors are code 50035).
    BadRequest,
    /// 401: missing or invalid `Authorization`; stop using the token.
    Unauthorized,
    /// 403: the token lacks permission (for example `VIEW_CHANNEL`, `READ_MESSAGE_HISTORY`,
    /// `SEND_MESSAGES`).
    Forbidden,
    /// 404: the resource does not exist (or is not visible to the bot).
    NotFound,
    /// 405: the method is not valid for the location.
    MethodNotAllowed,
    /// 5xx, including 502 (no gateway available).
    Server,
    /// Any other non-2xx status (429 is [`Error::RateLimited`], never this).
    Other,
}

impl ErrorClass {
    fn of(status: StatusCode) -> Self {
        match status.as_u16() {
            400 => Self::BadRequest,
            401 => Self::Unauthorized,
            403 => Self::Forbidden,
            404 => Self::NotFound,
            405 => Self::MethodNotAllowed,
            500..=599 => Self::Server,
            _ => Self::Other,
        }
    }
}

/// A non-2xx, non-429 response. The status is authoritative whether or not its document could
/// be read.
#[derive(Debug)]
pub struct ApiError {
    /// The operation and top-level resource of the failed request.
    pub route: Route,
    /// Status, version and headers of the response (`Debug` prints header names only).
    pub meta: ResponseMeta,
    /// The error document, or why it could not be read: another media type
    /// (`ErrorKind::Metadata`), longer than [`ERROR_BODY_LIMIT`](crate::ERROR_BODY_LIMIT)
    /// (`Limit`), interrupted (`Transport`) or not Discord's shape (`Decode`).
    pub document: Result<ErrorDocument, concord::Error>,
}

impl ApiError {
    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.meta.status()
    }

    /// The documented failure class of the status.
    pub fn class(&self) -> ErrorClass {
        ErrorClass::of(self.status())
    }

    /// Discord's JSON code, when the document was read.
    pub fn code(&self) -> Option<JsonErrorCode> {
        self.document.as_ref().ok().map(|d| d.code)
    }
}

/// The scope a 429 applies to (`X-RateLimit-Scope`, `X-RateLimit-Global`, body `global`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateLimitScope {
    /// `user`: the per-bot limit of this route bucket and resource.
    User,
    /// `global`: the bot's global limit; every route is gated.
    Global,
    /// `shared`: a per-resource limit shared with other users of the resource.
    Shared,
    /// No scope was sent (or an unrecognized one) and nothing said global.
    Unspecified,
}

/// The JSON body of a 429: `{"message", "retry_after", "global", "code"?}`. `Debug` omits the
/// message.
#[derive(Clone, Deserialize)]
pub struct RateLimitBody {
    /// Discord's message (provider text).
    pub message: String,
    /// Seconds to wait, possibly fractional.
    pub retry_after: f64,
    /// Whether the global limit was hit.
    pub global: bool,
    /// An error code for some limits.
    pub code: Option<JsonErrorCode>,
}

impl fmt::Debug for RateLimitBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RateLimitBody")
            .field("retry_after", &self.retry_after)
            .field("global", &self.global)
            .field("code", &self.code.map(|c| c.0))
            .finish_non_exhaustive()
    }
}

/// An HTTP 429 from Discord. The SDK recorded the delay in the client's
/// [`RateLimiter`](crate::RateLimiter): until it elapses, requests in the same scope are refused
/// locally with [`Error::Gated`]. Nothing is retried.
#[derive(Debug)]
pub struct RateLimited {
    /// The operation and top-level resource of the refused request.
    pub route: Route,
    /// What the limit applies to.
    pub scope: RateLimitScope,
    /// The delay the SDK gates on: the larger of a valid `Retry-After` header and a valid body
    /// `retry_after`, else `X-RateLimit-Reset-After`; `None` if Discord sent no usable delay (the
    /// SDK then gates nothing and invents no zero).
    pub retry_after: Option<Duration>,
    /// The route's bucket hash, when sent (absent on a global 429).
    pub bucket: Option<BucketHash>,
    /// Status, version and headers of the response.
    pub meta: ResponseMeta,
    /// The 429 body, or why it could not be read; the scope and delay above also use the headers.
    pub body: Result<RateLimitBody, concord::Error>,
}

/// What a local refusal applies to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GateScope {
    /// The bot's global limit (after a global 429).
    Global,
    /// One route bucket for one top-level resource. `bucket` is the hash, or `None` when the
    /// route's bucket is not known yet (the route itself was limited).
    Bucket {
        /// Discord's bucket hash.
        bucket: Option<BucketHash>,
    },
}

/// A request refused by the SDK **before any I/O**: Discord's last headers or 429 say this scope
/// has no request left until `retry_after` elapses. Nothing was sent, so sending later is safe.
#[derive(Debug)]
pub struct Gated {
    /// The operation and top-level resource that was refused.
    pub route: Route,
    /// The gated scope.
    pub scope: GateScope,
    /// How long until the scope opens, by the limiter's clock.
    pub retry_after: Duration,
}

/// A write whose outcome is unknown: the request may have reached Discord, and the SDK did not
/// and will never resend it. Reconcile by reading (list the channel's recent messages and look
/// for your nonce), or resend deliberately with the same [`Nonce`](crate::Nonce) and
/// `enforce_nonce` within Discord's few-minute window.
#[derive(Debug)]
pub struct AmbiguousWrite {
    /// The operation and top-level resource of the write.
    pub route: Route,
    /// `None`: no response head arrived (a timeout, a dropped connection). `Some(status)`: Discord
    /// answered 2xx — the message was created — but its body could not be read or decoded, so
    /// the new ID is unknown.
    pub accepted: Option<StatusCode>,
    /// Concord's error (`Transport` with `transport().is_timeout()` for a timeout).
    pub error: concord::Error,
}

/// A response that breaks a documented rule the SDK relies on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolProblem {
    /// A message page was not ordered newest to oldest below its `before` anchor (following it
    /// could loop or skip).
    UnorderedPage,
    /// A message page held more messages than the requested `limit`.
    OversizedPage,
}

/// A documented provider rule was broken by a response.
#[derive(Debug)]
pub struct ProtocolError {
    /// The operation and top-level resource of the request.
    pub route: Route,
    /// The broken rule.
    pub problem: ProtocolProblem,
}

/// Every failure of a generated `send()` (and of the [`history`](crate::DiscordChannel::history)
/// traversal).
#[derive(Debug)]
pub enum Error {
    /// Refused locally before any I/O by the rate limiter.
    Gated(Box<Gated>),
    /// HTTP 429 from Discord.
    RateLimited(Box<RateLimited>),
    /// Another non-2xx response.
    Api(Box<ApiError>),
    /// A write (`POST`) whose outcome is unknown; never resent.
    AmbiguousWrite(Box<AmbiguousWrite>),
    /// A response broke a documented ordering or size rule.
    Protocol(Box<ProtocolError>),
    /// Build, transport on a read, success media type, success body limit or decoding failure. Concord's error keeps the
    /// response head when one arrived (`error.head()`).
    Concord {
        /// Concord's error, naming the operation.
        error: concord::Error,
        /// The route's top-level resource, when the request was built.
        resource: Option<Resource>,
    },
}

impl Error {
    pub(crate) fn concord(error: concord::Error, resource: Option<Resource>) -> Self {
        Self::Concord { error, resource }
    }

    /// The declared operation that failed.
    pub fn operation(&self) -> Option<Operation> {
        match self.route() {
            Some(route) => Some(route.operation()),
            None => match self {
                Self::Concord { error, .. } => error.operation(),
                _ => None,
            },
        }
    }

    /// The route of every failure after the request was built (all but `Concord`).
    pub fn route(&self) -> Option<&Route> {
        match self {
            Self::Gated(e) => Some(&e.route),
            Self::RateLimited(e) => Some(&e.route),
            Self::Api(e) => Some(&e.route),
            Self::AmbiguousWrite(e) => Some(&e.route),
            Self::Protocol(e) => Some(&e.route),
            Self::Concord { .. } => None,
        }
    }

    /// The channel or guild the failed request was for.
    pub fn resource(&self) -> Option<Resource> {
        match self {
            Self::Concord { resource, .. } => *resource,
            _ => self.route().and_then(Route::resource),
        }
    }

    /// The delay before this scope may be called again: for [`Gated`](Self::Gated) and
    /// [`RateLimited`](Self::RateLimited) (when Discord sent a usable delay).
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Gated(e) => Some(e.retry_after),
            Self::RateLimited(e) => e.retry_after,
            _ => None,
        }
    }

    /// The HTTP status, whenever a response head arrived.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::RateLimited(e) => Some(e.meta.status()),
            Self::Api(e) => Some(e.status()),
            Self::AmbiguousWrite(e) => e.accepted,
            Self::Concord { error, .. } => error.head().map(|head| head.status()),
            Self::Gated(_) | Self::Protocol(_) => None,
        }
    }

    /// Whether this is a write that may have happened ([`AmbiguousWrite`](Self::AmbiguousWrite)).
    pub fn is_ambiguous_write(&self) -> bool {
        matches!(self, Self::AmbiguousWrite(_))
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::concord(error, None)
    }
}

struct Seconds(Duration);

impl fmt::Display for Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}s", self.0.as_secs_f64())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gated(e) => {
                let scope = match &e.scope {
                    GateScope::Global => "the global rate limit",
                    GateScope::Bucket { .. } => "a route rate limit",
                };
                write!(
                    f,
                    "{} refused before sending: {scope} opens in {}",
                    e.route,
                    Seconds(e.retry_after)
                )
            }
            Self::RateLimited(e) => {
                write!(
                    f,
                    "{} was rate limited (HTTP 429, {:?} scope",
                    e.route, e.scope
                )?;
                match e.retry_after {
                    Some(delay) => write!(f, ", retry after {})", Seconds(delay)),
                    None => f.write_str(", no usable delay)"),
                }
            }
            Self::Api(e) => {
                write!(f, "{} failed with HTTP {}", e.route, e.status().as_u16())?;
                match &e.document {
                    Ok(document) => write!(f, " (Discord code {})", document.code.0),
                    Err(error) if error.kind() == ErrorKind::Limit => {
                        f.write_str(" (error document too large)")
                    }
                    Err(_) => f.write_str(" (no readable error document)"),
                }
            }
            Self::AmbiguousWrite(e) => match e.accepted {
                None => write!(
                    f,
                    "{} may or may not have been applied (no response); it was not resent",
                    e.route
                ),
                Some(status) => write!(
                    f,
                    "{} was accepted (HTTP {}) but its response could not be read; it was not resent",
                    e.route,
                    status.as_u16()
                ),
            },
            Self::Protocol(e) => write!(f, "{} broke a provider rule: {:?}", e.route, e.problem),
            Self::Concord { error, resource } => match resource {
                Some(resource) => write!(f, "{error} (on {resource})"),
                None => error.fmt(f),
            },
        }
    }
}

impl std::error::Error for Error {}
