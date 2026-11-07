use crate::{Feed, Problem};
use concord::ResponseMeta;
use std::fmt;

/// HTTP failures retain status even when the Atom error document cannot be decoded.
pub struct HttpFailure {
    /// Safe response metadata; header values require explicit access.
    pub meta: ResponseMeta,
    /// Bounded Atom document or its media, transport, size or decode failure.
    pub document: Result<Feed, concord::Error>,
}
impl fmt::Debug for HttpFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpFailure")
            .field("meta", &self.meta)
            .field("document_decoded", &self.document.is_ok())
            .finish()
    }
}
/// An Atom error entry received with a successful HTTP status.
#[derive(Debug)]
pub struct ApiFailure {
    /// Status and headers of the error feed.
    pub meta: ResponseMeta,
    /// Explicit provider detail, withheld by `Debug`.
    pub problem: Problem,
}
/// SDK failures. Formatting excludes terms, IDs, body text, URLs and response header values.
#[derive(Debug)]
pub enum Error {
    /// Input refused before admission or network I/O.
    Input(&'static str),
    /// Concord build, transport, metadata, decode or byte-limit failure.
    Concord(Box<concord::Error>),
    /// A non-2xx response, regardless of whether its document decoded.
    Http(Box<HttpFailure>),
    /// A provider error entry inside a 2xx feed.
    Api(Box<ApiFailure>),
    /// Paging metadata is inconsistent; traversal stops explicitly.
    Pagination(&'static str),
    /// Local admission failed, for example because a clock overflowed.
    Admission(concord_ratelimit::Error),
}
impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord(Box::new(error))
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(s) | Self::Pagination(s) => f.write_str(s),
            Self::Concord(e) => e.fmt(f),
            Self::Http(e) => write!(f, "arXiv HTTP {}", e.meta.status()),
            Self::Api(_) => f.write_str("arXiv returned an Atom error entry"),
            Self::Admission(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for Error {}
