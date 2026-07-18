use std::collections::VecDeque;
use std::fmt;
use std::future::IntoFuture;
use std::io;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use concord_test_support::executor::DeterministicExecutor;
use concord_test_support::gate::Gate;
use concord_test_support::script::{
    BodyCategory, ExpectedRequest, ScriptedResponse, TransportFailure,
};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
use futures_core::Stream;
use http::{Method, StatusCode};

use super::*;
use crate::body::{BodyError, BodyFactory, RequestBody, StreamBody};
use crate::codec::{
    CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, MediaType, OctetStream, Text,
};
use crate::error::{ErrorCategory, HookError, StatusBodyState};
use crate::hooks::{BoxFuture, RuntimeHooks};
use crate::http::HttpConfigBuilder;
use crate::request::{
    BodyMetadata, BufferedResponse, BytesResponse, CodecBody, EmptyBody, HeadBuilder,
    NoContentResponse, PreparedHead, RequestMeta, StreamResponseHandler,
};
use crate::runtime::RuntimeConfigBuilder;

const ENDPOINT: &str = "ExecutionApi::health::bytes";
const NO_CONTENT_ENDPOINT: &str = "ExecutionApi::health::ping";
const CODEC_ENDPOINT: &str = "ExecutionApi::codec::round_trip";
const BODY_ENDPOINT: &str = "ExecutionApi::body::upload";
const STREAM_ENDPOINT: &str = "ExecutionApi::stream::download";

struct StreamEndpoint<M> {
    marker: std::marker::PhantomData<fn() -> M>,
}

impl<M> StreamEndpoint<M> {
    fn new() -> Self {
        Self {
            marker: std::marker::PhantomData,
        }
    }
}

impl<M> EndpointSpec for StreamEndpoint<M>
where
    M: MediaType,
{
    type Body = EmptyBody;
    type Response = StreamResponseHandler<M>;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(STREAM_ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("stream")?;
        builder.push_static("download")?;
        Ok((builder.finish(), EmptyBody::metadata()))
    }

    fn into_body(self) -> Self::Body {
        EmptyBody::new()
    }
}

struct VendorStream;

impl MediaType for VendorStream {
    const MEDIA_TYPE: &'static str = "application/vnd.concord.stream; version=1";
}

struct InvalidStreamMedia;

impl MediaType for InvalidStreamMedia {
    const MEDIA_TYPE: &'static str = "not-a-media-type";
}

struct BodyEndpoint {
    body: RequestBody,
}

impl BodyEndpoint {
    fn new(body: RequestBody) -> Self {
        Self { body }
    }
}

impl EndpointSpec for BodyEndpoint {
    type Body = RequestBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(BODY_ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("body")?;
        builder.push_static("upload")?;
        Ok((
            builder.finish(),
            self.body.metadata(BODY_ENDPOINT, &Method::POST)?,
        ))
    }

    fn into_body(self) -> Self::Body {
        self.body
    }
}

enum ProbeFrame {
    Data(&'static [u8]),
    Failure(&'static str),
}

struct BodyPollProbe {
    frames: VecDeque<ProbeFrame>,
    polls: Arc<AtomicUsize>,
    dropped: Option<Arc<AtomicBool>>,
}

impl BodyPollProbe {
    fn new(frames: impl IntoIterator<Item = ProbeFrame>, polls: &Arc<AtomicUsize>) -> Self {
        Self {
            frames: frames.into_iter().collect(),
            polls: Arc::clone(polls),
            dropped: None,
        }
    }
}

impl Stream for BodyPollProbe {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(self.frames.pop_front().map(|frame| match frame {
            ProbeFrame::Data(bytes) => Ok(Bytes::from_static(bytes)),
            ProbeFrame::Failure(message) => Err(io::Error::other(message)),
        }))
    }
}

impl Drop for BodyPollProbe {
    fn drop(&mut self) {
        if let Some(dropped) = &self.dropped {
            dropped.store(true, Ordering::SeqCst);
        }
    }
}

struct ProbeBodyFactory {
    content_type_calls: Arc<AtomicUsize>,
    create_calls: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
    fail: bool,
}

impl BodyFactory for ProbeBodyFactory {
    fn content_type(&self) -> Option<http::HeaderValue> {
        self.content_type_calls.fetch_add(1, Ordering::SeqCst);
        Some(http::HeaderValue::from_static(
            "application/x-concord-factory",
        ))
    }

    fn create(&self) -> Result<StreamBody, BodyError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(BodyError::with_safe_message(
                crate::body::BodyErrorKind::Factory,
                "safe factory configuration failure",
            ))
        } else {
            Ok(StreamBody::from_stream(BodyPollProbe::new(
                [ProbeFrame::Data(b"factory")],
                &self.polls,
            ))
            .exact_length(7))
        }
    }
}

struct PendingBodyStream {
    polls: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
    reached: Arc<tokio::sync::Notify>,
}

impl Stream for PendingBodyStream {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        self.reached.notify_one();
        Poll::Pending
    }
}

impl Drop for PendingBodyStream {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

struct PendingBodyFactory {
    create_calls: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
    reached: Arc<tokio::sync::Notify>,
}

impl BodyFactory for PendingBodyFactory {
    fn create(&self) -> Result<StreamBody, BodyError> {
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        Ok(StreamBody::from_stream(PendingBodyStream {
            polls: Arc::clone(&self.polls),
            dropped: Arc::clone(&self.dropped),
            reached: Arc::clone(&self.reached),
        }))
    }
}

struct CodecEndpoint<Encoder, Decoder>
where
    Encoder: EncodeBody,
    Decoder: DecodeBody,
{
    input: Encoder::Input,
    marker: std::marker::PhantomData<Decoder>,
}

impl<Encoder, Decoder> CodecEndpoint<Encoder, Decoder>
where
    Encoder: EncodeBody,
    Decoder: DecodeBody,
{
    fn new(input: Encoder::Input) -> Self {
        Self {
            input,
            marker: std::marker::PhantomData,
        }
    }
}

impl<Encoder, Decoder> EndpointSpec for CodecEndpoint<Encoder, Decoder>
where
    Encoder: EncodeBody,
    Decoder: DecodeBody,
{
    type Body = CodecBody<Encoder>;
    type Response = BufferedResponse<Decoder>;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(CODEC_ENDPOINT, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("codec")?;
        builder.push_static("round-trip")?;
        Ok((
            builder.finish(),
            CodecBody::<Encoder>::metadata_for(CODEC_ENDPOINT, &Method::POST)?,
        ))
    }

    fn into_body(self) -> Self::Body {
        CodecBody::new(self.input)
    }
}

struct EncoderInput {
    bytes: Bytes,
    calls: Arc<AtomicUsize>,
    fail: bool,
}

struct ProbeEncoder;

impl EncodeBody for ProbeEncoder {
    type Input = EncoderInput;

    const CONTENT_TYPE: Option<&'static str> = Some("application/x-concord-probe");

    fn encode(input: Self::Input, context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        assert_eq!(context.endpoint, CODEC_ENDPOINT);
        assert_eq!(context.method, Method::POST);
        assert!(context.body_limit.is_some_and(|limit| limit > 0));
        input.calls.fetch_add(1, Ordering::SeqCst);
        if input.fail {
            Err(CodecError::new("request-codec-secret-ec2d"))
        } else {
            Ok(input.bytes)
        }
    }
}

static PROBE_DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);
static PROBE_VALIDATOR_CALLS: AtomicUsize = AtomicUsize::new(0);

struct ProbeDecoder;

impl DecodeBody for ProbeDecoder {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-probe");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        PROBE_VALIDATOR_CALLS.fetch_add(1, Ordering::SeqCst);
        if content_type == Some("application/x-concord-probe") {
            Ok(())
        } else {
            Err(CodecError::new("media-contract-secret-7cb1"))
        }
    }

    fn decode(body: Bytes, context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        PROBE_DECODER_CALLS.fetch_add(1, Ordering::SeqCst);
        assert_eq!(context.endpoint, CODEC_ENDPOINT);
        assert_eq!(context.method, Method::POST);
        assert_eq!(context.status, StatusCode::OK);
        assert_eq!(context.content_type, Some("application/x-concord-probe"));
        String::from_utf8(body.to_vec())
            .map(|value| format!("decoded:{value}"))
            .map_err(|error| {
                drop(error);
                CodecError::new("probe decoder UTF-8 failure")
            })
    }
}

static FAILING_DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct FailingDecoder;

impl DecodeBody for FailingDecoder {
    type Output = String;

    const ACCEPT: Option<&'static str> = None;

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        FAILING_DECODER_CALLS.fetch_add(1, Ordering::SeqCst);
        Err(CodecError::new("response-decoder-secret-c413"))
    }
}

static REJECTING_DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct RejectingDecoder;

impl DecodeBody for RejectingDecoder {
    type Output = ();

    const ACCEPT: Option<&'static str> = Some("application/x-required");

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Err(CodecError::new("content-type-secret-4fea"))
    }

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        REJECTING_DECODER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

