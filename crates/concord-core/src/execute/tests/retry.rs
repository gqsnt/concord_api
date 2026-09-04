use super::*;
use http::{HeaderMap, HeaderValue};

use crate::retry::{RetryAttempt, RetryBackoff, RetryDecision, RetryFailure, RetryPolicy};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompletionSnapshot {
    operation_id: OperationId,
    attempts_started: u32,
    policy_retries: u32,
    recovered_authentication: bool,
    outcome: CompletionOutcome,
    final_status: Option<StatusCode>,
}

#[derive(Default)]
struct RetryLifecycleProbe {
    before: Mutex<Vec<(OperationId, AttemptInfo)>>,
    responses: Mutex<Vec<(OperationId, AttemptInfo, StatusCode)>>,
    completions: Mutex<Vec<CompletionSnapshot>>,
    acquisitions: Mutex<Vec<AttemptInfo>>,
    observations: Mutex<Vec<(AttemptInfo, StatusCode)>>,
    trace: Mutex<Vec<&'static str>>,
    fail_acquire_attempt: Option<u32>,
    response_drop_required_before_attempt: Option<(u32, Arc<AtomicUsize>)>,
    acquire_gate: Option<(u32, Gate)>,
}

impl RetryLifecycleProbe {
    fn before_attempts(&self) -> Vec<(OperationId, AttemptInfo)> {
        self.before.lock().expect("before records").clone()
    }

    fn response_attempts(&self) -> Vec<(OperationId, AttemptInfo, StatusCode)> {
        self.responses.lock().expect("response records").clone()
    }

    fn completions(&self) -> Vec<CompletionSnapshot> {
        self.completions.lock().expect("completion records").clone()
    }

    fn acquisitions(&self) -> Vec<AttemptInfo> {
        self.acquisitions
            .lock()
            .expect("acquisition records")
            .clone()
    }

    fn observations(&self) -> Vec<(AttemptInfo, StatusCode)> {
        self.observations
            .lock()
            .expect("observation records")
            .clone()
    }

    fn trace(&self) -> Vec<&'static str> {
        self.trace.lock().expect("retry trace").clone()
    }
}

impl OperationObserver for RetryLifecycleProbe {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(event) => {
                self.trace
                    .lock()
                    .expect("retry trace")
                    .push("attempt_started");
                self.before
                    .lock()
                    .expect("before records")
                    .push((event.operation_id, event.attempt));
                if let Some((attempt, drops)) = &self.response_drop_required_before_attempt {
                    if event.attempt.index == *attempt {
                        assert_eq!(drops.load(Ordering::SeqCst), 1);
                    }
                }
            }
            OperationEvent::ResponseHeadObserved(event) => {
                self.trace
                    .lock()
                    .expect("retry trace")
                    .push("response_observed");
                self.responses.lock().expect("response records").push((
                    event.operation_id,
                    event.attempt,
                    event.status,
                ));
            }
            OperationEvent::OperationFinished(event) => {
                self.trace.lock().expect("retry trace").push("complete");
                self.completions
                    .lock()
                    .expect("completion records")
                    .push(CompletionSnapshot {
                        operation_id: event.operation_id,
                        attempts_started: event.attempts_started,
                        policy_retries: event.policy_retries,
                        recovered_authentication: event.recovered_authentication,
                        outcome: event.outcome,
                        final_status: event.final_status,
                    });
            }
            OperationEvent::PaginationFinished(_) => {}
            _ => {}
        }
    }
}

impl RateLimiter for RetryLifecycleProbe {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        self.trace
            .lock()
            .expect("retry trace")
            .push("limiter_acquire");
        self.acquisitions
            .lock()
            .expect("acquisition records")
            .push(request.attempt);
        let gate = self
            .acquire_gate
            .as_ref()
            .filter(|(attempt, _)| *attempt == request.attempt.index)
            .map(|(_, gate)| gate.clone());
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.wait().await;
            }
            if self.fail_acquire_attempt == Some(request.attempt.index) {
                Err(RateLimitError::new(
                    crate::error::RateLimitErrorKind::Acquire,
                ))
            } else {
                Ok(RateLimitPermit::granted())
            }
        })
    }

    fn observe(&self, response: RateLimitResponse<'_>) {
        self.trace
            .lock()
            .expect("retry trace")
            .push("limiter_observe");
        self.observations
            .lock()
            .expect("observation records")
            .push((response.request.attempt, response.status));
    }
}

fn policy_without_budget<F>(maximum: u32, classifier: F) -> RetryPolicy
where
    F: for<'a> Fn(RetryAttempt<'a>) -> RetryDecision + Send + Sync + 'static,
{
    RetryPolicy::builder()
        .max_retries_per_operation(maximum)
        .without_budget()
        .classify_fn(classifier)
        .build()
        .expect("bounded retry policy")
}

