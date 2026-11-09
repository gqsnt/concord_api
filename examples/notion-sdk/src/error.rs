//! Typed provider failures with the actual response metadata, including Retry-After.

use crate::{
    Response,
    models::{List, QueryResult},
};
use concord::{ResponseMeta, bytes::Bytes};
use serde::Deserialize;
use serde_json::Value;
use std::fmt;

/// Notion error codes; unrecognized codes preserve the server's spelling.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub enum ErrorCode {
    /// Invalid JSON request.
    InvalidJson,
    /// Invalid request parameters.
    InvalidRequest,
    /// Provider validation failure.
    ValidationError,
    /// Authentication failure.
    Unauthorized,
    /// Missing capabilities.
    RestrictedResource,
    /// Missing object or missing access to it.
    ObjectNotFound,
    /// Request rate exceeded.
    RateLimited,
    /// Conflicting request.
    ConflictError,
    /// Provider internal error.
    InternalServerError,
    /// Provider temporarily unavailable.
    ServiceUnavailable,
    /// Gateway timeout.
    GatewayTimeout,
    /// Provider row limit exceeded.
    RowLimitExceeded,
    /// Future or otherwise unmodeled error code.
    Other(String),
}

impl From<String> for ErrorCode {
    fn from(code: String) -> Self {
        match code.as_str() {
            "invalid_json" => Self::InvalidJson,
            "invalid_request" | "invalid_request_url" => Self::InvalidRequest,
            "validation_error" => Self::ValidationError,
            "unauthorized" => Self::Unauthorized,
            "restricted_resource" => Self::RestrictedResource,
            "object_not_found" => Self::ObjectNotFound,
            "rate_limited" => Self::RateLimited,
            "conflict_error" => Self::ConflictError,
            "internal_server_error" => Self::InternalServerError,
            "service_unavailable" => Self::ServiceUnavailable,
            "gateway_timeout" => Self::GatewayTimeout,
            "row_limit_exceeded" => Self::RowLimitExceeded,
            _ => Self::Other(code),
        }
    }
}

/// Provider error document. Its fields are unredacted; Error's Debug/Display omit them.
#[derive(Clone, Deserialize)]
pub struct ProviderError {
    /// Must be `error` for a recognized error envelope.
    pub object: String,
    /// Status in the document; the SDK checks it against the actual HTTP status.
    pub status: u16,
    /// Typed code, preserving unknown values.
    pub code: ErrorCode,
    /// Provider diagnostic text, available explicitly rather than logged automatically.
    pub message: String,
    /// Optional provider request ID from the document.
    pub request_id: Option<String>,
    /// Optional provider-specific details such as query retry guidance.
    pub additional_data: Option<Value>,
}

/// A contradictory list envelope, never treated as successful exhaustion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeProblem {
    /// has_more and the presence of next_cursor disagree.
    CursorMismatch,
    /// A present cursor is empty.
    EmptyCursor,
    /// The response returned the cursor that was just requested.
    CursorDidNotAdvance,
}

/// SDK failures. Response metadata is retained after every received head.
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// Build, transport, response-media, JSON decoding, or bounded-body failure.
    Api {
        #[source]
        /// Original Concord error, preserving operation identity and kind.
        source: concord::Error,
        /// Present once a response head was received.
        meta: Option<Box<ResponseMeta>>,
    },
    /// A valid Notion error response.
    Provider {
        /// Typed error document.
        error: Box<ProviderError>,
        /// Actual HTTP status, version, headers and declared operation.
        meta: Box<ResponseMeta>,
    },
    /// Non-2xx response whose body is not a valid Notion error for this HTTP status.
    InvalidErrorBody {
        /// Actual response metadata.
        meta: Box<ResponseMeta>,
        /// Bounded raw bytes, explicitly accessible for diagnostics.
        body: Bytes,
    },
    /// A malformed list continuation. This page's items are not emitted.
    Envelope {
        /// Which pagination invariant failed.
        problem: EnvelopeProblem,
        /// Response metadata.
        meta: Box<ResponseMeta>,
    },
    /// Notion marked this query incomplete, or returned an unfamiliar completeness status.
    IncompleteQuery {
        /// Entire current response, including this page's un-emitted results and status.
        response: Box<Response<List<QueryResult>>>,
    },
}

impl Error {
    pub(crate) fn api(source: concord::Error, meta: Option<ResponseMeta>) -> Self {
        Self::Api {
            source,
            meta: meta.map(Box::new),
        }
    }

    /// Actual response metadata, when a head was received.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Api { meta, .. } => meta.as_deref(),
            Self::Provider { meta, .. }
            | Self::InvalidErrorBody { meta, .. }
            | Self::Envelope { meta, .. } => Some(meta),
            Self::IncompleteQuery { response } => Some(&response.meta),
        }
    }

    /// Original Concord error for transport, decoding, media, build and body-limit failures.
    pub fn concord(&self) -> Option<&concord::Error> {
        match self {
            Self::Api { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(source: concord::Error) -> Self {
        let meta = source.head_meta();
        Self::api(source, meta)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api { source, meta } => f
                .debug_struct("Api")
                .field("source", source)
                .field("meta", meta)
                .finish(),
            Self::Provider { meta, .. } => f
                .debug_struct("Provider")
                .field("meta", meta)
                .finish_non_exhaustive(),
            Self::InvalidErrorBody { meta, body } => f
                .debug_struct("InvalidErrorBody")
                .field("meta", meta)
                .field("bytes", &body.len())
                .finish(),
            Self::Envelope { problem, meta } => f
                .debug_struct("Envelope")
                .field("problem", problem)
                .field("meta", meta)
                .finish(),
            Self::IncompleteQuery { response } => f
                .debug_struct("IncompleteQuery")
                .field("meta", &response.meta)
                .field("items", &response.value.results.len())
                .finish(),
        }
    }
}

fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::Api { source, .. } => std::fmt::Display::fmt(source, f),
        Error::Provider { meta, .. } => write!(f, "Notion HTTP {}", meta.status().as_u16()),
        Error::InvalidErrorBody { .. } => f.write_str("invalid Notion error document"),
        Error::Envelope { problem, .. } => write!(f, "invalid Notion pagination: {problem:?}"),
        Error::IncompleteQuery { .. } => f.write_str("Notion query completeness was not confirmed"),
    }
}
