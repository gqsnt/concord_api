//! Truthful operation journal, direct-request controls, replay authorities
//! and observer containment (V3-E01..E04, V3-E17, V3-E18, V3-E20, V3-E21,
//! CONTROL-01..03, EXEC-01..04, RETRY-01..03).

use std::marker::PhantomData;
use std::net::TcpListener;
use std::thread::ThreadId;

use http::{HeaderMap, HeaderValue};

use super::*;
use crate::controls::{CallOptions, CancellationToken, ControlError, RequestControls};
use crate::endpoint::Effect;
use crate::error::{DispatchKnowledge, FailureStage};
use crate::retry::{RetryBackoff, RetryDecision, RetryPolicy};

const JOURNAL_ENDPOINT: &str = "execution_api.journal.write";
const BODY_SENTINEL: &str = "journal-body-sentinel-5d10c3";
const HEADER_SENTINEL: &str = "journal-header-sentinel-8e44a1";

/// A configurable bodiless endpoint whose method and effect select the
/// replay authority under test.
struct JournalEndpoint<R = Binary> {
    method: Method,
    effect: Effect,
    timeout: Option<Duration>,
    marker: PhantomData<fn() -> R>,
}

impl<R> JournalEndpoint<R> {
    fn new(method: Method, effect: Effect) -> Self {
        Self {
            method,
            effect,
            timeout: Some(Duration::from_secs(30)),
            marker: PhantomData,
        }
    }
}

impl<R> EndpointSpec for JournalEndpoint<R>
where
    R: TestResponseRepresentation + ResponseRepresentation<R::Output>,
{
    type Input = ();
    type Body = EmptyBody;
    type Output = R::Output;
    type Response = R;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new_with_effect(
            JOURNAL_ENDPOINT,
            self.method.clone(),
            self.timeout,
            self.effect,
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("journal")?;
        builder.finish()
    }

    fn into_body(self) {}
}

fn get_request(core: ClientCore) -> Request<JournalEndpoint> {
    Request::new(core, JournalEndpoint::new(Method::GET, Effect::Safe))
}

fn post_request(core: ClientCore) -> Request<JournalEndpoint> {
    Request::new(
        core,
        JournalEndpoint::new(Method::POST, Effect::NonIdempotent),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Completion {
    outcome: CompletionOutcome,
    attempts_started: u32,
    policy_retries: u32,
    final_status: Option<StatusCode>,
}

/// Observer and limiter probe: records completions, head observations and
/// the calling thread of every callback; optionally gates or fails the n-th
/// admission, panics on every event, or advances the test clock in the final
/// callback.
#[derive(Default)]
struct JournalProbe {
    completions: Mutex<Vec<Completion>>,
    heads: Mutex<Vec<StatusCode>>,
    attempts: Mutex<Vec<AttemptInfo>>,
    threads: Mutex<Vec<ThreadId>>,
    acquisitions: AtomicUsize,
    acquire_gate: Option<(usize, Gate)>,
    fail_acquisition: Option<usize>,
    head_seen: Arc<tokio::sync::Notify>,
    panic_everywhere: bool,
    completion_clock_advance: Option<Duration>,
    /// The server-delay fact of every observed head, as the observer saw it.
    observed_server_delays: Mutex<Vec<Option<Duration>>>,
    /// Every limiter feedback: status, server-delay fact and cooldown cap.
    limiter_feedback: Mutex<Vec<(StatusCode, Option<Duration>, Duration)>>,
}

impl JournalProbe {
    fn completions(&self) -> Vec<Completion> {
        self.completions.lock().expect("completion records").clone()
    }
}

impl OperationObserver for JournalProbe {
    fn observe(&self, event: OperationEvent<'_>) {
        self.threads
            .lock()
            .expect("thread records")
            .push(std::thread::current().id());
        match event {
            OperationEvent::AttemptStarted(event) => {
                self.attempts
                    .lock()
                    .expect("attempt records")
                    .push(event.attempt);
            }
            OperationEvent::ResponseHeadObserved(event) => {
                self.heads.lock().expect("head records").push(event.status);
                self.observed_server_delays
                    .lock()
                    .expect("observed server delays")
                    .push(event.server_delay);
                self.head_seen.notify_one();
            }
            OperationEvent::OperationFinished(event) => {
                self.completions
                    .lock()
                    .expect("completion records")
                    .push(Completion {
                        outcome: event.outcome,
                        attempts_started: event.attempts_started,
                        policy_retries: event.policy_retries,
                        final_status: event.final_status,
                    });
                if let Some(advance) = self.completion_clock_advance {
                    crate::operation::TestClockGuard::advance(advance);
                }
            }
            _ => {}
        }
        assert!(!self.panic_everywhere, "journal observer panic sentinel");
    }
}

impl RateLimiter for JournalProbe {
    fn capability(&self) -> crate::rate_limit::LimiterCapability {
        crate::rate_limit::LimiterCapability::new(
            crate::rate_limit::Enforcement::Local,
            "test.journal-probe",
            1,
        )
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let index = self.acquisitions.fetch_add(1, Ordering::SeqCst);
        let gate = self
            .acquire_gate
            .as_ref()
            .filter(|(gated, _)| *gated == index)
            .map(|(_, gate)| gate.clone());
        let fail = self.fail_acquisition == Some(index);
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.wait().await;
            }
            if fail {
                Err(RateLimitError::new(
                    crate::error::RateLimitErrorKind::Acquire,
                ))
            } else {
                Ok(RateLimitPermit::granted())
            }
        })
    }

    fn observe(&self, response: RateLimitResponse<'_>) {
        self.limiter_feedback
            .lock()
            .expect("limiter feedback")
            .push((
                response.status,
                response.server_delay,
                response.cooldown_cap,
            ));
    }
}

