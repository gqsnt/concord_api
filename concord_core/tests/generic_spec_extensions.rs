use std::future::poll_fn;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

use concord_core::advanced::{
    BeforeSend, BodyError, BodyFactory, BodyKind, BoxFuture, Buffered, ByteStream, Discard,
    HeaderValue, HookError, LengthHint, RateLimitError, RateLimitPermit, RateLimitRequest,
    RateLimiter, RequestBody, RequestBodyContext, RequestBodyMetadata, RequestBodySpec,
    RequestContentType, ResponseBodyError, ResponseBodyErrorKind, ResponseBodySpec, ResponseHead,
    RuntimeHooks, SafeProxy, StreamBody, Streaming,
};
use concord_core::prelude::{ApiResponse, Bytes, ErrorCategory};
use concord_macros::api;
use concord_test_support::server::{ResponseScript, ScriptedServer};
use futures_core::Stream as FuturesStream;

pub struct DownstreamBody;

pub struct DownstreamInput {
    bytes: Bytes,
    metadata_calls: Arc<AtomicUsize>,
    prepare_calls: Arc<AtomicUsize>,
}

impl RequestBodySpec for DownstreamBody {
    type Input = DownstreamInput;

    fn metadata(
        input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        input.metadata_calls.fetch_add(1, Ordering::SeqCst);
        Ok(RequestBodyMetadata {
            length: LengthHint::Exact(
                u64::try_from(input.bytes.len()).expect("test body length fits u64"),
            ),
            content_type: RequestContentType::Exact(HeaderValue::from_static(
                "application/x-downstream-body",
            )),
            kind: BodyKind::Buffered,
        })
    }

    fn prepare(
        input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        input.prepare_calls.fetch_add(1, Ordering::SeqCst);
        RequestBody::bytes(input.bytes).content_type("application/x-downstream-body")
    }
}

pub struct DownstreamBuffered;

impl ResponseBodySpec for DownstreamBuffered {
    type Output = String;
    type Mode = Buffered;

    fn accept() -> Option<&'static str> {
        Some("application/x-downstream-buffered")
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        if head
            .headers
            .get("content-type")
            .is_some_and(|value| value == "application/x-downstream-buffered")
        {
            Ok(())
        } else {
            Err(ResponseBodyError::new(
                concord_core::advanced::ResponseBodyErrorKind::InvalidMediaType,
            ))
        }
    }

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Self::Output, ResponseBodyError> {
        String::from_utf8(body.to_vec()).map_err(|error| {
            drop(error);
            ResponseBodyError::new(concord_core::advanced::ResponseBodyErrorKind::Decode)
        })
    }
}

pub struct DownstreamStreaming;

pub struct TaggedStream(ByteStream);

impl ResponseBodySpec for DownstreamStreaming {
    type Output = TaggedStream;
    type Mode = Streaming;

    fn accept() -> Option<&'static str> {
        Some("application/x-downstream-stream")
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        if head
            .headers
            .get("content-type")
            .is_some_and(|value| value == "application/x-downstream-stream")
        {
            Ok(())
        } else {
            Err(ResponseBodyError::new(
                concord_core::advanced::ResponseBodyErrorKind::InvalidMediaType,
            ))
        }
    }

    fn from_body(
        body: ByteStream,
        _head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        Ok(TaggedStream(body))
    }
}

pub struct DownstreamDiscard;

#[derive(Debug, Eq, PartialEq)]
pub struct Discarded;

impl ResponseBodySpec for DownstreamDiscard {
    type Output = Discarded;
    type Mode = Discard;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<Self::Output, ResponseBodyError> {
        Ok(Discarded)
    }
}

#[derive(Clone, Copy)]
pub enum Contradiction {
    Kind,
    ExactLength,
    LowerBound,
    UnverifiableLowerBound,
    ContentType,
    GeneratedOwnership,
    BodyPresence,
    ImpossibleManagedState,
}

