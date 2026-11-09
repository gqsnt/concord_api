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
#[derive(Debug, concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// Input refused before admission or network I/O.
    #[from(skip)]
    Input(&'static str),
    /// Concord build, transport, metadata, decode or byte-limit failure.
    Concord(Box<concord::Error>),
    /// A non-2xx response, regardless of whether its document decoded.
    #[from(concord::Error<crate::Feed>)]
    Http(Box<HttpFailure>),
    /// A provider error entry inside a 2xx feed.
    #[from(skip)]
    Api(Box<ApiFailure>),
    /// Paging metadata is inconsistent; traversal stops explicitly.
    #[from(skip)]
    Pagination(&'static str),
    /// Local admission failed, for example because a clock overflowed.
    #[from(skip)]
    Admission(concord_ratelimit::Error),
}

fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::Input(s) | Error::Pagination(s) => f.write_str(s),
        Error::Concord(e) => std::fmt::Display::fmt(e, f),
        Error::Http(e) => write!(f, "arXiv HTTP {}", e.meta.status()),
        Error::Api(_) => f.write_str("arXiv returned an Atom error entry"),
        Error::Admission(e) => std::fmt::Display::fmt(e, f),
    }
}

impl From<concord::Failure<crate::Feed>> for HttpFailure {
    fn from(failure: concord::Failure<crate::Feed>) -> Self {
        let (meta, document) = failure.into_document();
        Self { meta, document }
    }
}
