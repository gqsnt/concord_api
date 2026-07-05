use crate::codec::CodecError;
use crate::error::{ApiClientError, ErrorContext};
use crate::transport::{
    StreamBodyLimitError, StreamLimitDirection, TransportBody, TransportError, TransportErrorKind,
    TransportResponse,
};
use bytes::{Bytes, BytesMut};
use http::{HeaderMap, StatusCode};
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::time::Duration;

pub trait SseCodec<T>: Send + Sync + 'static {
    fn decode_event(event: SseRawEvent) -> Result<SseEvent<T>, CodecError>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SseRawEvent {
    pub event: Option<String>,
    pub id: Option<String>,
    pub retry: Option<Duration>,
    pub data: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent<T> {
    pub event: Option<String>,
    pub id: Option<String>,
    pub retry: Option<Duration>,
    pub data: T,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JsonSse;

impl<T> SseCodec<T> for JsonSse
where
    T: serde::de::DeserializeOwned + Send + 'static,
{
    fn decode_event(event: SseRawEvent) -> Result<SseEvent<T>, CodecError> {
        serde_json::from_str(&event.data)
            .map(|data| SseEvent {
                event: event.event,
                id: event.id,
                retry: event.retry,
                data,
            })
            .map_err(|_| CodecError::new("sse event decode failed"))
    }
}

#[derive(Clone)]
struct SseResponseMeta {
    meta: crate::transport::RequestMeta,
    url: url::Url,
    status: StatusCode,
    headers: HeaderMap,
    content_length: Option<u64>,
    rate_limit: crate::rate_limit::RateLimitPlan,
}

pub struct SseStream<T> {
    meta: SseResponseMeta,
    state: SseStreamState,
    decoder: fn(SseRawEvent) -> Result<SseEvent<T>, CodecError>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> SseStream<T> {
    pub(crate) fn new(
        resp: TransportResponse,
        response_limit: Option<usize>,
        decoder: fn(SseRawEvent) -> Result<SseEvent<T>, CodecError>,
    ) -> Self {
        let TransportResponse {
            meta,
            url,
            status,
            headers,
            content_length,
            rate_limit,
            body,
        } = resp;
        let meta = SseResponseMeta {
            meta: meta.clone(),
            url,
            status,
            headers,
            content_length,
            rate_limit,
        };
        let body = Box::new(LimitedTransportBody::new(
            body,
            meta.meta.clone(),
            response_limit,
        ));
        Self {
            meta,
            state: SseStreamState::new(body),
            decoder,
            _marker: PhantomData,
        }
    }

    pub fn meta(&self) -> &crate::transport::RequestMeta {
        &self.meta.meta
    }

    pub fn url(&self) -> &url::Url {
        &self.meta.url
    }

    pub fn status(&self) -> StatusCode {
        self.meta.status
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.meta.headers
    }

    pub fn content_length(&self) -> Option<u64> {
        self.meta.content_length
    }

    pub fn rate_limit(&self) -> &crate::rate_limit::RateLimitPlan {
        &self.meta.rate_limit
    }
}

impl<T: Send + 'static> SseStream<T> {
    pub async fn next_event(&mut self) -> Result<Option<SseEvent<T>>, ApiClientError> {
        let ctx = self.error_context();
        loop {
            if self.state.finished {
                return Ok(None);
            }

            if let Some(line) = self.state.take_line() {
                let decoded = match self.state.apply_line(line) {
                    Ok(decoded) => decoded,
                    Err(source) => {
                        self.state.finished = true;
                        return Err(Self::codec_error(ctx.clone(), source));
                    }
                };
                if let Some(raw) = decoded {
                    let event = match (self.decoder)(raw) {
                        Ok(event) => event,
                        Err(source) => {
                            self.state.finished = true;
                            return Err(Self::codec_error(ctx.clone(), source));
                        }
                    };
                    return Ok(Some(event));
                }
                continue;
            }

            if self.state.eof_seen {
                if let Some(raw) = self.state.finish_on_eof() {
                    let event = match (self.decoder)(raw) {
                        Ok(event) => event,
                        Err(source) => {
                            self.state.finished = true;
                            return Err(Self::codec_error(ctx, source));
                        }
                    };
                    return Ok(Some(event));
                }
                return Ok(None);
            }

            match self.state.body.next_chunk().await {
                Ok(Some(chunk)) => self.state.buffer.extend_from_slice(&chunk),
                Ok(None) => self.state.eof_seen = true,
                Err(source) => {
                    self.state.finished = true;
                    return Err(Self::body_error(ctx, source));
                }
            }
        }
    }
}

impl<T> SseStream<T> {
    fn error_context(&self) -> ErrorContext {
        ErrorContext {
            endpoint: self.meta.meta.endpoint,
            method: self.meta.meta.method.clone(),
        }
    }

    fn codec_error(ctx: ErrorContext, _source: CodecError) -> ApiClientError {
        ApiClientError::codec_error(ctx, Box::new(CodecError::new("sse event decode failed")))
    }

    fn body_error(ctx: ErrorContext, source: TransportError) -> ApiClientError {
        if let Some(limit_error) = source.source_error().downcast_ref::<StreamBodyLimitError>() {
            if matches!(limit_error.direction, StreamLimitDirection::Response) {
                return ApiClientError::ResponseBodyLimitExceeded {
                    ctx,
                    limit: limit_error.limit,
                };
            }
        }
        ApiClientError::Transport {
            ctx,
            source: TransportError::with_kind(
                source.kind(),
                std::io::Error::other("sse response body read failed"),
            ),
        }
    }
}

impl<T> fmt::Debug for SseStream<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SseStream")
            .field("meta", &self.meta.meta)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(&self.meta.url, [] as [&str; 0]),
            )
            .field("status", &self.meta.status)
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(&self.meta.headers),
            )
            .field("content_length", &self.meta.content_length)
            .field("rate_limit", &self.meta.rate_limit)
            .field("body", &"<sse stream>")
            .finish()
    }
}