pub struct ContradictoryBody;

impl RequestBodySpec for ContradictoryBody {
    type Input = Contradiction;

    fn metadata(
        input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        let metadata = match input {
            Contradiction::Kind => RequestBodyMetadata {
                length: LengthHint::Exact(0),
                content_type: RequestContentType::None,
                kind: BodyKind::Buffered,
            },
            Contradiction::ExactLength => RequestBodyMetadata {
                length: LengthHint::Exact(4),
                content_type: RequestContentType::None,
                kind: BodyKind::Buffered,
            },
            Contradiction::LowerBound => RequestBodyMetadata {
                length: LengthHint::AtLeast(4),
                content_type: RequestContentType::None,
                kind: BodyKind::Buffered,
            },
            Contradiction::UnverifiableLowerBound => RequestBodyMetadata {
                length: LengthHint::AtLeast(4),
                content_type: RequestContentType::None,
                kind: BodyKind::Stream,
            },
            Contradiction::ContentType => RequestBodyMetadata {
                length: LengthHint::Exact(3),
                content_type: RequestContentType::None,
                kind: BodyKind::Buffered,
            },
            Contradiction::GeneratedOwnership => RequestBodyMetadata {
                length: LengthHint::Exact(3),
                content_type: RequestContentType::Generated,
                kind: BodyKind::Buffered,
            },
            Contradiction::BodyPresence => RequestBodyMetadata {
                length: LengthHint::Exact(0),
                content_type: RequestContentType::None,
                kind: BodyKind::Empty,
            },
            Contradiction::ImpossibleManagedState => RequestBodyMetadata {
                length: LengthHint::Exact(4),
                content_type: RequestContentType::None,
                kind: BodyKind::Stream,
            },
        };
        Ok(metadata)
    }

    fn prepare(
        input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        match input {
            Contradiction::Kind => Ok(RequestBody::empty()),
            Contradiction::ExactLength | Contradiction::LowerBound => {
                Ok(RequestBody::bytes(Bytes::from_static(b"abc")))
            }
            Contradiction::UnverifiableLowerBound => Ok(RequestBody::factory(UnknownFactory)),
            Contradiction::ContentType => RequestBody::bytes(Bytes::from_static(b"abc"))
                .content_type("application/x-produced-only"),
            Contradiction::GeneratedOwnership | Contradiction::BodyPresence => {
                Ok(RequestBody::bytes(Bytes::from_static(b"abc")))
            }
            Contradiction::ImpossibleManagedState => Ok(RequestBody::stream(
                StreamBody::from_bytes(Bytes::from_static(b"abc")).exact_length(4),
            )),
        }
    }
}

static UNKNOWN_FACTORY_CREATES: AtomicUsize = AtomicUsize::new(0);

struct UnknownFactory;

impl BodyFactory for UnknownFactory {
    fn create(&self) -> Result<StreamBody, BodyError> {
        UNKNOWN_FACTORY_CREATES.fetch_add(1, Ordering::SeqCst);
        Ok(StreamBody::from_bytes(Bytes::from_static(b"abc")))
    }
}

static ORDER_VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static ORDER_FROM_BODY_CALLS: AtomicUsize = AtomicUsize::new(0);
static REJECT_VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static REJECT_FROM_BODY_CALLS: AtomicUsize = AtomicUsize::new(0);
static STREAM_VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static STREAM_FROM_BODY_CALLS: AtomicUsize = AtomicUsize::new(0);
static DISCARD_VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static DISCARD_FROM_BODY_CALLS: AtomicUsize = AtomicUsize::new(0);

pub struct OrderedBuffered;

impl ResponseBodySpec for OrderedBuffered {
    type Output = Bytes;
    type Mode = Buffered;

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        ORDER_VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Bytes, ResponseBodyError> {
        ORDER_FROM_BODY_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(body)
    }
}

pub struct RejectBeforePolling;

