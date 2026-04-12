use http::{HeaderMap, StatusCode};
use std::borrow::Cow;
use std::error::Error;
use std::fmt::Debug;
use thiserror::Error;

pub type FxError = Box<dyn Error + Send + Sync>;

#[derive(Clone, Debug)]
pub struct ErrorContext {
    pub endpoint: &'static str,
    pub method: http::Method,
}

impl core::fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} {}", self.method, self.endpoint)
    }
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ApiClientError {
    #[error("{ctx}: invalid/missing param: {param}")]
    InvalidParam {
        ctx: ErrorContext,
        param: &'static str,
    },

    #[error("{ctx}: build url error: {source}")]
    BuildUrl {
        ctx: ErrorContext,
        source: url::ParseError,
    },

    #[error("{ctx}: transport: {source}")]
    Transport {
        ctx: ErrorContext,
        source: crate::transport::TransportError,
    },

    #[error("{ctx}: status {status}")]
    HttpStatus {
        ctx: ErrorContext,
        status: StatusCode,
        headers: HeaderMap,
    },

    #[error("{ctx}: decode error: {source}")]
    Decode { ctx: ErrorContext, source: FxError },

    #[error("{ctx}: HEAD response requires NoContentEncoding")]
    HeadRequiresNoContent { ctx: ErrorContext },

    #[error("{ctx}: transform error: {source}")]
    Transform { ctx: ErrorContext, source: FxError },
    #[error("{ctx}: status {status} has no content; endpoint must use NoContentEncoding")]
    NoContentStatusRequiresNoContent {
        ctx: ErrorContext,
        status: StatusCode,
    },
    #[error("{ctx}: codec: {source}")]
    Codec { ctx: ErrorContext, source: FxError },

    #[error("{ctx}: pagination: {msg}")]
    Pagination {
        ctx: ErrorContext,
        msg: Cow<'static, str>,
    },

    #[error("{ctx}: pagination limit reached: {msg}")]
    PaginationLimit {
        ctx: ErrorContext,
        msg: Cow<'static, str>,
    },

    #[error("{ctx}: policy violation: {msg}")]
    PolicyViolation {
        ctx: ErrorContext,
        msg: &'static str,
    },
    #[error(
        "{ctx}: invalid host label: label[{index}]='{label}' (placeholder={placeholder:?}) reason={reason:?}"
    )]
    InvalidHostLabel {
        ctx: ErrorContext,
        label: String,
        index: usize,
        placeholder: Option<&'static str>,
        reason: HostLabelInvalidReason,
    },

    #[error("{ctx}: controller config error: key={key} expected={expected}")]
    ControllerConfig {
        ctx: ErrorContext,
        key: &'static str,
        expected: &'static str,
    },
}

#[derive(Copy, Clone, Debug)]
pub enum HostLabelInvalidReason {
    Empty,
    ContainsDot,
    ContainsSlash,
    ContainsScheme,
    ContainsWhitespace,
    StartsOrEndsDash,
    InvalidByte(u8),
    AbsoluteModePushLabel,
}

impl ApiClientError {
    #[inline]
    pub fn codec_error(ctx: ErrorContext, error: impl Into<FxError>) -> ApiClientError {
        ApiClientError::Codec {
            ctx,
            source: error.into(),
        }
    }
}
