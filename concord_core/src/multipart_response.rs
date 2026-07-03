use crate::codec::CodecError;
use crate::error::{ApiClientError, ErrorContext};
use crate::multipart::MultipartFormat;
use crate::rate_limit::RateLimitPlan;
use crate::transport::{
    StreamBodyLimitError, StreamLimitDirection, TransportBody, TransportError, TransportErrorKind,
    TransportResponse,
};
use bytes::{Bytes, BytesMut};
use http::{
    HeaderMap, HeaderValue, StatusCode,
    header::{CONTENT_DISPOSITION, CONTENT_TYPE},
};
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::Mutex;

pub trait MultipartDecodePart<F: MultipartFormat>: Send + 'static {
    fn decode_headers(_headers: &HeaderMap) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_part(part: RawResponsePart) -> Result<Self, CodecError>
    where
        Self: Sized;
}

#[derive(Clone)]
struct MultipartResponseMeta {
    meta: crate::transport::RequestMeta,
    url: url::Url,
    status: StatusCode,
    headers: HeaderMap,
    content_length: Option<u64>,
    rate_limit: RateLimitPlan,
}

pub struct MultipartStream<T> {
    meta: MultipartResponseMeta,
    state: Arc<Mutex<MultipartResponseState>>,
    header_decoder: fn(&HeaderMap) -> Result<(), CodecError>,
    decoder: fn(RawResponsePart) -> Result<T, CodecError>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> MultipartStream<T> {
    pub(crate) fn new(
        resp: TransportResponse,
        boundary: String,
        response_limit: Option<usize>,
        header_decoder: fn(&HeaderMap) -> Result<(), CodecError>,
        decoder: fn(RawResponsePart) -> Result<T, CodecError>,
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
        let meta = MultipartResponseMeta {
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
        let state = MultipartResponseState::new(status, body, boundary);
        Self {
            meta,
            state: Arc::new(Mutex::new(state)),
            header_decoder,
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

    pub fn rate_limit(&self) -> &RateLimitPlan {
        &self.meta.rate_limit
    }

    pub async fn next_part(&mut self) -> Result<Option<T>, ApiClientError>
    where
        T: Send + 'static,
    {
        let ctx = self.error_context();
        let spec = {
            let mut state = self.state.lock().await;
            state.next_raw_part().await
        };
        let Some(spec) = spec.map_err(|source| Self::body_read_error(ctx.clone(), source))? else {
            return Ok(None);
        };
        let part = RawResponsePart::new(
            ctx.clone(),
            self.meta.status,
            spec.headers,
            PartBodyHandle {
                state: Arc::clone(&self.state),
                token: spec.token,
                finished: false,
            },
        );
        if let Err(source) = (self.header_decoder)(part.headers()) {
            self.finish().await;
            return Err(Self::decode_error(ctx, source));
        }
        match (self.decoder)(part) {
            Ok(value) => Ok(Some(value)),
            Err(source) => {
                self.finish().await;
                Err(Self::decode_error(ctx, source))
            }
        }
    }

    fn error_context(&self) -> ErrorContext {
        ErrorContext {
            endpoint: self.meta.meta.endpoint,
            method: self.meta.meta.method.clone(),
        }
    }

    fn decode_error(ctx: ErrorContext, _source: CodecError) -> ApiClientError {
        ApiClientError::Codec {
            ctx,
            source: Box::new(CodecError::new("multipart response decode failed")),
        }
    }

    fn body_read_error(ctx: ErrorContext, source: TransportError) -> ApiClientError {
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
                std::io::Error::other("multipart response body read failed"),
            ),
        }
    }

    async fn finish(&self) {
        let mut state = self.state.lock().await;
        state.finish();
    }
}

impl<T> fmt::Debug for MultipartStream<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MultipartStream")
            .field("meta", &self.meta.meta)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(&self.meta.url, [] as [&str; 0]),
            )
            .field("status", &self.meta.status)
            .field(
                "headers",
                &crate::debug::RedactedHeaders(&self.meta.headers),
            )
            .field("content_length", &self.meta.content_length)
            .field("rate_limit", &self.meta.rate_limit)
            .field("body", &"<multipart stream>")
            .finish()
    }
}

impl<F: MultipartFormat> MultipartDecodePart<F> for RawResponsePart {
    fn decode_part(part: RawResponsePart) -> Result<Self, CodecError> {
        Ok(part)
    }
}

pub struct RawResponsePart {
    ctx: ErrorContext,
    status: StatusCode,
    headers: HeaderMap,
    body: PartBodyHandle,
}