fn retry_core(
    executor: &DeterministicExecutor,
    probe: Arc<RetryLifecycleProbe>,
    policy: RetryPolicy,
) -> ClientCore {
    #[cfg(not(feature = "default-tls"))]
    let _ = rustls::crypto::ring::default_provider().install_default();
    let limiter: Arc<dyn RateLimiter> = probe.clone();
    ClientCore::build_with_development(
        "http://execution.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .operation_observer(probe.clone())
            .operation_observer(probe.clone())
            .operation_observer(probe)
            .rate_limiter(limiter)
            .retry_policy(policy),
        executor,
    )
    .expect("retry development core")
}

fn retry_request(core: ClientCore, timeout: Duration) -> Request<BytesEndpoint> {
    Request::new(core, BytesEndpoint::new(Some(timeout)))
}

#[tokio::test]
async fn runtime_default_has_no_managed_retry_policy() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"terminal"),
        ))
        .expect("terminal default response");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"must-remain",
        )))
        .expect("unused response");
    let hooks = Arc::new(ProbeHooks::new());
    let core = development_core(&executor, hooks, RuntimeConfigBuilder::new());

    let error = bytes_request(core)
        .await
        .expect_err("default runtime does not retry status");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(
        executor
            .application()
            .remaining_scripts()
            .expect("remaining scripts"),
        1
    );
}

#[cfg(feature = "http2")]
#[tokio::test]
async fn private_protocol_recovery_is_one_visible_attempt_and_consumes_no_policy_retry_budget() {
    let server = crate::http::test_support::H2RefusedServer::start(1, 2).await;
    let physical_attempts = server.physical_attempts();
    let base_url = server.base_url();
    let classifications = Arc::new(AtomicUsize::new(0));
    let classifications_for_policy = Arc::clone(&classifications);
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(1)
        .classify_fn(move |_| {
            classifications_for_policy.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        })
        .build()
        .expect("bounded protocol-recovery observation policy");
    let shared_policy = policy.clone();
    let probe = Arc::new(RetryLifecycleProbe::default());
    let limiter: Arc<dyn RateLimiter> = probe.clone();
    let core = ClientCore::build(
        &base_url,
        HttpConfigBuilder::new().http2_prior_knowledge(),
        RuntimeConfigBuilder::new()
            .operation_observer(probe.clone())
            .operation_observer(probe.clone())
            .operation_observer(probe.clone())
            .rate_limiter(limiter)
            .retry_policy(policy),
    )
    .expect("HTTP/2 protocol-recovery core");

    let response = retry_request(core, Duration::from_secs(30))
        .response()
        .await
        .expect("private protocol recovery succeeds");

    server.finish().await;
    assert_eq!(physical_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(classifications.load(Ordering::SeqCst), 0);
    assert_eq!(probe.before_attempts().len(), 1);
    assert_eq!(probe.before_attempts()[0].1, AttemptInfo::INITIAL);
    assert_eq!(probe.acquisitions(), [AttemptInfo::INITIAL]);
    assert_eq!(
        probe.observations(),
        [(AttemptInfo::INITIAL, StatusCode::OK)]
    );
    assert_eq!(probe.response_attempts().len(), 1);
    assert_eq!(
        probe.completions(),
        [CompletionSnapshot {
            operation_id: probe.before_attempts()[0].0,
            attempts_started: 1,
            policy_retries: 0,
            recovered_authentication: false,
            outcome: CompletionOutcome::Success,
            final_status: Some(StatusCode::OK),
        }]
    );
    assert_eq!(response.metadata().attempts_started(), 1);
    assert_eq!(response.metadata().policy_retries(), 0);
    let logical_origin = url::Url::parse(&base_url).expect("logical retry origin");
    assert!(
        shared_policy.reserve_retry(&logical_origin),
        "a hidden protocol recovery must not consume managed retry reservation"
    );
}

#[cfg(feature = "http2")]
#[tokio::test]
async fn private_protocol_recovery_does_not_resend_a_one_shot_native_body() {
    let server = crate::http::test_support::H2RefusedServer::start(1, 1).await;
    let physical_attempts = server.physical_attempts();
    let probe = Arc::new(RetryLifecycleProbe::default());
    let limiter: Arc<dyn RateLimiter> = probe.clone();
    let core = ClientCore::build(
        &server.base_url(),
        HttpConfigBuilder::new().http2_prior_knowledge(),
        RuntimeConfigBuilder::new()
            .operation_observer(probe.clone())
            .operation_observer(probe.clone())
            .operation_observer(probe.clone())
            .rate_limiter(limiter),
    )
    .expect("HTTP/2 one-shot core");
    let polls = Arc::new(AtomicUsize::new(0));
    let body = RequestBody::stream(RequestStream::from_stream(BodyPollProbe::new(
        [ProbeFrame::Data(b"one-shot-protocol-body")],
        &polls,
    )));

    let error = body_request(core, body)
        .await
        .expect_err("REFUSED_STREAM cannot replay a one-shot native body");

    server.finish().await;
    assert!(matches!(
        error.category(),
        ErrorCategory::Connect | ErrorCategory::Transport
    ));
    assert_eq!(physical_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(probe.before_attempts().len(), 1);
    assert_eq!(probe.acquisitions(), [AttemptInfo::INITIAL]);
    assert!(probe.observations().is_empty());
    assert!(probe.response_attempts().is_empty());
    assert_eq!(probe.completions()[0].attempts_started, 1);
    assert_eq!(probe.completions()[0].policy_retries, 0);
}

#[tokio::test]
async fn connect_and_transport_failures_retry_with_one_visible_lifecycle() {
    for failure in [TransportFailure::Connect, TransportFailure::Transport] {
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_failure(failure)
            .expect("initial transport failure");
        executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"recovered")))
            .expect("retry success");
        let classified = Arc::new(Mutex::new(Vec::new()));
        let classified_for_policy = Arc::clone(&classified);
        let policy = policy_without_budget(1, move |attempt| {
            classified_for_policy
                .lock()
                .expect("classified failures")
                .push((attempt.attempt(), attempt.retries_performed()));
            RetryDecision::Retry
        });
        let probe = Arc::new(RetryLifecycleProbe::default());
        let core = retry_core(&executor, Arc::clone(&probe), policy);

        let response = retry_request(core, Duration::from_secs(30))
            .response()
            .await
            .expect("transport retry succeeds");
        assert_eq!(response.value(), &Bytes::from_static(b"recovered"));
        assert_eq!(
            executor
                .application()
                .request_count()
                .expect("request count"),
            2
        );
        let before = probe.before_attempts();
        assert_eq!(
            before
                .iter()
                .map(|(_, attempt)| *attempt)
                .collect::<Vec<_>>(),
            [
                AttemptInfo {
                    index: 0,
                    policy_retry_index: 0,
                    authentication_recovery: false,
                },
                AttemptInfo {
                    index: 1,
                    policy_retry_index: 1,
                    authentication_recovery: false,
                },
            ]
        );
        assert_eq!(before[0].0, before[1].0);
        assert_eq!(
            classified.lock().expect("classified failures").as_slice(),
            [(AttemptInfo::INITIAL, 0)]
        );
        assert_eq!(probe.acquisitions(), [AttemptInfo::INITIAL, before[1].1]);
        assert_eq!(
            probe.response_attempts(),
            [(before[0].0, before[1].1, StatusCode::OK)]
        );
        assert_eq!(
            probe.completions(),
            [CompletionSnapshot {
                operation_id: before[0].0,
                attempts_started: 2,
                policy_retries: 1,
                recovered_authentication: false,
                outcome: CompletionOutcome::Success,
                final_status: Some(StatusCode::OK),
            }]
        );
        assert_eq!(response.metadata().attempts_started(), 2);
        assert_eq!(response.metadata().policy_retries(), 1);
    }
}