impl ResponseBodySpec for RejectBeforePolling {
    type Output = ();
    type Mode = Buffered;

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        REJECT_VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        Err(ResponseBodyError::new(
            ResponseBodyErrorKind::InvalidMediaType,
        ))
    }

    fn from_body(_body: Bytes, _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        REJECT_FROM_BODY_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

pub struct OrderedStreaming;

impl ResponseBodySpec for OrderedStreaming {
    type Output = ByteStream;
    type Mode = Streaming;

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        STREAM_VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn from_body(
        body: ByteStream,
        _head: &ResponseHead<'_>,
    ) -> Result<ByteStream, ResponseBodyError> {
        STREAM_FROM_BODY_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(body)
    }
}

pub struct OrderedDiscard;

impl ResponseBodySpec for OrderedDiscard {
    type Output = ();
    type Mode = Discard;

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        DISCARD_VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        DISCARD_FROM_BODY_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct SynchronousDeadlineGate {
    entered: Barrier,
    release: Barrier,
}

static SYNCHRONOUS_DEADLINE_GATE: Mutex<Option<Arc<SynchronousDeadlineGate>>> = Mutex::new(None);

pub struct DeadlineChecked;

impl ResponseBodySpec for DeadlineChecked {
    type Output = Bytes;
    type Mode = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Bytes, ResponseBodyError> {
        let gate = SYNCHRONOUS_DEADLINE_GATE
            .lock()
            .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::Internal))?
            .as_ref()
            .cloned()
            .ok_or_else(|| ResponseBodyError::new(ResponseBodyErrorKind::Internal))?;
        gate.entered.wait();
        gate.release.wait();
        Ok(body)
    }
}

pub struct UnsafeFailingBuffered;

impl ResponseBodySpec for UnsafeFailingBuffered {
    type Output = ();
    type Mode = Buffered;

    fn from_body(_body: Bytes, _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Err(ResponseBodyError::with_safe_message(
            ResponseBodyErrorKind::Decode,
            "application-response-error-sentinel-7f61c2",
        ))
    }
}

api! {
    client ExtensionApi {
        base "http://generic-specification.example.test/";
        POST Buffered(body: crate::DownstreamBody) -> crate::DownstreamBuffered;
        GET Streaming() -> crate::DownstreamStreaming;
        GET Discard() -> crate::DownstreamDiscard;
        POST Contradiction(body: crate::ContradictoryBody) -> Bytes;
        GET OrderedBuffered() -> crate::OrderedBuffered;
        GET RejectBeforePolling() -> crate::RejectBeforePolling;
        GET OrderedStreaming() -> crate::OrderedStreaming;
        GET OrderedDiscard() -> crate::OrderedDiscard;
        GET DeadlineChecked() -> crate::DeadlineChecked;
        GET UnsafeFailingBuffered() -> crate::UnsafeFailingBuffered;
    }
}

struct ObservingHooks {
    before: AtomicUsize,
    known_length: AtomicUsize,
    veto: bool,
}

#[derive(Default)]
struct CountingLimiter {
    acquisitions: AtomicUsize,
}

impl RateLimiter for CountingLimiter {
    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
}

impl RuntimeHooks for ObservingHooks {
    fn before_send<'a>(&'a self, context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        self.before.fetch_add(1, Ordering::SeqCst);
        self.known_length.store(
            usize::try_from(context.known_body_length.unwrap_or_default()).unwrap_or(usize::MAX),
            Ordering::SeqCst,
        );
        Box::pin(async move {
            if self.veto {
                Err(HookError::new("generic specification hook veto"))
            } else {
                Ok(())
            }
        })
    }
}

fn client(server: &ScriptedServer, hooks: Arc<dyn RuntimeHooks>) -> ExtensionApi {
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    ExtensionApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime(move |runtime| {
            runtime
                .hooks(hooks)
                .request_body_limit(NonZeroUsize::new(1024).expect("positive test limit"))
        })
        .build()
        .expect("extension client")
}