static OVERFLOW_DECODER_CALLS: AtomicUsize = AtomicUsize::new(0);

struct OverflowDecoder;

impl DecodeBody for OverflowDecoder {
    type Output = ();

    const ACCEPT: Option<&'static str> = None;

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        OVERFLOW_DECODER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct RealCustomDecoder;

impl DecodeBody for RealCustomDecoder {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-probe");

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type == Some("application/x-concord-probe") {
            Ok(())
        } else {
            Err(CodecError::new("custom media mismatch"))
        }
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|error| {
            drop(error);
            CodecError::new("custom UTF-8 decode failed")
        })
    }
}

struct BytesEndpoint {
    inherited_timeout: Option<Duration>,
    trace: Option<Arc<Mutex<Vec<String>>>>,
}

impl BytesEndpoint {
    fn new(inherited_timeout: Option<Duration>) -> Self {
        Self {
            inherited_timeout,
            trace: None,
        }
    }

    fn traced(trace: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            inherited_timeout: Some(Duration::from_secs(30)),
            trace: Some(trace),
        }
    }
}

impl EndpointSpec for BytesEndpoint {
    type Body = EmptyBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, self.inherited_timeout)
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        if let Some(trace) = &self.trace {
            trace
                .lock()
                .expect("trace lock")
                .push("head preparation".into());
        }
        builder.push_static("health")?;
        builder.push_static("bytes")?;
        Ok((builder.finish(), EmptyBody::metadata()))
    }

    fn into_body(self) -> Self::Body {
        EmptyBody::new()
    }
}

struct NoContentEndpoint;

impl EndpointSpec for NoContentEndpoint {
    type Body = EmptyBody;
    type Response = NoContentResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            NO_CONTENT_ENDPOINT,
            Method::GET,
            Some(Duration::from_secs(30)),
        )
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_static("health")?;
        builder.push_static("ping")?;
        Ok((builder.finish(), EmptyBody::metadata()))
    }

    fn into_body(self) -> Self::Body {
        EmptyBody::new()
    }
}

struct InvalidEndpoint;

impl EndpointSpec for InvalidEndpoint {
    type Body = EmptyBody;
    type Response = BytesResponse;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(ENDPOINT, Method::GET, Some(Duration::from_secs(30)))
    }

    fn prepare_head_and_body_metadata(
        &self,
        mut builder: HeadBuilder,
    ) -> Result<(PreparedHead, BodyMetadata), ApiError> {
        builder.push_value(&"")?;
        Ok((builder.finish(), EmptyBody::metadata()))
    }

    fn into_body(self) -> Self::Body {
        EmptyBody::new()
    }
}

#[derive(Clone, Debug)]
enum LifecycleRecord {
    Before {
        operation_id: OperationId,
        body_kind: crate::hooks::BodyKind,
        known_body_length: Option<u64>,
    },
    Response(OperationId, StatusCode),
    Complete {
        operation_id: OperationId,
        outcome: CompletionOutcome,
        status: Option<StatusCode>,
        elapsed: Duration,
    },
}

struct ProbeHooks {
    records: Mutex<Vec<LifecycleRecord>>,
    trace: Option<Arc<Mutex<Vec<String>>>>,
    before_gate: Option<Gate>,
    response_gate: Option<Gate>,
    completion_gate: Option<Gate>,
    veto: AtomicBool,
    completion_returned: AtomicUsize,
}

impl ProbeHooks {
    fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            trace: None,
            before_gate: None,
            response_gate: None,
            completion_gate: None,
            veto: AtomicBool::new(false),
            completion_returned: AtomicUsize::new(0),
        }
    }

    fn traced(trace: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            trace: Some(trace),
            ..Self::new()
        }
    }

    fn records(&self) -> Vec<LifecycleRecord> {
        self.records.lock().expect("records lock").clone()
    }

    fn push_trace(&self, phase: &str) {
        if let Some(trace) = &self.trace {
            trace.lock().expect("trace lock").push(phase.into());
        }
    }
}

impl RuntimeHooks for ProbeHooks {
    fn before_send<'a>(&'a self, context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        self.push_trace("before_send");
        self.records
            .lock()
            .expect("records lock")
            .push(LifecycleRecord::Before {
                operation_id: context.operation_id,
                body_kind: context.body_kind,
                known_body_length: context.known_body_length,
            });
        Box::pin(async move {
            if let Some(gate) = &self.before_gate {
                gate.wait().await;
            }
            if self.veto.load(Ordering::SeqCst) {
                Err(HookError::new("safe hook veto"))
            } else {
                Ok(())
            }
        })
    }

    fn on_response_head<'a>(&'a self, context: ResponseHeadEvent<'a>) -> BoxFuture<'a, ()> {
        self.push_trace("on_response_head");
        self.records
            .lock()
            .expect("records lock")
            .push(LifecycleRecord::Response(
                context.operation_id,
                context.status,
            ));
        Box::pin(async move {
            if let Some(gate) = &self.response_gate {
                gate.wait().await;
            }
        })
    }

    fn on_complete<'a>(&'a self, context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        self.push_trace("on_complete");
        self.records
            .lock()
            .expect("records lock")
            .push(LifecycleRecord::Complete {
                operation_id: context.operation_id,
                outcome: context.outcome,
                status: context.final_status,
                elapsed: context.elapsed,
            });
        Box::pin(async move {
            if let Some(gate) = &self.completion_gate {
                gate.wait().await;
            }
            self.completion_returned.fetch_add(1, Ordering::SeqCst);
        })
    }
}

fn development_core(
    executor: &DeterministicExecutor,
    hooks: Arc<ProbeHooks>,
    runtime: RuntimeConfigBuilder,
) -> ClientCore {
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    ClientCore::build_with_development(
        "http://execution.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        runtime.hooks(hooks),
        executor,
    )
    .expect("development core")
}

fn real_core(
    server: &ScriptedServer,
    hooks: Arc<ProbeHooks>,
    runtime: RuntimeConfigBuilder,
) -> ClientCore {
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    ClientCore::build(
        &server.url("/v1/"),
        HttpConfigBuilder::new().http1_only(),
        runtime.hooks(hooks),
    )
    .expect("real HTTP core")
}

fn bytes_request(core: ClientCore) -> Request<BytesEndpoint> {
    Request::new(core, BytesEndpoint::new(Some(Duration::from_secs(30))))
}

fn no_content_request(core: ClientCore) -> Request<NoContentEndpoint> {
    Request::new(core, NoContentEndpoint)
}

fn codec_request<Encoder, Decoder>(
    core: ClientCore,
    input: Encoder::Input,
) -> Request<CodecEndpoint<Encoder, Decoder>>
where
    Encoder: EncodeBody,
    Decoder: DecodeBody,
{
    Request::new(core, CodecEndpoint::new(input))
}

fn body_request(core: ClientCore, body: RequestBody) -> Request<BodyEndpoint> {
    Request::new(core, BodyEndpoint::new(body))
}

fn stream_request<M>(core: ClientCore) -> Request<StreamEndpoint<M>>
where
    M: MediaType,
{
    Request::new(core, StreamEndpoint::new())
}

async fn next_stream_chunk<M>(
    response: &mut crate::response::StreamResponse<M>,
) -> Option<Result<Bytes, ApiError>> {
    std::future::poll_fn(|context| Pin::new(&mut *response).poll_next(context)).await
}

fn probe_input(value: &'static [u8], calls: &Arc<AtomicUsize>) -> EncoderInput {
    EncoderInput {
        bytes: Bytes::from_static(value),
        calls: Arc::clone(calls),
        fail: false,
    }
}

fn probe_response(body: impl Into<Bytes>) -> ScriptedResponse {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/x-concord-probe"),
    );
    ScriptedResponse::success(body).headers(headers)
}

fn completion(
    records: &[LifecycleRecord],
) -> (OperationId, CompletionOutcome, Option<StatusCode>, Duration) {
    records
        .iter()
        .find_map(|record| match record {
            LifecycleRecord::Complete {
                operation_id,
                outcome,
                status,
                elapsed,
            } => Some((*operation_id, *outcome, *status, *elapsed)),
            _ => None,
        })
        .expect("completion record")
}