#[tokio::test]
async fn timeout_and_request_body_failures_are_not_classifier_inputs() {
    for (failure, category) in [
        (TransportFailure::Timeout, ErrorCategory::Timeout),
        (TransportFailure::RequestBody, ErrorCategory::RequestBody),
    ] {
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_failure(failure)
            .expect("ineligible transport failure");
        executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"unused")))
            .expect("unused response");
        let classifications = Arc::new(AtomicUsize::new(0));
        let classifications_for_policy = Arc::clone(&classifications);
        let probe = Arc::new(RetryLifecycleProbe::default());
        let core = retry_core(
            &executor,
            probe,
            policy_without_budget(1, move |_| {
                classifications_for_policy.fetch_add(1, Ordering::SeqCst);
                RetryDecision::Retry
            }),
        );

        let error = retry_request(core, Duration::from_secs(30))
            .await
            .expect_err("ineligible failure remains terminal");
        assert_eq!(error.category(), category);
        assert_eq!(classifications.load(Ordering::SeqCst), 0);
        assert_eq!(executor.application().request_count().expect("count"), 1);
    }
}

#[tokio::test]
async fn rejected_status_observation_and_hook_precede_classifier_and_body_is_unpolled() {
    let executor = DeterministicExecutor::new();
    let body_polls = Arc::new(AtomicUsize::new(0));
    let body_drops = Arc::new(AtomicUsize::new(0));
    executor
        .application()
        .queue_response(
            ScriptedResponse::status(
                StatusCode::SERVICE_UNAVAILABLE,
                Bytes::from_static(b"intermediate-secret"),
            )
            .body_poll_counter(Arc::clone(&body_polls))
            .body_drop_counter(Arc::clone(&body_drops)),
        )
        .expect("intermediate 503");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"ok")))
        .expect("retry success");
    let probe = Arc::new(RetryLifecycleProbe {
        response_drop_required_before_attempt: Some((1, Arc::clone(&body_drops))),
        ..RetryLifecycleProbe::default()
    });
    let probe_for_policy = Arc::clone(&probe);
    let policy = policy_without_budget(1, move |attempt| {
        assert!(matches!(
            attempt.failure(),
            RetryFailure::RejectedStatus {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            }
        ));
        probe_for_policy
            .trace
            .lock()
            .expect("classifier trace")
            .push("classifier");
        RetryDecision::Retry
    });
    let core = retry_core(&executor, Arc::clone(&probe), policy);

    let output = retry_request(core, Duration::from_secs(30))
        .await
        .expect("503 retry succeeds");
    assert_eq!(output, Bytes::from_static(b"ok"));
    assert_eq!(body_polls.load(Ordering::SeqCst), 0);
    assert_eq!(body_drops.load(Ordering::SeqCst), 1);
    assert_eq!(
        probe.observations(),
        [
            (AttemptInfo::INITIAL, StatusCode::SERVICE_UNAVAILABLE),
            (
                AttemptInfo {
                    index: 1,
                    policy_retry_index: 1,
                    authentication_recovery: false,
                },
                StatusCode::OK,
            ),
        ]
    );
    assert_eq!(
        probe.trace(),
        [
            "attempt_started",
            "limiter_acquire",
            "limiter_observe",
            "response_observed",
            "classifier",
            "attempt_started",
            "limiter_acquire",
            "limiter_observe",
            "response_observed",
            "complete",
        ]
    );
}

