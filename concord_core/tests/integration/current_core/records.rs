use super::common::{TestAuthVars, TestCx, auth_policy};
use bytes::Bytes;
use concord_core::advanced::{
    AuthPlacement, CodecError, ContentType, DebugSink, NdJson, PostResponseHookContext,
    PreSendHookContext, RateLimitContext, RateLimitFuture, RateLimitPermit,
    RateLimitResponseAction, RateLimitResponseContext, RateLimiter, RecordBody, RecordDecoder,
    RecordEncoder, RecordFormat, RuntimeHooks, Transport, TransportBody, TransportError,
    TransportErrorKind, TransportRequest, TransportResponse,
};
#[cfg(feature = "records-csv")]
use concord_core::advanced::{Csv, CsvCommaDelim, CsvConfig, CsvSemicolonDelim, CsvTabDelim};
use concord_core::internal::{
    BodyPlan, EndpointMeta, EndpointPlan, RequestArgs, RequestOverrides, RequestPlan,
    ResolvedPolicy, ResolvedRoute, ResponsePlan,
};
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
use futures_core::Stream;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context, Poll};

fn request_replayability(body: &BodyPlan) -> concord_core::internal::Replayability {
    match body {
        BodyPlan::None | BodyPlan::Encoded { .. } => {
            concord_core::internal::Replayability::Replayable
        }
        BodyPlan::RawStream { .. } | BodyPlan::Multipart { .. } | BodyPlan::Records { .. } => {
            concord_core::internal::Replayability::NonReplayable
        }
    }
}

#[derive(Clone)]
struct RecordingDebugSink {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingDebugSink {
    fn new(events: Arc<StdMutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl DebugSink for RecordingDebugSink {
    fn request_start(
        &self,
        dbg: concord_core::prelude::DebugLevel,
        _method: &Method,
        _url: &str,
        endpoint: &'static str,
        page_index: u32,
    ) {
        self.events
            .lock()
            .expect("debug lock")
            .push(format!("debug_request:{dbg}:{endpoint}:{page_index}"));
    }

    fn request_headers(
        &self,
        dbg: concord_core::prelude::DebugLevel,
        _headers: concord_core::advanced::SanitizedHeaders<'_>,
    ) {
        self.events
            .lock()
            .expect("debug lock")
            .push(format!("debug_request_headers:{dbg}"));
    }

    fn response_status(
        &self,
        dbg: concord_core::prelude::DebugLevel,
        status: StatusCode,
        _url: &str,
        ok: bool,
    ) {
        self.events
            .lock()
            .expect("debug lock")
            .push(format!("debug_response:{dbg}:{status}:{ok}"));
    }

    fn response_headers(
        &self,
        dbg: concord_core::prelude::DebugLevel,
        _headers: concord_core::advanced::SanitizedHeaders<'_>,
    ) {
        self.events
            .lock()
            .expect("debug lock")
            .push(format!("debug_response_headers:{dbg}"));
    }
}

#[derive(Clone)]
struct RecordingHooks {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingHooks {
    fn new(events: Arc<StdMutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RuntimeHooks for RecordingHooks {
    fn pre_send<'a>(
        &'a self,
        ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("hooks lock")
                .push(format!("hook_pre_send:{}", ctx.meta.endpoint));
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("hooks lock")
                .push(format!("hook_post_response:{}", ctx.meta.endpoint));
        })
    }

    fn transport_error<'a>(
        &'a self,
        ctx: concord_core::advanced::TransportErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("hooks lock")
                .push(format!("hook_transport_error:{}", ctx.meta.endpoint));
        })
    }
}

#[derive(Clone)]
struct RecordingRateLimiter {
    events: Arc<StdMutex<Vec<String>>>,
}

impl RecordingRateLimiter {
    fn new(events: Arc<StdMutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

impl RateLimiter for RecordingRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("rate limit lock")
                .push("rate_limit_acquire".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events
                .lock()
                .expect("rate limit lock")
                .push("rate_limit_response".to_string());
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CapturedBody {
    Empty,
    Bytes(Bytes),
    Stream(Bytes),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedRequest {
    debug: String,
    content_type: Option<String>,
    body: CapturedBody,
}

#[derive(Clone, Debug)]
struct ResponseFixture {
    status: StatusCode,
    headers: HeaderMap,
    chunks: Vec<Bytes>,
    content_length: Option<u64>,
    poll_flag: Option<Arc<AtomicBool>>,
}

impl ResponseFixture {
    fn buffered_json(body: &'static str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Self {
            status: StatusCode::OK,
            headers,
            chunks: vec![Bytes::from_static(body.as_bytes())],
            content_length: Some(body.len() as u64),
            poll_flag: None,
        }
    }

    fn streamed_with_content_type(
        status: StatusCode,
        content_type: &'static str,
        chunks: Vec<Bytes>,
    ) -> Self {
        let content_length = chunks
            .iter()
            .try_fold(0u64, |len, chunk| len.checked_add(chunk.len() as u64));
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static(content_type),
        );
        Self {
            status,
            headers,
            chunks,
            content_length,
            poll_flag: None,
        }
    }

    fn ndjson(status: StatusCode, chunks: Vec<Bytes>) -> Self {
        Self::streamed_with_content_type(status, NdJson::CONTENT_TYPE, chunks)
    }

    fn with_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.poll_flag = Some(flag);
        self
    }

    fn content_length(mut self, value: Option<u64>) -> Self {
        self.content_length = value;
        self
    }
}

#[derive(Clone)]
struct RecordTransport {
    events: Arc<StdMutex<Vec<String>>>,
    captured: Arc<StdMutex<Vec<CapturedRequest>>>,
    responses: Arc<StdMutex<VecDeque<ResponseFixture>>>,
    transport_error: Option<TransportErrorKind>,
    send_count: Arc<AtomicUsize>,
}

impl RecordTransport {
    fn new(events: Arc<StdMutex<Vec<String>>>, responses: Vec<ResponseFixture>) -> Self {
        Self {
            events,
            captured: Arc::new(StdMutex::new(Vec::new())),
            responses: Arc::new(StdMutex::new(responses.into())),
            transport_error: None,
            send_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn transport_error(
        events: Arc<StdMutex<Vec<String>>>,
        responses: Vec<ResponseFixture>,
        kind: TransportErrorKind,
    ) -> Self {
        Self {
            events,
            captured: Arc::new(StdMutex::new(Vec::new())),
            responses: Arc::new(StdMutex::new(responses.into())),
            transport_error: Some(kind),
            send_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn send_count(&self) -> usize {
        self.send_count.load(Ordering::SeqCst)
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().expect("events lock").clone()
    }

    fn captured(&self) -> Vec<CapturedRequest> {
        self.captured
            .lock()
            .expect("captured requests lock")
            .clone()
    }
}

impl Transport for RecordTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let events = self.events.clone();
        let captured = self.captured.clone();
        let responses = self.responses.clone();
        let transport_error = self.transport_error;
        let send_count = self.send_count.clone();
        Box::pin(async move {
            send_count.fetch_add(1, Ordering::SeqCst);
            let debug = format!("{req:?}");
            events
                .lock()
                .expect("record events lock")
                .push("transport".to_string());
            events
                .lock()
                .expect("record events lock")
                .push(format!("transport_debug:{debug}"));
            let content_type = req
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let body = match req.body {
                concord_core::advanced::TransportRequestBody::Empty => CapturedBody::Empty,
                concord_core::advanced::TransportRequestBody::Bytes(bytes) => {
                    CapturedBody::Bytes(bytes)
                }
                concord_core::advanced::TransportRequestBody::Stream(stream) => {
                    CapturedBody::Stream(collect_stream(stream, &events).await?)
                }
            };
            captured
                .lock()
                .expect("captured requests lock")
                .push(CapturedRequest {
                    debug,
                    content_type,
                    body,
                });
            if let Some(kind) = transport_error {
                return Err(TransportError::with_kind(
                    kind,
                    std::io::Error::other("record transport failure"),
                ));
            }
            let mut responses = responses.lock().expect("responses lock");
            let response = responses.pop_front().ok_or_else(|| {
                TransportError::with_kind(
                    TransportErrorKind::Other,
                    std::io::Error::other("record transport exhausted"),
                )
            })?;
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: response.status,
                headers: response.headers,
                content_length: response.content_length,
                rate_limit: req.rate_limit,
                body: Box::new(ChunkBody::new(
                    events.clone(),
                    response.chunks,
                    response.poll_flag,
                )),
            })
        })
    }
}

struct ChunkBody {
    events: Arc<StdMutex<Vec<String>>>,
    chunks: VecDeque<Bytes>,
    poll_flag: Option<Arc<AtomicBool>>,
}

impl ChunkBody {
    fn new(
        events: Arc<StdMutex<Vec<String>>>,
        chunks: Vec<Bytes>,
        poll_flag: Option<Arc<AtomicBool>>,
    ) -> Self {
        Self {
            events,
            chunks: chunks.into(),
            poll_flag,
        }
    }
}

impl TransportBody for ChunkBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        let events = self.events.clone();
        let poll_flag = self.poll_flag.clone();
        let chunk = self.chunks.pop_front();
        Box::pin(async move {
            if let Some(flag) = poll_flag {
                flag.store(true, Ordering::SeqCst);
            }
            events
                .lock()
                .expect("events lock")
                .push("record_chunk_poll".to_string());
            Ok(chunk)
        })
    }
}

