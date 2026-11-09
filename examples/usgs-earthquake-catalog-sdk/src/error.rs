use std::{
    fmt,
    time::{Duration, SystemTime},
};

/// Classification of an unsuccessful HTTP response. 429 is defensive HTTP support.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpKind {
    /// 204 or 404: no matching data (also an unknown selected event).
    NoData,
    /// 400: invalid query or provider result ceiling.
    BadRequest,
    /// 401: credentials required by the responding server.
    Unauthorized,
    /// 403: access refused.
    Forbidden,
    /// 409: deleted event conflict.
    Deleted,
    /// 413: request too large.
    TooLarge,
    /// 414: URI too long.
    UriTooLong,
    /// 429: throttled; the SDK does not retry automatically.
    RateLimited,
    /// Any 5xx, including 500 and 503.
    Server,
    /// Any other status except 200.
    Unexpected,
}

impl HttpKind {
    pub(crate) fn of(status: u16) -> Self {
        match status {
            204 | 404 => Self::NoData,
            400 => Self::BadRequest,
            401 => Self::Unauthorized,
            403 => Self::Forbidden,
            409 => Self::Deleted,
            413 => Self::TooLarge,
            414 => Self::UriTooLong,
            429 => Self::RateLimited,
            500..=599 => Self::Server,
            _ => Self::Unexpected,
        }
    }
}

/// An HTTP failure, preserving its status even if its bounded text body cannot be read.
pub struct HttpFailure {
    /// Typed classification.
    pub kind: HttpKind,
    /// Status and headers. Header values require deliberate accessor use.
    pub meta: concord::ResponseMeta,
    pub(crate) document: Result<String, concord::Error>,
}

impl HttpFailure {
    /// Bounded provider/proxy text, or its read/decoding error. May contain echoed secrets.
    pub fn document_unredacted(&self) -> Result<&str, &concord::Error> {
        self.document.as_deref()
    }

    /// Parse every Retry-After field using the supplied clock; never sleeps or retries.
    pub fn retry_after(
        &self,
        now: SystemTime,
    ) -> Result<Option<Duration>, concord_retry::http::RetryAfterError> {
        concord_retry::http::retry_after(
            self.meta
                .headers()
                .get_all("retry-after")
                .iter()
                .map(|value| value.as_bytes()),
            now,
        )
    }
}

impl fmt::Debug for HttpFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpFailure")
            .field("kind", &self.kind)
            .field("meta", &self.meta)
            .field("document_error", &self.document.as_ref().err())
            .finish_non_exhaustive()
    }
}

/// SDK input, HTTP, response, or pagination-contract failure. Body text is never formatted.
#[derive(Debug, concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// Input refused before I/O, with a static explanation.
    #[from(skip)]
    InvalidInput(&'static str),
    /// Transport, metadata, decoding or body-size error from Concord.
    Concord(Box<concord::Error>),
    /// Provider status with bounded diagnostic text or its decoding failure.
    #[from(skip)]
    Http(Box<HttpFailure>),
    /// An inconsistent page or impossible continuation.
    #[from(skip)]
    Protocol(&'static str),
    /// A terminal CSV record/transport failure; preceding rows have already been delivered.
    #[from(skip)]
    Stream(concord_codecs::stream::ReadError),
    /// The CSV page budget was exhausted before a short or empty page established completion.
    PageBudget {
        /// The first page not requested (one-based).
        next_offset: std::num::NonZeroU64,
    },
    /// An export already failed; subsequent calls do not return a clean end.
    ExportFailed,
}

impl Error {
    /// Whether this result means no data. Pagination treats it as exhaustion.
    pub fn is_no_data(&self) -> bool {
        matches!(self, Self::Http(e) if e.kind == HttpKind::NoData)
    }
}

fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::InvalidInput(message) | Error::Protocol(message) => f.write_str(message),
        Error::Concord(error) => std::fmt::Display::fmt(error, f),
        Error::Stream(error) => std::fmt::Display::fmt(error, f),
        Error::PageBudget { .. } => f.write_str("CSV page budget exhausted"),
        Error::ExportFailed => f.write_str("CSV export already failed"),
        Error::Http(error) => write!(
            f,
            "USGS HTTP {} ({:?})",
            error.meta.status().as_u16(),
            error.kind
        ),
    }
}

// No source chain: native/codec sources and provider text can echo body or URL data.