struct AcceptServiceUnavailable;

impl TestResponseRepresentation for AcceptServiceUnavailable {
    type Output = Bytes;

    fn accepted_status() -> AcceptedStatus {
        AcceptedStatus::codes([StatusCode::SERVICE_UNAVAILABLE]).expect("nonempty test status set")
    }
}

impl ResponseRepresentation<Bytes> for AcceptServiceUnavailable {
    type Driver = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<Bytes, ResponseBodyError> {
        Ok(body)
    }
}

#[tokio::test]
async fn accepted_non_success_status_is_never_classified() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"accepted"),
        ))
        .expect("accepted 503");
    let classifications = Arc::new(AtomicUsize::new(0));
    let classifications_for_policy = Arc::clone(&classifications);
    let policy = policy_without_budget(1, move |_| {
        classifications_for_policy.fetch_add(1, Ordering::SeqCst);
        RetryDecision::Retry
    });
    let probe = Arc::new(RetryLifecycleProbe::default());
    let core = retry_core(&executor, Arc::clone(&probe), policy);

    let output = response_request::<AcceptServiceUnavailable>(core, Some(Duration::from_secs(30)))
        .await
        .expect("declared 503 acceptance");
    assert_eq!(output, Bytes::from_static(b"accepted"));
    assert_eq!(classifications.load(Ordering::SeqCst), 0);
    assert_eq!(probe.before_attempts().len(), 1);
    assert_eq!(probe.completions()[0].policy_retries, 0);
}

#[tokio::test]
async fn poisoned_retry_budget_preserves_the_original_rejected_status() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"ordinary-terminal-status"),
        ))
        .expect("terminal rejected status");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"unused")))
        .expect("unused retry response");

    let classifications = Arc::new(AtomicUsize::new(0));
    let classifications_for_policy = Arc::clone(&classifications);
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(1)
        .classify_fn(move |attempt| {
            classifications_for_policy.fetch_add(1, Ordering::SeqCst);
            assert!(matches!(
                attempt.failure(),
                RetryFailure::RejectedStatus {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    ..
                }
            ));
            RetryDecision::Retry
        })
        .build()
        .expect("budgeted retry policy");
    assert!(policy.poison_budget_state_for_test());
    let retained_policy = policy.clone();
    let probe = Arc::new(RetryLifecycleProbe::default());
    let core = retry_core(&executor, Arc::clone(&probe), policy);

    let error = retry_request(core, Duration::from_secs(30))
        .await
        .expect_err("poisoned budget suppresses the requested retry");

    assert_eq!(classifications.load(Ordering::SeqCst), 1);
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(
        executor
            .application()
            .remaining_scripts()
            .expect("remaining scripts"),
        1
    );
    assert_eq!(probe.before_attempts().len(), 1);
    assert_eq!(probe.before_attempts()[0].1, AttemptInfo::INITIAL);
    assert_eq!(probe.acquisitions(), [AttemptInfo::INITIAL]);
    assert_eq!(
        probe.observations(),
        [(AttemptInfo::INITIAL, StatusCode::SERVICE_UNAVAILABLE)]
    );
    assert_eq!(probe.response_attempts().len(), 1);
    assert_eq!(
        probe.trace(),
        [
            "attempt_started",
            "limiter_acquire",
            "limiter_observe",
            "response_observed",
            "complete",
        ]
    );

    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    let response = error.http_response().expect("captured terminal status");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.body().state(), StatusBodyState::Complete);
    assert_eq!(
        response.body().bytes(),
        Some(&Bytes::from_static(b"ordinary-terminal-status"))
    );
    assert_eq!(
        probe.completions(),
        [CompletionSnapshot {
            operation_id: probe.before_attempts()[0].0,
            attempts_started: 1,
            policy_retries: 0,
            recovered_authentication: false,
            outcome: CompletionOutcome::HttpStatus,
            final_status: Some(StatusCode::SERVICE_UNAVAILABLE),
        }]
    );

    let rendered = format!("{error}\n{error:?}").to_ascii_lowercase();
    for forbidden in [
        "mutex",
        "poison",
        "lock",
        "thread",
        "panic",
        "synchronization",
        "internal",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "public error formatting exposed {forbidden}"
        );
    }

    let logical_origin =
        url::Url::parse("http://execution.example.test/v1/").expect("logical retry origin");
    assert!(!retained_policy.reserve_retry(&logical_origin));
}

