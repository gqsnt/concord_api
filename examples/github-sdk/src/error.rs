//! The SDK's error type.
//!
//! Concord's error is kept as is (not stringified): its kind, endpoint, status and, for a status
//! or metadata error, the **unread** response remain available through [`GitHubError::concord`]
//! and [`GitHubError::into_concord`]. `Display` and `Debug` never contain the token, a URL, a link
//! target or a body: Concord's own error rendering, static labels and numbers only.

use std::fmt;

use concord::reqwest::StatusCode;

use crate::context::ContextError;
use crate::link::LinkError;
use crate::meta::ResponseMeta;
use crate::paging::Binding;

/// Why a page envelope was refused (see [`crate::paging::Listing`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvelopeProblem {
    /// `total_count` is smaller than the number of items on this one page.
    TotalBelowPage {
        /// The envelope's `total_count`.
        total_count: u64,
        /// Items on the page.
        items: usize,
    },
}

/// An error of this SDK.
#[non_exhaustive]
pub enum GitHubError {
    /// A Concord error: build, transport, non-2xx status, refused head or undecodable body.
    Api {
        /// The Concord error, unchanged (a status error keeps its unread response).
        error: concord::Error,
        /// The response metadata, when a response head arrived.
        meta: Option<Box<ResponseMeta>>,
    },
    /// A page's `Link` header could not be read; no next page was guessed.
    Link {
        /// The parse failure.
        error: LinkError,
        /// The page's metadata.
        meta: Box<ResponseMeta>,
    },
    /// A `rel="next"` target (or a restored continuation) points outside the configured API
    /// origin. It was not requested: the token is only ever sent to the API origin.
    ForeignLink {
        /// The page's metadata, when the link came from a response.
        meta: Option<Box<ResponseMeta>>,
    },
    /// A continuation was used with another operation or repository than the one that produced
    /// it.
    ContinuationMismatch {
        /// What the traversal expected.
        expected: Box<Binding>,
        /// What the continuation belongs to.
        found: Box<Binding>,
    },
    /// A page envelope is inconsistent with itself.
    Envelope {
        /// The problem.
        problem: EnvelopeProblem,
        /// The page's metadata.
        meta: Box<ResponseMeta>,
    },
    /// A native request could not be built from the context.
    Context(ContextError),
}

impl GitHubError {
    pub(crate) fn api(error: concord::Error, meta: Option<ResponseMeta>) -> Self {
        Self::Api {
            error,
            meta: meta.map(Box::new),
        }
    }

    /// The Concord error, if this is one.
    pub fn concord(&self) -> Option<&concord::Error> {
        match self {
            Self::Api { error, .. } => Some(error),
            _ => None,
        }
    }

    /// The Concord error, owned (for `concord::Error::into_response`), or `self` back.
    ///
    /// # Errors
    ///
    /// `self`, unchanged, when it is not a Concord error.
    pub fn into_concord(self) -> Result<concord::Error, Self> {
        match self {
            Self::Api { error, .. } => Ok(error),
            other => Err(other),
        }
    }

    /// The HTTP status of a Concord status error.
    pub fn status(&self) -> Option<StatusCode> {
        self.concord().and_then(concord::Error::status)
    }

    /// The metadata of the response involved, when one arrived.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Api { meta, .. } | Self::ForeignLink { meta } => meta.as_deref(),
            Self::Link { meta, .. } | Self::Envelope { meta, .. } => Some(meta),
            Self::ContinuationMismatch { .. } | Self::Context(_) => None,
        }
    }

    /// Whether GitHub signalled a rate limit ([`ResponseMeta::is_rate_limited`]).
    pub fn is_rate_limited(&self) -> bool {
        self.meta().is_some_and(ResponseMeta::is_rate_limited)
    }
}

impl From<ContextError> for GitHubError {
    fn from(error: ContextError) -> Self {
        Self::Context(error)
    }
}

/// A Concord error without captured metadata (for `prepare()?`).
impl From<concord::Error> for GitHubError {
    fn from(error: concord::Error) -> Self {
        Self::api(error, None)
    }
}

impl fmt::Display for GitHubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api { error, .. } => fmt::Display::fmt(error, f),
            Self::Link { error, .. } => write!(f, "GitHub page: {error}"),
            Self::ForeignLink { .. } => {
                f.write_str("GitHub page: next link is outside the API origin; not followed")
            }
            Self::ContinuationMismatch { expected, found } => write!(
                f,
                "GitHub continuation of `{}` used for `{}`",
                found.operation(),
                expected.operation()
            ),
            Self::Envelope { problem, .. } => match problem {
                EnvelopeProblem::TotalBelowPage { total_count, items } => write!(
                    f,
                    "GitHub page: total_count {total_count} is below the {items} items on the page"
                ),
            },
            Self::Context(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl fmt::Debug for GitHubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api { error, meta } => f
                .debug_struct("Api")
                .field("error", error)
                .field("meta", meta)
                .finish(),
            Self::Link { error, meta } => f
                .debug_struct("Link")
                .field("error", error)
                .field("meta", meta)
                .finish(),
            Self::ForeignLink { meta } => {
                f.debug_struct("ForeignLink").field("meta", meta).finish()
            }
            Self::ContinuationMismatch { expected, found } => f
                .debug_struct("ContinuationMismatch")
                .field("expected", expected)
                .field("found", found)
                .finish(),
            Self::Envelope { problem, meta } => f
                .debug_struct("Envelope")
                .field("problem", problem)
                .field("meta", meta)
                .finish(),
            Self::Context(error) => f.debug_tuple("Context").field(error).finish(),
        }
    }
}

/// No `source()`: the Concord error has no chain by design, and the others are leaves.
impl std::error::Error for GitHubError {}