fn journal_core(
    executor: &DeterministicExecutor,
    probe: &Arc<JournalProbe>,
    policy: Option<RetryPolicy>,
) -> ClientCore {
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    let limiter: Arc<dyn RateLimiter> = probe.clone();
    let mut runtime = RuntimeConfigBuilder::new()
        .operation_observer(probe.clone())
        .rate_limiter(limiter);
    if let Some(policy) = policy {
        runtime = runtime.retry_policy(policy);
    }
    ClientCore::build_with_development(
        "http://execution.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        runtime,
        executor,
    )
    .expect("journal development core")
}

fn loopback_core(server: &ScriptedServer, probe: &Arc<JournalProbe>) -> ClientCore {
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    ClientCore::build(
        &server.url("/v1/"),
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().operation_observer(probe.clone()),
    )
    .expect("loopback core")
}

/// A policy that always recommends a replay and records the settled
/// dispatch knowledge each classification saw.
fn recording_policy(maximum: u32, seen: &Arc<Mutex<Vec<DispatchKnowledge>>>) -> RetryPolicy {
    let seen = Arc::clone(seen);
    RetryPolicy::builder()
        .max_retries_per_operation(maximum)
        .without_budget()
        .classify_fn(move |attempt| {
            seen.lock()
                .expect("classification records")
                .push(attempt.dispatch_knowledge());
            RetryDecision::Retry
        })
        .build()
        .expect("recording policy")
}

fn status(code: StatusCode) -> ScriptedResponse {
    ScriptedResponse::status(code, Bytes::new())
}

// ---------------------------------------------------------------------------
// V3-E01 / V3-E02: facts after an accepted head are never lost.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e01_truncated_accepted_body_is_response_observed_acquisition_with_actual_status() {
    let server = ScriptedServer::start([ResponseScript::Truncated {
        status: 200,
        declared_length: 64,
        prefix: BODY_SENTINEL.as_bytes().to_vec(),
    }])
    .expect("truncating loopback server");
    let probe = Arc::new(JournalProbe::default());
    let error = get_request(loopback_core(&server, &probe))
        .await
        .expect_err("a truncated accepted body must fail");

    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(error.failure_stage(), FailureStage::ResponseAcquisition);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert!(error.may_have_reached_server());
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    let last = error.last_attempt().expect("last attempt facts");
    assert_eq!(last.index(), 0);
    assert_eq!(last.status(), Some(StatusCode::OK));
    assert_eq!(
        last.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert!(error.operation_id().is_some());
    assert_eq!(error.parent_operation_id(), None);
    assert_eq!(
        error
            .operation_counts()
            .expect("counts")
            .application_handoffs(),
        1
    );
    assert_omits(&format!("{error} {error:?}"), &[BODY_SENTINEL]);
    assert_eq!(
        probe.completions(),
        [Completion {
            outcome: CompletionOutcome::ResponseBody,
            attempts_started: 1,
            policy_retries: 0,
            final_status: Some(StatusCode::OK),
        }]
    );
}

#[tokio::test]
async fn v3_e02_accepted_decode_failure_retains_status_and_is_never_retried() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"not-utf8-\xff",
        )))
        .expect("accepted response");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"never-sent")))
        .expect("unused response");
    let classifications = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(
        &executor,
        &probe,
        Some(recording_policy(16, &classifications)),
    );
    let error = Request::new(
        core,
        JournalEndpoint::<FailingDecoder>::new(Method::GET, Effect::Safe),
    )
    .await
    .expect_err("decode failure");

    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    assert_eq!(error.failure_stage(), FailureStage::ResponseConversion);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert!(
        classifications.lock().expect("classifications").is_empty(),
        "accepted conversion is terminal: the classifier is never consulted"
    );
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(
        executor.application().remaining_scripts().expect("scripts"),
        1
    );
}

static HEAD_REJECTING_DECODES: AtomicUsize = AtomicUsize::new(0);

/// A decoder whose accepted-head validation always rejects the response
/// media, so the body is never decoded.
struct HeadRejectingDecoder;

impl DecodeBody for HeadRejectingDecoder {
    type Output = ();

    const ACCEPT: Option<&'static str> = Some("application/x-required");

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Err(CodecError::new(CodecErrorKind::Decode))
    }

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        HEAD_REJECTING_DECODES.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// RETRY-01: an accepted-head `validate()` failure is terminal even under a
/// policy that recommends a replay for every failure on a safe, replayable
/// request: the classifier is never consulted, no retry is committed and the
/// application request is sent exactly once.
#[tokio::test]
async fn retry_01_accepted_head_validation_failure_is_terminal_under_an_always_retry_policy() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"head-rejected-body",
        )))
        .expect("accepted response");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"never-sent")))
        .expect("unused response");
    let classifications = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(
        &executor,
        &probe,
        Some(recording_policy(16, &classifications)),
    );
    let error = Request::new(
        core,
        JournalEndpoint::<HeadRejectingDecoder>::new(Method::GET, Effect::Safe),
    )
    .await
    .expect_err("accepted-head validation failure");

    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert!(
        classifications.lock().expect("classifications").is_empty(),
        "accepted-head validation is terminal: the classifier is never consulted"
    );
    assert_eq!(HEAD_REJECTING_DECODES.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(
        executor.application().remaining_scripts().expect("scripts"),
        1
    );
    assert_eq!(probe.acquisitions.load(Ordering::SeqCst), 1);
    let counts = error.operation_counts().expect("journal counts");
    assert_eq!(counts.attempts_started(), 1);
    assert_eq!(counts.policy_retries(), 0);
    assert_eq!(
        probe.completions(),
        vec![Completion {
            outcome: CompletionOutcome::ResponseContract,
            attempts_started: 1,
            policy_retries: 0,
            final_status: Some(StatusCode::OK),
        }]
    );
}