#[tokio::test]
async fn stop_and_every_retry_suppression_preserve_the_current_result() {
    let stop_executor = DeterministicExecutor::new();
    stop_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"terminal-status"),
        ))
        .expect("terminal status");
    let stop_probe = Arc::new(RetryLifecycleProbe::default());
    let stop_core = retry_core(
        &stop_executor,
        Arc::clone(&stop_probe),
        policy_without_budget(1, |_| RetryDecision::Stop),
    );
    let stopped = retry_request(stop_core, Duration::from_secs(30))
        .await
        .expect_err("classifier stop preserves status");
    assert_eq!(stopped.category(), ErrorCategory::HttpStatus);
    assert_eq!(
        stopped
            .http_response()
            .expect("captured status")
            .body()
            .bytes(),
        Some(&Bytes::from_static(b"terminal-status"))
    );

    let budget_executor = DeterministicExecutor::new();
    budget_executor
        .application()
        .queue_failure(TransportFailure::Transport)
        .expect("budget-denied failure");
    let budget_policy = RetryPolicy::builder()
        .max_extra_load(0.0)
        .classify_fn(|_| RetryDecision::Retry)
        .build()
        .expect("zero retry budget");
    let budget_probe = Arc::new(RetryLifecycleProbe::default());
    let budget_core = retry_core(&budget_executor, Arc::clone(&budget_probe), budget_policy);
    let denied = retry_request(budget_core, Duration::from_secs(30))
        .await
        .expect_err("budget denial preserves transport failure");
    assert_eq!(denied.category(), ErrorCategory::Transport);
    assert_eq!(budget_probe.before_attempts().len(), 1);
    assert_eq!(budget_probe.completions()[0].policy_retries, 0);

    let max_executor = DeterministicExecutor::new();
    for _ in 0..2 {
        max_executor
            .application()
            .queue_failure(TransportFailure::Connect)
            .expect("connect failure");
    }
    let max_probe = Arc::new(RetryLifecycleProbe::default());
    let max_core = retry_core(
        &max_executor,
        Arc::clone(&max_probe),
        policy_without_budget(1, |_| RetryDecision::Retry),
    );
    let maximum = retry_request(max_core, Duration::from_secs(30))
        .await
        .expect_err("per-operation maximum preserves final connect failure");
    assert_eq!(maximum.category(), ErrorCategory::Connect);
    assert_eq!(max_probe.before_attempts().len(), 2);
    assert_eq!(max_probe.completions()[0].policy_retries, 1);
}

#[tokio::test]
async fn one_shot_suppression_consumes_no_shared_retry_budget() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_failure(TransportFailure::Transport)
        .expect("one-shot failure");
    executor
        .application()
        .queue_failure(TransportFailure::Transport)
        .expect("reusable failure");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"budget-remained",
        )))
        .expect("reusable retry");
    let policy = RetryPolicy::builder()
        .classify_fn(|_| RetryDecision::Retry)
        .build()
        .expect("default shared budget");
    let probe = Arc::new(RetryLifecycleProbe::default());
    let core = retry_core(&executor, Arc::clone(&probe), policy);
    let polls = Arc::new(AtomicUsize::new(0));
    let one_shot = RequestBody::stream(
        RequestStream::from_stream(BodyPollProbe::new([ProbeFrame::Data(b"one-shot")], &polls))
            .exact_length(8),
    );

    let first = body_request(core.clone(), one_shot)
        .await
        .expect_err("one-shot transport failure is not retried");
    assert_eq!(first.category(), ErrorCategory::Transport);
    assert_eq!(probe.completions()[0].policy_retries, 0);

    let second = retry_request(core, Duration::from_secs(30))
        .await
        .expect("later replayable request retains the one allowed reservation");
    assert_eq!(second, Bytes::from_static(b"budget-remained"));
    assert_eq!(
        executor
            .application()
            .request_count()
            .expect("request count"),
        3
    );
    assert_eq!(probe.completions()[1].policy_retries, 1);
}