#[tokio::test]
async fn direct_and_metadata_terminals_use_one_typed_managed_path() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let expected_url = "http://execution.example.test/v1/health/bytes";
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::GET)
                .safe_url(expected_url)
                .body(Bytes::new()),
            ScriptedResponse::success(Bytes::from_static(b"direct")),
        )
        .expect("direct script");
    assert_eq!(
        bytes_request(core.clone()).await.expect("direct bytes"),
        b"direct"[..]
    );

    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"metadata")))
        .expect("metadata script");
    let response = bytes_request(core)
        .response()
        .await
        .expect("metadata response");
    assert_eq!(response.value(), &b"metadata"[..]);
    assert_eq!(response.url().as_str(), expected_url);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), ENDPOINT);
    assert_eq!(response.metadata().method(), Method::GET);
    assert_eq!(response.metadata().page_index(), 0);
    assert!(!response.metadata().recovered_authentication());

    let captures = executor.application().captures().expect("captures");
    assert_eq!(captures.len(), 2);
    assert_eq!(captures[0].body_category(), BodyCategory::Empty);
    assert_eq!(captures[0].known_body_length(), Some(0));
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn ordering_crosses_preparation_hooks_debug_transport_response_and_completion_once() {
    let executor = DeterministicExecutor::new();
    let transport_gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(transport_gate.clone())
        .expect("transport gate");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"ordered")))
        .expect("ordered response");
    let trace = Arc::new(Mutex::new(Vec::new()));
    let hooks = Arc::new(ProbeHooks::traced(Arc::clone(&trace)));
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new())
        .with_debug_output(Arc::clone(&trace));
    let request =
        Request::new(core, BytesEndpoint::traced(Arc::clone(&trace))).debug(DebugLevel::Headers);
    let task = tokio::spawn(async move { request.await });

    transport_gate.wait_until_reached().await;
    let before_transport = trace.lock().expect("trace lock").clone();
    assert_eq!(before_transport[0], "head preparation");
    assert_eq!(before_transport[1], "before_send");
    assert!(before_transport[2].starts_with("request-ready"));
    assert_eq!(executor.application().request_count().expect("count"), 0);

    transport_gate.open();
    assert_eq!(
        task.await.expect("join").expect("ordered bytes"),
        b"ordered"[..]
    );
    let trace = trace.lock().expect("trace lock");
    assert!(trace[3].starts_with("response-head"));
    assert_eq!(trace[4], "on_response_head");
    assert!(trace[5].starts_with("completion"));
    assert_eq!(trace[6], "on_complete");
    assert_eq!(trace.len(), 7);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn before_send_veto_prevents_native_execution_and_later_response_phases() {
    let executor = DeterministicExecutor::new();
    let mut probe = ProbeHooks::new();
    probe.veto = AtomicBool::new(true);
    let hooks = Arc::new(probe);
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = bytes_request(core).await.expect_err("hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let records = hooks.records();
    assert!(matches!(
        records.as_slice(),
        [
            LifecycleRecord::Before { .. },
            LifecycleRecord::Complete {
                outcome: CompletionOutcome::Hook,
                status: None,
                ..
            }
        ]
    ));
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn vetoed_buffered_request_does_not_invoke_its_encoder() {
    let executor = DeterministicExecutor::new();
    let mut probe = ProbeHooks::new();
    probe.veto = AtomicBool::new(true);
    let hooks = Arc::new(probe);
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let encoder_calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, ProbeDecoder>(
        core,
        probe_input(b"veto-body-secret-78f4", &encoder_calls),
    )
    .await
    .expect_err("hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    assert!(matches!(
        hooks.records().first(),
        Some(LifecycleRecord::Before {
            body_kind: crate::hooks::BodyKind::Buffered,
            known_body_length: None,
            ..
        })
    ));
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("veto-body-secret-78f4"));
    }
}

#[tokio::test]
async fn request_stream_and_factory_remain_lazy_through_hook_veto() {
    let stream_executor = DeterministicExecutor::new();
    let mut stream_probe = ProbeHooks::new();
    stream_probe.veto = AtomicBool::new(true);
    let stream_hooks = Arc::new(stream_probe);
    let stream_core = development_core(
        &stream_executor,
        Arc::clone(&stream_hooks),
        RuntimeConfigBuilder::new(),
    );
    let stream_polls = Arc::new(AtomicUsize::new(0));
    let stream = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Data(b"lazy-stream-secret-a8ef")],
        &stream_polls,
    ))
    .exact_length(23);
    assert_eq!(stream_polls.load(Ordering::SeqCst), 0);
    let request = body_request(stream_core, RequestBody::stream(stream));
    assert_eq!(stream_polls.load(Ordering::SeqCst), 0);
    let error = request.await.expect_err("stream hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(stream_polls.load(Ordering::SeqCst), 0);
    assert_eq!(stream_executor.application().request_count().unwrap(), 0);
    assert!(matches!(
        stream_hooks.records().first(),
        Some(LifecycleRecord::Before {
            body_kind: crate::hooks::BodyKind::Stream,
            known_body_length: Some(23),
            ..
        })
    ));

    let factory_executor = DeterministicExecutor::new();
    let mut factory_probe = ProbeHooks::new();
    factory_probe.veto = AtomicBool::new(true);
    let factory_hooks = Arc::new(factory_probe);
    let factory_core = development_core(
        &factory_executor,
        Arc::clone(&factory_hooks),
        RuntimeConfigBuilder::new(),
    );
    let content_type_calls = Arc::new(AtomicUsize::new(0));
    let create_calls = Arc::new(AtomicUsize::new(0));
    let factory_polls = Arc::new(AtomicUsize::new(0));
    let request = body_request(
        factory_core,
        RequestBody::factory(ProbeBodyFactory {
            content_type_calls: Arc::clone(&content_type_calls),
            create_calls: Arc::clone(&create_calls),
            polls: Arc::clone(&factory_polls),
            fail: false,
        }),
    );
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 0);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);
    assert_eq!(factory_polls.load(Ordering::SeqCst), 0);
    let error = request.await.expect_err("factory hook veto");
    assert_eq!(error.category(), ErrorCategory::Hook);
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);
    assert_eq!(factory_polls.load(Ordering::SeqCst), 0);
    assert_eq!(factory_executor.application().request_count().unwrap(), 0);
}

#[tokio::test]
async fn managed_request_stream_enforces_exact_length_and_aggregate_limit() {
    let success_executor = DeterministicExecutor::new();
    let success_hooks = Arc::new(ProbeHooks::new());
    let success_core = development_core(
        &success_executor,
        Arc::clone(&success_hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(5).expect("positive limit")),
    );
    success_executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .safe_url("http://execution.example.test/v1/body/upload")
                .body("abcde"),
            ScriptedResponse::success("accepted"),
        )
        .expect("stream success script");
    let success_polls = Arc::new(AtomicUsize::new(0));
    let success_body = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Data(b"ab"), ProbeFrame::Data(b"cde")],
        &success_polls,
    ))
    .exact_length(5);
    assert_eq!(
        body_request(success_core, RequestBody::stream(success_body))
            .await
            .expect("exact stream success"),
        b"accepted"[..]
    );
    assert_eq!(success_polls.load(Ordering::SeqCst), 3);
    let captures = success_executor.application().captures().unwrap();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].body_category(), BodyCategory::Streaming);
    assert_eq!(captures[0].known_body_length(), Some(5));

    let preflight_executor = DeterministicExecutor::new();
    let preflight_hooks = Arc::new(ProbeHooks::new());
    let preflight_core = development_core(
        &preflight_executor,
        Arc::clone(&preflight_hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    let preflight_polls = Arc::new(AtomicUsize::new(0));
    let preflight_body = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Data(b"never-polled")],
        &preflight_polls,
    ))
    .exact_length(5);
    let error = body_request(preflight_core, RequestBody::stream(preflight_body))
        .await
        .expect_err("known impossible stream size");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(preflight_polls.load(Ordering::SeqCst), 0);
    assert_eq!(preflight_executor.application().request_count().unwrap(), 0);
    assert!(preflight_hooks.records().iter().all(|record| !matches!(
        record,
        LifecycleRecord::Before { .. } | LifecycleRecord::Response(..)
    )));

    async fn managed_failure(
        body: StreamBody,
        polls: &Arc<AtomicUsize>,
        expected_polls: usize,
        limit: usize,
    ) -> ApiError {
        let executor = DeterministicExecutor::new();
        let hooks = Arc::new(ProbeHooks::new());
        let core = development_core(
            &executor,
            hooks,
            RuntimeConfigBuilder::new()
                .request_body_limit(NonZeroUsize::new(limit).expect("positive limit")),
        );
        let error = body_request(core, RequestBody::stream(body))
            .await
            .expect_err("managed stream failure");
        assert_eq!(error.category(), ErrorCategory::RequestBody);
        assert_eq!(polls.load(Ordering::SeqCst), expected_polls);
        assert_eq!(executor.application().request_count().unwrap(), 0);
        error
    }

    let underflow_polls = Arc::new(AtomicUsize::new(0));
    managed_failure(
        StreamBody::from_stream(BodyPollProbe::new(
            [ProbeFrame::Data(b"abc")],
            &underflow_polls,
        ))
        .exact_length(4),
        &underflow_polls,
        2,
        64,
    )
    .await;

    let exact_overflow_polls = Arc::new(AtomicUsize::new(0));
    managed_failure(
        StreamBody::from_stream(BodyPollProbe::new(
            [ProbeFrame::Data(b"abc"), ProbeFrame::Data(b"de")],
            &exact_overflow_polls,
        ))
        .exact_length(4),
        &exact_overflow_polls,
        2,
        64,
    )
    .await;

    let limit_overflow_polls = Arc::new(AtomicUsize::new(0));
    managed_failure(
        StreamBody::from_stream(BodyPollProbe::new(
            [ProbeFrame::Data(b"abc"), ProbeFrame::Data(b"de")],
            &limit_overflow_polls,
        )),
        &limit_overflow_polls,
        2,
        4,
    )
    .await;
}