fn input(
    value: &'static [u8],
    metadata_calls: &Arc<AtomicUsize>,
    prepare_calls: &Arc<AtomicUsize>,
) -> DownstreamInput {
    DownstreamInput {
        bytes: Bytes::from_static(value),
        metadata_calls: Arc::clone(metadata_calls),
        prepare_calls: Arc::clone(prepare_calls),
    }
}

#[tokio::test]
async fn downstream_request_and_all_sealed_response_modes_work_without_macro_changes() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"buffered-output".to_vec())
            .with_header("content-type", "application/x-downstream-buffered"),
        ResponseScript::ok(b"buffered-metadata-output".to_vec())
            .with_header("content-type", "application/x-downstream-buffered"),
        ResponseScript::ok(b"stream-output".to_vec())
            .with_header("content-type", "application/x-downstream-stream"),
        ResponseScript::ok(b"stream-direct-output".to_vec())
            .with_header("content-type", "application/x-downstream-stream"),
        ResponseScript::ok(b"discarded-body-sentinel".to_vec()),
        ResponseScript::ok(b"discarded-metadata-body-sentinel".to_vec()),
        ResponseScript::ok(b"prepared-output".to_vec())
            .with_header("content-type", "application/x-downstream-buffered"),
    ])
    .expect("extension server");
    let hooks = Arc::new(ObservingHooks {
        before: AtomicUsize::new(0),
        known_length: AtomicUsize::new(0),
        veto: false,
    });
    let client = client(&server, hooks.clone());
    let metadata_calls = Arc::new(AtomicUsize::new(0));
    let prepare_calls = Arc::new(AtomicUsize::new(0));

    let buffered = client
        .buffered(input(
            b"request-body-sentinel",
            &metadata_calls,
            &prepare_calls,
        ))
        .await
        .expect("custom buffered response");
    assert_eq!(buffered, "buffered-output");
    assert_eq!(metadata_calls.load(Ordering::SeqCst), 1);
    assert_eq!(prepare_calls.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.known_length.load(Ordering::SeqCst), 21);

    let buffered_response: ApiResponse<String> = client
        .buffered(input(
            b"metadata-request-body",
            &metadata_calls,
            &prepare_calls,
        ))
        .response()
        .await
        .expect("custom buffered metadata");
    assert_eq!(buffered_response.value(), "buffered-metadata-output");

    let response: ApiResponse<TaggedStream> = client
        .streaming()
        .response()
        .await
        .expect("custom streaming metadata");
    assert_eq!(response.status(), concord_core::advanced::StatusCode::OK);
    let mut stream = response.into_value().0;
    let chunk = poll_fn(|context| Pin::new(&mut stream).poll_next(context))
        .await
        .expect("stream chunk")
        .expect("valid stream chunk");
    assert_eq!(chunk, b"stream-output"[..]);

    let mut direct_stream = client
        .streaming()
        .await
        .expect("custom direct streaming output")
        .0;
    let chunk = poll_fn(|context| Pin::new(&mut direct_stream).poll_next(context))
        .await
        .expect("direct stream chunk")
        .expect("valid direct stream chunk");
    assert_eq!(chunk, b"stream-direct-output"[..]);

    assert_eq!(client.discard().await.expect("custom discard"), Discarded);
    let discard_response: ApiResponse<Discarded> = client
        .discard()
        .response()
        .await
        .expect("custom discard metadata");
    assert_eq!(discard_response.value(), &Discarded);

    let prepared = concord_core::advanced::PreparedEndpoint::builder(
        "ExtensionApi::prepared",
        concord_core::advanced::Method::POST,
    )
    .body_spec::<DownstreamBody>(input(b"prepared-request", &metadata_calls, &prepare_calls))
    .response::<DownstreamBuffered>()
    .build()
    .expect("generic prepared endpoint");
    assert_eq!(
        client.request(prepared).await.expect("prepared output"),
        "prepared-output"
    );
    assert_eq!(prepare_calls.load(Ordering::SeqCst), 3);

    server
        .wait_for_requests(7, Duration::from_secs(2))
        .expect("seven extension requests");
    assert_eq!(
        server.requests()[0].header("content-type"),
        Some("application/x-downstream-body")
    );
}

