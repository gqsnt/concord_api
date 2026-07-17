use std::fmt;
use std::future::IntoFuture;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use concord_test_support::executor::DeterministicExecutor;
use concord_test_support::gate::Gate;
use concord_test_support::script::{
    BodyCategory, ExpectedRequest, ScriptedResponse, TransportFailure,
};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
use http::{Method, StatusCode};

use super::*;
use crate::codec::{CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, Text};
use crate::error::{ErrorCategory, HookError, StatusBodyState};
use crate::hooks::{BoxFuture, RuntimeHooks};
use crate::http::HttpConfigBuilder;
use crate::request::{
    BodyMetadata, BodySpec, BufferedResponse, BytesResponse, CodecBody, EmptyBody, HeadBuilder,
    NoContentResponse, PreparedHead, RequestMeta,
};
use crate::runtime::RuntimeConfigBuilder;

const ENDPOINT: &str = "ExecutionApi::health::bytes";
const NO_CONTENT_ENDPOINT: &str = "ExecutionApi::health::ping";
const CODEC_ENDPOINT: &str = "ExecutionApi::codec::round_trip";

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
            <CodecBody<Encoder> as BodySpec>::metadata(),
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