#[tokio::test]
async fn producer_failure_is_request_body_primary_and_body_free() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    let polls = Arc::new(AtomicUsize::new(0));
    let body = StreamBody::from_stream(BodyPollProbe::new(
        [
            ProbeFrame::Data(b"accepted-prefix-secret-029b"),
            ProbeFrame::Failure("producer-authored-secret-80d1"),
        ],
        &polls,
    ));
    let error = body_request(core, RequestBody::stream(body))
        .await
        .expect_err("producer failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().unwrap(), 0);
    let recursive = format!("{error:?} {error}");
    assert!(!recursive.contains("accepted-prefix-secret-029b"));
    assert!(!recursive.contains("producer-authored-secret-80d1"));
}

#[tokio::test]
async fn body_factory_is_created_once_after_approval_and_failure_is_pre_native() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .safe_url("http://execution.example.test/v1/body/upload")
                .body("factory"),
            ScriptedResponse::success("factory-ok"),
        )
        .expect("factory script");
    let content_type_calls = Arc::new(AtomicUsize::new(0));
    let create_calls = Arc::new(AtomicUsize::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    let request = body_request(
        core,
        RequestBody::factory(ProbeBodyFactory {
            content_type_calls: Arc::clone(&content_type_calls),
            create_calls: Arc::clone(&create_calls),
            polls: Arc::clone(&polls),
            fail: false,
        }),
    );
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 0);
    assert_eq!(create_calls.load(Ordering::SeqCst), 0);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert_eq!(request.await.expect("factory request"), b"factory-ok"[..]);
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(executor.application().request_count().unwrap(), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);

    let failing_executor = DeterministicExecutor::new();
    let failing_hooks = Arc::new(ProbeHooks::new());
    let failing_core = development_core(
        &failing_executor,
        failing_hooks,
        RuntimeConfigBuilder::new(),
    );
    let failing_content_type_calls = Arc::new(AtomicUsize::new(0));
    let failing_create_calls = Arc::new(AtomicUsize::new(0));
    let failing_polls = Arc::new(AtomicUsize::new(0));
    let error = body_request(
        failing_core,
        RequestBody::factory(ProbeBodyFactory {
            content_type_calls: Arc::clone(&failing_content_type_calls),
            create_calls: Arc::clone(&failing_create_calls),
            polls: Arc::clone(&failing_polls),
            fail: true,
        }),
    )
    .await
    .expect_err("factory creation failure");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(failing_content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_polls.load(Ordering::SeqCst), 0);
    assert_eq!(failing_executor.application().request_count().unwrap(), 0);
    let formatted = format!("{error:?} {error}");
    assert!(!formatted.contains("safe factory configuration failure"));
}

#[tokio::test]
async fn bytes_reader_and_file_stream_constructors_use_the_same_native_path() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    for (expected, response) in [
        (b"bytes-source".as_slice(), "bytes-ok"),
        (b"reader-source".as_slice(), "reader-ok"),
        (b"file-source".as_slice(), "file-ok"),
    ] {
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new()
                    .method(Method::POST)
                    .safe_url("http://execution.example.test/v1/body/upload")
                    .body(Bytes::copy_from_slice(expected)),
                ScriptedResponse::success(response),
            )
            .expect("constructor script");
    }

    assert_eq!(
        body_request(
            core.clone(),
            RequestBody::stream(StreamBody::from_bytes(Bytes::from_static(b"bytes-source"))),
        )
        .await
        .expect("bytes stream"),
        b"bytes-ok"[..]
    );
    assert_eq!(
        body_request(
            core.clone(),
            RequestBody::stream(
                StreamBody::from_reader(std::io::Cursor::new(b"reader-source".to_vec()))
                    .exact_length(13),
            ),
        )
        .await
        .expect("reader stream"),
        b"reader-ok"[..]
    );

    let file_path = std::env::temp_dir().join(format!(
        "concord-phase4-stream-{}-{}.tmp",
        std::process::id(),
        BODY_ENDPOINT.len()
    ));
    tokio::fs::write(&file_path, b"file-source")
        .await
        .expect("write stream fixture");
    let file_body = StreamBody::from_file(&file_path)
        .await
        .expect("open stream fixture");
    let file_result = body_request(core, RequestBody::stream(file_body)).await;
    tokio::fs::remove_file(&file_path)
        .await
        .expect("remove stream fixture");
    assert_eq!(file_result.expect("file stream"), b"file-ok"[..]);
    assert_eq!(executor.application().request_count().unwrap(), 3);
}

fn stream_headers(content_type: &'static str) -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static(content_type),
    );
    headers
}

fn server_stream_headers(content_type: &str) -> Vec<(String, String)> {
    vec![("Content-Type".to_owned(), content_type.to_owned())]
}

#[tokio::test]
async fn stream_response_carries_metadata_and_completes_once_when_opened() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let mut headers = stream_headers("application/octet-stream");
    headers.insert(
        "x-stream-sentinel",
        http::HeaderValue::from_static("stream-header-secret-c62f"),
    );
    executor
        .application()
        .queue_response(ScriptedResponse::success("stream-value-secret-8d0a").headers(headers))
        .expect("stream response script");

    let mut response = stream_request::<OctetStream>(core)
        .await
        .expect("stream response");
    assert_eq!(
        response.url().as_str(),
        "http://execution.example.test/v1/stream/download"
    );
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.content_length(), Some(24));
    assert_eq!(response.metadata().endpoint(), STREAM_ENDPOINT);
    assert_eq!(response.metadata().method(), Method::GET);
    assert_eq!(response.metadata().page_index(), 0);
    assert!(!response.metadata().recovered_authentication());
    assert_eq!(
        response.headers()["x-stream-sentinel"],
        "stream-header-secret-c62f"
    );
    let records = hooks.records();
    assert_eq!(
        completion(&records).1,
        CompletionOutcome::StreamOpened,
        "completion must describe stream creation, not consumption"
    );
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);

    let debug = format!("{response:?}");
    for forbidden in [
        "stream-value-secret-8d0a",
        "stream-header-secret-c62f",
        "execution.example.test",
        "/stream/download",
    ] {
        assert!(
            !debug.contains(forbidden),
            "stream Debug leaked {forbidden}"
        );
    }
    assert_eq!(
        next_stream_chunk(&mut response)
            .await
            .expect("stream chunk")
            .expect("valid chunk"),
        b"stream-value-secret-8d0a"[..]
    );
    assert!(next_stream_chunk(&mut response).await.is_none());
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stream_response_known_length_and_media_fail_before_body_polling() {
    let known_gate = BodyGate::closed();
    let known_server = ScriptedServer::start([ResponseScript::GatedBodyWithHeaders {
        status: 200,
        headers: server_stream_headers("application/octet-stream"),
        body: b"known-excess-secret-f45c".to_vec(),
        gate: known_gate.clone(),
    }])
    .expect("known stream server");
    let known_core = real_core(
        &known_server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new()
            .stream_response_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        stream_request::<OctetStream>(known_core),
    )
    .await
    .expect("known excess must not await the body")
    .expect_err("known stream excess");
    known_gate.release();
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{error:?} {error}").contains("known-excess-secret-f45c"));

    let media_gate = BodyGate::closed();
    let media_server = ScriptedServer::start([ResponseScript::GatedBodyWithHeaders {
        status: 200,
        headers: server_stream_headers("text/plain"),
        body: b"media-mismatch-secret-18bf".to_vec(),
        gate: media_gate.clone(),
    }])
    .expect("media stream server");
    let media_core = real_core(
        &media_server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        stream_request::<OctetStream>(media_core),
    )
    .await
    .expect("media mismatch must not await the body")
    .expect_err("stream media mismatch");
    media_gate.release();
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert!(!format!("{error:?} {error}").contains("media-mismatch-secret-18bf"));
}

#[tokio::test]
async fn invalid_stream_marker_is_rejected_preflight_and_required_parameters_are_enforced() {
    let invalid_executor = DeterministicExecutor::new();
    let invalid_hooks = Arc::new(ProbeHooks::new());
    let invalid_core = development_core(
        &invalid_executor,
        Arc::clone(&invalid_hooks),
        RuntimeConfigBuilder::new(),
    );
    let error = stream_request::<InvalidStreamMedia>(invalid_core)
        .await
        .expect_err("invalid media marker");
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert_eq!(invalid_executor.application().request_count().unwrap(), 0);
    assert!(invalid_hooks.records().iter().all(|record| !matches!(
        record,
        LifecycleRecord::Before { .. } | LifecycleRecord::Response(..)
    )));

    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());
    executor
        .application()
        .queue_response(ScriptedResponse::success("vendor").headers(stream_headers(
            "application/vnd.concord.stream; charset=utf-8; version=1",
        )))
        .expect("valid vendor media");
    let mut accepted = stream_request::<VendorStream>(core.clone())
        .await
        .expect("required media parameter accepted with extras");
    assert_eq!(
        next_stream_chunk(&mut accepted)
            .await
            .expect("vendor chunk")
            .expect("vendor chunk success"),
        b"vendor"[..]
    );

    executor
        .application()
        .queue_response(
            ScriptedResponse::success("must-not-be-yielded")
                .headers(stream_headers("application/vnd.concord.stream; version=2")),
        )
        .expect("invalid vendor media");
    let error = stream_request::<VendorStream>(core)
        .await
        .expect_err("required media parameter mismatch");
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
}