struct PollFlagRecordStream {
    polled: Arc<AtomicBool>,
    item: Option<RecordItem>,
}

impl PollFlagRecordStream {
    fn new(polled: Arc<AtomicBool>, item: RecordItem) -> Self {
        Self {
            polled,
            item: Some(item),
        }
    }
}

impl Stream for PollFlagRecordStream {
    type Item = Result<RecordItem, concord_core::advanced::CodecError>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polled.store(true, Ordering::SeqCst);
        Poll::Ready(self.item.take().map(Ok))
    }
}

struct ErrorRecordStream {
    item: Option<Result<RecordItem, concord_core::advanced::CodecError>>,
}

impl ErrorRecordStream {
    fn new() -> Self {
        Self {
            item: Some(Err(concord_core::advanced::CodecError::new(
                "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR",
            ))),
        }
    }
}

impl Stream for ErrorRecordStream {
    type Item = Result<RecordItem, concord_core::advanced::CodecError>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.item.take())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct RecordItem {
    id: u32,
}

fn pipe_record_bytes(entries: &[PipeRecord]) -> Bytes {
    let mut out = String::new();
    for entry in entries {
        out.push_str(&format!("{}|{}\n", entry.id, entry.message));
    }
    Bytes::from(out)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct PipeRecord {
    id: u64,
    message: String,
}

const PIPE_RECORD_SENTINEL: &str = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR";

#[derive(Clone, Copy, Debug, Default)]
struct PipeText;

impl ContentType for PipeText {
    const CONTENT_TYPE: &'static str = "text/x-pipe-records";
}

struct PipeTextEncoder;

impl RecordEncoder<PipeRecord> for PipeTextEncoder {
    fn encode_record(&mut self, value: PipeRecord) -> Result<Bytes, CodecError> {
        if value.message.contains('|')
            || value.message.contains('\n')
            || value.message.contains('\r')
        {
            return Err(CodecError::new(PIPE_RECORD_SENTINEL));
        }
        Ok(Bytes::from(format!("{}|{}\n", value.id, value.message)))
    }
}

#[derive(Default)]
struct PipeTextDecoder {
    buffer: Vec<u8>,
}

impl PipeTextDecoder {
    fn decode_line(&self, line: &[u8]) -> Result<PipeRecord, CodecError> {
        let text = std::str::from_utf8(line).map_err(|_| CodecError::new(PIPE_RECORD_SENTINEL))?;
        if text.is_empty() || text.contains('\r') {
            return Err(CodecError::new(PIPE_RECORD_SENTINEL));
        }
        let mut parts = text.split('|');
        let id = parts
            .next()
            .ok_or_else(|| CodecError::new(PIPE_RECORD_SENTINEL))?;
        let message = parts
            .next()
            .ok_or_else(|| CodecError::new(PIPE_RECORD_SENTINEL))?;
        if parts.next().is_some() || id.is_empty() {
            return Err(CodecError::new(PIPE_RECORD_SENTINEL));
        }
        let id = id
            .parse::<u64>()
            .map_err(|_| CodecError::new(PIPE_RECORD_SENTINEL))?;
        Ok(PipeRecord {
            id,
            message: message.to_string(),
        })
    }

    fn parse_available(&mut self, finalizing: bool) -> Result<Vec<PipeRecord>, CodecError> {
        let mut out = Vec::new();
        while let Some(pos) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=pos).collect();
            line.pop();
            out.push(self.decode_line(&line)?);
        }
        if finalizing && !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            out.push(self.decode_line(&line)?);
        }
        Ok(out)
    }
}

impl RecordDecoder<PipeRecord> for PipeTextDecoder {
    fn push_chunk(&mut self, chunk: Bytes) -> Result<Vec<PipeRecord>, CodecError> {
        self.buffer.extend_from_slice(&chunk);
        self.parse_available(false)
    }