// ---------------------------------------------------------------------------
// V3-E03: aggregate knowledge versus last-attempt facts.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e03_observed_attempt_then_proven_no_send_keeps_aggregate_and_separate_last_attempt() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(status(StatusCode::SERVICE_UNAVAILABLE))
        .expect("observed first attempt");
    executor
        .application()
        .queue_failure(TransportFailure::NotSubmitted)
        .expect("proven no-send second attempt");
    let classifications = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(
        &executor,
        &probe,
        Some(recording_policy(1, &classifications)),
    );
    let error = get_request(core).await.expect_err("second attempt fails");

    assert_eq!(error.category(), ErrorCategory::Connect);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    let last = error.last_attempt().expect("last attempt");
    assert_eq!(last.index(), 1);
    assert_eq!(last.attempt().policy_retry_index, 1);
    assert_eq!(last.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(last.status(), None);
    assert_eq!(
        error.last_response_status(),
        Some(StatusCode::SERVICE_UNAVAILABLE)
    );
    let counts = error.operation_counts().expect("counts");
    assert_eq!(counts.attempts_started(), 2);
    assert_eq!(counts.application_handoffs(), 2);
    assert_eq!(counts.policy_retries(), 1);
}

#[tokio::test]
async fn v3_e03_observed_attempt_then_admission_failure_is_rate_admission_with_observed_aggregate()
{
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(status(StatusCode::SERVICE_UNAVAILABLE))
        .expect("observed first attempt");
    let classifications = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(JournalProbe {
        fail_acquisition: Some(1),
        ..JournalProbe::default()
    });
    let core = journal_core(
        &executor,
        &probe,
        Some(recording_policy(1, &classifications)),
    );
    let error = get_request(core).await.expect_err("second admission fails");

    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(error.failure_stage(), FailureStage::RateAdmission);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    let last = error.last_attempt().expect("last attempt");
    assert_eq!(last.index(), 1);
    assert_eq!(last.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(
        error.last_response_status(),
        Some(StatusCode::SERVICE_UNAVAILABLE)
    );
    let counts = error.operation_counts().expect("counts");
    assert_eq!(counts.application_handoffs(), 1);
    assert_eq!(counts.quota_admissions(), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

// ---------------------------------------------------------------------------
// V3-E04: cancellation and timeout at each pipeline boundary.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e04_token_cancelled_before_first_poll_prevents_every_effect() {
    let executor = DeterministicExecutor::new();
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, None);
    let token = CancellationToken::new();
    token.cancel();
    let error = get_request(core)
        .cancel_with(token)
        .expect("one source")
        .await
        .expect_err("cancelled before start");

    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(error.failure_stage(), FailureStage::BeforeDispatch);
    assert_eq!(error.operation_id(), None);
    assert_eq!(error.last_attempt(), None);
    assert_eq!(probe.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(probe.threads.lock().expect("threads").is_empty());
}

#[tokio::test]
async fn v3_e04_cancellation_during_admission_is_not_dispatched_and_never_sends() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"unused")))
        .expect("unused response");
    let gate = Gate::closed();
    let probe = Arc::new(JournalProbe {
        acquire_gate: Some((0, gate.clone())),
        ..JournalProbe::default()
    });
    let core = journal_core(&executor, &probe, None);
    let token = CancellationToken::new();
    let task = tokio::spawn(
        get_request(core)
            .cancel_with(token.clone())
            .expect("one source")
            .into_future(),
    );
    gate.wait_until_reached().await;
    token.cancel();
    let error = task
        .await
        .expect("join")
        .expect_err("cancelled in admission");
    gate.open();

    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(error.failure_stage(), FailureStage::RateAdmission);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert!(error.operation_id().is_some());
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert_eq!(probe.completions()[0].outcome, CompletionOutcome::Cancelled);
}

#[tokio::test]
async fn v3_e04_cancellation_at_native_handoff_is_possibly_dispatched_never_not_dispatched() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(gate.clone())
        .expect("handoff gate");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("unused response");
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, None);
    let token = CancellationToken::new();
    let task = tokio::spawn(
        post_request(core)
            .response()
            .cancel_with(token.clone())
            .expect("one source")
            .into_future(),
    );
    gate.wait_until_reached().await;
    token.cancel();
    let error = task.await.expect("join").expect_err("cancelled at handoff");
    gate.open();

    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(error.failure_stage(), FailureStage::DuringDispatchOrUnknown);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched
    );
    let last = error.last_attempt().expect("last attempt");
    assert_eq!(
        last.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched
    );
    assert_eq!(
        error
            .operation_counts()
            .expect("counts")
            .application_handoffs(),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn v3_e04_timeout_at_native_handoff_is_possibly_dispatched() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(gate.clone())
        .expect("handoff gate");
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, None);
    let error = post_request(core)
        .timeout(Duration::from_millis(50))
        .expect("valid timeout")
        .await
        .expect_err("handoff times out");
    gate.open();

    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.failure_stage(), FailureStage::DuringDispatchOrUnknown);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched
    );
}