#[tokio::test]
async fn unknown_stream_is_lazy_and_first_cumulative_excess_chunk_is_withheld() {
    let lazy_gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedChunkedBodyWithHeaders {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            chunks: vec![b"ab".to_vec(), b"cd".to_vec()],
            gate: lazy_gate.clone(),
        },
        ResponseScript::Chunked {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            chunks: vec![b"abc".to_vec(), b"first-excess-secret-4fa2".to_vec()],
        },
    ])
    .expect("unknown stream server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(
        &server,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .response_body_limit(NonZeroUsize::new(1).expect("positive limit"))
            .stream_response_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    let mut lazy = tokio::time::timeout(
        Duration::from_secs(1),
        stream_request::<OctetStream>(core.clone()),
    )
    .await
    .expect("unknown stream must return before body gate")
    .expect("lazy stream response");
    assert_eq!(lazy.content_length(), None);
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );
    lazy_gate.release();
    assert_eq!(
        next_stream_chunk(&mut lazy).await.unwrap().unwrap(),
        b"ab"[..]
    );
    assert_eq!(
        next_stream_chunk(&mut lazy).await.unwrap().unwrap(),
        b"cd"[..]
    );
    assert!(next_stream_chunk(&mut lazy).await.is_none());

    let mut overflow = stream_request::<OctetStream>(core)
        .await
        .expect("overflow stream opens");
    assert_eq!(
        next_stream_chunk(&mut overflow).await.unwrap().unwrap(),
        b"abc"[..]
    );
    let error = next_stream_chunk(&mut overflow)
        .await
        .expect("overflow result")
        .expect_err("first excess chunk withheld");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{error:?} {error}").contains("first-excess-secret-4fa2"));
    assert!(next_stream_chunk(&mut overflow).await.is_none());
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
    assert!(
        hooks
            .records()
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Complete { .. }))
            .all(|record| matches!(
                record,
                LifecycleRecord::Complete {
                    outcome: CompletionOutcome::StreamOpened,
                    ..
                }
            ))
    );
}

#[cfg(feature = "gzip")]
#[tokio::test]
async fn response_stream_limit_counts_reqwest_post_decompression_bytes() {
    const GZIP_128_X: &[u8] = &[
        31, 139, 8, 0, 0, 0, 0, 0, 0, 10, 171, 168, 24, 88, 0, 0, 55, 138, 224, 172, 128, 0, 0, 0,
    ];
    let server = ScriptedServer::start([ResponseScript::Complete {
        status: 200,
        headers: vec![
            ("Content-Encoding".into(), "gzip".into()),
            ("Content-Type".into(), "application/octet-stream".into()),
        ],
        body: GZIP_128_X.to_vec(),
    }])
    .expect("compressed stream server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new()
            .stream_response_limit(NonZeroUsize::new(64).expect("positive limit")),
    );
    let mut response = stream_request::<OctetStream>(core)
        .await
        .expect("compressed stream opens");
    assert_eq!(response.content_length(), None);
    let error = next_stream_chunk(&mut response)
        .await
        .expect("post-decompression limit result")
        .expect_err("128 decoded bytes exceed a 64-byte stream limit");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(next_stream_chunk(&mut response).await.is_none());
}

#[tokio::test]
async fn response_stream_native_failure_is_safe_and_emits_no_second_completion() {
    let server = ScriptedServer::start([ResponseScript::TruncatedWithHeaders {
        status: 200,
        declared_length: 6,
        prefix: b"short".to_vec(),
        headers: server_stream_headers("application/octet-stream"),
    }])
    .expect("truncated stream server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let mut response = stream_request::<OctetStream>(core)
        .await
        .expect("truncated stream opens after valid head");
    let mut saw_native_error = None;
    while let Some(chunk) = next_stream_chunk(&mut response).await {
        match chunk {
            Ok(bytes) => assert_eq!(bytes, b"short"[..]),
            Err(error) => {
                saw_native_error = Some(error);
                break;
            }
        }
    }
    let error = saw_native_error.expect("truncation must become a stream item error");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{error:?} {error}").contains("short"));
    assert!(next_stream_chunk(&mut response).await.is_none());
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    assert_eq!(
        completion(&hooks.records()).1,
        CompletionOutcome::StreamOpened
    );
}

#[tokio::test]
async fn caller_stream_polling_is_outside_operation_deadline_and_drop_is_terminal() {
    let deadline_gate = BodyGate::closed();
    let drop_gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedChunkedBodyWithHeaders {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            chunks: vec![b"after-deadline".to_vec()],
            gate: deadline_gate.clone(),
        },
        ResponseScript::GatedChunkedWithHeaders {
            status: 200,
            headers: server_stream_headers("application/octet-stream"),
            first_chunk: b"first".to_vec(),
            remaining_chunk: b"must-not-be-observed".to_vec(),
            gate: drop_gate.clone(),
        },
    ])
    .expect("deadline and drop stream server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let mut after_deadline = stream_request::<OctetStream>(core.clone())
        .timeout(Duration::from_millis(100))
        .await
        .expect("stream opens inside deadline");
    tokio::time::sleep(Duration::from_millis(180)).await;
    deadline_gate.release();
    assert_eq!(
        next_stream_chunk(&mut after_deadline)
            .await
            .expect("post-deadline chunk")
            .expect("post-deadline polling is allowed"),
        b"after-deadline"[..]
    );
    assert!(next_stream_chunk(&mut after_deadline).await.is_none());

    let mut dropped = stream_request::<OctetStream>(core)
        .await
        .expect("drop stream opens");
    assert_eq!(
        next_stream_chunk(&mut dropped).await.unwrap().unwrap(),
        b"first"[..]
    );
    drop(dropped);
    drop_gate.release();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
    assert!(
        hooks
            .records()
            .iter()
            .filter(|record| matches!(record, LifecycleRecord::Complete { .. }))
            .all(|record| matches!(
                record,
                LifecycleRecord::Complete {
                    outcome: CompletionOutcome::StreamOpened,
                    ..
                }
            ))
    );
}

#[tokio::test]
async fn buffered_codec_direct_and_metadata_results_share_one_execution_path() {
    PROBE_DECODER_CALLS.store(0, Ordering::SeqCst);
    PROBE_VALIDATOR_CALLS.store(0, Ordering::SeqCst);
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let runtime = RuntimeConfigBuilder::new()
        .request_body_limit(NonZeroUsize::new(5).expect("positive limit"));
    let core = development_core(&executor, Arc::clone(&hooks), runtime);
    let encoder_calls = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .method(Method::POST)
                .safe_url("http://execution.example.test/v1/codec/round-trip")
                .body("exact"),
            probe_response("first"),
        )
        .expect("direct codec response");
    let value = codec_request::<ProbeEncoder, ProbeDecoder>(
        core.clone(),
        probe_input(b"exact", &encoder_calls),
    )
    .await
    .expect("direct codec output");
    assert_eq!(value, "decoded:first");

    executor
        .application()
        .queue_response(probe_response("second"))
        .expect("metadata codec response");
    let response =
        codec_request::<ProbeEncoder, ProbeDecoder>(core, probe_input(b"again", &encoder_calls))
            .response()
            .await
            .expect("metadata codec output");
    assert_eq!(response.value(), "decoded:second");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.metadata().endpoint(), CODEC_ENDPOINT);
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 2);
    assert_eq!(PROBE_VALIDATOR_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(PROBE_DECODER_CALLS.load(Ordering::SeqCst), 2);
    let captures = executor.application().captures().expect("captures");
    assert_eq!(captures.len(), 2);
    for capture in captures {
        assert_eq!(capture.body_category(), BodyCategory::Bytes);
        assert_eq!(capture.known_body_length(), Some(5));
    }
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn encoding_failure_and_request_limit_prevent_native_execution_safely() {
    let failing_executor = DeterministicExecutor::new();
    let failing_hooks = Arc::new(ProbeHooks::new());
    let failing_core = development_core(
        &failing_executor,
        Arc::clone(&failing_hooks),
        RuntimeConfigBuilder::new(),
    );
    let failing_calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, ProbeDecoder>(
        failing_core,
        EncoderInput {
            bytes: Bytes::from_static(b"request-encoding-body-secret-98d2"),
            calls: Arc::clone(&failing_calls),
            fail: true,
        },
    )
    .await
    .expect_err("encoder failure");
    assert_eq!(error.category(), ErrorCategory::RequestEncoding);
    assert_eq!(failing_calls.load(Ordering::SeqCst), 1);
    assert_eq!(failing_executor.application().request_count().unwrap(), 0);
    assert_eq!(failing_hooks.completion_returned.load(Ordering::SeqCst), 1);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("request-encoding-body-secret-98d2"));
        assert!(!formatted.contains("request-codec-secret-ec2d"));
    }

    let limit_executor = DeterministicExecutor::new();
    let limit_hooks = Arc::new(ProbeHooks::new());
    let limit_core = development_core(
        &limit_executor,
        Arc::clone(&limit_hooks),
        RuntimeConfigBuilder::new()
            .request_body_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    limit_executor
        .application()
        .queue_response(ScriptedResponse::success("ok"))
        .expect("below-limit response");
    assert_eq!(
        codec_request::<Text, Text>(limit_core.clone(), "abc".to_owned())
            .await
            .expect("below request limit"),
        "ok"
    );
    let limit_calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, ProbeDecoder>(
        limit_core,
        probe_input(b"first-excess-request-secret", &limit_calls),
    )
    .await
    .expect_err("request body limit");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(limit_calls.load(Ordering::SeqCst), 1);
    assert_eq!(limit_executor.application().request_count().unwrap(), 1);
    assert_eq!(limit_hooks.completion_returned.load(Ordering::SeqCst), 2);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("first-excess-request-secret"));
    }
}