    fn finish(&mut self) -> Result<Vec<PipeRecord>, CodecError> {
        self.parse_available(true)
    }
}

impl RecordFormat<PipeRecord> for PipeText {
    fn encoder() -> Box<dyn RecordEncoder<PipeRecord>> {
        Box::new(PipeTextEncoder)
    }

    fn decoder() -> Box<dyn RecordDecoder<PipeRecord>> {
        Box::new(PipeTextDecoder::default())
    }
}

fn record_request_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: BodyPlan,
    args: RequestArgs,
    accept: &'static str,
) -> RequestPlan {
    let replayability = request_replayability(&body);
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method: method.clone(),
                idempotent: matches!(method, Method::GET | Method::HEAD),
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            body,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static(accept)),
                no_content: false,
                format: concord_core::internal::Format::Text,
            },
            pagination: None,
        },
        args,
        overrides: RequestOverrides::default(),
        replayability,
    }
}

fn record_response_plan_with_accept(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: BodyPlan,
    args: RequestArgs,
    accept: &'static str,
) -> RequestPlan {
    let replayability = request_replayability(&body);
    RequestPlan {
        endpoint: EndpointPlan {
            meta: EndpointMeta {
                name,
                method: method.clone(),
                idempotent: matches!(method, Method::GET | Method::HEAD),
                facade_path: &[],
            },
            route: ResolvedRoute::new(http::uri::Scheme::HTTPS, "example.com", path),
            policy,
            body,
            response: ResponsePlan {
                accept: Some(HeaderValue::from_static(accept)),
                no_content: false,
                format: concord_core::internal::Format::Text,
            },
            pagination: None,
        },
        args,
        overrides: RequestOverrides::default(),
        replayability,
    }
}

fn record_response_plan(
    name: &'static str,
    method: Method,
    path: &'static str,
    policy: ResolvedPolicy,
    body: BodyPlan,
    args: RequestArgs,
) -> RequestPlan {
    record_response_plan_with_accept(name, method, path, policy, body, args, NdJson::CONTENT_TYPE)
}

fn record_retry_policy() -> ResolvedPolicy {
    ResolvedPolicy {
        retry: concord_core::internal::RetrySetting::Config(concord_core::advanced::RetryConfig {
            max_attempts: 2,
            methods: Vec::new(),
            statuses: Vec::new(),
            transport_errors: vec![TransportErrorKind::Other],
            backoff: concord_core::advanced::RetryBackoff::None,
            respect_retry_after: false,
            idempotency: concord_core::advanced::RetryIdempotency::SafeMethodsOnly,
        }),
        ..Default::default()
    }
}

async fn collect_stream(
    mut stream: concord_core::advanced::TransportByteStream,
    events: &Arc<StdMutex<Vec<String>>>,
) -> Result<Bytes, TransportError> {
    let mut out = Vec::new();
    loop {
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        match next {
            Some(Ok(chunk)) => {
                events
                    .lock()
                    .expect("record transport events lock")
                    .push("record_request_poll".to_string());
                out.extend_from_slice(&chunk);
            }
            Some(Err(error)) => return Err(error),
            None => break,
        }
    }
    Ok(Bytes::from(out))
}

#[tokio::test]
async fn ndjson_record_request_reaches_transport_and_is_body_free_in_debug()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":10}\n")],
        )],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    #[derive(Serialize, Deserialize)]
    struct SensitiveRecord {
        msg: String,
    }

    let sentinel = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR".to_string();
    let plan = record_request_plan(
        "RecordRequest",
        Method::POST,
        "/records",
        ResolvedPolicy::default(),
        BodyPlan::Records {
            content_type: HeaderValue::from_static(NdJson::CONTENT_TYPE),
            format: concord_core::internal::Format::Text,
        },
        RequestArgs::with_record_body::<SensitiveRecord, NdJson>(RecordBody::from_iter(vec![
            SensitiveRecord {
                msg: sentinel.clone(),
            },
        ])),
        NdJson::CONTENT_TYPE,
    );

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await?;
    assert_eq!(decoded.into_value(), "{\"id\":10}\n");
    assert_eq!(transport.send_count(), 1);
    let captured = transport.captured();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].content_type.as_deref(),
        Some(NdJson::CONTENT_TYPE)
    );
    match &captured[0].body {
        CapturedBody::Stream(bytes) => {
            assert_eq!(bytes, &Bytes::from(format!("{{\"msg\":\"{sentinel}\"}}\n")))
        }
        other => panic!("expected streamed record body, got {other:?}"),
    }
    assert!(!captured[0].debug.contains(&sentinel));
    let rendered = captured
        .iter()
        .map(|request| format!("{} {:?}", request.debug, request.content_type))
        .collect::<Vec<_>>()
        .join("|");
    assert!(!rendered.contains(&sentinel));
    Ok(())
}

#[tokio::test]
async fn record_request_is_not_polled_before_auth_collision_validation() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":1}\n")],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars {
            token: Some("token".to_string()),
            identity: "anon",
        },
        transport.clone(),
    );
    let mut policy = auth_policy(AuthPlacement::Bearer);
    policy.headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_static("public"),
    );

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(record_request_plan(
            "RecordAuthCollision",
            Method::POST,
            "/record-auth-collision",
            policy,
            BodyPlan::Records {
                content_type: HeaderValue::from_static(NdJson::CONTENT_TYPE),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_record_body::<RecordItem, NdJson>(RecordBody::from_stream(
                PollFlagRecordStream::new(polled.clone(), RecordItem { id: 1 }),
            )),
            NdJson::CONTENT_TYPE,
        ))
        .await
        .expect_err("auth collision should fail before transport");

    assert!(matches!(err, ApiClientError::Auth { .. }));
    assert_eq!(transport.send_count(), 0);
    assert!(!polled.load(Ordering::SeqCst));
}

#[tokio::test]
async fn record_request_is_not_polled_before_rate_limit_acquisition() -> Result<(), ApiClientError>
{
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":1}\n")],
        )],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(record_request_plan(
            "RecordOrdering",
            Method::POST,
            "/record-ordering",
            ResolvedPolicy::default(),
            BodyPlan::Records {
                content_type: HeaderValue::from_static(NdJson::CONTENT_TYPE),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_record_body::<RecordItem, NdJson>(RecordBody::from_iter(vec![
                RecordItem { id: 1 },
            ])),
            NdJson::CONTENT_TYPE,
        ))
        .await?;

    assert_eq!(decoded.into_value(), "{\"id\":1}\n");
    let events = events.lock().expect("event lock").clone();
    let rate_limit = events
        .iter()
        .position(|event| event == "rate_limit_acquire")
        .expect("rate limit acquisition");
    let transport = events
        .iter()
        .position(|event| event == "transport")
        .expect("transport send");
    let request_poll = events
        .iter()
        .position(|event| event == "record_request_poll")
        .expect("request poll");
    assert!(rate_limit < transport);
    assert!(transport < request_poll);
    Ok(())
}