impl RawResponsePart {
    fn new(
        ctx: ErrorContext,
        status: StatusCode,
        headers: HeaderMap,
        body: PartBodyHandle,
    ) -> Self {
        Self {
            ctx,
            status,
            headers,
            body,
        }
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn content_type(&self) -> Option<&HeaderValue> {
        self.headers.get(CONTENT_TYPE)
    }

    pub fn content_disposition(&self) -> Option<&HeaderValue> {
        self.headers.get(CONTENT_DISPOSITION)
    }

    pub fn into_body(self) -> Box<dyn TransportBody> {
        Box::new(self.body)
    }

    pub async fn next_chunk(&mut self) -> Result<Option<Bytes>, ApiClientError> {
        let ctx = self.ctx.clone();
        self.body
            .next_chunk()
            .await
            .map_err(|source| Self::body_error(ctx, source))
    }

    pub async fn bytes_limited(self, limit: usize) -> Result<Bytes, ApiClientError> {
        let ctx = self.ctx.clone();
        let mut body = self.into_body();
        let mut out = Vec::new();
        let mut seen = 0usize;
        loop {
            match body.next_chunk().await {
                Ok(Some(chunk)) => {
                    let next_seen = seen.checked_add(chunk.len()).unwrap_or(usize::MAX);
                    if next_seen > limit {
                        return Err(ApiClientError::ResponseBodyLimitExceeded { ctx, limit });
                    }
                    seen = next_seen;
                    out.extend_from_slice(&chunk);
                }
                Ok(None) => return Ok(Bytes::from(out)),
                Err(source) => return Err(Self::body_error(ctx, source)),
            }
        }
    }

    pub async fn text_limited(self, limit: usize) -> Result<String, ApiClientError> {
        let ctx = self.ctx.clone();
        let bytes = self.bytes_limited(limit).await?;
        String::from_utf8(bytes.to_vec()).map_err(|_| ApiClientError::Codec {
            ctx,
            source: Box::new(CodecError::new("multipart response text decode failed")),
        })
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
                std::io::Error::other("multipart response body read failed"),
            ),
        }
    }
}

impl fmt::Debug for RawResponsePart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawResponsePart")
            .field("ctx", &self.ctx)
            .field("status", &self.status)
            .field("headers", &crate::debug::RedactedHeaders(&self.headers))
            .field("body", &"<stream>")
            .finish()
    }
}

struct PartBodyHandle {
    state: Arc<Mutex<MultipartResponseState>>,
    token: u64,
    finished: bool,
}

impl TransportBody for PartBodyHandle {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>,
    > {
        Box::pin(async move {
            if self.finished {
                return Ok(None);
            }
            let outcome = {
                let mut state = self.state.lock().await;
                state.read_part_chunk(self.token).await
            };
            match outcome {
                Ok(PartChunk::Data(bytes)) => Ok(Some(bytes)),
                Ok(PartChunk::Finished) => {
                    self.finished = true;
                    Ok(None)
                }
                Err(source) => {
                    self.finished = true;
                    Err(source)
                }
            }
        })
    }
}

impl fmt::Debug for PartBodyHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<stream>")
    }
}

enum PartChunk {
    Data(Bytes),
    Finished,
}

struct MultipartResponseState {
    body: Box<dyn TransportBody>,
    boundary_prefix: Bytes,
    body_boundary_prefix: Bytes,
    buffer: BytesMut,
    stage: MultipartStage,
    next_token: u64,
}

enum MultipartStage {
    SeekingFirstBoundary,
    ReadingHeaders,
    ReadingBody { token: u64, boundary_pending: bool },
    Finished,
    Failed,
}

impl MultipartResponseState {
    fn new(_status: StatusCode, body: Box<dyn TransportBody>, boundary: String) -> Self {
        Self {
            body,
            boundary_prefix: Bytes::from(format!("--{boundary}")),
            body_boundary_prefix: Bytes::from(format!("\r\n--{boundary}")),
            buffer: BytesMut::new(),
            stage: MultipartStage::SeekingFirstBoundary,
            next_token: 0,
        }
    }

