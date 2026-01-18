use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD as B64;
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
        body: String,
    },

    #[error("{ctx}: decode error: {source}")]
    Decode {
        ctx: ErrorContext,
        source: FxError,
        body: String,
    },

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

pub fn body_as_text(headers: &HeaderMap, body: &bytes::Bytes, full_len: Option<usize>) -> String {
    const MAX: usize = 8 * 1024;
    let ct = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let slice = if body.len() > MAX {
        &body[..MAX]
    } else {
        &body[..]
    };
    let total_len = full_len.unwrap_or(body.len());
    if ct.starts_with("application/json") || ct.starts_with("text/") {
        match std::str::from_utf8(slice) {
            Ok(s) => {
                if total_len > slice.len() {
                    format!("{}...", s)
                } else {
                    s.to_owned()
                }
            }
            Err(_) => format!("<non-utf8-text; {} bytes>", slice.len()),
        }
    } else {
        let b64 = B64.encode(slice);
        format!(
            "<non-text; {} bytes; base64:{}{}>",
            total_len,
            &b64[..b64.len().min(1024)],
            if b64.len() > 1024 { "..." } else { "" }
        )
    }
}
