use std::{
    fmt,
    time::{Duration, SystemTime},
};

use concord::reqwest::{StatusCode, header::HeaderMap};
use serde::Deserialize;

use crate::RetryAfterError;

/// A singleton unsigned rate header. Missing and invalid never become a fabricated value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderNumber {
    /// No field was returned.
    Missing,
    /// One valid unsigned decimal integer.
    Value(u64),
    /// Malformed, repeated, signed or overflowing field.
    Invalid,
}

/// Intercom's rate observations. These do not authorize replay or schedule requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimits {
    /// Maximum requests in the provider window.
    pub limit: HeaderNumber,
    /// Requests remaining in the provider window.
    pub remaining: HeaderNumber,
    /// Absolute UNIX seconds; unknown on a headerless 429, with no fallback clock.
    pub reset_unix_seconds: HeaderNumber,
}

impl RateLimits {
    /// Read all occurrences of each header, refusing duplicates rather than selecting one.
    pub fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            limit: number(headers, "x-ratelimit-limit"),
            remaining: number(headers, "x-ratelimit-remaining"),
            reset_unix_seconds: number(headers, "x-ratelimit-reset"),
        }
    }
}

fn number(headers: &HeaderMap, name: &str) -> HeaderNumber {
    let mut values = headers.get_all(name).iter();
    let Some(value) = values.next() else {
        return HeaderNumber::Missing;
    };
    if values.next().is_some() {
        return HeaderNumber::Invalid;
    }
    let Ok(value) = value.to_str() else {
        return HeaderNumber::Invalid;
    };
    let value = value.trim_matches([' ', '\t']);
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return HeaderNumber::Invalid;
    }
    value
        .parse()
        .map(HeaderNumber::Value)
        .unwrap_or(HeaderNumber::Invalid)
}

/// Intercom normally sends a string code; some validation errors send an integer.
#[derive(Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ErrorCode {
    /// Known or future provider code, available explicitly without logging it by default.
    Text(String),
    /// Numeric validation code.
    Number(i64),
}

/// One provider error. All fields are unredacted explicit access; Debug withholds content.
#[derive(Deserialize)]
pub struct ErrorItem {
    /// Provider code (unknown strings are preserved).
    pub code: ErrorCode,
    /// Optional explanation, which may echo credentials or contact information.
    pub message: Option<String>,
    /// Optional provider details, such as required permissions.
    pub data: Option<serde_json::Value>,
}

#[derive(Deserialize)]
enum ErrorType {
    #[serde(rename = "error.list")]
    ErrorList,
}

/// Typed Intercom error envelope. Request IDs are optional in the provider contract.
#[derive(Deserialize)]
pub struct ErrorDocument {
    #[serde(rename = "type")]
    _kind: ErrorType,
    /// Provider request ID, if any. Access is unredacted.
    pub request_id: Option<String>,
    /// All supplied errors, without an SDK-maintained exhaustive code vocabulary.
    #[serde(deserialize_with = "nonempty_errors")]
    pub errors: Vec<ErrorItem>,
}

fn nonempty_errors<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ErrorItem>, D::Error> {
    let errors = Vec::<ErrorItem>::deserialize(deserializer)?;
    if errors.is_empty() {
        Err(serde::de::Error::custom(
            "an Intercom error list must contain an error",
        ))
    } else {
        Ok(errors)
    }
}

/// Non-success HTTP response, preserving its status even when its document cannot be read.
#[derive(Debug)]
pub struct ProviderError {
    /// HTTP metadata; raw header access is unredacted.
    pub meta: concord::ResponseMeta,
    /// Bounded strict JSON document, or the precise reason it was unavailable.
    pub document: Result<ErrorDocument, concord::Error>,
}

impl ProviderError {
    /// Rate headers as observed, including absent or invalid reset values.
    pub fn rate_limits(&self) -> RateLimits {
        RateLimits::from_headers(self.meta.headers())
    }

    /// Parse Retry-After with the shared strict parser; this never waits or replays.
    /// `Ok(None)` means missing; malformed/repeated fields are errors. The injected
    /// wall clock is used only for HTTP dates, never to invent an Intercom reset.
    pub fn retry_after(&self, now: SystemTime) -> Result<Option<Duration>, RetryAfterError> {
        concord_retry::http::retry_after(
            self.meta
                .headers()
                .get_all("retry-after")
                .iter()
                .map(|v| v.as_bytes()),
            now,
        )
    }
}

/// SDK failure. No variant automatically retries, including ambiguous creates.
#[derive(Debug)]
pub enum Error {
    /// Request construction, transport, media, decoding or body limit failure.
    Concord(concord::Error),
    /// Any non-2xx response, regardless of error body validity.
    Provider(Box<ProviderError>),
    /// Invalid local list size, refused before I/O.
    PageSize,
    /// Empty or immediately repeated provider cursor; no guessed next page.
    InvalidCursor(Box<concord::ResponseMeta>),
}

impl Error {
    /// Response status after a head arrived, including success-body failures.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Concord(e) => e.head().map(|h| h.status()),
            Self::Provider(e) => Some(e.meta.status()),
            Self::InvalidCursor(meta) => Some(meta.status()),
            _ => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord(e) => e.fmt(f),
            Self::Provider(e) => write!(f, "Intercom returned HTTP {}", e.meta.status().as_u16()),
            Self::PageSize => f.write_str("Intercom page size must be between 1 and 150"),
            Self::InvalidCursor(_) => {
                f.write_str("Intercom returned an empty or nonadvancing cursor")
            }
        }
    }
}

impl std::error::Error for Error {}

macro_rules! redacted_debug {
    ($($ty:ty),+ $(,)?) => {$(
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($ty), " { <redacted> }"))
            }
        }
    )+};
}
redacted_debug!(ErrorCode, ErrorItem, ErrorDocument);
