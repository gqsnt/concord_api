use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD as B64;
use http::{HeaderMap, StatusCode};
use std::borrow::Cow;
use std::error::Error;
use std::fmt::Debug;
use thiserror::Error;

pub type FxError = Box<dyn Error + Send + Sync>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ApiClientError {
    #[error("invalid/missing param: {0}")]
    InvalidParam(&'static str),

    #[error("build url error: {0}")]
    BuildUrl(#[from] url::ParseError),

    #[error("transport: {0}")]
    Transport(#[from] crate::transport::TransportError),

    #[error("status {status}")]
    HttpStatus {
        status: StatusCode,
        headers: HeaderMap,
        body: String,
    },

    #[error("decode error: {source}")]
    Decode { source: FxError, body: String },
    #[error("HEAD response requires NoContentEncoding (endpoint={endpoint})")]
    HeadRequiresNoContent { endpoint: &'static str },
    #[error("transform error (endpoint={endpoint}): {source}")]
    Transform {
        endpoint: &'static str,
        source: FxError,
    },
    #[error(
        "status {status} has no content; endpoint must use NoContentEncoding (endpoint={endpoint})"
    )]
    NoContentStatusRequiresNoContent {
        endpoint: &'static str,
        status: StatusCode,
    },
    #[error("codec: {0}")]
    Codec(#[from] FxError),

    #[error("pagination: {0}")]
    Pagination(Cow<'static, str>),
    #[error("pagination limit reached: {0}")]
    PaginationLimit(Cow<'static, str>),

    #[error("in endpoint {endpoint}: {source}")]
    InEndpoint {
        endpoint: &'static str,
        source: Box<ApiClientError>,
    },
    #[error("policy violation: {0}")]
    PolicyViolation(&'static str),
    #[error(
        "invalid host label in endpoint {endpoint}: label[{index}]='{label}' (placeholder={placeholder:?}) reason={reason:?}"
    )]
    InvalidHostLabel {
        endpoint: &'static str,
        label: String,
        index: usize,
        placeholder: Option<&'static str>,
        reason: HostLabelInvalidReason,
    },

    #[error("controller config error: key={key} expected={expected}")]
    ControllerConfig {
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
    pub fn codec_error(error: impl Into<FxError>) -> ApiClientError {
        ApiClientError::Codec(error.into())
    }
    #[inline]
    pub fn in_endpoint(endpoint: &'static str, e: ApiClientError) -> ApiClientError {
        match e {
            ApiClientError::InEndpoint { .. } => e,
            _ => ApiClientError::InEndpoint {
                endpoint,
                source: Box::new(e),
            },
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
