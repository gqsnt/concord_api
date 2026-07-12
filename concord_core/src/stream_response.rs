use crate::body::{BodyError, BodyErrorKind, DynBody};
use crate::codec::ContentType;
use crate::error::{ApiClientError, ErrorContext};
use crate::transport::{AttemptResponse, TransportError, TransportErrorKind};
use bytes::Bytes;
use http::{HeaderMap, StatusCode, Version, header::CONTENT_LENGTH};
use http_body_util::BodyExt as _;
use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub struct StreamResponse<M> {
    resp: AttemptResponse,
    _media: PhantomData<fn() -> M>,
}

impl<M> StreamResponse<M> {
    pub(crate) fn new(mut resp: AttemptResponse, limit: Option<usize>) -> Self {
        if let Some(limit) = limit {
            let body = std::mem::replace(resp.message.body_mut(), DynBody::empty());
            *resp.message.body_mut() = body.limited(limit as u64);
        }
        Self {
            resp,
            _media: PhantomData,
        }
    }

    pub fn meta(&self) -> &crate::transport::RequestMeta {
        &self.resp.context.meta
    }

    pub fn url(&self) -> &url::Url {
        &self.resp.context.request_url
    }

    pub fn status(&self) -> StatusCode {
        self.resp.message.status()
    }

    pub fn version(&self) -> Version {
        self.resp.message.version()
    }

    pub fn headers(&self) -> &HeaderMap {
        self.resp.message.headers()
    }

    pub fn extensions(&self) -> &http::Extensions {
        self.resp.message.extensions()
    }

    pub fn content_length(&self) -> Option<u64> {
        self.headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
    }

    pub fn rate_limit(&self) -> &crate::rate_limit::RateLimitPlan {
        &self.resp.context.rate_limit
    }

    pub fn into_body(self) -> DynBody {
        self.resp.message.into_body()
    }
}

impl<M: ContentType> StreamResponse<M> {
    pub fn media_type(&self) -> &'static str {
        M::CONTENT_TYPE
    }

    pub async fn next_chunk(&mut self) -> Result<Option<Bytes>, ApiClientError> {
        let ctx = self.error_context();
        loop {
            let Some(frame) = self
                .resp
                .message
                .body_mut()
                .frame()
                .await
                .transpose()
                .map_err(|source| Self::sanitize_body_error(ctx.clone(), source))?
            else {
                return Ok(None);
            };
            if let Ok(data) = frame.into_data() {
                return Ok(Some(data));
            }
        }
    }

    pub async fn write_to_file(&mut self, path: impl AsRef<Path>) -> Result<(), ApiClientError> {
        let ctx = self.error_context();
        let mut file = tokio::fs::File::create(path.as_ref())
            .await
            .map_err(|source| {
                Self::io_error(
                    ctx.clone(),
                    "failed to create stream response output file",
                    source,
                )
            })?;
        while let Some(chunk) = self.next_chunk().await? {
            file.write_all(&chunk).await.map_err(|source| {
                Self::io_error(
                    ctx.clone(),
                    "failed to write stream response output file",
                    source,
                )
            })?;
        }
        file.flush().await.map_err(|source| {
            Self::io_error(ctx, "failed to flush stream response output file", source)
        })?;
        Ok(())
    }
}

impl<M> StreamResponse<M> {
    fn error_context(&self) -> ErrorContext {
        ErrorContext {
            endpoint: self.resp.context.meta.endpoint,
            method: self.resp.context.meta.method.clone(),
        }
    }

    fn sanitize_body_error(ctx: ErrorContext, source: BodyError) -> ApiClientError {
        if source.kind() == BodyErrorKind::LimitExceeded {
            return ApiClientError::ResponseBodyLimitExceeded {
                ctx,
                limit: source.limit().unwrap_or_default() as usize,
            };
        }
        ApiClientError::response_body_error(ctx, source)
    }

    fn io_error(ctx: ErrorContext, msg: &'static str, source: std::io::Error) -> ApiClientError {
        let kind = if matches!(
            source.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ) {
            TransportErrorKind::Timeout
        } else {
            TransportErrorKind::Io
        };
        Self::transport_error(ctx, kind, msg)
    }

    fn transport_error(
        ctx: ErrorContext,
        kind: TransportErrorKind,
        msg: &'static str,
    ) -> ApiClientError {
        ApiClientError::Transport {
            ctx,
            source: TransportError::with_kind(kind, std::io::Error::other(msg)),
        }
    }
}

impl<M: ContentType> fmt::Debug for StreamResponse<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamResponse")
            .field("meta", self.meta())
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(self.url(), [] as [&str; 0]),
            )
            .field("status", &self.status())
            .field("version", &self.version())
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(self.headers()),
            )
            .field("content_length", &self.content_length())
            .field("rate_limit", self.rate_limit())
            .field("body", &"<stream>")
            .field("media_type", &M::CONTENT_TYPE)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_body_failures_keep_the_safe_body_kind() {
        let error = StreamResponse::<()>::sanitize_body_error(
            ErrorContext {
                endpoint: "StreamBodyFailure",
                method: http::Method::GET,
            },
            BodyError::input(),
        );
        assert!(matches!(
            error,
            ApiClientError::ResponseBody {
                kind: BodyErrorKind::Input,
                ..
            }
        ));
        assert!(!error.to_string().contains("producer"));
    }
}