#[tokio::test]
async fn failures_after_retry_reservation_use_their_ordinary_categories() {
    let limiter_executor = DeterministicExecutor::new();
    limiter_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::new(),
        ))
        .expect("limiter retry status");
    let limiter_probe = Arc::new(RetryLifecycleProbe {
        fail_acquire_attempt: Some(1),
        ..RetryLifecycleProbe::default()
    });
    let limiter_core = retry_core(
        &limiter_executor,
        Arc::clone(&limiter_probe),
        policy_without_budget(1, |_| RetryDecision::Retry),
    );
    let limiter = retry_request(limiter_core, Duration::from_secs(30))
        .await
        .expect_err("retry limiter failure");
    assert_eq!(limiter.category(), ErrorCategory::RateLimit);
    assert_eq!(limiter_probe.acquisitions().len(), 2);
    assert_eq!(limiter_probe.completions()[0].policy_retries, 1);
}

struct FailSecondFactory {
    calls: Arc<AtomicUsize>,
}

impl BodyFactory for FailSecondFactory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            Ok(RequestStream::from_bytes(Bytes::from_static(b"initial")))
        } else {
            Err(BodyError::new(crate::body::BodyErrorKind::Factory))
        }
    }
}

#[tokio::test]
async fn retry_body_reconstruction_failure_is_request_body_primary() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::new(),
        ))
        .expect("factory retry status");
    let calls = Arc::new(AtomicUsize::new(0));
    let body = RequestBody::factory(FailSecondFactory {
        calls: Arc::clone(&calls),
    });
    let probe = Arc::new(RetryLifecycleProbe::default());
    let core = retry_core(
        &executor,
        Arc::clone(&probe),
        policy_without_budget(1, |_| RetryDecision::Retry),
    );

    let error = body_request(core, body)
        .await
        .expect_err("second factory call fails");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        executor
            .application()
            .request_count()
            .expect("request count"),
        1
    );
    assert_eq!(probe.completions()[0].attempts_started, 2);
    assert_eq!(probe.completions()[0].policy_retries, 1);
}

#[tokio::test]
async fn classifier_and_later_attempt_share_one_operation_deadline() {
    let classifier_executor = DeterministicExecutor::new();
    classifier_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::new(),
        ))
        .expect("slow classifier status");
    let classifier_probe = Arc::new(RetryLifecycleProbe::default());
    let classifier_core = retry_core(
        &classifier_executor,
        Arc::clone(&classifier_probe),
        policy_without_budget(1, |_| {
            std::thread::sleep(Duration::from_millis(30));
            RetryDecision::Retry
        }),
    );
    let classifier_timeout = retry_request(classifier_core, Duration::from_millis(5))
        .await
        .expect_err("classifier exhausts deadline");
    assert_eq!(classifier_timeout.category(), ErrorCategory::Timeout);
    assert_eq!(classifier_probe.before_attempts().len(), 1);
    assert_eq!(classifier_probe.completions()[0].policy_retries, 0);

    let attempt_executor = DeterministicExecutor::new();
    attempt_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::new(),
        ))
        .expect("retry status");
    let gate = Gate::closed();
    attempt_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"late")).delivery_gate(gate))
        .expect("blocked retry response");
    let attempt_probe = Arc::new(RetryLifecycleProbe::default());
    let attempt_core = retry_core(
        &attempt_executor,
        Arc::clone(&attempt_probe),
        policy_without_budget(1, |_| RetryDecision::Retry),
    );
    let attempt_timeout = retry_request(attempt_core, Duration::from_millis(20))
        .await
        .expect_err("later attempt uses original deadline");
    assert_eq!(attempt_timeout.category(), ErrorCategory::Timeout);
    assert_eq!(attempt_probe.before_attempts().len(), 2);
    assert_eq!(attempt_probe.completions()[0].attempts_started, 2);
    assert_eq!(attempt_probe.completions()[0].policy_retries, 1);
}

#[derive(Clone, Copy)]
enum RetryCancellationStage {
    LimiterAcquire,
    NativeExecution,
}

