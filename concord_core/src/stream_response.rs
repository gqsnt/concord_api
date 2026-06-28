use crate::error::{ApiClientError, ErrorContext};
use crate::media::MediaType;
use crate::transport::{
    StreamBodyLimitError, StreamLimitDirection, TransportBody, TransportError, TransportErrorKind,
    TransportResponse,
};
use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub struct StreamResponse<M> {
    resp: TransportResponse,
    _media: PhantomData<fn() -> M>,
}

impl<M> StreamResponse<M> {
    pub(crate) fn new(resp: TransportResponse, limit: Option<usize>) -> Self {
        let TransportResponse {
            meta,
            url,
            status,
            headers,
            content_length,
            rate_limit,
            body,
        } = resp;
        let resp = TransportResponse {
            body: Box::new(LimitedTransportBody::new(body, meta.clone(), limit)),
            meta,
            url,
            status,
            headers,
            content_length,
            rate_limit,
        };
        Self {
            resp,
            _media: PhantomData,
        }
    }

    pub fn meta(&self) -> &crate::transport::RequestMeta {
        &self.resp.meta
    }

    pub fn url(&self) -> &url::Url {
        &self.resp.url
    }

    pub fn status(&self) -> StatusCode {
        self.resp.status
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.resp.headers
    }

    pub fn content_length(&self) -> Option<u64> {
        self.resp.content_length
    }

    pub fn rate_limit(&self) -> &crate::rate_limit::RateLimitPlan {
        &self.resp.rate_limit
    }

    pub fn into_body(self) -> Box<dyn TransportBody> {
        self.resp.body
    }

    pub(crate) fn into_transport_response(self) -> TransportResponse {
        self.resp
    }
}

impl<M: MediaType> StreamResponse<M> {
    pub fn media_type(&self) -> &'static str {
        M::CONTENT_TYPE
    }

    pub async fn next_chunk(&mut self) -> Result<Option<Bytes>, ApiClientError> {
        let ctx = self.error_context();
        self.resp
            .body
            .next_chunk()
            .await
            .map_err(|source| Self::sanitize_body_error(ctx, source))
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
            endpoint: self.resp.meta.endpoint,
            method: self.resp.meta.method.clone(),
        }
    }

    fn sanitize_body_error(ctx: ErrorContext, source: TransportError) -> ApiClientError {
        if let Some(limit_error) = source.source_error().downcast_ref::<StreamBodyLimitError>() {
            if matches!(limit_error.direction, StreamLimitDirection::Response) {
                return ApiClientError::ResponseBodyLimitExceeded {
                    ctx,
                    limit: limit_error.limit,
                };
            }
        }
        Self::transport_error(ctx, source.kind(), "stream response body read failed")
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

impl<M: MediaType> fmt::Debug for StreamResponse<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamResponse")
            .field("meta", &self.resp.meta)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(&self.resp.url, [] as [&str; 0]),
            )
            .field("status", &self.resp.status)
            .field(
                "headers",
                &crate::debug::RedactedHeaders(&self.resp.headers),
            )
            .field("content_length", &self.resp.content_length)
            .field("rate_limit", &self.resp.rate_limit)
            .field("body", &"<stream>")
            .field("media_type", &M::CONTENT_TYPE)
            .finish()
    }
}

struct LimitedTransportBody {
    body: Box<dyn TransportBody>,
    limit: Option<usize>,
    seen: usize,
    meta: crate::transport::RequestMeta,
    exhausted: bool,
}

impl LimitedTransportBody {
    fn new(
        body: Box<dyn TransportBody>,
        meta: crate::transport::RequestMeta,
        limit: Option<usize>,
    ) -> Self {
        Self {
            body,
            limit,
            seen: 0,
            meta,
            exhausted: false,
        }
    }
}

impl TransportBody for LimitedTransportBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>,
    > {
        Box::pin(async move {
            if self.exhausted {
                return Ok(None);
            }
            let Some(chunk) = self.body.next_chunk().await? else {
                self.exhausted = true;
                return Ok(None);
            };
            if let Some(limit) = self.limit {
                let next_seen = self.seen.checked_add(chunk.len()).unwrap_or(usize::MAX);
                if next_seen > limit {
                    self.exhausted = true;
                    return Err(TransportError::with_kind(
                        TransportErrorKind::Request,
                        StreamBodyLimitError {
                            meta: self.meta.clone(),
                            direction: StreamLimitDirection::Response,
                            limit,
                            seen: next_seen,
                        },
                    ));
                }
                self.seen = next_seen;
            }
            Ok(Some(chunk))
        })
    }
}