#[tokio::test]
async fn v3_e04_cancellation_after_head_keeps_response_observed_and_status() {
    let body_gate = BodyGate::closed();
    let server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 200,
        body: BODY_SENTINEL.as_bytes().to_vec(),
        gate: body_gate.clone(),
    }])
    .expect("gated loopback server");
    let probe = Arc::new(JournalProbe::default());
    let head_seen = Arc::clone(&probe.head_seen);
    let token = CancellationToken::new();
    let task = tokio::spawn(
        get_request(loopback_core(&server, &probe))
            .cancel_with(token.clone())
            .expect("one source")
            .into_future(),
    );
    head_seen.notified().await;
    token.cancel();
    let error = task.await.expect("join").expect_err("cancelled after head");
    body_gate.release();

    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(error.failure_stage(), FailureStage::ResponseAcquisition);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert_omits(&format!("{error} {error:?}"), &[BODY_SENTINEL]);
}

#[tokio::test]
async fn v3_e04_timeout_after_head_keeps_response_observed_and_status() {
    let body_gate = BodyGate::closed();
    let server = ScriptedServer::start([ResponseScript::GatedBody {
        status: 200,
        body: b"late".to_vec(),
        gate: body_gate.clone(),
    }])
    .expect("gated loopback server");
    let probe = Arc::new(JournalProbe::default());
    let error = get_request(loopback_core(&server, &probe))
        .timeout(Duration::from_millis(300))
        .expect("valid timeout")
        .await
        .expect_err("body acquisition times out");
    body_gate.release();

    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.failure_stage(), FailureStage::ResponseAcquisition);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
}

#[tokio::test]
async fn v3_e04_connect_category_alone_never_settles_not_dispatched() {
    // Deterministic transport: a scripted `Connect` without proof.
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_failure(TransportFailure::Connect)
        .expect("connect failure");
    let probe = Arc::new(JournalProbe::default());
    let error = post_request(journal_core(&executor, &probe, None))
        .await
        .expect_err("connect failure");
    assert_eq!(error.category(), ErrorCategory::Connect);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched
    );
    assert_eq!(error.failure_stage(), FailureStage::DuringDispatchOrUnknown);

    // Real native transport: a refused loopback connection.
    let reserved = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("reserve port");
    let address = reserved.local_addr().expect("address");
    drop(reserved);
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    let core = ClientCore::build(
        &format!("http://{address}/v1/"),
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new(),
    )
    .expect("closed-port core");
    let error = post_request(core).await.expect_err("refused connection");
    assert!(matches!(
        error.category(),
        ErrorCategory::Connect | ErrorCategory::Transport
    ));
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched
    );
}

#[tokio::test]
async fn v3_e04_dropping_a_started_request_emits_one_cancelled_completion() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(gate.clone())
        .expect("handoff gate");
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, None);
    let task = tokio::spawn(post_request(core).into_future());
    gate.wait_until_reached().await;
    task.abort();
    assert!(task.await.expect_err("aborted").is_cancelled());
    gate.open();
    tokio::task::yield_now().await;
    assert_eq!(
        probe.completions(),
        [Completion {
            outcome: CompletionOutcome::Cancelled,
            attempts_started: 1,
            policy_retries: 0,
            final_status: None,
        }]
    );
}

// ---------------------------------------------------------------------------
// V3-E17: policy replay authority.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e17_non_idempotent_ambiguous_or_observed_failure_is_never_replayed() {
    for failure in [TransportFailure::Connect, TransportFailure::Transport] {
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_failure(failure)
            .expect("ambiguous failure");
        executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::new()))
            .expect("must remain queued");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Arc::new(JournalProbe::default());
        let core = journal_core(&executor, &probe, Some(recording_policy(2, &seen)));
        let error = post_request(core).await.expect_err("no replay");
        assert_eq!(
            error.dispatch_knowledge(),
            DispatchKnowledge::PossiblyDispatched
        );
        assert_eq!(executor.application().request_count().expect("count"), 1);
        assert_eq!(
            seen.lock().expect("seen").as_slice(),
            [DispatchKnowledge::PossiblyDispatched]
        );
    }

    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(status(StatusCode::SERVICE_UNAVAILABLE))
        .expect("observed failure");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("must remain queued");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, Some(recording_policy(2, &seen)));
    let error = post_request(core).await.expect_err("no replay");
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn v3_e17_conclusively_not_submitted_non_idempotent_attempt_can_use_policy_replay() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_failure(TransportFailure::NotSubmitted)
        .expect("proven no-send failure");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"written-once",
        )))
        .expect("replay success");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, Some(recording_policy(1, &seen)));
    let response = post_request(core)
        .response()
        .await
        .expect("permitted policy replay");
    assert_eq!(response.value(), &Bytes::from_static(b"written-once"));
    assert_eq!(response.metadata().attempts_started(), 2);
    assert_eq!(response.metadata().policy_retries(), 1);
    assert_eq!(
        seen.lock().expect("seen").as_slice(),
        [DispatchKnowledge::NotDispatched]
    );
}

// ---------------------------------------------------------------------------
// V3-E18: separate counts, intersecting caps, no reset through wrappers.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e18_send_cap_counts_every_handoff_and_intersects_policy_retries() {
    let executor = DeterministicExecutor::new();
    for _ in 0..5 {
        executor
            .application()
            .queue_response(status(StatusCode::SERVICE_UNAVAILABLE))
            .expect("rejected attempt");
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, Some(recording_policy(4, &seen)));
    let error = get_request(core)
        .with_options(&CallOptions::new().cap_sends(2))
        .await
        .expect_err("send cap stops the third submission");
    assert_eq!(executor.application().request_count().expect("count"), 2);
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    let counts = error.operation_counts().expect("counts");
    assert_eq!(counts.application_handoffs(), 2);
    assert_eq!(counts.attempts_started(), 2);
    assert_eq!(counts.policy_retries(), 1);
    assert_eq!(counts.authentication_recoveries(), 0);
    assert_eq!(counts.automatic_replays(), 1);
    assert_eq!(counts.freshness_restarts(), 0);
}

