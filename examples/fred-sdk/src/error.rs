/// A FRED error document. Its message may echo the key and is never formatted.
#[derive(serde::Deserialize, concord::Debug)]
pub struct Problem {
    /// Provider document code; the HTTP status remains authoritative.
    pub error_code: u16,
    #[sensitive]
    pub(crate) error_message: String,
}

impl Problem {
    /// Explicit access to provider text, which can contain credentials.
    pub fn message_unredacted(&self) -> &str {
        &self.error_message
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
#[derive(Debug, concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// Build, transport, metadata, decode or HTTP-body limit failure.
    Concord(concord::Error),
    /// Any non-2xx, including FRED's 400, 404, 423, 429 and 500.
    #[from(skip)]
    Http(Box<HttpError>),
    /// A page size outside the provider's range.
    InvalidLimit {
        /// Inclusive maximum.
        maximum: u32,
    },
    /// An observation control outside the documented value set; input is not retained.
    InvalidParameter {
        /// The rejected parameter's static wire name.
        parameter: &'static str,
    },
    /// Offset/count/limit/items contradict the request or cannot advance safely.
    Pagination,
}

fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::Concord(e) => std::fmt::Display::fmt(e, f),
        Error::Http(e) => write!(f, "FRED HTTP {}", e.meta.status().as_u16()),
        Error::InvalidLimit { maximum } => write!(f, "FRED limit must be in 1..={maximum}"),
        Error::InvalidParameter { parameter } => write!(f, "invalid FRED {parameter}"),
        Error::Pagination => f.write_str("FRED pagination envelope is inconsistent"),
    }
}

// No source chain: even a provider's error text may contain a query key.