#[tokio::test]
async fn hook_veto_observes_metadata_and_prevents_downstream_prepare() {
    let server = ScriptedServer::start([]).expect("empty extension server");
    let hooks = Arc::new(ObservingHooks {
        before: AtomicUsize::new(0),
        known_length: AtomicUsize::new(0),
        veto: true,
    });
    let client = client(&server, hooks.clone());
    let metadata_calls = Arc::new(AtomicUsize::new(0));
    let prepare_calls = Arc::new(AtomicUsize::new(0));
    let error = client
        .buffered(input(
            b"vetoed-request-secret",
            &metadata_calls,
            &prepare_calls,
        ))
        .await
        .expect_err("hook veto");
    assert_eq!(error.category(), concord_core::prelude::ErrorCategory::Hook);
    assert_eq!(metadata_calls.load(Ordering::SeqCst), 1);
    assert_eq!(prepare_calls.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 1);
    assert_eq!(hooks.known_length.load(Ordering::SeqCst), 21);
    assert_eq!(server.requests().len(), 0);
    assert!(!format!("{error} {error:?}").contains("vetoed-request-secret"));
}

#[tokio::test]
async fn metadata_contradictions_fail_after_hook_and_before_limiter_or_native_send() {
    UNKNOWN_FACTORY_CREATES.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([]).expect("contradiction server");
    let proxy = SafeProxy::parse(server.url("")).expect("contradiction proxy");
    let hooks = Arc::new(ObservingHooks {
        before: AtomicUsize::new(0),
        known_length: AtomicUsize::new(0),
        veto: false,
    });
    let limiter = Arc::new(CountingLimiter::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = hooks.clone();
    let configured_limiter: Arc<dyn RateLimiter> = limiter.clone();
    let client = ExtensionApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime(move |runtime| {
            runtime
                .hooks(configured_hooks)
                .rate_limiter(configured_limiter)
        })
        .build()
        .expect("contradiction client");

    for contradiction in [
        Contradiction::Kind,
        Contradiction::ExactLength,
        Contradiction::LowerBound,
        Contradiction::UnverifiableLowerBound,
        Contradiction::ContentType,
        Contradiction::GeneratedOwnership,
        Contradiction::BodyPresence,
        Contradiction::ImpossibleManagedState,
    ] {
        let error = client
            .contradiction(contradiction)
            .await
            .expect_err("metadata contradiction must fail closed");
        assert_eq!(error.category(), ErrorCategory::RequestBody);
        assert!(!format!("{error} {error:?}").contains("application/x-produced-only"));
    }

    assert_eq!(hooks.before.load(Ordering::SeqCst), 8);
    assert_eq!(UNKNOWN_FACTORY_CREATES.load(Ordering::SeqCst), 0);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(server.requests().len(), 0);
}

#[tokio::test]
async fn known_metadata_limit_fails_before_hook_prepare_limiter_and_native_send() {
    let server = ScriptedServer::start([]).expect("metadata limit server");
    let proxy = SafeProxy::parse(server.url("")).expect("metadata limit proxy");
    let hooks = Arc::new(ObservingHooks {
        before: AtomicUsize::new(0),
        known_length: AtomicUsize::new(0),
        veto: false,
    });
    let limiter = Arc::new(CountingLimiter::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = hooks.clone();
    let configured_limiter: Arc<dyn RateLimiter> = limiter.clone();
    let client = ExtensionApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime(move |runtime| {
            runtime
                .hooks(configured_hooks)
                .rate_limiter(configured_limiter)
                .request_body_limit(NonZeroUsize::new(4).expect("positive request limit"))
        })
        .build()
        .expect("metadata limit client");
    let metadata_calls = Arc::new(AtomicUsize::new(0));
    let prepare_calls = Arc::new(AtomicUsize::new(0));
    let endpoint = concord_core::advanced::PreparedEndpoint::builder(
        "ExtensionApi::metadata_limit",
        concord_core::advanced::Method::POST,
    )
    .body_spec::<DownstreamBody>(input(
        b"first-excess-request-metadata-sentinel",
        &metadata_calls,
        &prepare_calls,
    ))
    .response_bytes()
    .build()
    .expect("metadata limit endpoint");

    let error = client
        .request(endpoint)
        .await
        .expect_err("known request metadata limit");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(metadata_calls.load(Ordering::SeqCst), 1);
    assert_eq!(prepare_calls.load(Ordering::SeqCst), 0);
    assert_eq!(hooks.before.load(Ordering::SeqCst), 0);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(server.requests().len(), 0);
    assert!(!format!("{error} {error:?}").contains("first-excess-request-metadata-sentinel"));
}

#[tokio::test]
async fn custom_response_modes_observe_status_preflight_validation_and_acquisition_order() {
    ORDER_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    ORDER_FROM_BODY_CALLS.store(0, Ordering::SeqCst);
    REJECT_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    REJECT_FROM_BODY_CALLS.store(0, Ordering::SeqCst);
    STREAM_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    STREAM_FROM_BODY_CALLS.store(0, Ordering::SeqCst);
    DISCARD_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    DISCARD_FROM_BODY_CALLS.store(0, Ordering::SeqCst);

    let reject_gate = concord_test_support::server::BodyGate::closed();
    let stream_gate = concord_test_support::server::BodyGate::closed();
    let discard_gate = concord_test_support::server::BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::status(404, b"status-body-sentinel".to_vec()),
        ResponseScript::ok(b"known-length-excess".to_vec()),
        ResponseScript::GatedBody {
            status: 200,
            body: b"validation-body-sentinel".to_vec(),
            gate: reject_gate.clone(),
        },
        ResponseScript::Chunked {
            status: 200,
            headers: Vec::new(),
            chunks: vec![b"complete-".to_vec(), b"buffered".to_vec()],
        },
        ResponseScript::GatedBody {
            status: 200,
            body: b"lazy-stream-body".to_vec(),
            gate: stream_gate.clone(),
        },
        ResponseScript::GatedBody {
            status: 200,
            body: b"discard-body-sentinel".to_vec(),
            gate: discard_gate.clone(),
        },
    ])
    .expect("response ordering server");

    let default_client = client(
        &server,
        Arc::new(ObservingHooks {
            before: AtomicUsize::new(0),
            known_length: AtomicUsize::new(0),
            veto: false,
        }),
    );
    let status_error = default_client
        .ordered_buffered()
        .await
        .expect_err("non-success status precedes response specification");
    assert_eq!(status_error.category(), ErrorCategory::HttpStatus);
    assert_eq!(ORDER_VALIDATE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(ORDER_FROM_BODY_CALLS.load(Ordering::SeqCst), 0);

    let limited_proxy = SafeProxy::parse(server.url("")).expect("limited response proxy");
    let limited_client = ExtensionApi::builder()
        .http(move |http| http.proxy(limited_proxy))
        .runtime(|runtime| {
            runtime.response_body_limit(NonZeroUsize::new(4).expect("positive response limit"))
        })
        .build()
        .expect("limited response client");
    let limit_error = limited_client
        .ordered_buffered()
        .await
        .expect_err("known length preflight");
    assert_eq!(limit_error.category(), ErrorCategory::ResponseBody);
    assert_eq!(ORDER_VALIDATE_CALLS.load(Ordering::SeqCst), 0);

    let validation_error = tokio::time::timeout(
        Duration::from_secs(1),
        default_client.reject_before_polling(),
    )
    .await
    .expect("validation must not wait for the body gate")
    .expect_err("custom validation rejection");
    assert_eq!(validation_error.category(), ErrorCategory::ResponseContract);
    assert_eq!(REJECT_VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(REJECT_FROM_BODY_CALLS.load(Ordering::SeqCst), 0);
    reject_gate.release();

    let complete = default_client
        .ordered_buffered()
        .await
        .expect("complete custom buffered response");
    assert_eq!(complete, b"complete-buffered"[..]);
    assert_eq!(ORDER_VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(ORDER_FROM_BODY_CALLS.load(Ordering::SeqCst), 1);

    let mut stream =
        tokio::time::timeout(Duration::from_secs(1), default_client.ordered_streaming())
            .await
            .expect("stream opening must not wait for body bytes")
            .expect("custom streaming response");
    assert_eq!(STREAM_VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(STREAM_FROM_BODY_CALLS.load(Ordering::SeqCst), 1);
    stream_gate.release();
    let chunk = poll_fn(|context| Pin::new(&mut stream).poll_next(context))
        .await
        .expect("stream chunk")
        .expect("safe stream chunk");
    assert_eq!(chunk, b"lazy-stream-body"[..]);

    tokio::time::timeout(Duration::from_secs(1), default_client.ordered_discard())
        .await
        .expect("discard must not wait for body bytes")
        .expect("custom discard response");
    assert_eq!(DISCARD_VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(DISCARD_FROM_BODY_CALLS.load(Ordering::SeqCst), 1);
    discard_gate.release();
}

#[tokio::test]
async fn synchronous_response_extension_return_is_checked_against_the_operation_deadline() {
    const OPERATION_BUDGET: Duration = Duration::from_secs(1);

    let server = ScriptedServer::start([ResponseScript::ok(
        b"deadline-response-body-sentinel".to_vec(),
    )])
    .expect("synchronous deadline server");
    let client = client(
        &server,
        Arc::new(ObservingHooks {
            before: AtomicUsize::new(0),
            known_length: AtomicUsize::new(0),
            veto: false,
        }),
    );
    let gate = Arc::new(SynchronousDeadlineGate {
        entered: Barrier::new(2),
        release: Barrier::new(2),
    });
    *SYNCHRONOUS_DEADLINE_GATE
        .lock()
        .expect("synchronous deadline gate lock") = Some(Arc::clone(&gate));

    let helper = std::thread::spawn(move || {
        gate.entered.wait();
        let guaranteed_expiry = std::time::Instant::now() + OPERATION_BUDGET;
        loop {
            let remaining = guaranteed_expiry.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            std::thread::park_timeout(remaining);
        }
        gate.release.wait();
    });
    let error = client
        .deadline_checked()
        .timeout(OPERATION_BUDGET)
        .await
        .expect_err("synchronous extension exceeded the operation deadline");
    helper.join().expect("deadline helper thread");
    *SYNCHRONOUS_DEADLINE_GATE
        .lock()
        .expect("synchronous deadline gate cleanup") = None;

    assert_eq!(error.category(), ErrorCategory::Timeout);
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains("deadline-response-body-sentinel"));
}

#[tokio::test]
async fn application_response_errors_and_complete_body_bytes_are_sanitized_by_core() {
    let server = ScriptedServer::start([ResponseScript::ok(
        b"unsafe-response-body-sentinel-c92ad8".to_vec(),
    )])
    .expect("response error server");
    let client = client(
        &server,
        Arc::new(ObservingHooks {
            before: AtomicUsize::new(0),
            known_length: AtomicUsize::new(0),
            veto: false,
        }),
    );
    let error = client
        .unsafe_failing_buffered()
        .await
        .expect_err("application response error");
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    let rendered = format!("{error} {error:?}");
    for sentinel in [
        "application-response-error-sentinel-7f61c2",
        "unsafe-response-body-sentinel-c92ad8",
    ] {
        assert!(!rendered.contains(sentinel));
    }
}