#[tokio::test]
async fn v3_e18_zero_replay_cap_prevents_policy_replay() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(status(StatusCode::SERVICE_UNAVAILABLE))
        .expect("rejected attempt");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("must remain queued");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, Some(recording_policy(4, &seen)));
    let error = get_request(core)
        .with_options(&CallOptions::new().cap_replays(0))
        .await
        .expect_err("replay cap zero");
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

#[tokio::test]
async fn v3_e18_caps_never_reset_through_terminal_wrappers_or_timeout_selection() {
    let executor = DeterministicExecutor::new();
    for _ in 0..3 {
        executor
            .application()
            .queue_response(status(StatusCode::SERVICE_UNAVAILABLE))
            .expect("rejected attempt");
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, Some(recording_policy(4, &seen)));
    let caps = CallOptions::new()
        .cap_sends(1)
        .cap_timeout(Duration::from_secs(5))
        .expect("timeout cap");
    let terminal = get_request(core)
        .with_options(&caps)
        .response()
        .timeout(Duration::from_secs(60))
        .expect("selection")
        .reset_timeout()
        .timeout(Duration::from_secs(90))
        .expect("selection")
        .with_options(&CallOptions::new().cap_sends(3));
    let summary = terminal.control_summary();
    assert_eq!(summary.send_cap(), Some(1));
    assert_eq!(summary.timeout_cap(), Some(Duration::from_secs(5)));
    assert_eq!(summary.selected_timeout(), Some(Duration::from_secs(90)));
    assert_eq!(summary.effective_timeout(), Some(Duration::from_secs(5)));
    assert_eq!(summary.declared_timeout(), Some(Duration::from_secs(30)));
    let error = terminal.await.expect_err("single send");
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(
        error
            .operation_counts()
            .expect("counts")
            .application_handoffs(),
        1
    );

    // The accepted-bytes terminal carries the same caps.
    let executor = DeterministicExecutor::new();
    for _ in 0..3 {
        executor
            .application()
            .queue_response(status(StatusCode::SERVICE_UNAVAILABLE))
            .expect("rejected attempt");
    }
    let core = journal_core(&executor, &probe, Some(recording_policy(4, &seen)));
    let accepted = get_request(core)
        .with_options(&CallOptions::new().cap_sends(1))
        .accepted_bytes()
        .reset_timeout();
    assert_eq!(accepted.control_summary().send_cap(), Some(1));
    let _ = accepted.await.expect_err("single send");
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

// ---------------------------------------------------------------------------
// V3-E20: server delay at the executor boundary.
// ---------------------------------------------------------------------------

fn retry_after_response(values: &[&str]) -> ScriptedResponse {
    let mut headers = HeaderMap::new();
    for value in values {
        headers.append(
            http::header::RETRY_AFTER,
            HeaderValue::from_str(value).expect("retry-after value"),
        );
    }
    status(StatusCode::SERVICE_UNAVAILABLE).headers(headers)
}

fn delayed_policy(willingness: Duration) -> RetryPolicy {
    RetryPolicy::builder()
        .max_retries_per_operation(1)
        .without_budget()
        .backoff(RetryBackoff::fixed(Duration::from_secs(1)))
        .honor_server_delay_up_to(willingness)
        .classify_fn(|_| RetryDecision::Retry)
        .build()
        .expect("delayed policy")
}

#[tokio::test(start_paused = true)]
async fn v3_e20_minimum_above_willingness_or_deadline_or_overlarge_refuses_retry() {
    for (value, willingness, timeout) in [
        // Untruncated minimum above willingness.
        ("20", Duration::from_secs(10), Duration::from_secs(30)),
        // Within willingness but not before the remaining deadline.
        ("20", Duration::from_secs(60), Duration::from_secs(10)),
        // Overflowing hint forbids retry even with unlimited willingness.
        (
            "99999999999999999999999",
            Duration::MAX,
            Duration::from_secs(30),
        ),
    ] {
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_response(retry_after_response(&[value]))
            .expect("delayed rejection");
        executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::new()))
            .expect("must remain queued");
        let probe = Arc::new(JournalProbe::default());
        let core = journal_core(&executor, &probe, Some(delayed_policy(willingness)));
        let started = tokio::time::Instant::now();
        let error = get_request(core)
            .timeout(timeout)
            .expect("valid timeout")
            .await
            .expect_err("retry refused");
        assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
        assert_eq!(executor.application().request_count().expect("count"), 1);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "a refused retry never sleeps"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn v3_e20_duplicate_retry_after_is_ignored_and_backoff_alone_applies() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(retry_after_response(&["30", "30"]))
        .expect("ambiguous rejection");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"after-backoff",
        )))
        .expect("retry success");
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, Some(delayed_policy(Duration::ZERO)));
    let started = tokio::time::Instant::now();
    let output = get_request(core).await.expect("retry after backoff only");
    assert_eq!(output, Bytes::from_static(b"after-backoff"));
    let waited = started.elapsed();
    assert!(waited >= Duration::from_secs(1));
    assert!(waited < Duration::from_secs(30));
}

