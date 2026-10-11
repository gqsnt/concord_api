//! v3 body/media authority and outcome evidence (V3-E05..E08, V3-E10).

use super::*;
use crate::body::{BodyDescription, LengthHint, RequestBodyContext, RequestRepresentation};
use crate::endpoint::{AbsentStatus, Effect};
use crate::response::{BodyDisposition, Discard, ExpectedAbsence, HeadAbsence};
use crate::retry::{RetryDecision, RetryPolicy};

const OUTCOME_ENDPOINT: &str = "execution_api.outcomes.find";
const PREPARE_ENDPOINT: &str = "execution_api.outcomes.prepare";

fn retry_everything(maximum: u32) -> RetryPolicy {
    RetryPolicy::builder()
        .max_retries_per_operation(maximum)
        .without_budget()
        .classify_fn(|_| RetryDecision::Retry)
        .build()
        .expect("bounded retry policy")
}

// ---------------------------------------------------------------------------
// Counting request representations
// ---------------------------------------------------------------------------

/// Counts representation calls and produces a fixed, replayable body.
struct CountingBytes;

struct CountingInput {
    calls: Arc<AtomicUsize>,
    body: &'static [u8],
    content_type: &'static str,
}

impl RequestRepresentation<CountingInput> for CountingBytes {
    const DESCRIPTION: BodyDescription = BodyDescription::buffered(Some("application/x-counting"));

    fn prepare(
        input: CountingInput,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        input.calls.fetch_add(1, Ordering::SeqCst);
        PreparedBody::bytes(Bytes::from_static(input.body)).content_type(input.content_type)
    }
}

/// Counts representation calls and returns a reconstructible factory body.
struct CountingFactory;

struct FactoryInput {
    calls: Arc<AtomicUsize>,
    creates: Arc<AtomicUsize>,
}

struct CountingStreamFactory {
    creates: Arc<AtomicUsize>,
}

impl BodyFactory for CountingStreamFactory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        Ok(RequestStream::from_bytes(Bytes::from_static(
            b"factory-body",
        )))
    }
}

impl RequestRepresentation<FactoryInput> for CountingFactory {
    fn prepare(
        input: FactoryInput,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        input.calls.fetch_add(1, Ordering::SeqCst);
        PreparedBody::factory(CountingStreamFactory {
            creates: input.creates,
        })
        .content_type("application/x-factory")
    }
}

/// A representation whose prepared value is invalid in a selectable way.
struct InvalidBody;

#[derive(Clone, Copy)]
enum InvalidKind {
    MediaType,
    ConflictingMediaType,
    KnownLengthOverLimit,
    DishonestStructuralLength,
}

impl RequestRepresentation<InvalidKind> for InvalidBody {
    fn prepare(
        input: InvalidKind,
        context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        match input {
            InvalidKind::MediaType => {
                PreparedBody::bytes(Bytes::from_static(b"x")).content_type("application/*")
            }
            InvalidKind::ConflictingMediaType => PreparedBody::bytes(Bytes::from_static(b"x"))
                .content_type("text/plain")?
                .content_type("application/json"),
            InvalidKind::KnownLengthOverLimit => Ok(PreparedBody::bytes(Bytes::from(vec![
                b'x';
                context
                    .body_limit
                    .get(
                    )
                    + 1
            ]))),
            InvalidKind::DishonestStructuralLength => Ok(PreparedBody::stream(
                RequestStream::from_bytes(Bytes::from_static(b"four")).exact_length(3),
            )),
        }
    }
}

struct PrepareEndpoint<B, I> {
    input: I,
    marker: std::marker::PhantomData<fn() -> B>,
}

impl<B, I> PrepareEndpoint<B, I> {
    fn new(input: I) -> Self {
        Self {
            input,
            marker: std::marker::PhantomData,
        }
    }
}

