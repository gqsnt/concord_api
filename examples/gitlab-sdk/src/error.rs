//! The SDK's error: GitLab's answer for a non-2xx status (with its error document), a Concord
//! failure (with the response metadata when a response arrived), or a broken pagination contract.
//!
//! `Display` names the operation and the stage, never a URL, a header value, the token or a body
//! text; GitLab's message is reached through [`ApiError::message`].

use std::fmt;

use concord::reqwest::StatusCode;
use concord::{ErrorKind, Operation};
use serde::Deserialize;

use crate::link::LinkError;
use crate::meta::ResponseMeta;

/// Every failure of a GitLab call made through this SDK.
#[derive(Debug)]
#[non_exhaustive]
pub enum GitLabError {
    /// GitLab (or something in front of it) answered with a non-2xx status.
    Api(Box<ApiError>),
    /// Concord could not build the request (nothing was sent), send it, accept the 2xx response
    /// head (for example a body not labelled JSON) or decode the body. `meta` is `Some` once a
    /// response head arrived.
    Request {
        /// Concord's error; its `kind()` says which stage failed.
        error: concord::Error,
        /// The response metadata, when a response arrived.
        meta: Option<Box<ResponseMeta>>,
    },
    /// A paginated response broke the pagination contract, or a continuation was refused before
    /// anything was sent. The traversal stops; it is never treated as the last page.
    Pagination {
        /// What was wrong.
        problem: PaginationProblem,
        /// The response that carried the problem (`None` when refused before sending).
        meta: Option<Box<ResponseMeta>>,
    },
}

impl GitLabError {
    /// The HTTP status of an [`Api`](Self::Api) error.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Api(api) => Some(api.status),
            _ => None,
        }
    }

    /// Whether GitLab answered `404 Not Found` (a missing resource, or one the token cannot see).
    pub fn is_not_found(&self) -> bool {
        self.status() == Some(StatusCode::NOT_FOUND)
    }

    /// The metadata of the response involved, if one arrived.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Api(api) => Some(&api.meta),
            Self::Request { meta, .. } | Self::Pagination { meta, .. } => meta.as_deref(),
        }
    }

    /// Concord's error kind of a [`Request`](Self::Request) error.
    pub fn request_kind(&self) -> Option<ErrorKind> {
        match self {
            Self::Request { error, .. } => Some(error.kind()),
            _ => None,
        }
    }
}

impl From<concord::Error> for GitLabError {
    fn from(error: concord::Error) -> Self {
        Self::Request { error, meta: None }
    }
}

impl fmt::Display for GitLabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(api) => fmt::Display::fmt(api, f),
            // Concord's own `Display`: endpoint, stage, static labels; no value, URL or body.
            Self::Request { error, .. } => fmt::Display::fmt(error, f),
            Self::Pagination { problem, .. } => write!(f, "GitLab pagination: {problem}"),
        }
    }
}

impl std::error::Error for GitLabError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request { error, .. } => Some(error),
            Self::Pagination {
                problem: PaginationProblem::MalformedLink(error),
                ..
            } => Some(error),
            _ => None,
        }
    }
}

/// A non-2xx answer.
#[non_exhaustive]
pub struct ApiError {
    /// The operation (`GET GitLab.project.issue`).
    pub operation: Option<Operation>,
    /// The status.
    pub status: StatusCode,
    /// What the body held.
    pub body: ErrorBody,
    /// The response metadata (request id, `Retry-After`).
    pub meta: ResponseMeta,
}

impl ApiError {
    /// GitLab's `message`, when the body was a GitLab error document.
    pub fn message(&self) -> Option<&ErrorMessage> {
        match &self.body {
            ErrorBody::Document(ErrorDocument { message }) => Some(message),
            _ => None,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(operation) = &self.operation {
            write!(f, "{operation}: ")?;
        }
        write!(f, "GitLab answered HTTP {}", self.status)?;
        f.write_str(match &self.body {
            ErrorBody::Document(_) => " with an error document",
            ErrorBody::Unrecognized(_) => " with a body that is not a GitLab error document",
            ErrorBody::TooLarge { .. } => " with an error body over the SDK's limit",
            ErrorBody::Unreadable(_) => "; its body could not be read",
        })
    }
}

/// `Debug` shows GitLab's message (a short server text such as `404 Project Not Found`), the
/// status, the operation and the metadata; an unrecognized body is shown by its length only.
impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiError")
            .field("operation", &self.operation)
            .field("status", &self.status.as_u16())
            .field("body", &self.body)
            .field("meta", &self.meta)
            .finish()
    }
}

/// The body of a non-2xx answer, read up to [`ERROR_BODY_LIMIT`] bytes.
#[non_exhaustive]
pub enum ErrorBody {
    /// A GitLab error document: a JSON object with `message`.
    Document(ErrorDocument),
    /// Something else (an HTML page from a proxy, empty, other JSON), kept as bytes.
    Unrecognized(concord::bytes::Bytes),
    /// Longer than the limit; the rest was not read.
    TooLarge {
        /// The limit, in bytes.
        limit: usize,
    },
    /// Reading the body failed (Concord's `Transport` error).
    Unreadable(concord::Error),
}

impl fmt::Debug for ErrorBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(document) => f.debug_tuple("Document").field(document).finish(),
            Self::Unrecognized(bytes) => write!(f, "Unrecognized({} bytes)", bytes.len()),
            Self::TooLarge { limit } => write!(f, "TooLarge {{ limit: {limit} }}"),
            Self::Unreadable(error) => f.debug_tuple("Unreadable").field(&error.kind()).finish(),
        }
    }
}

/// The most error-body bytes the SDK reads (64 KiB).
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// GitLab's error document: `{"message": ...}`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct ErrorDocument {
    /// The message.
    pub message: ErrorMessage,
}

/// `message`: a text (`"401 Unauthorized"`, `"400 (Bad request) \"title\" not given"`) or, for a
/// validation error, an object of property name to messages (nested for embedded entities).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ErrorMessage {
    /// A text.
    Text(String),
    /// A validation object, as sent.
    Fields(serde_json::Map<String, serde_json::Value>),
}

impl ErrorMessage {
    /// The text form, if it is one.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Fields(_) => None,
        }
    }
}

/// How a paginated response broke the contract, or why a continuation was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PaginationProblem {
    /// The `Link` header could not be read.
    MalformedLink(LinkError),
    /// `rel="next"` is on another origin (scheme, host, port) than the API: it is not followed,
    /// so the token is never sent there.
    ForeignLink,
    /// `rel="next"` is not a project issue collection under the API's base path.
    OtherCollection,
    /// `rel="next"` repeats the URL just requested; following it would never end.
    NotAdvancing,
    /// No `rel="next"`, but `x-next-page` names a next page: ending here could drop issues.
    MissingNextLink,
    /// A continuation from another listing (another project or collection) was passed to
    /// [`pages_from`](crate::api::GitLabProjectIssues::pages_from).
    ContinuationMismatch,
}

impl fmt::Display for PaginationProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedLink(error) => fmt::Display::fmt(error, f),
            Self::ForeignLink => f.write_str("the next link leaves the API origin"),
            Self::OtherCollection => {
                f.write_str("the next link is not a project issue collection of this API")
            }
            Self::NotAdvancing => f.write_str("the next link repeats the page just requested"),
            Self::MissingNextLink => {
                f.write_str("x-next-page names a next page but no Link rel=\"next\" was sent")
            }
            Self::ContinuationMismatch => {
                f.write_str("the continuation belongs to another listing")
            }
        }
    }
}
