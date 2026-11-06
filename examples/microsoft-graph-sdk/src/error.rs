//! Typed failures with retained HTTP metadata and redacted diagnostics.
use std::{collections::BTreeMap, fmt};

use concord::ResponseMeta;
use serde::Deserialize;
use serde_json::Value;

/// A Graph error document. Values are available explicitly but omitted from `Debug`.
#[derive(Clone, Deserialize)]
pub struct ProviderError {
    /// Machine-readable provider code, including unknown codes.
    pub code: String,
    /// Unredacted provider message; not a stable discriminator or safe log message.
    pub message: String,
    /// Nested provider detail, accepting both documented capitalizations.
    #[serde(rename = "innerError", alias = "innererror")]
    pub inner_error: Option<ErrorDetail>,
    /// Additional error details, when supplied.
    #[serde(default)]
    pub details: Vec<ErrorDetail>,
    /// Unrecognized error properties.
    #[serde(flatten)]
    pub additional: BTreeMap<String, Value>,
}

/// Recursive error detail; some inner objects contain only request-id/date metadata.
#[derive(Clone, Deserialize)]
pub struct ErrorDetail {
    /// More specific provider code, when supplied.
    pub code: Option<String>,
    /// Optional unredacted diagnostic text.
    pub message: Option<String>,
    /// Nested detail.
    #[serde(rename = "innerError", alias = "innererror")]
    pub inner_error: Option<Box<ErrorDetail>>,
    /// Request IDs, dates, or other properties exactly as returned.
    #[serde(flatten)]
    pub additional: BTreeMap<String, Value>,
}

impl ProviderError {
    /// Whether this code or a nested inner-error code matches (ASCII case insensitive).
    pub fn has_code(&self, code: &str) -> bool {
        if self.code.eq_ignore_ascii_case(code) {
            return true;
        }
        let mut inner = self.inner_error.as_ref();
        while let Some(detail) = inner {
            if detail
                .code
                .as_deref()
                .is_some_and(|v| v.eq_ignore_ascii_case(code))
            {
                return true;
            }
            inner = detail.inner_error.as_deref();
        }
        false
    }
}

impl fmt::Debug for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProviderError(<redacted>)")
    }
}
impl fmt::Debug for ErrorDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ErrorDetail(<redacted>)")
    }
}

/// Configuration rejected before sending any request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigurationProblem {
    /// Tenant or principal/permission context is empty.
    EmptyContext,
    /// Bearer value is empty or contains characters outside the bearer-token alphabet.
    InvalidToken,
    /// Test URL is not HTTP at a literal loopback IP and exactly `/v1.0`.
    InvalidLoopbackBase,
    /// The fixed transport could not be constructed.
    Transport,
}

/// Invalid continuation. No request is sent to a refused target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkProblem {
    /// Storage is not this SDK's versioned JSON representation.
    MalformedStorage,
    /// Link kind, context, selection, base or collection does not match the listing.
    BindingMismatch,
    /// URL is not absolute, has user information/fragment, or leaves the origin/collection.
    UnsafeDestination,
    /// A next link repeats the request URL. A delta link may legitimately repeat.
    DidNotAdvance,
}

/// A successful HTTP response violated the declared collection contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolProblem {
    /// These three operations require 200, not any arbitrary 2xx status.
    UnexpectedStatus,
    /// A delta page must have exactly one of nextLink or deltaLink.
    DeltaLinkCardinality,
    /// A regular collection must not carry a deltaLink.
    UnexpectedDeltaLink,
    /// An item has an empty identifier or required directory type.
    InvalidObject,
}

/// What happened while reading a non-200 provider response.
#[derive(Debug)]
pub enum ErrorBody {
    /// Parsed JSON error envelope, regardless of the response media type.
    Parsed(Box<ProviderError>),
    /// Bounded bytes arrived, but did not form the documented error envelope.
    Malformed,
    /// Body exceeded its bound or failed in transport; retains Concord's typed error.
    Unavailable(concord::Error),
}