#[tokio::test(start_paused = true)]
async fn v3_e20_wait_is_max_of_backoff_and_remaining_server_minimum_not_their_sum() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(retry_after_response(&["3"]))
        .expect("delayed rejection");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"on-time")))
        .expect("retry success");
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(
        &executor,
        &probe,
        Some(delayed_policy(Duration::from_secs(10))),
    );
    let started = tokio::time::Instant::now();
    let output = get_request(core).await.expect("delayed retry");
    assert_eq!(output, Bytes::from_static(b"on-time"));
    let waited = started.elapsed();
    // max(1 s backoff, 3 s server minimum) = 3 s, never 4 s.
    assert!(waited >= Duration::from_secs(3));
    assert!(waited < Duration::from_secs(4));
}

/// V3-E20 over-cap (§13.6, RETRY-03): a `Retry-After: 120` exceeds the
/// 60-second default diagnostics/cooldown cap. The retry gate uses the
/// untruncated minimum: with willingness 300 s the replay waits at least
/// 120 s (never the clamped 60 s); with willingness 90 s — above the clamp
/// but below the observed minimum — it refuses without sleeping. The status
/// diagnostics (`error.retry_after()`) retain the clamped 60 s, while the
/// observer and the limiter receive the one untruncated parsed fact, the
/// limiter together with the 60 s cap for its own cooldown storage.
#[tokio::test(start_paused = true)]
async fn v3_e20_over_cap_minimum_is_honoured_untruncated_while_diagnostics_report_the_clamp() {
    const OBSERVED: Duration = Duration::from_secs(120);
    const CLAMPED: Duration = Duration::from_secs(60);
    let _clock = crate::operation::TestClockGuard::reset();
    let too_many = || {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::RETRY_AFTER, HeaderValue::from_static("120"));
        status(StatusCode::TOO_MANY_REQUESTS).headers(headers)
    };

    // Willingness above the observed minimum: one replay after >= 120 s.
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(too_many())
        .expect("first over-cap rejection");
    executor
        .application()
        .queue_response(too_many())
        .expect("second over-cap rejection");
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(
        &executor,
        &probe,
        Some(delayed_policy(Duration::from_secs(300))),
    );
    let started = tokio::time::Instant::now();
    let error = get_request(core)
        .timeout(Duration::from_secs(600))
        .expect("valid timeout")
        .await
        .expect_err("terminal after the one permitted replay");
    let waited = started.elapsed();
    assert!(waited >= OBSERVED, "never the clamped minimum: {waited:?}");
    assert!(
        waited < OBSERVED + Duration::from_secs(1),
        "max(1 s backoff, 120 s minimum), not their sum: {waited:?}"
    );
    assert_eq!(executor.application().request_count().expect("count"), 2);
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::TOO_MANY_REQUESTS));
    assert_eq!(error.retry_after(), Some(CLAMPED));
    let counts = error.operation_counts().expect("counts");
    assert_eq!(counts.attempts_started(), 2);
    assert_eq!(counts.policy_retries(), 1);
    assert_eq!(
        probe.completions(),
        [Completion {
            outcome: CompletionOutcome::HttpStatus,
            attempts_started: 2,
            policy_retries: 1,
            final_status: Some(StatusCode::TOO_MANY_REQUESTS),
        }]
    );
    assert_eq!(
        *probe
            .observed_server_delays
            .lock()
            .expect("observed server delays"),
        [Some(OBSERVED), Some(OBSERVED)]
    );
    assert_eq!(
        *probe.limiter_feedback.lock().expect("limiter feedback"),
        [
            (StatusCode::TOO_MANY_REQUESTS, Some(OBSERVED), CLAMPED),
            (StatusCode::TOO_MANY_REQUESTS, Some(OBSERVED), CLAMPED),
        ]
    );

    // Willingness above the clamp but below the observed minimum: refused,
    // because the gate never uses the clamped 60 s.
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(too_many())
        .expect("over-cap rejection");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("must remain queued");
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(
        &executor,
        &probe,
        Some(delayed_policy(Duration::from_secs(90))),
    );
    let started = tokio::time::Instant::now();
    let error = get_request(core)
        .timeout(Duration::from_secs(600))
        .expect("valid timeout")
        .await
        .expect_err("retry refused");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "a refused retry never sleeps"
    );
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(error.http_status(), Some(StatusCode::TOO_MANY_REQUESTS));
    assert_eq!(error.retry_after(), Some(CLAMPED));
    assert_eq!(
        error.operation_counts().expect("counts").policy_retries(),
        0
    );
    assert_eq!(
        *probe
            .observed_server_delays
            .lock()
            .expect("observed server delays"),
        [Some(OBSERVED)]
    );
    assert_eq!(
        *probe.limiter_feedback.lock().expect("limiter feedback"),
        [(StatusCode::TOO_MANY_REQUESTS, Some(OBSERVED), CLAMPED)]
    );
}