#[tokio::test]
async fn content_type_and_decoder_failures_are_safe_and_phase_ordered() {
    REJECTING_DECODER_CALLS.store(0, Ordering::SeqCst);
    let contract_executor = DeterministicExecutor::new();
    contract_executor
        .application()
        .queue_response(ScriptedResponse::success("body-must-not-reach-decoder"))
        .expect("contract response");
    let contract_core = development_core(
        &contract_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, RejectingDecoder>(
        contract_core,
        probe_input(b"input", &calls),
    )
    .await
    .expect_err("content-type mismatch");
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert_eq!(REJECTING_DECODER_CALLS.load(Ordering::SeqCst), 0);

    FAILING_DECODER_CALLS.store(0, Ordering::SeqCst);
    let decode_executor = DeterministicExecutor::new();
    decode_executor
        .application()
        .queue_response(ScriptedResponse::success(
            "successful-response-body-secret-0b6c",
        ))
        .expect("decode response");
    let decode_core = development_core(
        &decode_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error =
        codec_request::<ProbeEncoder, FailingDecoder>(decode_core, probe_input(b"input", &calls))
            .await
            .expect_err("decoder failure");
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    assert_eq!(FAILING_DECODER_CALLS.load(Ordering::SeqCst), 1);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("successful-response-body-secret-0b6c"));
        assert!(!formatted.contains("response-decoder-secret-c413"));
        assert!(!formatted.contains("content-type-secret-4fea"));
    }
}

#[tokio::test]
async fn preparation_failure_skips_before_send_and_transport_but_still_completes() {
    let executor = DeterministicExecutor::new();
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = Request::new(core, InvalidEndpoint)
        .await
        .expect_err("invalid route");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let records = hooks.records();
    assert!(matches!(
        records.as_slice(),
        [LifecycleRecord::Complete {
            outcome: CompletionOutcome::Configuration,
            status: None,
            ..
        }]
    ));
}

#[tokio::test(start_paused = true)]
async fn timeout_in_before_send_is_fixed_then_completion_runs_outside_deadline() {
    let executor = DeterministicExecutor::new();
    let before_gate = Gate::closed();
    let completion_gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks {
        before_gate: Some(before_gate.clone()),
        completion_gate: Some(completion_gate.clone()),
        ..ProbeHooks::new()
    });
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let task =
        tokio::spawn(async move { bytes_request(core).timeout(Duration::from_secs(5)).await });

    before_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    completion_gate.wait_until_reached().await;
    assert!(
        !task.is_finished(),
        "completion hook must delay future delivery"
    );

    let records = hooks.records();
    let before_id = match records.first() {
        Some(LifecycleRecord::Before { operation_id, .. }) => *operation_id,
        _ => panic!("before-send record"),
    };
    let (completion_id, outcome, status, elapsed) = completion(&records);
    assert_eq!(completion_id, before_id);
    assert_eq!(outcome, CompletionOutcome::Timeout);
    assert_eq!(status, None);
    assert_eq!(elapsed, Duration::from_secs(5));

    tokio::time::advance(Duration::from_secs(60)).await;
    assert_eq!(completion(&hooks.records()).3, elapsed);
    assert!(!task.is_finished());
    completion_gate.open();
    let error = task.await.expect("join").expect_err("fixed timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn success_and_status_results_remain_fixed_while_completion_is_delayed() {
    let success_executor = DeterministicExecutor::new();
    success_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"fixed")))
        .expect("success script");
    let success_gate = Gate::closed();
    let success_hooks = Arc::new(ProbeHooks {
        completion_gate: Some(success_gate.clone()),
        ..ProbeHooks::new()
    });
    let success_core = development_core(
        &success_executor,
        Arc::clone(&success_hooks),
        RuntimeConfigBuilder::new(),
    );
    let success_task = tokio::spawn(async move {
        bytes_request(success_core)
            .timeout(Duration::from_secs(1))
            .await
    });
    success_gate.wait_until_reached().await;
    let success_elapsed = completion(&success_hooks.records()).3;
    tokio::time::advance(Duration::from_secs(10)).await;
    assert!(!success_task.is_finished());
    assert_eq!(completion(&success_hooks.records()).3, success_elapsed);
    success_gate.open();
    assert_eq!(
        success_task.await.expect("join").expect("fixed success"),
        b"fixed"[..]
    );

    let status_executor = DeterministicExecutor::new();
    status_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"status"),
        ))
        .expect("status script");
    let status_gate = Gate::closed();
    let status_hooks = Arc::new(ProbeHooks {
        completion_gate: Some(status_gate.clone()),
        ..ProbeHooks::new()
    });
    let status_core = development_core(
        &status_executor,
        Arc::clone(&status_hooks),
        RuntimeConfigBuilder::new(),
    );
    let status_task = tokio::spawn(async move {
        bytes_request(status_core)
            .timeout(Duration::from_secs(1))
            .await
    });
    status_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(10)).await;
    status_gate.open();
    let error = status_task.await.expect("join").expect_err("fixed status");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(
        completion(&status_hooks.records()).1,
        CompletionOutcome::HttpStatus
    );
}

#[tokio::test]
async fn cancellation_while_awaiting_completion_may_prevent_delivery() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("response");
    let completion_gate = Gate::closed();
    let hooks = Arc::new(ProbeHooks {
        completion_gate: Some(completion_gate.clone()),
        ..ProbeHooks::new()
    });
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let task = tokio::spawn(async move { bytes_request(core).await });
    completion_gate.wait_until_reached().await;
    task.abort();
    assert!(task.await.expect_err("cancelled join").is_cancelled());
    completion_gate.open();
    tokio::task::yield_now().await;
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.records().len(), 3);
}

#[tokio::test(start_paused = true)]
async fn one_deadline_bounds_native_execution_and_response_head_hook() {
    let native_executor = DeterministicExecutor::new();
    let native_gate = Gate::closed();
    native_executor
        .application()
        .pause_before_execution(native_gate.clone())
        .expect("native gate");
    native_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("native response");
    let native_hooks = Arc::new(ProbeHooks::new());
    let native_core = development_core(
        &native_executor,
        Arc::clone(&native_hooks),
        RuntimeConfigBuilder::new(),
    );
    let native_task = tokio::spawn(async move {
        bytes_request(native_core)
            .timeout(Duration::from_secs(2))
            .await
    });
    native_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(2)).await;
    let error = native_task
        .await
        .expect("join")
        .expect_err("native timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(completion(&native_hooks.records()).2, None);

    let response_executor = DeterministicExecutor::new();
    response_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("response");
    let response_gate = Gate::closed();
    let response_hooks = Arc::new(ProbeHooks {
        response_gate: Some(response_gate.clone()),
        ..ProbeHooks::new()
    });
    let response_core = development_core(
        &response_executor,
        Arc::clone(&response_hooks),
        RuntimeConfigBuilder::new(),
    );
    let response_task = tokio::spawn(async move {
        bytes_request(response_core)
            .timeout(Duration::from_secs(2))
            .await
    });
    response_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(2)).await;
    let error = response_task
        .await
        .expect("join")
        .expect_err("response hook timeout");
    assert_eq!(error.category(), ErrorCategory::Timeout);
    let records = response_hooks.records();
    let before_id = match records[0] {
        LifecycleRecord::Before { operation_id, .. } => operation_id,
        _ => panic!("before-send record"),
    };
    let response_id = match records[1] {
        LifecycleRecord::Response(id, StatusCode::OK) => id,
        _ => panic!("response-head record"),
    };
    assert_eq!(response_id, before_id);
    assert_eq!(completion(&records).0, before_id);
    assert_eq!(completion(&records).2, Some(StatusCode::OK));
}