impl<B, I> EndpointSpec for PrepareEndpoint<B, I>
where
    B: RequestRepresentation<I>,
    I: Send + 'static,
{
    type Input = I;
    type Body = B;
    type Output = Bytes;
    type Response = Binary;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new_with_effect(
            PREPARE_ENDPOINT,
            Method::PUT,
            Some(Duration::from_secs(30)),
            Effect::Idempotent,
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("prepare")?;
        builder.finish()
    }

    fn into_body(self) -> I {
        self.input
    }
}

fn development_core_with(
    executor: &DeterministicExecutor,
    hooks: Arc<ProbeHooks>,
    limiter: Arc<ProbeRateLimiter>,
    runtime: RuntimeConfigBuilder,
) -> ClientCore {
    let limiter: Arc<dyn RateLimiter> = limiter;
    development_core(executor, hooks, runtime.rate_limiter(limiter))
}

fn before_count(hooks: &ProbeHooks) -> usize {
    hooks
        .records()
        .iter()
        .filter(|record| matches!(record, LifecycleRecord::Before { .. }))
        .count()
}

#[tokio::test]
async fn v3_e05_invalid_body_media_and_length_fail_before_admission_and_dispatch() {
    for kind in [
        InvalidKind::MediaType,
        InvalidKind::ConflictingMediaType,
        InvalidKind::KnownLengthOverLimit,
        InvalidKind::DishonestStructuralLength,
    ] {
        let executor = DeterministicExecutor::new();
        let hooks = Arc::new(ProbeHooks::new());
        let limiter = Arc::new(ProbeRateLimiter::new());
        let core = development_core_with(
            &executor,
            Arc::clone(&hooks),
            Arc::clone(&limiter),
            RuntimeConfigBuilder::new().request_body_limit(16),
        );
        let error = Request::new(core, PrepareEndpoint::<InvalidBody, _>::new(kind))
            .await
            .expect_err("invalid prepared body");
        assert_eq!(error.category(), ErrorCategory::RequestBody);
        assert_eq!(
            error.dispatch_knowledge(),
            crate::error::DispatchKnowledge::NotDispatched
        );
        assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
        assert_eq!(executor.application().request_count().expect("count"), 0);
        assert_eq!(
            before_count(&hooks),
            0,
            "no attempt starts for an invalid body"
        );
    }
}

