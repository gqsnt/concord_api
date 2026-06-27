use http::{HeaderMap, StatusCode};
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{self, Debug, Display};
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

#[derive(Error)]
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

    #[error("{ctx}: response Content-Length {actual} exceeds limit {limit} bytes")]
    ResponseTooLarge {
        ctx: ErrorContext,
        limit: usize,
        actual: u64,
    },

    #[error("{ctx}: response body exceeded limit {limit} bytes while reading")]
    ResponseBodyLimitExceeded { ctx: ErrorContext, limit: usize },

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

    #[error("{ctx}: runtime state error in {subsystem}: {msg}")]
    RuntimeState {
        ctx: ErrorContext,
        subsystem: &'static str,
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

impl Debug for ApiClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParam { ctx, param } => f
                .debug_struct("InvalidParam")
                .field("ctx", ctx)
                .field("param", param)
                .finish(),
            Self::BuildUrl { ctx, source } => f
                .debug_struct("BuildUrl")
                .field("ctx", ctx)
                .field("source", source)
                .finish(),
            Self::Transport { ctx, source } => f
                .debug_struct("Transport")
                .field("ctx", ctx)
                .field("source", source)
                .finish(),
            Self::ResponseTooLarge { ctx, limit, actual } => f
                .debug_struct("ResponseTooLarge")
                .field("ctx", ctx)
                .field("limit", limit)
                .field("actual", actual)
                .finish(),
            Self::ResponseBodyLimitExceeded { ctx, limit } => f
                .debug_struct("ResponseBodyLimitExceeded")
                .field("ctx", ctx)
                .field("limit", limit)
                .finish(),
            Self::HttpStatus {
                ctx,
                status,
                headers,
                rate_limit,
            } => f
                .debug_struct("HttpStatus")
                .field("ctx", ctx)
                .field("status", status)
                .field("headers", &crate::debug::RedactedHeaders(headers.as_ref()))
                .field("rate_limit", rate_limit)
                .finish(),
            Self::Decode { ctx, source } => f
                .debug_struct("Decode")
                .field("ctx", ctx)
                .field("source", source)
                .finish(),
            Self::HeadRequiresNoContent { ctx } => f
                .debug_struct("HeadRequiresNoContent")
                .field("ctx", ctx)
                .finish(),
            Self::Transform { ctx, source } => f
                .debug_struct("Transform")
                .field("ctx", ctx)
                .field("source", source)
                .finish(),
            Self::NoContentStatusRequiresNoContent { ctx, status } => f
                .debug_struct("NoContentStatusRequiresNoContent")
                .field("ctx", ctx)
                .field("status", status)
                .finish(),
            Self::Codec { ctx, source } => f
                .debug_struct("Codec")
                .field("ctx", ctx)
                .field("source", source)
                .finish(),
            Self::Pagination { ctx, msg } => f
                .debug_struct("Pagination")
                .field("ctx", ctx)
                .field("msg", msg)
                .finish(),
            Self::PaginationLimit { ctx, msg } => f
                .debug_struct("PaginationLimit")
                .field("ctx", ctx)
                .field("msg", msg)
                .finish(),
            Self::Auth { ctx, source } => f
                .debug_struct("Auth")
                .field("ctx", ctx)
                .field("source", source)
                .finish(),
            Self::PolicyViolation { ctx, msg } => f
                .debug_struct("PolicyViolation")
                .field("ctx", ctx)
                .field("msg", msg)
                .finish(),
            Self::RuntimeState {
                ctx,
                subsystem,
                msg,
            } => f
                .debug_struct("RuntimeState")
                .field("ctx", ctx)
                .field("subsystem", subsystem)
                .field("msg", msg)
                .finish(),
            Self::InvalidHostLabel {
                ctx,
                label,
                index,
                placeholder,
                reason,
            } => f
                .debug_struct("InvalidHostLabel")
                .field("ctx", ctx)
                .field("label", label)
                .field("index", index)
                .field("placeholder", placeholder)
                .field("reason", reason)
                .finish(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorCategory {
    Config,
    MissingCredential,
    AuthRejected,
    Transport,
    Timeout,
    HttpStatus,
    Decode,
    Pagination,
    RateLimit,
    InternalInvariant,
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
            | ApiClientError::ResponseTooLarge { ctx, .. }
            | ApiClientError::ResponseBodyLimitExceeded { ctx, .. }
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
            | ApiClientError::RuntimeState { ctx, .. }
            | ApiClientError::InvalidHostLabel { ctx, .. } => ctx,
        }
    }

    #[inline]
    pub fn category(&self) -> ErrorCategory {
        match self {
            ApiClientError::InvalidParam { .. }
            | ApiClientError::BuildUrl { .. }
            | ApiClientError::InvalidHostLabel { .. } => ErrorCategory::Config,
            ApiClientError::Transport { source, .. }
                if source.kind() == crate::transport::TransportErrorKind::Timeout =>
            {
                ErrorCategory::Timeout
            }
            ApiClientError::Transport { .. } => ErrorCategory::Transport,
            ApiClientError::ResponseTooLarge { .. }
            | ApiClientError::ResponseBodyLimitExceeded { .. } => ErrorCategory::Decode,
            ApiClientError::HttpStatus { rate_limit, .. } if rate_limit.is_some() => {
                ErrorCategory::RateLimit
            }
            ApiClientError::HttpStatus { .. } => ErrorCategory::HttpStatus,
            ApiClientError::Decode { .. }
            | ApiClientError::HeadRequiresNoContent { .. }
            | ApiClientError::Transform { .. }
            | ApiClientError::NoContentStatusRequiresNoContent { .. }
            | ApiClientError::Codec { .. } => ErrorCategory::Decode,
            ApiClientError::Pagination { .. } | ApiClientError::PaginationLimit { .. } => {
                ErrorCategory::Pagination
            }
            ApiClientError::Auth { source, .. }
                if source.kind == crate::auth::AuthErrorKind::MissingCredential =>
            {
                ErrorCategory::MissingCredential
            }
            ApiClientError::Auth { source, .. }
                if source.kind == crate::auth::AuthErrorKind::RejectedCredential =>
            {
                ErrorCategory::AuthRejected
            }
            ApiClientError::Auth { .. } => ErrorCategory::AuthRejected,
            ApiClientError::PolicyViolation { .. } | ApiClientError::RuntimeState { .. } => {
                ErrorCategory::InternalInvariant
            }
        }
    }

    #[inline]
    pub fn endpoint(&self) -> &'static str {
        self.context().endpoint
    }

    #[inline]
    pub fn method(&self) -> &http::Method {
        &self.context().method
    }

    #[inline]
    pub fn redacted_url(&self) -> Option<&str> {
        None
    }

    #[inline]
    pub fn phase(&self) -> Option<&'static str> {
        None
    }

    #[inline]
    pub fn page_index(&self) -> Option<u32> {
        None
    }

    #[inline]
    pub fn attempt_index(&self) -> Option<u32> {
        None
    }

    #[inline]
    pub fn attempt_count(&self) -> Option<u32> {
        None
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
    pub fn decode_status(&self) -> Option<StatusCode> {
        match self {
            ApiClientError::Decode { source, .. } => source
                .downcast_ref::<ContextualDecodeError>()
                .map(|err| err.status),
            _ => None,
        }
    }

    #[inline]
    pub fn decode_content_type(&self) -> Option<&str> {
        match self {
            ApiClientError::Decode { source, .. } => source
                .downcast_ref::<ContextualDecodeError>()
                .map(|err| err.content_type.as_str()),
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

    #[test]
    fn category_and_context_accessors_are_structured() {
        let ctx = ErrorContext {
            endpoint: "Ping",
            method: http::Method::GET,
        };
        let err = ApiClientError::invalid_param(ctx, "id");

        assert_eq!(err.category(), ErrorCategory::Config);
        assert_eq!(err.endpoint(), "Ping");
        assert_eq!(*err.method(), http::Method::GET);
        assert_eq!(err.redacted_url(), None);
    }

    #[test]
    fn decode_accessors_include_status_and_content_type() {
        let ctx = ErrorContext {
            endpoint: "GetUser",
            method: http::Method::GET,
        };
        let err = ApiClientError::decode_error(
            ctx,
            StatusCode::BAD_GATEWAY,
            Some("application/json"),
            std::io::Error::new(std::io::ErrorKind::InvalidData, "bad json"),
        );

        assert_eq!(err.category(), ErrorCategory::Decode);
        assert_eq!(err.decode_status(), Some(StatusCode::BAD_GATEWAY));
        assert_eq!(err.decode_content_type(), Some("application/json"));
        assert!(err.to_string().contains("GET GetUser"));
        assert!(err.to_string().contains("content-type=application/json"));
    }

    #[test]
    fn missing_credential_category_is_distinct() {
        let ctx = ErrorContext {
            endpoint: "Protected",
            method: http::Method::GET,
        };
        let err = ApiClientError::Auth {
            ctx,
            source: crate::auth::AuthError::new(
                crate::auth::AuthErrorKind::MissingCredential,
                "missing credential 'session'",
            ),
        };

        assert_eq!(err.category(), ErrorCategory::MissingCredential);
        assert!(err.to_string().contains("GET Protected"));
        assert!(err.to_string().contains("session"));
    }
}