#[tokio::test(start_paused = true)]
async fn delayed_polling_consumes_no_budget_and_disable_timeout_is_distinct() {
    let delayed_executor = DeterministicExecutor::new();
    delayed_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"late poll")))
        .expect("response");
    let delayed_core = development_core(
        &delayed_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let request = Request::new(
        delayed_core,
        BytesEndpoint::new(Some(Duration::from_secs(1))),
    );
    tokio::time::advance(Duration::from_secs(100)).await;
    assert_eq!(
        request.await.expect("deadline begins on poll"),
        b"late poll"[..]
    );

    let disabled_executor = DeterministicExecutor::new();
    let disabled_gate = Gate::closed();
    disabled_executor
        .application()
        .pause_before_execution(disabled_gate.clone())
        .expect("disabled gate");
    disabled_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"disabled")))
        .expect("response");
    let disabled_core = development_core(
        &disabled_executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let task = tokio::spawn(async move { bytes_request(disabled_core).disable_timeout().await });
    disabled_gate.wait_until_reached().await;
    tokio::time::advance(Duration::from_secs(100)).await;
    assert!(!task.is_finished());
    disabled_gate.open();
    assert_eq!(
        task.await.expect("join").expect("disabled timeout"),
        b"disabled"[..]
    );
}

#[tokio::test]
async fn transport_failures_are_safely_classified_and_complete_once() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_failure(TransportFailure::Connect)
        .expect("failure");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = bytes_request(core).await.expect_err("connect failure");
    assert_eq!(error.category(), ErrorCategory::Connect);
    let records = hooks.records();
    assert_eq!(records.len(), 2);
    assert_eq!(completion(&records).1, CompletionOutcome::Connect);
}

#[tokio::test]
async fn real_reqwest_transmits_and_decodes_text_and_custom_buffered_codecs() {
    let server = ScriptedServer::start([
        ResponseScript::ok("text-response-♥".as_bytes().to_vec()),
        ResponseScript::ok(b"custom-response".to_vec())
            .with_header("Content-Type", "application/x-concord-probe"),
    ])
    .expect("codec server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let text = codec_request::<Text, Text>(core.clone(), "text-request-♥".to_owned())
        .response()
        .await
        .expect("text response");
    assert_eq!(text.value(), "text-response-♥");

    let encoder_calls = Arc::new(AtomicUsize::new(0));
    let custom = codec_request::<ProbeEncoder, RealCustomDecoder>(
        core,
        probe_input(b"custom-request", &encoder_calls),
    )
    .await
    .expect("custom response");
    assert_eq!(custom, "custom-response");
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 1);
    server
        .wait_for_requests(2, Duration::from_secs(1))
        .expect("request recording");
    let requests = server.requests();
    assert_eq!(requests[0].method(), "POST");
    assert_eq!(requests[0].body(), "text-request-♥".as_bytes());
    assert_eq!(
        requests[0].header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(requests[0].header("accept"), Some("text/plain"));
    assert_eq!(requests[1].body(), b"custom-request");
    assert_eq!(
        requests[1].header("content-type"),
        Some("application/x-concord-probe")
    );
    assert_eq!(
        requests[1].header("accept"),
        Some("application/x-concord-probe")
    );
}

#[cfg(feature = "json")]
#[tokio::test]
async fn real_reqwest_json_round_trip_is_feature_owned() {
    use crate::codec::Json;

    let server = ScriptedServer::start([ResponseScript::ok(b"[3,5,8]".to_vec())
        .with_header("Content-Type", "application/problem+json; version=1")])
    .expect("JSON server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let output = codec_request::<Json<Vec<u32>>, Json<Vec<u32>>>(core, vec![1, 2])
        .await
        .expect("JSON response");
    assert_eq!(output, vec![3, 5, 8]);
    server
        .wait_for_requests(1, Duration::from_secs(1))
        .expect("JSON request recording");
    let requests = server.requests();
    assert_eq!(requests[0].body(), b"[1,2]");
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
    assert_eq!(requests[0].header("accept"), Some("application/json"));
}

#[tokio::test]
async fn buffered_decoder_never_observes_known_or_first_excess_response_bytes() {
    OVERFLOW_DECODER_CALLS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([
        ResponseScript::ok(b"four".to_vec()),
        ResponseScript::ok(b"known".to_vec()),
        ResponseScript::Chunked {
            status: 200,
            headers: Vec::new(),
            chunks: vec![b"four".to_vec(), b"first-excess-secret".to_vec()],
        },
    ])
    .expect("response limit server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new()
            .response_body_limit(NonZeroUsize::new(4).expect("positive limit")),
    );
    assert_eq!(
        codec_request::<Text, Text>(core.clone(), "four".to_owned())
            .await
            .expect("exact response limit"),
        "four"
    );
    for expected_path in ["known response preflight", "unknown response overflow"] {
        let error = codec_request::<Text, OverflowDecoder>(core.clone(), "input".to_owned())
            .await
            .expect_err(expected_path);
        assert_eq!(error.category(), ErrorCategory::ResponseBody);
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains("known"));
            assert!(!formatted.contains("first-excess-secret"));
        }
    }
    assert_eq!(OVERFLOW_DECODER_CALLS.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn real_content_type_mismatch_rejects_before_response_body_polling() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 200,
        body: b"content-contract-body-secret-5dd4".to_vec(),
        gate: gate.clone(),
    }])
    .expect("content contract server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let error = tokio::time::timeout(
        Duration::from_secs(1),
        codec_request::<Text, RejectingDecoder>(core, "input".to_owned()),
    )
    .await
    .expect("content validation must not wait for the body")
    .expect_err("missing content type");
    gate.release();
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    for formatted in [error.to_string(), format!("{error:?}")] {
        assert!(!formatted.contains("content-contract-body-secret-5dd4"));
    }
}

#[tokio::test]
async fn real_reqwest_stream_upload_transmits_exact_bytes_and_owned_media_type() {
    let server = ScriptedServer::start([ResponseScript::ok(b"upload-ok".to_vec())])
        .expect("stream upload server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let polls = Arc::new(AtomicUsize::new(0));
    let stream = StreamBody::from_stream(BodyPollProbe::new(
        [
            ProbeFrame::Data(b"real-"),
            ProbeFrame::Data(b"stream-"),
            ProbeFrame::Data(b"upload"),
        ],
        &polls,
    ))
    .exact_length(18);
    let body = RequestBody::stream(stream)
        .content_type("application/x-concord-stream")
        .expect("valid stream content type");
    assert_eq!(
        body_request(core, body).await.expect("stream upload"),
        b"upload-ok"[..]
    );
    assert_eq!(polls.load(Ordering::SeqCst), 3);
    server
        .wait_for_requests(1, Duration::from_secs(2))
        .expect("recorded stream request");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method(), "POST");
    assert_eq!(requests[0].target(), "/v1/body/upload");
    assert_eq!(requests[0].body(), b"real-stream-upload");
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/x-concord-stream")
    );
    assert_eq!(requests[0].header("content-length"), Some("18"));
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn managed_producer_failure_precedes_reqwest_transport_failure() {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("managed producer listener");
    let address = listener.local_addr().expect("managed producer address");
    let peer = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.expect("managed producer accept");
        std::future::pending::<()>().await;
    });
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    let hooks = Arc::new(ProbeHooks::new());
    let core = ClientCore::build(
        &format!("http://{address}/v1/"),
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().hooks(hooks.clone()),
    )
    .expect("managed producer core");
    let polls = Arc::new(AtomicUsize::new(0));
    let body = StreamBody::from_stream(BodyPollProbe::new(
        [ProbeFrame::Failure("native-producer-secret-88be")],
        &polls,
    ))
    .exact_length(1);
    let error = tokio::time::timeout(
        Duration::from_secs(3),
        body_request(core, RequestBody::stream(body)),
    )
    .await
    .expect("managed producer failure should be prompt")
    .expect_err("managed producer request must fail");
    peer.abort();
    let _ = peer.await;
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.completion_returned.load(Ordering::SeqCst), 1);
    let formatted = format!("{error:?} {error}");
    assert!(!formatted.contains("native-producer-secret-88be"));
}