#[tokio::test]
async fn cancellation_drops_every_awaited_retry_stage_without_completion() {
    for stage in [
        RetryCancellationStage::LimiterAcquire,
        RetryCancellationStage::NativeExecution,
    ] {
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_response(ScriptedResponse::status(
                StatusCode::SERVICE_UNAVAILABLE,
                Bytes::new(),
            ))
            .expect("initial retry status");
        let gate = Gate::closed();
        let retry_response = ScriptedResponse::success(Bytes::from_static(b"cancelled"));
        let retry_response = if matches!(stage, RetryCancellationStage::NativeExecution) {
            retry_response.delivery_gate(gate.clone())
        } else {
            retry_response
        };
        executor
            .application()
            .queue_response(retry_response)
            .expect("retry response");
        let mut probe = RetryLifecycleProbe::default();
        match stage {
            RetryCancellationStage::LimiterAcquire => {
                probe.acquire_gate = Some((1, gate.clone()));
            }
            RetryCancellationStage::NativeExecution => {}
        }
        let probe = Arc::new(probe);
        let core = retry_core(
            &executor,
            Arc::clone(&probe),
            policy_without_budget(1, |_| RetryDecision::Retry),
        );
        let operation =
            tokio::spawn(async move { retry_request(core, Duration::from_secs(30)).await });
        gate.wait_until_reached().await;

        operation.abort();
        assert!(operation.await.is_err());
        tokio::task::yield_now().await;
        assert!(probe.completions().is_empty());
        assert_eq!(probe.before_attempts().len(), 2);
        let expected_requests = match stage {
            RetryCancellationStage::LimiterAcquire => 1,
            RetryCancellationStage::NativeExecution => 2,
        };
        assert_eq!(
            executor
                .application()
                .request_count()
                .expect("request count"),
            expected_requests
        );
    }
}

#[tokio::test]
async fn buffered_bytes_are_reused_for_each_visible_retry() {
    let executor = DeterministicExecutor::new();
    let request_body = Bytes::from_static(b"same-buffered-body");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().body(request_body.clone()),
            ScriptedResponse::status(StatusCode::SERVICE_UNAVAILABLE, Bytes::new()),
        )
        .expect("buffered retry status");
    executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().body(request_body.clone()),
            ScriptedResponse::success(Bytes::from_static(b"buffered-ok")),
        )
        .expect("buffered retry success");
    let probe = Arc::new(RetryLifecycleProbe::default());
    let core = retry_core(
        &executor,
        probe,
        policy_without_budget(1, |_| RetryDecision::Retry),
    );

    let output = body_request(core, RequestBody::bytes(request_body))
        .await
        .expect("buffered bytes retry succeeds");

    assert_eq!(output, Bytes::from_static(b"buffered-ok"));
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