#[tokio::test]
async fn record_request_is_not_retried_on_transport_error() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::transport_error(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":1}\n")],
        )],
        TransportErrorKind::Other,
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(record_request_plan(
            "RecordNoRetry",
            Method::GET,
            "/record-no-retry",
            record_retry_policy(),
            BodyPlan::Records {
                content_type: HeaderValue::from_static(NdJson::CONTENT_TYPE),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_record_body::<RecordItem, NdJson>(RecordBody::from_iter(vec![
                RecordItem { id: 1 },
                RecordItem { id: 2 },
            ])),
            NdJson::CONTENT_TYPE,
        ))
        .await
        .expect_err("stream-like record requests must not retry");

    assert_eq!(transport.send_count(), 1);
    assert!(matches!(err, ApiClientError::Transport { .. }));
}

#[tokio::test]
async fn record_request_encoding_error_maps_to_codec_error() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":1}\n")],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(record_request_plan(
            "RecordCodecError",
            Method::POST,
            "/record-codec-error",
            ResolvedPolicy::default(),
            BodyPlan::Records {
                content_type: HeaderValue::from_static(NdJson::CONTENT_TYPE),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_record_body::<RecordItem, NdJson>(RecordBody::from_stream(
                ErrorRecordStream::new(),
            )),
            NdJson::CONTENT_TYPE,
        ))
        .await
        .expect_err("request encoding error should surface as codec error");

    assert_eq!(transport.send_count(), 1);
    assert!(matches!(err, ApiClientError::Codec { .. }));
    assert!(!format!("{err:?}").contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
    assert!(!format!("{err}").contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
}

#[tokio::test]
async fn record_request_limit_applies_to_encoded_stream() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":1}\n")],
        )],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_request_body_bytes(10);
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
    });
    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(record_request_plan(
            "RecordLimit",
            Method::POST,
            "/record-limit",
            ResolvedPolicy::default(),
            BodyPlan::Records {
                content_type: HeaderValue::from_static(NdJson::CONTENT_TYPE),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_record_body::<RecordItem, NdJson>(RecordBody::from_iter(vec![
                RecordItem { id: 1 },
                RecordItem { id: 2 },
            ])),
            NdJson::CONTENT_TYPE,
        ))
        .await
        .expect_err("request stream limit should fail");

    assert_eq!(transport.send_count(), 1);
    assert!(matches!(
        err,
        ApiClientError::RequestBodyLimitExceeded { limit: 10, .. }
    ));
}

#[tokio::test]
async fn ndjson_record_response_yields_records_incrementally() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![
                Bytes::from_static(b"{\"id\":1}\n{\"id\":"),
                Bytes::from_static(b"2}"),
            ],
        )],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordResponse",
            Method::GET,
            "/records-response",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    let events_before = events.lock().expect("events lock").clone();
    assert!(
        !events_before
            .iter()
            .any(|event| event == "record_chunk_poll")
    );
    assert!(!format!("{response:?}").contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
    let first = response
        .next_record()
        .await?
        .expect("first record should exist");
    let second = response
        .next_record()
        .await?
        .expect("second record should exist");
    assert_eq!(first, RecordItem { id: 1 });
    assert_eq!(second, RecordItem { id: 2 });
    assert!(response.next_record().await?.is_none());
    Ok(())
}

#[tokio::test]
async fn ndjson_record_response_next_batch_consumes_in_explicit_batches()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(
                b"{\"id\":1}\n{\"id\":2}\n{\"id\":3}\n{\"id\":4}\n",
            )],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordResponseBatch",
            Method::GET,
            "/records-response-batch",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    let first_batch = response
        .next_batch(2)
        .await?
        .expect("first batch should exist");
    assert_eq!(
        first_batch,
        vec![RecordItem { id: 1 }, RecordItem { id: 2 }]
    );
    assert!(
        transport
            .events()
            .iter()
            .any(|event| event.as_str() == "record_chunk_poll")
    );

    let second = response
        .next_record()
        .await?
        .expect("third record should exist");
    assert_eq!(second, RecordItem { id: 3 });

    let tail_batch = response
        .next_batch(2)
        .await?
        .expect("tail batch should exist");
    assert_eq!(tail_batch, vec![RecordItem { id: 4 }]);
    assert!(response.next_batch(2).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn record_response_final_line_without_newline_is_accepted() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":1}\n{\"id\":2}")],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordFinalLine",
            Method::GET,
            "/record-final-line",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    assert_eq!(response.next_record().await?.unwrap(), RecordItem { id: 1 });
    assert_eq!(response.next_record().await?.unwrap(), RecordItem { id: 2 });
    assert!(response.next_record().await?.is_none());
    Ok(())
}