#[tokio::test]
async fn v3_e05_retries_prepare_once_and_replay_or_reconstruct_mechanically() {
    let executor = DeterministicExecutor::new();
    for _ in 0..2 {
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new()
                    .header(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/x-counting"),
                    )
                    .body(Bytes::from_static(b"replayable-body")),
                ScriptedResponse::status(StatusCode::SERVICE_UNAVAILABLE, Bytes::new()),
            )
            .expect("retryable status");
    }
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().body(Bytes::from_static(b"replayable-body")),
            ScriptedResponse::success(Bytes::from_static(b"ok")),
        )
        .expect("success");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(
        &executor,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new().retry_policy(retry_everything(2)),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let output = Request::new(
        core,
        PrepareEndpoint::<CountingBytes, _>::new(CountingInput {
            calls: Arc::clone(&calls),
            body: b"replayable-body",
            content_type: "application/x-counting",
        }),
    )
    .await
    .expect("replayed bytes succeed");
    assert_eq!(output, Bytes::from_static(b"ok"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "prepared once per operation"
    );
    assert_eq!(executor.application().request_count().expect("count"), 3);
    assert_eq!(before_count(&hooks), 3);

    // A reconstructible factory body: one representation call, one factory
    // stream per admitted attempt.
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().body(Bytes::from_static(b"factory-body")),
            ScriptedResponse::status(StatusCode::SERVICE_UNAVAILABLE, Bytes::new()),
        )
        .expect("retryable status");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .header(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/x-factory"),
                )
                .body(Bytes::from_static(b"factory-body")),
            ScriptedResponse::success(Bytes::from_static(b"ok")),
        )
        .expect("success");
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().retry_policy(retry_everything(1)),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let creates = Arc::new(AtomicUsize::new(0));
    Request::new(
        core,
        PrepareEndpoint::<CountingFactory, _>::new(FactoryInput {
            calls: Arc::clone(&calls),
            creates: Arc::clone(&creates),
        }),
    )
    .await
    .expect("reconstructed factory succeeds");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(creates.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn v3_e06_one_shot_stream_body_is_never_replayed() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"terminal"),
        ))
        .expect("retryable status");
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().retry_policy(retry_everything(3)),
    );
    let body = PreparedBody::stream(RequestStream::from_bytes(Bytes::from_static(b"one-shot")));
    assert!(!body.is_replayable());
    let error = body_request(core, body)
        .await
        .expect_err("a one-shot body suppresses the policy retry");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn v3_e05_dishonest_streamed_length_reports_truthful_dispatch_context() {
    let server =
        ScriptedServer::start([ResponseScript::ok(b"never".to_vec())]).expect("loopback server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );
    let polls = Arc::new(AtomicUsize::new(0));
    let body = PreparedBody::stream(
        RequestStream::from_stream(BodyPollProbe::new([ProbeFrame::Data(b"four")], &polls))
            .exact_length(10),
    );
    assert_eq!(body.length_hint(), LengthHint::Exact(10));
    let error = body_request(core, body)
        .await
        .expect_err("truncated production");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(
        error.dispatch_knowledge(),
        crate::error::DispatchKnowledge::PossiblyDispatched,
        "a body failure after native handoff never claims no send"
    );
}

#[tokio::test]
async fn body_02_outer_absent_body_calls_no_representation_and_sends_no_media() {
    type OptionalCounting = crate::body::OptionalRequestRepresentation<CountingBytes>;

    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().body(Bytes::new()),
            ScriptedResponse::success(Bytes::from_static(b"absent-body-ok")),
        )
        .expect("absent body response");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .header(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/x-counting"),
                )
                .body(Bytes::new()),
            ScriptedResponse::success(Bytes::from_static(b"present-empty-ok")),
        )
        .expect("present empty body response");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, Arc::clone(&hooks), RuntimeConfigBuilder::new());

    let calls = Arc::new(AtomicUsize::new(0));
    let output = Request::new(
        core.clone(),
        PrepareEndpoint::<OptionalCounting, Option<CountingInput>>::new(None),
    )
    .await
    .expect("outer-absent body");
    assert_eq!(output, Bytes::from_static(b"absent-body-ok"));
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no representation call");

    // A present payload whose representation produces zero bytes remains a
    // present body with its body-owned media type.
    let output = Request::new(
        core,
        PrepareEndpoint::<OptionalCounting, Option<CountingInput>>::new(Some(CountingInput {
            calls: Arc::clone(&calls),
            body: b"",
            content_type: "application/x-counting",
        })),
    )
    .await
    .expect("present empty body");
    assert_eq!(output, Bytes::from_static(b"present-empty-ok"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let captures = executor.application().captures().expect("captures");
    assert_eq!(captures.len(), 2);
    assert!(
        captures[0]
            .headers()
            .iter()
            .all(|header| header.name() != http::header::CONTENT_TYPE),
        "an outer-absent body places no body-owned media header"
    );
    assert_eq!(captures[0].body_category(), BodyCategory::Empty);
    assert_eq!(captures[1].body_category(), BodyCategory::Bytes);
    assert!(matches!(
        hooks.records().first(),
        Some(LifecycleRecord::Before {
            body_kind: crate::hooks::BodyKind::Empty,
            known_body_length: Some(0),
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// Multipart boundary ownership (V3-E06)
// ---------------------------------------------------------------------------

#[cfg(feature = "multipart")]
mod multipart_boundary {
    use super::*;
    use crate::body::{Multipart, generated_boundaries_on_this_thread};

    struct MultipartEndpoint {
        body: MultipartBody,
    }

    impl EndpointSpec for MultipartEndpoint {
        type Input = MultipartBody;
        type Body = Multipart;
        type Output = Bytes;
        type Response = Binary;

        fn request_meta(&self) -> RequestMeta {
            RequestMeta::new(
                "execution_api.outcomes.upload",
                Method::POST,
                Some(Duration::from_secs(30)),
            )
        }

        fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
            builder.push_static("upload")?;
            builder.finish()
        }

        fn into_body(self) -> MultipartBody {
            self.body
        }
    }

    fn recipe() -> MultipartBody {
        MultipartBody::new()
            .part(
                "note",
                MultipartPart::text("boundary-owned-text").expect("bounded text"),
            )
            .expect("text part")
            .part(
                "blob",
                MultipartPart::bytes(Bytes::from_static(b"boundary-owned-bytes"))
                    .expect("bounded bytes")
                    .file_name("blob.bin")
                    .expect("filename"),
            )
            .expect("bytes part")
    }

    #[tokio::test]
    async fn v3_e06_boundary_is_generated_once_and_exact_in_header_and_body() {
        let server = ScriptedServer::start([ResponseScript::ok(b"uploaded".to_vec())])
            .expect("loopback server");
        let core = real_core(
            &server,
            Arc::new(ProbeHooks::new()),
            RuntimeConfigBuilder::new(),
        );

        let request = Request::new(core, MultipartEndpoint { body: recipe() });
        let description = crate::endpoint::EndpointDescription::runtime(
            EndpointId::from_static("execution_api.outcomes.upload"),
            Method::POST,
            Arc::from("upload"),
            Arc::from("upload"),
            crate::endpoint::RequestBodyCategory::Specification,
            Some("Multipart"),
            crate::endpoint::ResponseAcquisition::Buffered,
            crate::endpoint::EndpointCapabilities::generated(false, false, false, false, false),
            crate::endpoint::TimeoutSelection::Explicit(Duration::from_secs(30)),
            crate::endpoint::AcceptedStatus::Success,
            crate::endpoint::StatusSource::DefaultSuccess,
            Effect::NonIdempotent,
            crate::endpoint::EffectSource::MethodDefault,
        )
        .with_body_description(<Multipart as RequestRepresentation<MultipartBody>>::DESCRIPTION);

        let before_plan = generated_boundaries_on_this_thread();
        let plan = crate::__private::request_plan(&request, &description).expect("plan");
        assert_eq!(
            generated_boundaries_on_this_thread(),
            before_plan,
            "planning never generates a boundary"
        );
        assert_eq!(
            plan.body_description().to_string(),
            "multipart/form-data; boundary generated during execution"
        );
        assert!(!plan.dynamic_body_metadata_available());
        assert!(
            !plan
                .public_headers()
                .contains_key(&http::header::CONTENT_TYPE)
        );

        let before = generated_boundaries_on_this_thread();
        let output = request.await.expect("multipart upload");
        assert_eq!(output, Bytes::from_static(b"uploaded"));
        assert_eq!(
            generated_boundaries_on_this_thread(),
            before + 1,
            "exactly one boundary per prepared operation"
        );

        let requests = server.requests();
        assert_eq!(requests.len(), 1);
        let recorded = &requests[0];
        let content_type = recorded.header("content-type").expect("content type");
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("generated multipart media type");
        assert!(boundary.starts_with("concord-"));
        let body = recorded.body();
        assert!(body.starts_with(format!("--{boundary}\r\n").as_bytes()));
        assert!(body.ends_with(format!("\r\n--{boundary}--\r\n").as_bytes()));
        let delimiter = format!("--{boundary}");
        let occurrences = body
            .windows(delimiter.len())
            .filter(|window| *window == delimiter.as_bytes())
            .count();
        assert_eq!(occurrences, 3, "two parts and one closing delimiter");
        assert_eq!(
            recorded.header("content-length"),
            Some(body.len().to_string().as_str())
        );
    }

    #[tokio::test]
    async fn v3_e06_replayed_multipart_reuses_the_retained_boundary() {
        let server = ScriptedServer::start([
            ResponseScript::status(503, b"retry".to_vec()),
            ResponseScript::ok(b"uploaded".to_vec()),
        ])
        .expect("loopback server");
        let core = real_core(
            &server,
            Arc::new(ProbeHooks::new()),
            RuntimeConfigBuilder::new().retry_policy(retry_everything(1)),
        );
        let before = generated_boundaries_on_this_thread();
        let body = PreparedBody::multipart(recipe()).expect("prepared multipart");
        let output = body_request(core, body).await.expect("replayed upload");
        assert_eq!(output, Bytes::from_static(b"uploaded"));
        assert_eq!(generated_boundaries_on_this_thread(), before + 1);
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].header("content-type"),
            requests[1].header("content-type")
        );
        assert_eq!(requests[0].body(), requests[1].body());
    }
}

// ---------------------------------------------------------------------------
// Streaming and serialization limits (V3-E07)
// ---------------------------------------------------------------------------

struct Oversized;

impl EncodeBody for Oversized {
    type Input = usize;
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-oversized");

    fn encode(input: usize, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        // Deliberately ignores the limit; the bridge must still refuse it.
        Ok(Bytes::from(vec![b'o'; input]))
    }
}

#[tokio::test]
async fn v3_e07_request_body_limits_hold_during_streaming_and_serialization() {
    // A codec ignoring the limit is refused by the one bridge before dispatch.
    let executor = DeterministicExecutor::new();
    let limiter = Arc::new(ProbeRateLimiter::new());
    let core = development_core_with(
        &executor,
        Arc::new(ProbeHooks::new()),
        Arc::clone(&limiter),
        RuntimeConfigBuilder::new().request_body_limit(8),
    );
    let error = Request::new(core, PrepareEndpoint::<Oversized, _>::new(9))
        .await
        .expect_err("serialized excess");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    // An unknown-length stream stops at the first excess chunk.
    let server =
        ScriptedServer::start([ResponseScript::ok(b"never".to_vec())]).expect("loopback server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().request_body_limit(8),
    );
    let polls = Arc::new(AtomicUsize::new(0));
    let body = PreparedBody::stream(RequestStream::from_stream(BodyPollProbe::new(
        [
            ProbeFrame::Data(b"12345"),
            ProbeFrame::Data(b"67890"),
            ProbeFrame::Data(b"never-polled"),
        ],
        &polls,
    )));
    let error = body_request(core, body).await.expect_err("streamed excess");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(
        polls.load(Ordering::SeqCst),
        2,
        "no polling after the first excess"
    );
}

// ---------------------------------------------------------------------------
// Head-only expected absence (V3-E08)
// ---------------------------------------------------------------------------

struct AbsentEndpoint {
    accepted: crate::endpoint::AcceptedStatus,
    absent: StatusCode,
}

impl AbsentEndpoint {
    fn new() -> Self {
        Self {
            accepted: crate::endpoint::AcceptedStatus::Success,
            absent: StatusCode::NOT_FOUND,
        }
    }
}

impl EndpointSpec for AbsentEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = Option<String>;
    type Response = HeadAbsence<Text>;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new_with_contract(
            OUTCOME_ENDPOINT,
            Method::GET,
            Some(Duration::from_secs(5)),
            self.accepted.clone(),
            Effect::Safe,
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("items")?;
        builder.finish()
    }

    fn expected_absence(&self) -> Option<ExpectedAbsence<Self::Output, Self::Response>> {
        Some(ExpectedAbsence::new(
            AbsentStatus::codes([self.absent]).expect("absence status"),
        ))
    }

    fn into_body(self) {}
}

#[tokio::test]
async fn v3_e08_absence_is_head_only_and_never_polls_the_irrelevant_body() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::ok(b"present value".to_vec()).with_header("Content-Type", "text/plain"),
        // Huge declared and truncated: never polled.
        ResponseScript::TruncatedWithHeaders {
            status: 404,
            declared_length: 512 * 1024 * 1024,
            prefix: b"partial".to_vec(),
            headers: vec![(
                "Content-Type".to_owned(),
                "application/x-garbage".to_owned(),
            )],
        },
        // Stalled body that never arrives while the call runs.
        ResponseScript::GatedBody {
            status: 404,
            body: vec![b'z'; 64],
            gate: gate.clone(),
        },
        ResponseScript::status(500, b"server failure".to_vec()),
    ])
    .expect("loopback server");
    let hooks = Arc::new(ProbeHooks::new());
    let limiter = Arc::new(ProbeRateLimiter::new());
    let configured: Arc<dyn RateLimiter> = limiter.clone();
    let core = real_core(
        &server,
        Arc::clone(&hooks),
        RuntimeConfigBuilder::new()
            .rate_limiter(configured)
            .response_body_limit(64),
    );

    let present = Request::new(core.clone(), AbsentEndpoint::new())
        .await
        .expect("present branch");
    assert_eq!(present, Some("present value".to_owned()));

    let absent = Request::new(core.clone(), AbsentEndpoint::new())
        .response()
        .await
        .expect("huge irrelevant body is never polled");
    assert_eq!(absent.status(), StatusCode::NOT_FOUND);
    assert_eq!(absent.value(), &None);
    assert_eq!(absent.body_disposition(), BodyDisposition::DiscardedUnread);

    let stalled = tokio::time::timeout(
        Duration::from_secs(2),
        Request::new(core.clone(), AbsentEndpoint::new()).into_future(),
    )
    .await
    .expect("a stalled irrelevant body cannot delay absence")
    .expect("stalled 404 is absent");
    assert_eq!(stalled, None);
    gate.release();

    let error = Request::new(core, AbsentEndpoint::new())
        .await
        .expect_err("other statuses remain rejected");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));

    // Rate feedback observed every response head, including the absences.
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn v3_e08_conflicting_absence_capabilities_are_rejected_before_sending() {
    let executor = DeterministicExecutor::new();
    let core = development_core(
        &executor,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new(),
    );

    // Absence overlapping the accepted payload statuses.
    let overlapping = AbsentEndpoint {
        accepted: crate::endpoint::AcceptedStatus::codes([StatusCode::OK, StatusCode::NOT_FOUND])
            .expect("accepted set"),
        absent: StatusCode::NOT_FOUND,
    };
    let error = Request::new(core.clone(), overlapping)
        .await
        .expect_err("overlapping absence");
    assert_eq!(error.category(), ErrorCategory::Configuration);

    // Absence cannot combine with the accepted-bytes terminal.
    let error = crate::__private::accepted_bytes_request(Request::new(core, AbsentEndpoint::new()))
        .await
        .expect_err("absence with accepted bytes");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(executor.application().request_count().expect("count"), 0);

    // Informational and duplicate absence statuses are rejected.
    assert!(AbsentStatus::codes([StatusCode::CONTINUE]).is_err());
    assert!(AbsentStatus::codes([StatusCode::NOT_FOUND, StatusCode::NOT_FOUND]).is_err());
    assert!(AbsentStatus::codes(std::iter::empty()).is_err());
}