#[tokio::test]
async fn buffered_encoder_and_reconstructible_factory_follow_attempt_ownership() {
    let codec_executor = DeterministicExecutor::new();
    codec_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::new(),
        ))
        .expect("codec retry status");
    codec_executor
        .application()
        .queue_response(probe_response(Bytes::from_static(b"decoded")))
        .expect("codec retry success");
    let encoder_calls = Arc::new(AtomicUsize::new(0));
    let codec_probe = Arc::new(RetryLifecycleProbe::default());
    let codec_core = retry_core(
        &codec_executor,
        Arc::clone(&codec_probe),
        policy_without_budget(1, |_| RetryDecision::Retry),
    );
    let decoded = codec_request::<ProbeEncoder, ProbeDecoder>(
        codec_core,
        probe_input(b"encoded-once", &encoder_calls),
    )
    .await
    .expect("buffered codec retry succeeds");
    assert_eq!(decoded, "decoded:decoded");
    assert_eq!(encoder_calls.load(Ordering::SeqCst), 1);

    let factory_executor = DeterministicExecutor::new();
    factory_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::new(),
        ))
        .expect("factory retry status");
    factory_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"factory-ok")))
        .expect("factory retry success");
    let content_type_calls = Arc::new(AtomicUsize::new(0));
    let create_calls = Arc::new(AtomicUsize::new(0));
    let polls = Arc::new(AtomicUsize::new(0));
    let factory_body = RequestBody::factory(ProbeBodyFactory {
        content_type_calls: Arc::clone(&content_type_calls),
        create_calls: Arc::clone(&create_calls),
        polls: Arc::clone(&polls),
        fail: false,
    });
    let factory_probe = Arc::new(RetryLifecycleProbe::default());
    let factory_core = retry_core(
        &factory_executor,
        factory_probe,
        policy_without_budget(1, |_| RetryDecision::Retry),
    );
    let output = body_request(factory_core, factory_body)
        .await
        .expect("factory retry succeeds");
    assert_eq!(output, Bytes::from_static(b"factory-ok"));
    assert_eq!(content_type_calls.load(Ordering::SeqCst), 1);
    assert_eq!(create_calls.load(Ordering::SeqCst), 2);
    assert!(polls.load(Ordering::SeqCst) >= 2);
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn multipart_retry_follows_recipe_replayability() {
    let replayable_executor = DeterministicExecutor::new();
    replayable_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::new(),
        ))
        .expect("replayable multipart retry status");
    replayable_executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(
            b"multipart-ok",
        )))
        .expect("replayable multipart success");
    let replayable = MultipartBody::new()
        .part(
            "field",
            MultipartPart::text("replayable multipart value").expect("bounded multipart text"),
        )
        .expect("bounded multipart recipe");
    let replayable_probe = Arc::new(RetryLifecycleProbe::default());
    let replayable_core = retry_core(
        &replayable_executor,
        Arc::clone(&replayable_probe),
        policy_without_budget(1, |_| RetryDecision::Retry),
    );
    let output = body_request(replayable_core, RequestBody::multipart(replayable))
        .await
        .expect("replayable multipart retries");
    assert_eq!(output, Bytes::from_static(b"multipart-ok"));
    assert_eq!(
        replayable_executor
            .application()
            .request_count()
            .expect("request count"),
        2
    );
    assert_eq!(replayable_probe.completions()[0].policy_retries, 1);

    let one_shot_executor = DeterministicExecutor::new();
    one_shot_executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(b"one-shot-terminal"),
        ))
        .expect("one-shot multipart status");
    let one_shot = MultipartBody::new()
        .part(
            "stream",
            MultipartPart::stream(RequestStream::from_bytes(Bytes::from_static(
                b"one-shot multipart",
            ))),
        )
        .expect("bounded one-shot multipart recipe");
    let one_shot_probe = Arc::new(RetryLifecycleProbe::default());
    let one_shot_core = retry_core(
        &one_shot_executor,
        Arc::clone(&one_shot_probe),
        policy_without_budget(1, |_| RetryDecision::Retry),
    );
    let error = body_request(one_shot_core, RequestBody::multipart(one_shot))
        .await
        .expect_err("one-shot multipart suppresses optional retry");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(
        error
            .http_response()
            .expect("terminal one-shot response")
            .body()
            .bytes(),
        Some(&Bytes::from_static(b"one-shot-terminal"))
    );
    assert_eq!(
        one_shot_executor
            .application()
            .request_count()
            .expect("request count"),
        1
    );
    assert_eq!(one_shot_probe.completions()[0].policy_retries, 0);
}

#[tokio::test(start_paused = true)]
async fn retry_waits_for_maximum_of_backoff_and_server_minimum() {
    let executor = DeterministicExecutor::new();
    let mut headers = HeaderMap::new();
    headers.insert(http::header::RETRY_AFTER, HeaderValue::from_static("5"));
    executor
        .application()
        .queue_response(
            ScriptedResponse::status(StatusCode::SERVICE_UNAVAILABLE, Bytes::new())
                .headers(headers),
        )
        .expect("delayed retry status");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"delayed-ok")))
        .expect("delayed retry success");
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(1)
        .without_budget()
        .backoff(RetryBackoff::fixed(Duration::from_secs(2)))
        .honor_server_delay_up_to(Duration::from_secs(10))
        .classify_fn(|_| RetryDecision::Retry)
        .build()
        .expect("delayed retry policy");
    let probe = Arc::new(RetryLifecycleProbe::default());
    let core = retry_core(&executor, Arc::clone(&probe), policy);
    let task = tokio::spawn(retry_request(core, Duration::from_secs(30)).into_future());

    tokio::task::yield_now().await;
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(probe.acquisitions().len(), 1);
    tokio::time::advance(Duration::from_secs(4)).await;
    tokio::task::yield_now().await;
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_eq!(probe.acquisitions().len(), 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    let output = task.await.expect("retry task").expect("delayed retry");
    assert_eq!(output, Bytes::from_static(b"delayed-ok"));
    assert_eq!(probe.acquisitions().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn cancellation_during_backoff_consumes_no_shared_retry_budget() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::new(),
        ))
        .expect("retry status");
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(1)
        .max_extra_load(1.0)
        .backoff(RetryBackoff::fixed(Duration::from_secs(10)))
        .classify_fn(|_| RetryDecision::Retry)
        .build()
        .expect("budgeted delayed policy");
    let retained = policy.clone();
    let probe = Arc::new(RetryLifecycleProbe::default());
    let core = retry_core(&executor, probe, policy);
    let task = tokio::spawn(retry_request(core, Duration::from_secs(30)).into_future());

    tokio::task::yield_now().await;
    assert_eq!(executor.application().request_count().expect("count"), 1);
    task.abort();
    let _ = task.await;

    let logical_origin =
        url::Url::parse("http://execution.example.test/v1/").expect("logical retry origin");
    assert!(
        retained.reserve_retry(&logical_origin),
        "backoff cancellation must leave the shared retry allowance unused"
    );
}