#[tokio::test]
async fn record_response_next_batch_respects_eof_and_zero_size() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(StatusCode::OK, vec![])],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordEmptyBatch",
            Method::GET,
            "/record-empty-batch",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    let err = response
        .next_batch(0)
        .await
        .expect_err("zero batch size should fail");
    assert!(matches!(err, ApiClientError::InvalidParam { .. }));
    assert!(!format!("{err:?}").contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
    assert!(response.next_batch(2).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn record_response_next_batch_zero_size_does_not_consume_pending_error()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":1}\n{\"id\":2}\n{\"id\":")],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordBatchZeroPending",
            Method::GET,
            "/record-batch-zero-pending",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    let batch = response
        .next_batch(500)
        .await?
        .expect("partial batch should be returned");
    assert_eq!(batch, vec![RecordItem { id: 1 }, RecordItem { id: 2 }]);

    let zero_err = response
        .next_batch(0)
        .await
        .expect_err("zero batch size should fail before pending error");
    assert!(matches!(zero_err, ApiClientError::InvalidParam { .. }));

    let pending_err = response
        .next_batch(500)
        .await
        .expect_err("pending decode error should remain queued");
    assert!(matches!(
        pending_err,
        ApiClientError::Decode { .. } | ApiClientError::Codec { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn record_response_next_batch_large_requested_size_is_capped_but_logically_bounded()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":1}\n")],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordBatchHugeRequest",
            Method::GET,
            "/record-batch-huge-request",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    let batch = response
        .next_batch(usize::MAX)
        .await?
        .expect("single record batch should exist");
    assert_eq!(batch, vec![RecordItem { id: 1 }]);
    assert!(response.next_batch(usize::MAX).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn record_response_middle_blank_line_is_rejected() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![
                Bytes::from_static(b"{\"id\":1}\n"),
                Bytes::from_static(b"\n{\"id\":2}\n"),
            ],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordBlankLine",
            Method::GET,
            "/record-blank-line",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    assert_eq!(response.next_record().await?.unwrap(), RecordItem { id: 1 });
    let err = response
        .next_record()
        .await
        .expect_err("blank line should be rejected");
    assert!(!format!("{err:?}").contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
    assert!(!format!("{err}").contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
    Ok(())
}

#[tokio::test]
async fn record_response_next_batch_before_first_error_is_sanitized() -> Result<(), ApiClientError>
{
    let events = Arc::new(StdMutex::new(Vec::new()));
    let sentinel = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":")],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordBatchDecodeError",
            Method::GET,
            "/record-batch-decode-error",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    let err = response
        .next_batch(4)
        .await
        .expect_err("malformed first record should fail");
    assert!(matches!(
        err,
        ApiClientError::Decode { .. } | ApiClientError::Codec { .. }
    ));
    assert!(!format!("{err:?}").contains(sentinel));
    assert!(!format!("{err}").contains(sentinel));
    Ok(())
}

#[tokio::test]
async fn record_response_next_batch_retains_partial_batch_then_pending_error()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let sentinel = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":1}\n{\"id\":2}\n{\"id\":")],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordBatchPartialDecodeError",
            Method::GET,
            "/record-batch-partial-decode-error",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    let batch = response
        .next_batch(4)
        .await?
        .expect("partial batch should be returned");
    assert_eq!(batch, vec![RecordItem { id: 1 }, RecordItem { id: 2 }]);

    let err = response
        .next_batch(4)
        .await
        .expect_err("pending decode error should surface on next call");
    assert!(matches!(
        err,
        ApiClientError::Decode { .. } | ApiClientError::Codec { .. }
    ));
    assert!(!format!("{err:?}").contains(sentinel));
    assert!(!format!("{err}").contains(sentinel));
    Ok(())
}

#[tokio::test]
async fn record_response_next_batch_composes_with_next_record() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(
                b"{\"id\":1}\n{\"id\":2}\n{\"id\":3}\n{\"id\":4}\n",
            )],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordBatchCompose",
            Method::GET,
            "/record-batch-compose",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    assert_eq!(response.next_record().await?, Some(RecordItem { id: 1 }));
    let batch = response.next_batch(2).await?.expect("batch should exist");
    assert_eq!(batch, vec![RecordItem { id: 2 }, RecordItem { id: 3 }]);
    assert_eq!(response.next_record().await?, Some(RecordItem { id: 4 }));
    assert!(response.next_record().await?.is_none());
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct NonCloneRecord {
    id: u32,
}

#[tokio::test]
async fn record_response_next_batch_works_without_clone_bound() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from_static(b"{\"id\":1}\n{\"id\":2}\n")],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let mut response = <concord_core::advanced::RecordResponse<NonCloneRecord, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordNoCloneBatch",
            Method::GET,
            "/record-no-clone-batch",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    let batch = response.next_batch(4).await?.expect("batch should exist");
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].id, 1);
    assert_eq!(batch[1].id, 2);
    assert!(response.next_batch(4).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn record_response_content_length_exceeds_limit_before_body_exposure() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let polled = Arc::new(AtomicBool::new(false));
    let transport = RecordTransport::new(
        events,
        vec![
            ResponseFixture::ndjson(StatusCode::OK, vec![Bytes::from_static(b"{\"id\":1}\n")])
                .content_length(Some(32))
                .with_flag(polled.clone()),
        ],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_response_body_bytes(8);
    });

    let err = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordResponseLimit",
            Method::GET,
            "/record-response-limit",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await
        .expect_err("content length above limit should fail");

    assert!(matches!(err, ApiClientError::ResponseTooLarge { .. }));
    assert!(!polled.load(Ordering::SeqCst));
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn record_response_unknown_length_is_counted_while_reading() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events,
        vec![
            ResponseFixture::ndjson(
                StatusCode::OK,
                vec![
                    Bytes::from_static(b"{\"id\":1}\n"),
                    Bytes::from_static(b"{\"id\":2}\n"),
                ],
            )
            .content_length(None),
        ],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_response_body_bytes(10);
    });

    let mut response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordUnknownLimit",
            Method::GET,
            "/record-unknown-limit",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    assert_eq!(response.next_record().await?.unwrap(), RecordItem { id: 1 });
    let err = response
        .next_record()
        .await
        .expect_err("second record should exceed configured limit");
    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { .. }
    ));
    assert!(!format!("{err:?}").contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
    Ok(())
}

#[tokio::test]
async fn record_stream_debug_is_body_free() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let sentinel = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::ndjson(
            StatusCode::OK,
            vec![Bytes::from(format!("{{\"msg\":\"{sentinel}\"}}\n"))],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    let response = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordDebug",
            Method::GET,
            "/record-debug",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    let events_before = transport.events.lock().expect("events lock").clone();
    assert!(
        !events_before
            .iter()
            .any(|event| event == "record_chunk_poll")
    );
    let rendered = format!("{response:?}");
    assert!(rendered.contains("<record stream>"));
    assert!(!rendered.contains(sentinel));
    Ok(())
}

#[tokio::test]
async fn custom_record_request_reaches_transport_and_is_body_free_in_debug()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::buffered_json(r#"{"ok":true}"#)],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let sentinel = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR".to_string();
    let plan = record_request_plan(
        "PipeRecordRequest",
        Method::POST,
        "/pipe-records",
        ResolvedPolicy::default(),
        BodyPlan::Records {
            content_type: HeaderValue::from_static(PipeText::CONTENT_TYPE),
            format: concord_core::internal::Format::Text,
        },
        RequestArgs::with_record_body::<PipeRecord, PipeText>(RecordBody::from_iter(vec![
            PipeRecord {
                id: 1,
                message: sentinel.clone(),
            },
            PipeRecord {
                id: 2,
                message: "world".to_string(),
            },
        ])),
        PipeText::CONTENT_TYPE,
    );

    let decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(plan)
        .await?;
    assert_eq!(decoded.into_value(), "{\"ok\":true}");
    assert_eq!(transport.send_count(), 1);
    let captured = transport.captured();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].content_type.as_deref(),
        Some(PipeText::CONTENT_TYPE)
    );
    match &captured[0].body {
        CapturedBody::Stream(bytes) => {
            assert_eq!(
                bytes,
                &pipe_record_bytes(&[
                    PipeRecord {
                        id: 1,
                        message: sentinel.clone(),
                    },
                    PipeRecord {
                        id: 2,
                        message: "world".to_string(),
                    },
                ])
            );
        }
        other => panic!("expected streamed pipe body, got {other:?}"),
    }
    assert!(!captured[0].debug.contains(&sentinel));
    let events = transport.events();
    assert!(events.iter().any(|event| event == "rate_limit_acquire"));
    assert!(events.iter().any(|event| event == "record_request_poll"));
    let transport_idx = events
        .iter()
        .position(|event| event == "transport")
        .expect("transport event");
    let poll_idx = events
        .iter()
        .position(|event| event == "record_request_poll")
        .expect("request stream poll event");
    assert!(transport_idx < poll_idx);
    Ok(())
}