/// Operational classification; unknown provider codes and all headers remain available.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorClass {
    /// Local configuration, continuation, format or protocol failure.
    Local,
    /// HTTP 400.
    InvalidRequest,
    /// HTTP 401.
    Authentication,
    /// HTTP 403, including insufficient claims or hidden-member permissions.
    Permission,
    /// HTTP 404.
    NotFound,
    /// HTTP 429; inspect Retry-After, with no automatic retry.
    Throttled,
    /// HTTP 410 or syncStateNotFound; no automatic reset/restart is performed.
    SyncStateExpired,
    /// HTTP 5xx.
    Service,
    /// Another HTTP status, including an unfollowed redirect.
    OtherHttp,
}

/// SDK error. Rendering omits tokens, links, response header values and provider messages.
#[derive(Debug)]
pub enum GraphError {
    /// Invalid configuration.
    Configuration(ConfigurationProblem),
    /// A build, transport, JSON metadata, decode or response-size failure.
    Concord {
        /// Original Concord error, including operation identity.
        error: concord::Error,
        /// Response head if one arrived, retained even after body failure.
        meta: Option<Box<ResponseMeta>>,
    },
    /// Non-success status, including redirects (the SDK never follows redirects).
    Provider {
        /// Actual status, all headers, and generated operation identity.
        meta: Box<ResponseMeta>,
        /// Parsed document or explicit body failure.
        body: ErrorBody,
    },
    /// Unsafe, misbound, malformed or nonadvancing continuation.
    Link {
        /// Reason the link was rejected.
        problem: LinkProblem,
        /// Originating response, when the link was returned in a page.
        meta: Option<Box<ResponseMeta>>,
    },
    /// Invalid collection envelope or object.
    Protocol {
        /// Contract violation.
        problem: ProtocolProblem,
        /// Actual response metadata.
        meta: Box<ResponseMeta>,
    },
}

impl GraphError {
    /// Metadata of the involved response, if any.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Concord { meta, .. } | Self::Link { meta, .. } => meta.as_deref(),
            Self::Provider { meta, .. } | Self::Protocol { meta, .. } => Some(meta),
            Self::Configuration(_) => None,
        }
    }

    /// Parsed provider error, if its bounded body decoded successfully.
    pub fn provider(&self) -> Option<&ProviderError> {
        match self {
            Self::Provider {
                body: ErrorBody::Parsed(error),
                ..
            } => Some(error),
            _ => None,
        }
    }

    /// Status/code classification; this does not trigger retry, refresh or reset.
    pub fn class(&self) -> ErrorClass {
        if !matches!(self, Self::Provider { .. }) {
            return ErrorClass::Local;
        }
        if self
            .provider()
            .is_some_and(|e| e.has_code("syncStateNotFound"))
        {
            return ErrorClass::SyncStateExpired;
        }
        match self.meta().map(|m| m.status().as_u16()) {
            Some(400) => ErrorClass::InvalidRequest,
            Some(401) => ErrorClass::Authentication,
            Some(403) => ErrorClass::Permission,
            Some(404) => ErrorClass::NotFound,
            Some(410) => ErrorClass::SyncStateExpired,
            Some(429) => ErrorClass::Throttled,
            Some(500..=599) => ErrorClass::Service,
            _ => ErrorClass::OtherHttp,
        }
    }

    pub(crate) fn link(problem: LinkProblem) -> Self {
        Self::Link {
            problem,
            meta: None,
        }
    }

    pub(crate) fn with_meta(self, meta: &ResponseMeta) -> Self {
        match self {
            Self::Link { problem, .. } => Self::Link {
                problem,
                meta: Some(Box::new(meta.clone())),
            },
            other => other,
        }
    }
}

impl From<concord::Error> for GraphError {
    fn from(error: concord::Error) -> Self {
        Self::Concord { error, meta: None }
    }
}
impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(p) => write!(f, "Graph configuration: {p:?}"),
            Self::Concord { error, .. } => fmt::Display::fmt(error, f),
            Self::Provider { meta, .. } => write!(
                f,
                "Graph HTTP {} ({:?})",
                meta.status().as_u16(),
                self.class()
            ),
            Self::Link { problem, .. } => write!(f, "Graph continuation: {problem:?}"),
            Self::Protocol { problem, .. } => write!(f, "Graph response: {problem:?}"),
        }
    }
}
impl std::error::Error for GraphError {}

#[derive(Deserialize)]
pub(crate) struct ErrorEnvelope {
    pub error: ProviderError,
}