#[tokio::test]
async fn cancellation_drops_direct_and_factory_created_request_streams_without_completion() {
    async fn cancel(body: RequestBody, reached: Arc<tokio::sync::Notify>) -> Arc<ProbeHooks> {
        let executor = DeterministicExecutor::new();
        let hooks = Arc::new(ProbeHooks::new());
        let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());
        let task = tokio::spawn(async move { body_request(core, body).await });
        reached.notified().await;
        assert_eq!(executor.application().request_count().unwrap(), 0);
        task.abort();
        let join = task.await.expect_err("request task must be cancelled");
        assert!(join.is_cancelled());
        tokio::task::yield_now().await;
        assert_eq!(executor.application().request_count().unwrap(), 0);
        hooks
    }

    let direct_polls = Arc::new(AtomicUsize::new(0));
    let direct_dropped = Arc::new(AtomicBool::new(false));
    let direct_reached = Arc::new(tokio::sync::Notify::new());
    let direct_body = RequestBody::stream(StreamBody::from_stream(PendingBodyStream {
        polls: Arc::clone(&direct_polls),
        dropped: Arc::clone(&direct_dropped),
        reached: Arc::clone(&direct_reached),
    }));
    let direct_hooks = cancel(direct_body, Arc::clone(&direct_reached)).await;
    let direct_polls_at_drop = direct_polls.load(Ordering::SeqCst);
    assert!(direct_polls_at_drop > 0);
    assert!(direct_dropped.load(Ordering::SeqCst));
    assert_eq!(direct_hooks.completion_returned.load(Ordering::SeqCst), 0);
    tokio::task::yield_now().await;
    assert_eq!(direct_polls.load(Ordering::SeqCst), direct_polls_at_drop);

    let factory_calls = Arc::new(AtomicUsize::new(0));
    let factory_polls = Arc::new(AtomicUsize::new(0));
    let factory_dropped = Arc::new(AtomicBool::new(false));
    let factory_reached = Arc::new(tokio::sync::Notify::new());
    let factory_body = RequestBody::factory(PendingBodyFactory {
        create_calls: Arc::clone(&factory_calls),
        polls: Arc::clone(&factory_polls),
        dropped: Arc::clone(&factory_dropped),
        reached: Arc::clone(&factory_reached),
    });
    let factory_hooks = cancel(factory_body, Arc::clone(&factory_reached)).await;
    let factory_polls_at_drop = factory_polls.load(Ordering::SeqCst);
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    assert!(factory_polls_at_drop > 0);
    assert!(factory_dropped.load(Ordering::SeqCst));
    assert_eq!(factory_hooks.completion_returned.load(Ordering::SeqCst), 0);
    tokio::task::yield_now().await;
    assert_eq!(factory_polls.load(Ordering::SeqCst), factory_polls_at_drop);
}

#[tokio::test]
async fn real_reqwest_bytes_are_finite_complete_only_and_body_failures_are_safe() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"within".to_vec()),
        ResponseScript::ok(b"known-too-large".to_vec()),
        ResponseScript::Chunked {
            status: 200,
            headers: Vec::new(),
            chunks: vec![b"ab".to_vec(), b"cd".to_vec()],
        },
        ResponseScript::Chunked {
            status: 200,
            headers: Vec::new(),
            chunks: vec![b"abcd".to_vec(), b"excess".to_vec()],
        },
        ResponseScript::Truncated {
            status: 200,
            declared_length: 6,
            prefix: b"short".to_vec(),
        },
    ])
    .expect("bytes server");
    let runtime = || RuntimeConfigBuilder::new().response_body_limit(NonZeroUsize::new(6).unwrap());
    let core = real_core(&server, Arc::new(ProbeHooks::new()), runtime());
    assert_eq!(
        bytes_request(core.clone()).await.expect("within"),
        b"within"[..]
    );
    assert_eq!(
        bytes_request(core.clone())
            .await
            .expect_err("known excessive length")
            .category(),
        ErrorCategory::ResponseBody
    );
    assert_eq!(
        bytes_request(core.clone()).await.expect("unknown within"),
        b"abcd"[..]
    );
    let overflow = bytes_request(core.clone())
        .await
        .expect_err("unknown overflow");
    assert_eq!(overflow.category(), ErrorCategory::ResponseBody);
    assert!(!format!("{overflow:?} {overflow}").contains("abcd"));
    assert!(!format!("{overflow:?} {overflow}").contains("excess"));
    assert_eq!(
        bytes_request(core)
            .await
            .expect_err("native body failure")
            .category(),
        ErrorCategory::ResponseBody
    );
    assert_eq!(server.request_count(), 5);
}

#[tokio::test]
async fn real_reqwest_bytes_collection_is_bounded_by_operation_deadline() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 200,
        body: b"delayed body".to_vec(),
        gate: gate.clone(),
    }])
    .expect("gated bytes server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(&server, Arc::clone(&hooks), RuntimeConfigBuilder::new());
    let error = bytes_request(core)
        .timeout(Duration::from_millis(500))
        .await
        .expect_err("body deadline");
    gate.release();
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(completion(&hooks.records()).2, Some(StatusCode::OK));
}

#[tokio::test]
async fn no_content_accepts_every_2xx_without_polling_the_body() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedBody {
            status: 200,
            body: b"must-not-be-polled".to_vec(),
            gate: gate.clone(),
        },
        ResponseScript::status(204, Vec::new()),
        ResponseScript::status(205, b"ignored".to_vec()),
    ])
    .expect("no-content server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    tokio::time::timeout(Duration::from_secs(1), no_content_request(core.clone()))
        .await
        .expect("200 must not wait for body gate")
        .expect("200 no-content");
    gate.release();
    no_content_request(core.clone())
        .await
        .expect("204 no-content");
    let response = no_content_request(core)
        .response()
        .await
        .expect("205 metadata");
    assert_eq!(response.status(), StatusCode::RESET_CONTENT);
    assert_eq!(response.value(), &());
}

#[tokio::test]
async fn all_non_2xx_statuses_use_complete_only_bounded_terminal_capture() {
    let deadline_gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::status(302, b"redirect".to_vec()),
        ResponseScript::status(404, b"missing".to_vec()),
        ResponseScript::status(500, b"known-too-large".to_vec()),
        ResponseScript::Chunked {
            status: 502,
            headers: Vec::new(),
            chunks: vec![b"prefix".to_vec(), b"overflow".to_vec()],
        },
        ResponseScript::Truncated {
            status: 503,
            declared_length: 7,
            prefix: b"short".to_vec(),
        },
        ResponseScript::GatedBody {
            status: 504,
            body: b"late".to_vec(),
            gate: deadline_gate.clone(),
        },
    ])
    .expect("status server");
    let hooks = Arc::new(ProbeHooks::new());
    let core = real_core(
        &server,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().status_body_limit(NonZeroUsize::new(8).unwrap()),
    );

    let redirect = bytes_request(core.clone()).await.expect_err("3xx terminal");
    assert_eq!(redirect.http_status(), Some(StatusCode::FOUND));
    assert_eq!(
        redirect.http_response().unwrap().body().state(),
        StatusBodyState::Complete
    );
    let missing = bytes_request(core.clone()).await.expect_err("4xx terminal");
    assert_eq!(missing.http_status(), Some(StatusCode::NOT_FOUND));
    assert_eq!(
        missing
            .http_response()
            .unwrap()
            .body()
            .bytes()
            .unwrap()
            .as_ref(),
        b"missing"
    );
    let known = bytes_request(core.clone()).await.expect_err("5xx terminal");
    assert_eq!(known.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(
        known.http_response().unwrap().body().state(),
        StatusBodyState::OmittedKnownTooLarge
    );
    let overflow = bytes_request(core.clone())
        .await
        .expect_err("unknown overflow");
    assert_eq!(overflow.http_status(), Some(StatusCode::BAD_GATEWAY));
    assert_eq!(
        overflow.http_response().unwrap().body().state(),
        StatusBodyState::OmittedLimitExceeded
    );
    assert!(overflow.http_response().unwrap().body().bytes().is_none());
    let read = bytes_request(core.clone())
        .await
        .expect_err("status read failure");
    assert_eq!(read.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(
        read.http_response().unwrap().body().state(),
        StatusBodyState::UnavailableReadFailure
    );
    let deadline = bytes_request(core)
        .timeout(Duration::from_millis(500))
        .await
        .expect_err("status capture deadline");
    deadline_gate.release();
    assert_eq!(deadline.category(), ErrorCategory::HttpStatus);
    assert_eq!(deadline.http_status(), Some(StatusCode::GATEWAY_TIMEOUT));
    assert_eq!(
        deadline.http_response().unwrap().body().state(),
        StatusBodyState::UnavailableDeadline
    );
    let records = hooks.records();
    let completions = records
        .iter()
        .filter_map(|record| match record {
            LifecycleRecord::Complete {
                outcome, status, ..
            } => Some((*outcome, *status)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(completions.len(), 6);
    assert!(
        completions
            .iter()
            .all(|(outcome, _)| *outcome == CompletionOutcome::HttpStatus)
    );
    assert_eq!(
        completions
            .iter()
            .map(|(_, status)| *status)
            .collect::<Vec<_>>(),
        [
            Some(StatusCode::FOUND),
            Some(StatusCode::NOT_FOUND),
            Some(StatusCode::INTERNAL_SERVER_ERROR),
            Some(StatusCode::BAD_GATEWAY),
            Some(StatusCode::SERVICE_UNAVAILABLE),
            Some(StatusCode::GATEWAY_TIMEOUT),
        ]
    );
}

#[test]
fn public_associated_futures_are_named_sendable_box_futures() {
    fn assert_box_future<T>(_: BoxFuture<'static, T>) {}

    let executor = DeterministicExecutor::new();
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    assert_box_future(bytes_request(core.clone()).into_future());
    assert_box_future(bytes_request(core).response().into_future());
}

impl fmt::Debug for ProbeHooks {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProbeHooks")
            .field(
                "record_count",
                &self.records.lock().map(|records| records.len()).ok(),
            )
            .field("veto", &self.veto.load(Ordering::SeqCst))
            .field(
                "completion_returned",
                &self.completion_returned.load(Ordering::SeqCst),
            )
            .finish()
    }
}