#[tokio::test]
async fn custom_record_response_yields_records_incrementally() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::streamed_with_content_type(
            StatusCode::OK,
            PipeText::CONTENT_TYPE,
            vec![
                Bytes::from_static(b"1|hello\n2|wor"),
                Bytes::from_static(b"ld"),
            ],
        )],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.set_debug_sink(Arc::new(RecordingDebugSink::new(events.clone())));
    client.set_runtime_hooks(Arc::new(RecordingHooks::new(events.clone())));
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(RecordingRateLimiter::new(events.clone())));
        cfg.debug(DebugLevel::VV);
    });

    let mut stream = <concord_core::advanced::RecordResponse<PipeRecord, PipeText> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan_with_accept(
            "PipeRecordResponse",
            Method::GET,
            "/pipe-records-response",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
            PipeText::CONTENT_TYPE,
        ))
        .await?;

    let events_before = transport.events();
    assert!(
        !events_before
            .iter()
            .any(|event| event == "response_record_poll")
    );
    let first = stream
        .next_record()
        .await?
        .expect("first record should exist");
    assert_eq!(
        first,
        PipeRecord {
            id: 1,
            message: "hello".to_string(),
        }
    );
    assert!(
        transport
            .events()
            .iter()
            .any(|event| event == "record_chunk_poll")
    );
    let second = stream
        .next_record()
        .await?
        .expect("second record should exist");
    assert_eq!(
        second,
        PipeRecord {
            id: 2,
            message: "world".to_string(),
        }
    );
    assert!(stream.next_record().await?.is_none());
    let rendered = format!("{stream:?}");
    assert!(rendered.contains("<record stream>"));
    assert!(!rendered.contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
    Ok(())
}

#[tokio::test]
async fn custom_record_response_decoder_error_is_sanitized() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let sentinel = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::streamed_with_content_type(
            StatusCode::OK,
            PipeText::CONTENT_TYPE,
            vec![Bytes::from_static(b"bad|line|extra\n")],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut stream = <concord_core::advanced::RecordResponse<PipeRecord, PipeText> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan_with_accept(
            "PipeRecordResponseError",
            Method::GET,
            "/pipe-records-response-error",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
            PipeText::CONTENT_TYPE,
        ))
        .await?;

    let err = stream
        .next_record()
        .await
        .expect_err("malformed pipe record should fail");
    assert!(matches!(err, ApiClientError::Decode { .. }));
    assert!(!format!("{err:?}").contains(sentinel));
    assert!(!format!("{err}").contains(sentinel));
    Ok(())
}

#[cfg(feature = "records-csv")]
#[tokio::test]
async fn csv_record_request_sets_text_csv_and_remains_streamed() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::buffered_json(r#"{"ok":true}"#)],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let _decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(record_request_plan(
            "CsvRecordRequest",
            Method::POST,
            "/csv-record-request",
            ResolvedPolicy::default(),
            BodyPlan::Records {
                content_type: HeaderValue::from_static(Csv::<CsvCommaDelim>::CONTENT_TYPE),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_record_body::<PipeRecord, Csv<CsvCommaDelim>>(RecordBody::from_iter(
                vec![
                    PipeRecord {
                        id: 1,
                        message: "alpha".to_string(),
                    },
                    PipeRecord {
                        id: 2,
                        message: "beta".to_string(),
                    },
                ],
            )),
            Csv::<CsvCommaDelim>::CONTENT_TYPE,
        ))
        .await
        .expect("csv record request should succeed");
    let captured = transport.captured();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].content_type.as_deref(),
        Some(Csv::<CsvCommaDelim>::CONTENT_TYPE)
    );
    match &captured[0].body {
        CapturedBody::Stream(bytes) => {
            let rendered = String::from_utf8(bytes.to_vec()).expect("csv utf8");
            assert!(rendered.contains("id"));
            assert!(rendered.contains("message"));
            assert!(rendered.contains("alpha"));
            assert!(rendered.contains("beta"));
        }
        other => panic!("expected streamed csv body, got {other:?}"),
    }
    assert!(
        transport
            .events()
            .iter()
            .any(|event| event == "record_request_poll")
    );
    Ok(())
}

#[cfg(feature = "records-csv")]
#[tokio::test]
async fn csv_record_request_large_batch_does_not_accumulate_bytes() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::buffered_json(r#"{"ok":true}"#)],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let records = (0..128).map(|_| PipeRecord {
        id: 7,
        message: "repeat".to_string(),
    });
    let _decoded = client
        .execute_plan::<concord_core::prelude::Text<String>>(record_request_plan(
            "CsvRecordRequestLargeBatch",
            Method::POST,
            "/csv-record-request-large-batch",
            ResolvedPolicy::default(),
            BodyPlan::Records {
                content_type: HeaderValue::from_static(Csv::<CsvCommaDelim>::CONTENT_TYPE),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_record_body::<PipeRecord, Csv<CsvCommaDelim>>(RecordBody::from_iter(
                records,
            )),
            Csv::<CsvCommaDelim>::CONTENT_TYPE,
        ))
        .await
        .expect("csv record request should succeed");

    let captured = transport.captured();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0].content_type.as_deref(),
        Some(Csv::<CsvCommaDelim>::CONTENT_TYPE)
    );
    let rendered = match &captured[0].body {
        CapturedBody::Stream(bytes) => String::from_utf8(bytes.to_vec()).expect("csv utf8"),
        other => panic!("expected streamed csv body, got {other:?}"),
    };
    let mut lines = rendered.lines();
    assert_eq!(lines.next(), Some("id,message"));
    assert!(lines.all(|line| line == "7,repeat"));
    assert_eq!(rendered.lines().count(), 129);
    Ok(())
}

