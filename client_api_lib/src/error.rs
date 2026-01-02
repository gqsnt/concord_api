use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD as B64;
use http::{HeaderMap, StatusCode};
use std::error::Error;
use std::fmt::Debug;
use thiserror::Error;

pub type FxError = Box<dyn Error + Send + Sync>;

#[derive(Error, Debug)]
pub enum BuildError {
    #[error("missing required var: {0}")]
    MissingVar(&'static str),
    #[error("ambiguous type for key: {0}")]
    AmbiguousType(&'static str),
    #[error("invalid/missing param: {0}")]
    InvalidParam(&'static str),
    #[error("required query not provided: {0}")]
    MissingQuery(&'static str),
    #[error("required header not provided: {0}")]
    MissingHeader(&'static str),
}

#[derive(Error, Debug)]
pub enum ApiClientError {
    #[error("build: {0}")]
    Build(#[from] BuildError),

    #[error("build url error: {0}")]
    BuildUrl(#[from] url::ParseError),

    #[error("http: {0}")]
    Http(#[from] reqwest::Error),

    #[error("status {status}")]
    HttpStatus {
        status: StatusCode,
        headers: HeaderMap,
        body: String,
    },

    #[error("decode error: {source}")]
    Decode { source: FxError, body: String },

    #[error("codec: {0}")]
    Codec(#[from] FxError),
}

impl ApiClientError {
    pub fn codec_error(error: impl Into<FxError>) -> ApiClientError {
        ApiClientError::Codec(error.into())
    }
}

pub fn body_as_text(headers: &HeaderMap, body: &bytes::Bytes) -> String {
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

    if ct.starts_with("application/json") || ct.starts_with("text/") {
        match std::str::from_utf8(slice) {
            Ok(s) => s.to_owned(),
            Err(_) => format!("<non-utf8-text; {} bytes>", slice.len()),
        }
    } else {
        let b64 = B64.encode(slice);
        format!(
            "<non-text; {} bytes; base64:{}{}>",
            body.len(),
            &b64[..b64.len().min(1024)],
            if b64.len() > 1024 { "..." } else { "" }
        )
    }
}