    async fn next_raw_part(&mut self) -> Result<Option<RawPartSpec>, TransportError> {
        loop {
            match self.stage {
                MultipartStage::Finished => return Ok(None),
                MultipartStage::Failed => return Err(multipart_parse_error()),
                MultipartStage::SeekingFirstBoundary => match self.consume_boundary(true).await? {
                    BoundaryOutcome::Part => {
                        self.stage = MultipartStage::ReadingHeaders;
                    }
                    BoundaryOutcome::Finished => {
                        self.stage = MultipartStage::Finished;
                        return Ok(None);
                    }
                },
                MultipartStage::ReadingBody { token, .. } => {
                    self.drain_current_part(token).await?;
                }
                MultipartStage::ReadingHeaders => {
                    let headers = self.read_headers().await?;
                    let token = self.next_token;
                    self.next_token = self.next_token.checked_add(1).unwrap_or(u64::MAX);
                    self.stage = MultipartStage::ReadingBody {
                        token,
                        boundary_pending: false,
                    };
                    return Ok(Some(RawPartSpec { headers, token }));
                }
            }
        }
    }

    async fn read_part_chunk(&mut self, token: u64) -> Result<PartChunk, TransportError> {
        loop {
            match self.stage {
                MultipartStage::Finished => return Ok(PartChunk::Finished),
                MultipartStage::Failed => return Err(multipart_parse_error()),
                MultipartStage::ReadingBody {
                    token: active_token,
                    boundary_pending,
                } if active_token == token => {
                    if boundary_pending {
                        match self.consume_boundary(false).await? {
                            BoundaryOutcome::Part | BoundaryOutcome::Finished => {}
                        }
                        return Ok(PartChunk::Finished);
                    }

                    if let Some(idx) = find_subslice(&self.buffer, &self.body_boundary_prefix) {
                        if idx > 0 {
                            let chunk = self.buffer.split_to(idx).freeze();
                            self.stage = MultipartStage::ReadingBody {
                                token,
                                boundary_pending: true,
                            };
                            return Ok(PartChunk::Data(chunk));
                        }
                        match self.consume_boundary(false).await? {
                            BoundaryOutcome::Part | BoundaryOutcome::Finished => {}
                        }
                        return Ok(PartChunk::Finished);
                    }

                    if self.buffer.len() > self.body_boundary_prefix.len().saturating_sub(1) {
                        let keep = self.body_boundary_prefix.len().saturating_sub(1);
                        let take = self.buffer.len() - keep;
                        if take > 0 {
                            let chunk = self.buffer.split_to(take).freeze();
                            return Ok(PartChunk::Data(chunk));
                        }
                    }

                    if self.fill_buffer().await? == FillOutcome::Eof {
                        return Err(multipart_parse_error());
                    }
                }
                MultipartStage::ReadingHeaders | MultipartStage::SeekingFirstBoundary => {
                    return Ok(PartChunk::Finished);
                }
                MultipartStage::ReadingBody { .. } => {
                    return Ok(PartChunk::Finished);
                }
            }
        }
    }

    async fn drain_current_part(&mut self, token: u64) -> Result<(), TransportError> {
        loop {
            match self.read_part_chunk(token).await? {
                PartChunk::Data(_) => continue,
                PartChunk::Finished => return Ok(()),
            }
        }
    }

    async fn read_headers(&mut self) -> Result<HeaderMap, TransportError> {
        let terminator = Bytes::from_static(b"\r\n\r\n");
        loop {
            if let Some(idx) = find_subslice(&self.buffer, &terminator) {
                let header_block = self.buffer.split_to(idx).freeze();
                let _ = self.buffer.split_to(terminator.len());
                let headers = parse_headers(&header_block)?;
                return Ok(headers);
            }
            if self.fill_buffer().await? == FillOutcome::Eof {
                return Err(multipart_parse_error());
            }
        }
    }

    async fn consume_boundary(&mut self, initial: bool) -> Result<BoundaryOutcome, TransportError> {
        loop {
            let prefix = if initial {
                &self.boundary_prefix
            } else {
                &self.body_boundary_prefix
            };
            if !self.buffer.as_ref().starts_with(prefix.as_ref()) {
                if self.fill_buffer().await? == FillOutcome::Eof {
                    return Err(multipart_parse_error());
                }
                continue;
            }

            let mut consumed = prefix.len();
            let remainder = &self.buffer[consumed..];
            if remainder.starts_with(b"--") {
                if remainder.len() < 4 {
                    if self.fill_buffer().await? == FillOutcome::Eof {
                        return Err(multipart_parse_error());
                    }
                    continue;
                }
                if &remainder[2..4] != b"\r\n" {
                    self.finish_with_parse_error();
                    return Err(multipart_parse_error());
                }
                consumed += 4;
                let _ = self.buffer.split_to(consumed);
                self.consume_epilogue()?;
                self.stage = MultipartStage::Finished;
                return Ok(BoundaryOutcome::Finished);
            }

            if remainder.len() < 2 {
                if self.fill_buffer().await? == FillOutcome::Eof {
                    return Err(multipart_parse_error());
                }
                continue;
            }
            if &remainder[..2] != b"\r\n" {
                self.finish_with_parse_error();
                return Err(multipart_parse_error());
            }
            consumed += 2;
            let _ = self.buffer.split_to(consumed);
            self.stage = MultipartStage::ReadingHeaders;
            return Ok(BoundaryOutcome::Part);
        }
    }