#[cfg(feature = "records-csv")]
#[tokio::test]
async fn csv_record_response_streams_incrementally_across_chunks() -> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::streamed_with_content_type(
            StatusCode::OK,
            Csv::<CsvCommaDelim>::CONTENT_TYPE,
            vec![
                Bytes::from_static(b"id,message\n1,hel"),
                Bytes::from_static(b"lo\n2,world\n"),
            ],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut stream = <concord_core::advanced::RecordResponse<PipeRecord, Csv<CsvCommaDelim>> as concord_core::advanced::ResponseEntity>::execute(&client,
            record_response_plan_with_accept(
                "CsvRecordResponse",
                Method::GET,
                "/csv-record-response",
                ResolvedPolicy::default(),
                BodyPlan::None,
                RequestArgs::default(),
                Csv::<CsvCommaDelim>::CONTENT_TYPE,
            ),
        )
        .await?;

    let events_before = transport.events();
    assert!(
        !events_before
            .iter()
            .any(|event| event == "record_chunk_poll")
    );

    let first = stream
        .next_record()
        .await?
        .expect("first csv record should exist");
    assert_eq!(
        first,
        PipeRecord {
            id: 1,
            message: "hello".to_string(),
        }
    );
    assert!(
        transport
            .events()
            .iter()
            .any(|event| event == "record_chunk_poll")
    );
    let second = stream
        .next_record()
        .await?
        .expect("second csv record should exist");
    assert_eq!(
        second,
        PipeRecord {
            id: 2,
            message: "world".to_string(),
        }
    );
    assert!(stream.next_record().await?.is_none());
    Ok(())
}

#[cfg(feature = "records-csv")]
#[tokio::test]
async fn csv_record_response_next_batch_consumes_in_explicit_batches() -> Result<(), ApiClientError>
{
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::streamed_with_content_type(
            StatusCode::OK,
            Csv::<CsvCommaDelim>::CONTENT_TYPE,
            vec![Bytes::from_static(
                b"id,message\n1,one\n2,two\n3,three\n4,four\n",
            )],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut stream = <concord_core::advanced::RecordResponse<PipeRecord, Csv<CsvCommaDelim>> as concord_core::advanced::ResponseEntity>::execute(&client,
            record_response_plan_with_accept(
                "CsvRecordResponseBatch",
                Method::GET,
                "/csv-record-response-batch",
                ResolvedPolicy::default(),
                BodyPlan::None,
                RequestArgs::default(),
                Csv::<CsvCommaDelim>::CONTENT_TYPE,
            ),
        )
        .await?;

    let batch = stream.next_batch(2).await?.expect("csv batch should exist");
    assert_eq!(
        batch,
        vec![
            PipeRecord {
                id: 1,
                message: "one".to_string(),
            },
            PipeRecord {
                id: 2,
                message: "two".to_string(),
            },
        ]
    );
    assert_eq!(
        stream.next_record().await?,
        Some(PipeRecord {
            id: 3,
            message: "three".to_string(),
        })
    );
    assert_eq!(
        stream.next_batch(2).await?,
        Some(vec![PipeRecord {
            id: 4,
            message: "four".to_string(),
        }])
    );
    assert!(stream.next_batch(2).await?.is_none());
    Ok(())
}

#[cfg(feature = "records-csv")]
#[tokio::test]
async fn csv_record_response_streams_many_records_across_chunks() -> Result<(), ApiClientError> {
    let mut body = String::from("id,message\n");
    for idx in 0..256u32 {
        body.push_str(&format!("{idx},row{idx}\n"));
    }
    let bytes = Bytes::from(body);
    let chunk_size = bytes.len() / 5;
    let chunks = vec![
        Bytes::copy_from_slice(&bytes[..chunk_size]),
        Bytes::copy_from_slice(&bytes[chunk_size..chunk_size * 2]),
        Bytes::copy_from_slice(&bytes[chunk_size * 2..chunk_size * 3]),
        Bytes::copy_from_slice(&bytes[chunk_size * 3..chunk_size * 4]),
        Bytes::copy_from_slice(&bytes[chunk_size * 4..]),
    ];
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::streamed_with_content_type(
            StatusCode::OK,
            Csv::<CsvCommaDelim>::CONTENT_TYPE,
            chunks,
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut stream = <concord_core::advanced::RecordResponse<PipeRecord, Csv<CsvCommaDelim>> as concord_core::advanced::ResponseEntity>::execute(&client,
            record_response_plan_with_accept(
                "CsvRecordResponseMany",
                Method::GET,
                "/csv-record-response-many",
                ResolvedPolicy::default(),
                BodyPlan::None,
                RequestArgs::default(),
                Csv::<CsvCommaDelim>::CONTENT_TYPE,
            ),
        )
        .await?;

    let mut count = 0usize;
    while let Some(record) = stream.next_record().await? {
        assert_eq!(record.message, format!("row{}", count));
        count += 1;
    }
    assert_eq!(count, 256);
    assert!(
        transport
            .events()
            .iter()
            .filter(|event| event.as_str() == "record_chunk_poll")
            .count()
            > 1
    );
    Ok(())
}

#[tokio::test]
async fn ndjson_record_response_next_batch_stops_at_requested_batch_size()
-> Result<(), ApiClientError> {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let mut chunks = Vec::new();
    for idx in 0..16u32 {
        chunks.push(Bytes::from(format!("{{\"id\":{idx}}}\n")));
    }
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::ndjson(StatusCode::OK, chunks)],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());

    let mut stream = <concord_core::advanced::RecordResponse<RecordItem, NdJson> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan(
            "RecordBatchBounded",
            Method::GET,
            "/record-batch-bounded",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
        ))
        .await?;

    let batch = stream.next_batch(5).await?.expect("batch should exist");
    assert_eq!(batch.len(), 5);
    assert_eq!(batch[0], RecordItem { id: 0 });
    assert_eq!(batch[4], RecordItem { id: 4 });
    assert_eq!(
        transport
            .events()
            .iter()
            .filter(|event| event.as_str() == "record_chunk_poll")
            .count(),
        5
    );
    assert_eq!(stream.next_record().await?, Some(RecordItem { id: 5 }));
    Ok(())
}

