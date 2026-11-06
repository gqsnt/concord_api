use std::fmt;

/// A FRED error document. Its message may echo the key and is never formatted.
#[derive(serde::Deserialize)]
pub struct Problem {
    /// Provider document code; the HTTP status remains authoritative.
    pub error_code: u16,
    pub(crate) error_message: String,
}

impl Problem {
    /// Explicit access to provider text, which can contain credentials.
    pub fn message_unredacted(&self) -> &str {
        &self.error_message
    }
}

impl fmt::Debug for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Problem")
            .field("error_code", &self.error_code)
            .field("message", &"<redacted>")
            .finish()
    }
}

/// A non-2xx status, retained even when its error document fails.
#[derive(Debug)]
pub struct HttpError {
    /// Safe-formatting metadata, including the authoritative status.
    pub meta: concord::ResponseMeta,
    /// Typed XML/JSON problem, or the bounded read/head/decode failure.
    pub document: Result<Problem, concord::Error>,
    /// Strict shared parsing of every Retry-After line (seconds or HTTP date).
    /// Invalid/repeated/overflowing values remain distinguishable. No sleep or retry occurs.
    pub retry_after: Result<Option<std::time::Duration>, concord_retry::http::RetryAfterError>,
}

/// SDK failures. No formatter includes a URL, key, provider message or response body.
#[derive(Debug)]
pub enum Error {
    /// Build, transport, metadata, decode or HTTP-body limit failure.
    Concord(concord::Error),
    /// Any non-2xx, including FRED's 400, 404, 423, 429 and 500.
    Http(Box<HttpError>),
    /// A page size outside the provider's range.
    InvalidLimit {
        /// Inclusive maximum.
        maximum: u32,
    },
    /// Offset/count/limit/items contradict the request or cannot advance safely.
    Pagination,
}

impl From<concord::Error> for Error {
    fn from(value: concord::Error) -> Self {
        Self::Concord(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concord(e) => e.fmt(f),
            Self::Http(e) => write!(f, "FRED HTTP {}", e.meta.status().as_u16()),
            Self::InvalidLimit { maximum } => write!(f, "FRED limit must be in 1..={maximum}"),
            Self::Pagination => f.write_str("FRED pagination envelope is inconsistent"),
        }
    }
}

// No source chain: even a provider's error text may contain a query key.
impl std::error::Error for Error {}
