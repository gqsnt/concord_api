use http::{HeaderMap, StatusCode};
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display};
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
        param: Cow<'static, str>,
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
        headers: Box<HeaderMap>,
        rate_limit: Option<Box<crate::rate_limit::RateLimitResponseAction>>,
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

    #[error("{ctx}: auth: {source}")]
    Auth {
        ctx: ErrorContext,
        source: crate::auth::AuthError,
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
    pub fn decode_error(
        ctx: ErrorContext,
        status: StatusCode,
        content_type: Option<&str>,
        error: impl Into<FxError>,
    ) -> ApiClientError {
        ApiClientError::Decode {
            ctx,
            source: Box::new(ContextualDecodeError {
                status,
                content_type: content_type
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| "<missing>".to_string()),
                source: error.into(),
            }),
        }
    }

    #[inline]
    pub fn codec_error(ctx: ErrorContext, error: impl Into<FxError>) -> ApiClientError {
        ApiClientError::Codec {
            ctx,
            source: error.into(),
        }
    }

    #[inline]
    pub fn invalid_param(ctx: ErrorContext, param: impl Into<Cow<'static, str>>) -> ApiClientError {
        ApiClientError::InvalidParam {
            ctx,
            param: param.into(),
        }
    }

    #[inline]
    pub fn context(&self) -> &ErrorContext {
        match self {
            ApiClientError::InvalidParam { ctx, .. }
            | ApiClientError::BuildUrl { ctx, .. }
            | ApiClientError::Transport { ctx, .. }
            | ApiClientError::HttpStatus { ctx, .. }
            | ApiClientError::Decode { ctx, .. }
            | ApiClientError::HeadRequiresNoContent { ctx }
            | ApiClientError::Transform { ctx, .. }
            | ApiClientError::NoContentStatusRequiresNoContent { ctx, .. }
            | ApiClientError::Codec { ctx, .. }
            | ApiClientError::Pagination { ctx, .. }
            | ApiClientError::PaginationLimit { ctx, .. }
            | ApiClientError::Auth { ctx, .. }
            | ApiClientError::PolicyViolation { ctx, .. }
            | ApiClientError::InvalidHostLabel { ctx, .. } => ctx,
        }
    }

    #[inline]
    pub fn http_status(&self) -> Option<StatusCode> {
        match self {
            ApiClientError::HttpStatus { status, .. } => Some(*status),
            _ => None,
        }
    }

    #[inline]
    pub fn http_headers(&self) -> Option<&HeaderMap> {
        match self {
            ApiClientError::HttpStatus { headers, .. } => Some(headers.as_ref()),
            _ => None,
        }
    }

    #[inline]
    pub fn rate_limit_response_action(
        &self,
    ) -> Option<&crate::rate_limit::RateLimitResponseAction> {
        match self {
            ApiClientError::HttpStatus { rate_limit, .. } => rate_limit.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct ContextualDecodeError {
    status: StatusCode,
    content_type: String,
    source: FxError,
}

impl Display for ContextualDecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "status={} content-type={}: {}",
            self.status, self.content_type, self.source
        )
    }
}

impl Error for ContextualDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::{RateLimitResponseAction, RateLimitTarget};

    #[test]
    fn http_status_accessors_hide_boxing_details() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", http::HeaderValue::from_static("1"));
        let ctx = ErrorContext {
            endpoint: "Ping",
            method: http::Method::GET,
        };
        let err = ApiClientError::HttpStatus {
            ctx,
            status: StatusCode::TOO_MANY_REQUESTS,
            headers: Box::new(headers),
            rate_limit: Some(Box::new(RateLimitResponseAction::Limited {
                retry_after: Some(std::time::Duration::from_secs(1)),
                target: RateLimitTarget::Request,
                cooldown_stored: false,
            })),
        };

        assert_eq!(err.context().endpoint, "Ping");
        assert_eq!(err.http_status(), Some(StatusCode::TOO_MANY_REQUESTS));
        assert_eq!(
            err.http_headers()
                .and_then(|headers| headers.get("retry-after")),
            Some(&http::HeaderValue::from_static("1"))
        );
        assert!(err.rate_limit_response_action().is_some());
    }
}