// ---------------------------------------------------------------------------
// V3-E21: observer containment and fixed results.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e21_panicking_and_slow_final_observer_cannot_change_fixed_results_or_leak() {
    let _clock = crate::operation::TestClockGuard::reset();
    let before_panics = crate::hooks::observer_panic_count();

    // Success: every callback panics and the final one advances the clock far
    // beyond the deadline; the fixed success is still delivered.
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"fixed")))
        .expect("success");
    let probe = Arc::new(JournalProbe {
        panic_everywhere: true,
        completion_clock_advance: Some(Duration::from_secs(20)),
        ..JournalProbe::default()
    });
    let output = get_request(journal_core(&executor, &probe, None))
        .timeout(Duration::from_secs(1))
        .expect("valid timeout")
        .await
        .expect("fixed success survives the final observer");
    assert_eq!(output, Bytes::from_static(b"fixed"));
    assert_eq!(probe.completions()[0].outcome, CompletionOutcome::Success);
    // Attempt, head and completion callbacks all panicked and were contained.
    assert!(crate::hooks::observer_panic_count() >= before_panics.saturating_add(3));
    // Every callback ran synchronously on the operation's own thread: no
    // detached observer or exporter task exists.
    let test_thread = std::thread::current().id();
    let threads = probe.threads.lock().expect("threads").clone();
    assert_eq!(threads.len(), 3);
    assert!(threads.iter().all(|thread| *thread == test_thread));

    // Failure: the fixed rejected status and its journal survive, and nothing
    // private is formatted.
    let executor = DeterministicExecutor::new();
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::SET_COOKIE,
        HeaderValue::from_static(HEADER_SENTINEL),
    );
    executor
        .application()
        .queue_response(
            ScriptedResponse::status(
                StatusCode::BAD_GATEWAY,
                Bytes::from_static(BODY_SENTINEL.as_bytes()),
            )
            .headers(headers),
        )
        .expect("rejection");
    let probe = Arc::new(JournalProbe {
        panic_everywhere: true,
        completion_clock_advance: Some(Duration::from_secs(20)),
        ..JournalProbe::default()
    });
    let error = get_request(journal_core(&executor, &probe, None))
        .timeout(Duration::from_secs(1))
        .expect("valid timeout")
        .await
        .expect_err("fixed rejection");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::BAD_GATEWAY));
    assert_eq!(error.failure_stage(), FailureStage::RejectedResponse);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(
        probe.completions()[0].outcome,
        CompletionOutcome::HttpStatus
    );
    assert_omits(
        &format!("{error} {error:?} {}", recursive_error_surfaces(&error)),
        &[BODY_SENTINEL, HEADER_SENTINEL],
    );
}

// ---------------------------------------------------------------------------
// CONTROL-01..03: direct-request control enforcement.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn zero_send_cap_performs_no_head_body_or_transport_work() {
    let executor = DeterministicExecutor::new();
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, None);
    let encoder_calls = Arc::new(AtomicUsize::new(0));
    let error = codec_request::<ProbeEncoder, ProbeDecoder>(
        core,
        probe_input(b"never-encoded", &encoder_calls),
    )
    .with_options(&CallOptions::new().cap_sends(0))
    .await
    .expect_err("zero send cap");

    assert_eq!(error.category(), ErrorCategory::SendCapExhausted);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(error.failure_stage(), FailureStage::BeforeDispatch);
    assert_eq!(error.operation_id(), None);
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 0);
    assert_eq!(probe.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    assert!(probe.threads.lock().expect("threads").is_empty());
}

#[tokio::test(start_paused = true)]
async fn timeout_cap_survives_timeout_and_reset_timeout() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .pause_before_execution(gate.clone())
        .expect("handoff gate");
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, None);
    let started = tokio::time::Instant::now();
    let error = get_request(core)
        .with_options(
            &CallOptions::new()
                .cap_timeout(Duration::from_secs(5))
                .expect("cap"),
        )
        .timeout(Duration::from_secs(60))
        .expect("selection")
        .reset_timeout()
        .timeout(Duration::from_secs(120))
        .expect("selection")
        .await
        .expect_err("cap still expires");
    gate.open();
    assert_eq!(error.category(), ErrorCategory::Timeout);
    let waited = started.elapsed();
    assert!(waited >= Duration::from_secs(5));
    assert!(waited < Duration::from_secs(60));
}

#[tokio::test]
async fn ninth_cancellation_source_is_a_control_error_on_every_terminal() {
    let executor = DeterministicExecutor::new();
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, None);
    let mut request = get_request(core.clone());
    let shared = CancellationToken::new();
    for _ in 0..8 {
        request = request
            .cancel_with(CancellationToken::new())
            .expect("within bound");
    }
    assert_eq!(request.control_summary().cancellation_sources(), 8);
    assert_eq!(
        request.cancel_with(CancellationToken::new()).err(),
        Some(ControlError::TooManyCancellationSources)
    );

    let mut terminal = get_request(core.clone()).response();
    for _ in 0..8 {
        terminal = terminal
            .cancel_with(CancellationToken::new())
            .expect("within bound");
    }
    // Re-adding an attached signal deduplicates by identity.
    let mut deduplicated = get_request(core.clone()).accepted_bytes();
    for _ in 0..3 {
        deduplicated = deduplicated.cancel_with(shared.clone()).expect("duplicate");
    }
    assert_eq!(deduplicated.control_summary().cancellation_sources(), 1);
    assert_eq!(
        RequestControls::cancel_with(terminal, CancellationToken::new()).err(),
        Some(ControlError::TooManyCancellationSources)
    );
    assert!(matches!(
        get_request(core).timeout(Duration::ZERO),
        Err(ControlError::ZeroDuration)
    ));
}