#[cfg(feature = "records-csv")]
#[tokio::test]
async fn csv_record_response_headerless_semicolon_and_tab_configs_work()
-> Result<(), ApiClientError> {
    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct CsvHeaderless {
        id: u64,
        message: String,
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct CsvHeaderlessPipe;

    impl CsvConfig for CsvHeaderlessPipe {
        const DELIMITER: u8 = b'|';
        const HAS_HEADERS: bool = false;
    }

    let headerless_transport = RecordTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![ResponseFixture::streamed_with_content_type(
            StatusCode::OK,
            Csv::<CsvHeaderlessPipe>::CONTENT_TYPE,
            vec![Bytes::from_static(b"1|hello\n2|world\n")],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        headerless_transport.clone(),
    );
    let mut stream = <concord_core::advanced::RecordResponse<CsvHeaderless, Csv<CsvHeaderlessPipe>> as concord_core::advanced::ResponseEntity>::execute(&client,
            record_response_plan_with_accept(
                "CsvHeaderlessResponse",
                Method::GET,
                "/csv-headerless-response",
                ResolvedPolicy::default(),
                BodyPlan::None,
                RequestArgs::default(),
                Csv::<CsvHeaderlessPipe>::CONTENT_TYPE,
            ),
        )
        .await?;
    assert_eq!(
        stream.next_record().await?,
        Some(CsvHeaderless {
            id: 1,
            message: "hello".to_string(),
        })
    );
    assert_eq!(
        stream.next_record().await?,
        Some(CsvHeaderless {
            id: 2,
            message: "world".to_string(),
        })
    );
    assert!(stream.next_record().await?.is_none());

    let semicolon_transport = RecordTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![ResponseFixture::streamed_with_content_type(
            StatusCode::OK,
            Csv::<CsvSemicolonDelim>::CONTENT_TYPE,
            vec![Bytes::from_static(b"id;message\n3;semi\n")],
        )],
    );
    let client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), semicolon_transport);
    let mut stream = <concord_core::advanced::RecordResponse<PipeRecord, Csv<CsvSemicolonDelim>> as concord_core::advanced::ResponseEntity>::execute(&client,
            record_response_plan_with_accept(
                "CsvSemicolonResponse",
                Method::GET,
                "/csv-semicolon-response",
                ResolvedPolicy::default(),
                BodyPlan::None,
                RequestArgs::default(),
                Csv::<CsvSemicolonDelim>::CONTENT_TYPE,
            ),
        )
        .await?;
    assert_eq!(
        stream.next_record().await?,
        Some(PipeRecord {
            id: 3,
            message: "semi".to_string(),
        })
    );
    assert!(stream.next_record().await?.is_none());

    let tab_transport = RecordTransport::new(
        Arc::new(StdMutex::new(Vec::new())),
        vec![ResponseFixture::streamed_with_content_type(
            StatusCode::OK,
            Csv::<CsvTabDelim>::CONTENT_TYPE,
            vec![Bytes::from_static(b"id\tmessage\n4\ttab\n")],
        )],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), tab_transport);
    let mut stream = <concord_core::advanced::RecordResponse<PipeRecord, Csv<CsvTabDelim>> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan_with_accept(
            "CsvTabResponse",
            Method::GET,
            "/csv-tab-response",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
            Csv::<CsvTabDelim>::CONTENT_TYPE,
        ))
        .await?;
    assert_eq!(
        stream.next_record().await?,
        Some(PipeRecord {
            id: 4,
            message: "tab".to_string(),
        })
    );
    assert!(stream.next_record().await?.is_none());

    Ok(())
}

#[tokio::test]
async fn custom_record_encoder_error_is_sanitized() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let sentinel = "SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR".to_string();
    let transport = RecordTransport::new(
        events,
        vec![ResponseFixture::buffered_json(r#"{"ok":true}"#)],
    );
    let client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(record_request_plan(
            "PipeRecordEncodeError",
            Method::POST,
            "/pipe-record-encode-error",
            ResolvedPolicy::default(),
            BodyPlan::Records {
                content_type: HeaderValue::from_static(PipeText::CONTENT_TYPE),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_record_body::<PipeRecord, PipeText>(RecordBody::from_iter(vec![
                PipeRecord {
                    id: 1,
                    message: "bad|value".to_string(),
                },
            ])),
            PipeText::CONTENT_TYPE,
        ))
        .await
        .expect_err("pipe encoding should fail");

    assert!(matches!(err, ApiClientError::Codec { .. }));
    assert!(!format!("{err:?}").contains(&sentinel));
    assert!(!format!("{err}").contains(&sentinel));
}

#[tokio::test]
async fn custom_record_request_stream_limit_applies() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events.clone(),
        vec![ResponseFixture::buffered_json(r#"{"ok":true}"#)],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_request_body_bytes(10);
    });

    let err = client
        .execute_plan::<concord_core::prelude::Text<String>>(record_request_plan(
            "PipeRecordRequestLimit",
            Method::POST,
            "/pipe-record-request-limit",
            ResolvedPolicy::default(),
            BodyPlan::Records {
                content_type: HeaderValue::from_static(PipeText::CONTENT_TYPE),
                format: concord_core::internal::Format::Text,
            },
            RequestArgs::with_record_body::<PipeRecord, PipeText>(RecordBody::from_iter(vec![
                PipeRecord {
                    id: 1,
                    message: "hello".to_string(),
                },
                PipeRecord {
                    id: 2,
                    message: "world".to_string(),
                },
            ])),
            PipeText::CONTENT_TYPE,
        ))
        .await
        .expect_err("request stream limit should fail");

    assert!(matches!(
        err,
        ApiClientError::RequestBodyLimitExceeded { limit: 10, .. }
    ));
    assert_eq!(transport.send_count(), 1);
}

#[tokio::test]
async fn custom_record_response_stream_limit_applies() {
    let events = Arc::new(StdMutex::new(Vec::new()));
    let transport = RecordTransport::new(
        events,
        vec![
            ResponseFixture::streamed_with_content_type(
                StatusCode::OK,
                PipeText::CONTENT_TYPE,
                vec![
                    Bytes::from_static(b"1|hello\n"),
                    Bytes::from_static(b"2|world\n"),
                ],
            )
            .content_length(None),
        ],
    );
    let mut client =
        ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport.clone());
    client.configure(|cfg| {
        cfg.max_stream_response_body_bytes(12);
    });

    let mut stream = <concord_core::advanced::RecordResponse<PipeRecord, PipeText> as concord_core::advanced::ResponseEntity>::execute(&client, record_response_plan_with_accept(
            "PipeRecordResponseLimit",
            Method::GET,
            "/pipe-record-response-limit",
            ResolvedPolicy::default(),
            BodyPlan::None,
            RequestArgs::default(),
            PipeText::CONTENT_TYPE,
        ))
        .await
        .expect("response should return stream");

    assert_eq!(
        stream
            .next_record()
            .await
            .expect("first record should decode"),
        Some(PipeRecord {
            id: 1,
            message: "hello".to_string(),
        })
    );
    let err = stream
        .next_record()
        .await
        .expect_err("response limit should fail");
    assert!(matches!(
        err,
        ApiClientError::ResponseBodyLimitExceeded { .. }
    ));
    assert!(!format!("{err:?}").contains("SECRET_RECORD_SENTINEL_MUST_NOT_APPEAR"));
}