#[tokio::test]
async fn discard_driver_drops_the_accepted_body_without_polling() {
    struct DiscardEndpoint;

    impl EndpointSpec for DiscardEndpoint {
        type Input = ();
        type Body = EmptyBody;
        type Output = ();
        type Response = Discard;

        fn request_meta(&self) -> RequestMeta {
            RequestMeta::new(
                "execution_api.outcomes.discard",
                Method::DELETE,
                Some(Duration::from_secs(5)),
            )
        }

        fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
            builder.push_static("discard")?;
            builder.finish()
        }

        fn into_body(self) {}
    }

    let gate = BodyGate::closed();
    let server = ScriptedServer::start([ResponseScript::GatedBodyWithHeaders {
        status: 200,
        headers: Vec::new(),
        body: vec![b'q'; 1024],
        gate: gate.clone(),
    }])
    .expect("loopback server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().response_body_limit(16),
    );
    let response = tokio::time::timeout(
        Duration::from_secs(2),
        Request::new(core, DiscardEndpoint).response().into_future(),
    )
    .await
    .expect("discard never waits for the body")
    .expect("discarded response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.body_disposition(),
        BodyDisposition::DiscardedUnread
    );
    gate.release();
}

// ---------------------------------------------------------------------------
// Complete-only status bodies (V3-E10)
// ---------------------------------------------------------------------------