    fn consume_epilogue(&mut self) -> Result<(), TransportError> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        if self
            .buffer
            .iter()
            .all(|byte| matches!(byte, b'\r' | b'\n' | b'\t' | b' '))
        {
            self.buffer.clear();
            return Ok(());
        }
        self.finish_with_parse_error();
        Err(multipart_parse_error())
    }

    fn stage_is_finished(&self) -> bool {
        matches!(self.stage, MultipartStage::Finished)
    }

    fn finish_with_parse_error(&mut self) {
        self.stage = MultipartStage::Failed;
        self.buffer.clear();
    }

    async fn fill_buffer(&mut self) -> Result<FillOutcome, TransportError> {
        if self.stage_is_finished() {
            return Ok(FillOutcome::Eof);
        }
        match self.body.next_chunk().await {
            Ok(Some(chunk)) => {
                self.buffer.extend_from_slice(&chunk);
                Ok(FillOutcome::More)
            }
            Ok(None) => {
                self.finish_with_parse_error();
                Ok(FillOutcome::Eof)
            }
            Err(error) => {
                self.finish_with_parse_error();
                Err(error)
            }
        }
    }

    fn finish(&mut self) {
        self.stage = MultipartStage::Finished;
        self.buffer.clear();
    }
}

enum BoundaryOutcome {
    Part,
    Finished,
}

#[derive(PartialEq, Eq)]
enum FillOutcome {
    More,
    Eof,
}

struct RawPartSpec {
    headers: HeaderMap,
    token: u64,
}

fn parse_headers(block: &Bytes) -> Result<HeaderMap, TransportError> {
    let text = std::str::from_utf8(block).map_err(|_| multipart_parse_error())?;
    let mut headers = HeaderMap::new();
    if text.is_empty() {
        return Ok(headers);
    }
    for line in text.split("\r\n") {
        if line.trim().is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(multipart_parse_error());
        };
        let name = http::header::HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| multipart_parse_error())?;
        let value = HeaderValue::from_str(value.trim()).map_err(|_| multipart_parse_error())?;
        headers.append(name, value);
    }
    Ok(headers)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn multipart_parse_error() -> TransportError {
    TransportError::with_kind(
        TransportErrorKind::Other,
        std::io::Error::other("multipart response parse failed"),
    )
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

impl fmt::Debug for LimitedTransportBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<stream>")
    }
}

pub(crate) fn parse_response_boundary<F: MultipartFormat>(
    headers: &HeaderMap,
    ctx: ErrorContext,
) -> Result<String, ApiClientError> {
    let Some(value) = headers.get(CONTENT_TYPE) else {
        return Err(ApiClientError::response_contract(
            ctx,
            "multipart response content type is missing a boundary parameter",
        ));
    };
    let text = value.to_str().map_err(|_| {
        ApiClientError::response_contract(
            ctx.clone(),
            "multipart response content type did not match expected media type",
        )
    })?;
    let mut segments = text.split(';');
    let base = segments.next().unwrap_or_default().trim();
    if !base.eq_ignore_ascii_case(F::CONTENT_TYPE) {
        return Err(ApiClientError::response_contract(
            ctx,
            "multipart response content type did not match expected media type",
        ));
    }
    for param in segments {
        let param = param.trim();
        if let Some(value) = param.strip_prefix("boundary=") {
            let boundary = parse_boundary_value(value).map_err(|_| {
                ApiClientError::response_contract(
                    ctx.clone(),
                    "multipart response boundary is invalid",
                )
            })?;
            return Ok(boundary);
        }
    }
    Err(ApiClientError::response_contract(
        ctx,
        "multipart response content type is missing a boundary parameter",
    ))
}

fn parse_boundary_value(value: &str) -> Result<String, ()> {
    let value = value.trim();
    let value = if let Some(stripped) = value.strip_prefix('"') {
        let Some(stripped) = stripped.strip_suffix('"') else {
            return Err(());
        };
        stripped
    } else {
        value
    };
    if value.is_empty() || !value.is_ascii() {
        return Err(());
    }
    if value
        .bytes()
        .any(|byte| matches!(byte, b'\r' | b'\n' | 0x00..=0x1f | 0x7f | b'"'))
    {
        return Err(());
    }
    Ok(value.to_string())
}