struct SseStreamState {
    body: Box<dyn TransportBody>,
    buffer: BytesMut,
    current: SseEventBuilder,
    eof_seen: bool,
    finished: bool,
}

impl SseStreamState {
    fn new(body: Box<dyn TransportBody>) -> Self {
        Self {
            body,
            buffer: BytesMut::new(),
            current: SseEventBuilder::default(),
            eof_seen: false,
            finished: false,
        }
    }

    fn take_line(&mut self) -> Option<Bytes> {
        if let Some(pos) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.buffer.split_to(pos + 1);
            line.truncate(pos);
            if line.last().is_some_and(|byte| *byte == b'\r') {
                line.truncate(line.len().saturating_sub(1));
            }
            return Some(line.freeze());
        }
        if self.eof_seen {
            if self.buffer.is_empty() {
                return None;
            }
            let mut line = self.buffer.split_to(self.buffer.len());
            if line.last().is_some_and(|byte| *byte == b'\r') {
                line.truncate(line.len().saturating_sub(1));
            }
            return Some(line.freeze());
        }
        None
    }

    fn apply_line(&mut self, line: Bytes) -> Result<Option<SseRawEvent>, CodecError> {
        let line = String::from_utf8(line.to_vec())
            .map_err(|_| CodecError::new("sse event decode failed"))?;
        if line.is_empty() {
            return Ok(self.finish_current_event());
        }
        if line.starts_with(':') {
            return Ok(None);
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line.as_str(), ""),
        };
        match field {
            "data" => {
                self.current.has_data = true;
                self.current.data.push_back(value.to_string());
            }
            "event" => self.current.event = Some(value.to_string()),
            "id" => self.current.id = Some(value.to_string()),
            "retry" => {
                let millis = value
                    .parse::<u64>()
                    .map_err(|_| CodecError::new("sse event decode failed"))?;
                self.current.retry = Some(Duration::from_millis(millis));
            }
            _ => {}
        }
        Ok(None)
    }

    fn finish_current_event(&mut self) -> Option<SseRawEvent> {
        let raw = self.current.finish();
        if raw.is_none() {
            self.current = SseEventBuilder::default();
            return None;
        }
        self.current = SseEventBuilder::default();
        raw
    }

    fn finish_on_eof(&mut self) -> Option<SseRawEvent> {
        self.finished = true;
        self.finish_current_event()
    }
}

#[derive(Default)]
struct SseEventBuilder {
    event: Option<String>,
    id: Option<String>,
    retry: Option<Duration>,
    data: VecDeque<String>,
    has_data: bool,
}

impl SseEventBuilder {
    fn finish(&mut self) -> Option<SseRawEvent> {
        if !self.has_data {
            return None;
        }
        let data = self.data.drain(..).collect::<Vec<_>>().join("\n");
        Some(SseRawEvent {
            event: self.event.take(),
            id: self.id.take(),
            retry: self.retry.take(),
            data,
        })
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