struct TextEndpoint;

impl EndpointSpec for TextEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = String;
    type Response = Text;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            "execution_api.outcomes.text",
            Method::GET,
            Some(Duration::from_millis(600)),
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("text")?;
        builder.finish()
    }

    fn into_body(self) {}
}

#[tokio::test]
async fn v3_e10_status_body_states_are_complete_only_and_never_replace_the_status() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::status(500, b"complete error".to_vec())
            .with_header("Content-Type", "text/plain"),
        ResponseScript::status(502, Vec::new()),
        ResponseScript::status(503, vec![b'k'; 64]),
        ResponseScript::Chunked {
            status: 504,
            headers: Vec::new(),
            chunks: vec![vec![b'a'; 20], vec![b'b'; 20]],
        },
        ResponseScript::Truncated {
            status: 500,
            declared_length: 30,
            prefix: b"cut".to_vec(),
        },
        ResponseScript::GatedBody {
            status: 500,
            body: b"late".to_vec(),
            gate: gate.clone(),
        },
    ])
    .expect("loopback server");
    let core = real_core(
        &server,
        Arc::new(ProbeHooks::new()),
        RuntimeConfigBuilder::new().status_body_limit(32),
    );
    let expected = [
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusBodyState::Complete,
            Some(14),
        ),
        (StatusCode::BAD_GATEWAY, StatusBodyState::Empty, Some(0)),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            StatusBodyState::OmittedKnownTooLarge,
            None,
        ),
        (
            StatusCode::GATEWAY_TIMEOUT,
            StatusBodyState::OmittedLimitExceeded,
            None,
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusBodyState::UnavailableReadFailure,
            None,
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusBodyState::UnavailableDeadline,
            None,
        ),
    ];
    for (status, state, len) in expected {
        let error = Request::new(core.clone(), TextEndpoint)
            .await
            .expect_err("rejected status");
        assert_eq!(error.category(), ErrorCategory::HttpStatus, "{state:?}");
        assert_eq!(error.http_status(), Some(status), "{state:?}");
        let failure = error.http_response().expect("status failure");
        assert_eq!(failure.body().state(), state);
        assert_eq!(failure.body().len(), len);
        assert_eq!(
            failure.body().bytes().is_some(),
            state == StatusBodyState::Complete
        );
        let debug = format!("{:?}", failure.body());
        assert!(!debug.contains("complete error"));
        let decoded = failure.decode_status_body::<Text, String>();
        if state == StatusBodyState::Complete {
            assert_eq!(decoded.expect("explicit decode"), "complete error");
        } else {
            assert_eq!(
                decoded.expect_err("no complete body").kind(),
                crate::error::StatusBodyDecodeErrorKind::Unavailable(state)
            );
        }
        // Explicit decoding never mutates the original classification.
        assert_eq!(error.category(), ErrorCategory::HttpStatus);
    }
    gate.release();
}

#[test]
fn static_accept_values_are_validated_once_at_compile_or_bind_time() {
    const _: () = crate::__private::assert_response_accept(Some("application/json"));
    const _: () =
        crate::__private::assert_response_accept(<Text as ResponseRepresentation<String>>::ACCEPT);
    const _: () = crate::__private::assert_response_accept(None);

    struct InjectedAccept;

    impl ResponseRepresentation<()> for InjectedAccept {
        type Driver = crate::response::DiscardDriver;
        const ACCEPT: Option<&'static str> = Some("text/plain\r\nx-injected: 1");

        fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
            Ok(())
        }
    }

    let error = RuntimeEndpoint::builder(EndpointId::from_static("runtime.accept"), Method::GET)
        .response::<InjectedAccept, ()>()
        .build()
        .expect_err("invalid Accept rejected at runtime build");
    assert_eq!(
        error.kind(),
        crate::error::ConfigurationErrorKind::InvalidHeader
    );
    assert!(
        crate::__private::validate_response_accept::<InjectedAccept, ()>().is_err(),
        "the client-build bridge rejects the same value"
    );
    assert!(crate::__private::validate_response_accept::<Text, String>().is_ok());
}