#[tokio::test]
async fn cancelled_signal_observed_before_handoff_prevents_dispatch() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("unused response");
    let gate = Gate::closed();
    let probe = Arc::new(JournalProbe {
        acquire_gate: Some((0, gate.clone())),
        ..JournalProbe::default()
    });
    let core = journal_core(&executor, &probe, None);
    let token = CancellationToken::new();
    let request = get_request(core)
        .cancel_with(token.clone())
        .expect("one source");
    let task = tokio::spawn(request.into_future());
    gate.wait_until_reached().await;
    token.cancel();
    gate.open();
    let error = task.await.expect("join").expect_err("cancelled");
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn response_byte_cap_narrows_buffered_and_status_capture_ceilings() {
    let cap = CallOptions::new().cap_response_bytes(NonZeroUsize::new(8).expect("positive"));

    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"thirty-two-bytes-of-accepted-bod",
        )))
        .expect("oversized accepted body");
    let probe = Arc::new(JournalProbe::default());
    let error = get_request(journal_core(&executor, &probe, None))
        .with_options(&cap)
        .await
        .expect_err("capped accepted body");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(error.failure_stage(), FailureStage::ResponseAcquisition);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );

    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::BAD_GATEWAY,
            Bytes::from_static(b"thirty-two-bytes-of-rejected-bod"),
        ))
        .expect("oversized status body");
    let error = get_request(journal_core(&executor, &probe, None))
        .with_options(&cap)
        .await
        .expect_err("capped status body");
    assert_eq!(error.http_status(), Some(StatusCode::BAD_GATEWAY));
    let body = error.http_response().expect("status wrapper").body();
    assert_ne!(body.state(), StatusBodyState::Complete);
    assert!(body.bytes().is_none());
}

#[tokio::test]
async fn v3_e19_exhausted_retry_load_budget_denies_the_extra_retry_but_never_the_initial_send() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(status(StatusCode::SERVICE_UNAVAILABLE))
        .expect("rejected attempt");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::new()))
        .expect("must remain queued");
    let classifications = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&classifications);
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(4)
        .max_extra_load(0.0)
        .classify_fn(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        })
        .build()
        .expect("zero-load policy");
    let probe = Arc::new(JournalProbe::default());
    let error = get_request(journal_core(&executor, &probe, Some(policy)))
        .await
        .expect_err("extra retry denied");
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(classifications.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(
        error.operation_counts().expect("counts").policy_retries(),
        0
    );
}

/// The §23.2 shared helper: generic over the open control trait and the
/// terminal future, with no native request trait or private seal.
async fn execute_with<R, T>(request: R, options: &CallOptions) -> Result<T, ApiError>
where
    R: RequestControls + IntoFuture<Output = Result<T, ApiError>>,
{
    request.with_options(options).await
}

#[tokio::test]
async fn shared_execute_helper_applies_caps_to_request_and_terminal_wrappers() {
    let executor = DeterministicExecutor::new();
    for _ in 0..3 {
        executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"ok")))
            .expect("success");
    }
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, None);
    let options = CallOptions::new().cap_sends(1).cap_replays(0);
    let plain: Bytes = execute_with(get_request(core.clone()), &options)
        .await
        .expect("plain request");
    assert_eq!(plain, Bytes::from_static(b"ok"));
    let response = execute_with(get_request(core.clone()).response(), &options)
        .await
        .expect("response terminal");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = execute_with(get_request(core).accepted_bytes(), &options)
        .await
        .expect("accepted-bytes terminal");
    assert_eq!(bytes.value(), &Bytes::from_static(b"ok"));

    let zero = CallOptions::new().cap_sends(0);
    let error = execute_with(
        get_request(journal_core(&executor, &probe, None)).response(),
        &zero,
    )
    .await
    .expect_err("zero send cap through the helper");
    assert_eq!(error.category(), ErrorCategory::SendCapExhausted);
}

/// A §23.6-style write outcome: reconciliation is required whenever the
/// aggregate says the request may have reached the server.
#[derive(Debug, Eq, PartialEq)]
enum WriteOutcome {
    Written,
    NotSent,
    OutcomeUnknown,
}

async fn write_with_command_id(core: ClientCore) -> WriteOutcome {
    match Request::new(
        core,
        JournalEndpoint::<FailingDecoder>::new(Method::POST, Effect::NonIdempotent),
    )
    .await
    {
        Ok(_) => WriteOutcome::Written,
        Err(error) if error.may_have_reached_server() => WriteOutcome::OutcomeUnknown,
        Err(_) => WriteOutcome::NotSent,
    }
}

#[tokio::test]
async fn write_reconciliation_classifies_observed_success_with_decode_failure_as_unknown() {
    // An observed 2xx followed by a decoder failure may have committed the
    // write: the caller must reconcile with its durable command ID.
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"{}")))
        .expect("accepted write");
    let probe = Arc::new(JournalProbe::default());
    assert_eq!(
        write_with_command_id(journal_core(&executor, &probe, None)).await,
        WriteOutcome::OutcomeUnknown
    );

    // An ambiguous transport failure is also outcome-unknown.
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_failure(TransportFailure::Transport)
        .expect("ambiguous write");
    assert_eq!(
        write_with_command_id(journal_core(&executor, &probe, None)).await,
        WriteOutcome::OutcomeUnknown
    );

    // Only a proven no-send (or a pre-dispatch failure) is definitely not sent.
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_failure(TransportFailure::NotSubmitted)
        .expect("proven unsent write");
    assert_eq!(
        write_with_command_id(journal_core(&executor, &probe, None)).await,
        WriteOutcome::NotSent
    );
}

#[tokio::test]
async fn preparation_failure_of_a_started_operation_is_contextualized_not_dispatched() {
    let executor = DeterministicExecutor::new();
    let probe = Arc::new(JournalProbe::default());
    let core = journal_core(&executor, &probe, None);
    let error = Request::new(core, InvalidEndpoint)
        .await
        .expect_err("invalid head");
    assert_eq!(error.failure_stage(), FailureStage::Preparation);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert!(error.operation_id().is_some());
    assert_eq!(error.last_attempt(), None);
    assert_eq!(
        probe.completions()[0].outcome,
        CompletionOutcome::from_error_category(error.category())
    );
}
