//! A transferred head retains its journal facts at the deadline post-check.
//! Production executor controls distinguish an unread response from a withheld head.
use super::*;
use crate::controls::CancellationToken;
use crate::error::{
    DispatchKnowledge, FailureStage, ProviderAcquisitionFacts, ProviderAcquisitionOutcome,
};
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::Poll;

struct BoundaryEndpoint;
impl EndpointSpec for BoundaryEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = Bytes;
    type Response = Binary;
    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            "s1.head_boundary",
            Method::GET,
            Some(Duration::from_secs(1)),
        )
    }
    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("head-boundary")?;
        builder.finish()
    }
    fn into_body(self) {}
}

#[derive(Default)]
struct BoundaryObserver {
    heads: AtomicUsize,
    limiter_feedback: AtomicUsize,
    finished: AtomicUsize,
    providers: Mutex<Vec<ProviderAcquisitionFacts>>,
}
impl OperationObserver for BoundaryObserver {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::ResponseHeadObserved(_) => {
                self.heads.fetch_add(1, Ordering::SeqCst);
            }
            OperationEvent::OperationFinished(_) => {
                self.finished.fetch_add(1, Ordering::SeqCst);
            }
            OperationEvent::ProviderAcquisitionFinished(event) => {
                self.providers.lock().unwrap().push(event.facts);
            }
            _ => (),
        }
    }
}
impl RateLimiter for BoundaryObserver {
    fn capability(&self) -> crate::rate_limit::LimiterCapability {
        crate::rate_limit::LimiterCapability::new(
            crate::rate_limit::Enforcement::Local,
            "s1.head-boundary",
            1,
        )
    }
    fn acquire<'a>(
        &'a self,
        _: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }
    fn observe(&self, _: RateLimitResponse<'_>) {
        self.limiter_feedback.fetch_add(1, Ordering::SeqCst);
    }
}
fn boundary_core(executor: &DeterministicExecutor, observer: Arc<BoundaryObserver>) -> ClientCore {
    ClientCore::build_with_development(
        "https://boundary.example.test/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .rate_limiter(observer.clone())
            .operation_observer(observer),
        executor,
    )
    .unwrap()
}
async fn poll_once<F: Future>(mut future: Pin<&mut F>) -> Poll<F::Output> {
    std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx))).await
}

#[tokio::test(start_paused = true)]
async fn s1_native_application_head_expiry_poll_discriminates_transfer_from_abandonment() {
    // Native ObservedBody is constructed only AFTER the delivery gate's raw
    // ScriptedResponse has committed and converted to a reqwest::Response.
    // Thus drops=1/polls=0 proves returned native response ownership and an
    // unread synchronous drop; cancelling a raw script instead gives drops=0.
    for (name, elapsed, release) in [
        ("early", Duration::from_millis(999), true),
        ("exact_expiry", Duration::from_millis(1000), true),
        ("withheld_at_expiry", Duration::from_millis(1000), false),
    ] {
        let executor = DeterministicExecutor::new();
        let gate = Gate::closed();
        let polls = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let observer = Arc::new(BoundaryObserver::default());
        executor
            .application()
            .queue_response(
                ScriptedResponse::success(Bytes::from_static(b"head-owned-body"))
                    .delivery_gate(gate.clone())
                    .body_poll_counter(polls.clone())
                    .body_drop_counter(drops.clone()),
            )
            .unwrap();
        let core = boundary_core(&executor, observer.clone());
        let mut operation = Box::pin(
            Request::new(core, BoundaryEndpoint)
                .response()
                .into_future(),
        );
        let started = tokio::time::Instant::now();
        assert!(poll_once(operation.as_mut()).await.is_pending());
        assert_eq!(
            gate.arrivals(),
            1,
            "production native handoff reached delivery gate"
        );
        assert_eq!(
            tokio::time::Instant::now(),
            started,
            "initial poll did not advance time"
        );
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        tokio::time::advance(elapsed).await;
        assert_eq!(tokio::time::Instant::now() - started, elapsed);
        if release {
            gate.open();
        }
        let Poll::Ready(result) = poll_once(operation.as_mut()).await else {
            panic!("one controlled poll must settle ready transport or timer");
        };
        let body_polls = polls.load(Ordering::SeqCst);
        let body_drops = drops.load(Ordering::SeqCst);
        let heads = observer.heads.load(Ordering::SeqCst);
        if name == "early" {
            let response = result.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.value(), &Bytes::from_static(b"head-owned-body"));
            assert_eq!(heads, 1);
            assert_eq!(observer.limiter_feedback.load(Ordering::SeqCst), 1);
            assert!(body_polls > 0);
            assert_eq!(body_drops, 1);
            println!(
                "S1_HEAD_BOUNDARY case={name} elapsed_ms={} status=200 head_events={heads} native_body_polls={body_polls} native_body_drops={body_drops} handoffs=1",
                elapsed.as_millis()
            );
        } else {
            let error = api_error(result);
            assert_eq!(error.category(), ErrorCategory::Timeout);
            let expected_knowledge = if release {
                DispatchKnowledge::ResponseObserved
            } else {
                DispatchKnowledge::PossiblyDispatched
            };
            assert_eq!(error.dispatch_knowledge(), expected_knowledge);
            assert_eq!(
                error.failure_stage(),
                if release {
                    FailureStage::ResponseAcquisition
                } else {
                    FailureStage::DuringDispatchOrUnknown
                }
            );
            assert_eq!(
                error.last_response_status(),
                release.then_some(StatusCode::OK)
            );
            assert_eq!(
                error.last_attempt().unwrap().dispatch_knowledge(),
                expected_knowledge
            );
            assert_eq!(
                error.last_attempt().unwrap().status(),
                release.then_some(StatusCode::OK)
            );
            assert_eq!(error.operation_counts().unwrap().application_handoffs(), 1);
            assert_eq!(error.operation_counts().unwrap().attempts_started(), 1);
            assert_eq!(heads, usize::from(release));
            assert_eq!(
                observer.limiter_feedback.load(Ordering::SeqCst),
                0,
                "expired head supplies factual evidence without policy feedback"
            );
            assert_eq!(body_polls, 0);
            assert_eq!(
                body_drops,
                usize::from(release),
                "returned native response versus undelivered raw script"
            );
            println!(
                "S1_HEAD_BOUNDARY case={name} elapsed_ms={} category={:?} stage={:?} knowledge={:?} status={:?} head_events={heads} native_body_polls={body_polls} native_body_drops={body_drops} counts={:?}",
                elapsed.as_millis(),
                error.category(),
                error.failure_stage(),
                error.dispatch_knowledge(),
                error.last_response_status(),
                error.operation_counts()
            );
        }
        assert_eq!(executor.application().request_count().unwrap(), 1);
        assert_eq!(observer.finished.load(Ordering::SeqCst), 1);
    }
}

struct BoundaryProvider {
    origins: [ProviderOrigin; 1],
}
impl CredentialProvider for BoundaryProvider {
    type Material = AccessToken;
    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
        Box::pin(async move {
            context
                .http
                .send(ProviderRequest::new(
                    Method::POST,
                    url::Url::parse("https://boundary-provider.example.test/token").unwrap(),
                ))
                .await?;
            Ok(access_token(INITIAL))
        })
    }
    fn http_origins(&self) -> &[ProviderOrigin] {
        &self.origins
    }
}

#[tokio::test(start_paused = true)]
async fn s1_native_provider_head_before_cancel_retains_journal_without_completed_acquisition() {
    for delivered in [true, false] {
        let executor = DeterministicExecutor::new();
        let head_gate = Gate::closed();
        if delivered {
            head_gate.open();
        }
        let body_gate = Gate::closed();
        let polls = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let observer = Arc::new(BoundaryObserver::default());
        let mut rejection_headers = http::HeaderMap::new();
        rejection_headers.insert(http::header::RETRY_AFTER, HeaderValue::from_static("10"));
        executor
            .provider()
            .queue_response(
                ScriptedResponse::status(
                    StatusCode::TOO_MANY_REQUESTS,
                    Bytes::from_static(b"rejection"),
                )
                .headers(rejection_headers)
                .delivery_gate(head_gate.clone())
                .body_gate(body_gate)
                .body_poll_counter(polls.clone())
                .body_drop_counter(drops.clone()),
            )
            .unwrap();
        let selector = SlotSelector::provider(
            BoundaryProvider {
                origins: [ProviderOrigin::parse("https://boundary-provider.example.test").unwrap()],
            },
            Duration::ZERO,
        );
        let core = boundary_core(&executor, observer.clone());
        let token = CancellationToken::new();
        let mut operation = Box::pin(
            request(
                core,
                &selector,
                ChallengePolicy::Never,
                PreparedBody::bytes(Bytes::new()),
            )
            .cancel_with(token.clone())
            .unwrap()
            .response()
            .into_future(),
        );
        assert!(poll_once(operation.as_mut()).await.is_pending());
        assert_eq!(head_gate.arrivals(), 1);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert_eq!(polls.load(Ordering::SeqCst) > 0, delivered);
        token.cancel();
        let Poll::Ready(Err(error)) = poll_once(operation.as_mut()).await else {
            panic!("cancel must settle operation");
        };
        assert_eq!(error.category(), ErrorCategory::Cancelled);
        let facts = error.provider_acquisition().unwrap();
        assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Cancelled);
        assert_eq!(facts.helper_sends(), 1);
        assert_eq!(facts.parent_operation_id(), error.operation_id());
        assert_eq!(
            facts.last_status(),
            delivered.then_some(StatusCode::TOO_MANY_REQUESTS)
        );
        assert_eq!(
            facts.dispatch_knowledge(),
            if delivered {
                DispatchKnowledge::ResponseObserved
            } else {
                DispatchKnowledge::PossiblyDispatched
            }
        );
        assert_eq!(*observer.providers.lock().unwrap(), vec![facts]);
        assert_eq!(observer.finished.load(Ordering::SeqCst), 1);
        assert_eq!(drops.load(Ordering::SeqCst), usize::from(delivered));
        assert!(!selector.is_set());
        assert_eq!(executor.application().request_count().unwrap(), 0);
        println!(
            "S1_PROVIDER_HEAD_CANCEL delivered={delivered} category={:?} provider_outcome={:?} provider_knowledge={:?} provider_status={:?} helper_sends={} native_body_polls={} native_body_drops={} application_handoffs=0",
            error.category(),
            facts.outcome(),
            facts.dispatch_knowledge(),
            facts.last_status(),
            facts.helper_sends(),
            polls.load(Ordering::SeqCst),
            drops.load(Ordering::SeqCst)
        );
    }
}

mod callback_budget {
    use super::*;
    use std::task::{Context, Waker};

    struct CallbackBudgetFixture {
        overrun: bool,
        expires: Mutex<Option<tokio::time::Instant>>,
        debug: Arc<Mutex<Vec<String>>>,
        body_polls: Arc<AtomicUsize>,
        trace: Mutex<Vec<&'static str>>,
        feedback: AtomicUsize,
        heads: AtomicUsize,
        finished: Mutex<Vec<(CompletionOutcome, Option<StatusCode>)>>,
    }

    // Frozen Tokio 1.53.1 advances its real paused clock before advance()'s
    // first yield. Poll it once inside the synchronous callback, verify the
    // clock mutation directly, then drop the pending yield. This does not
    // block, nest a runtime, or modify only OperationDeadline's test offset.
    fn advance_paused_clock_synchronously(amount: Duration) {
        let before = tokio::time::Instant::now();
        let mut advance = Box::pin(tokio::time::advance(amount));
        let mut context = Context::from_waker(Waker::noop());
        let _ = advance.as_mut().poll(&mut context);
        assert_eq!(tokio::time::Instant::now(), before + amount);
    }

    impl RateLimiter for CallbackBudgetFixture {
        fn capability(&self) -> crate::rate_limit::LimiterCapability {
            crate::rate_limit::LimiterCapability::new(
                crate::rate_limit::Enforcement::Local,
                "head-callback-budget",
                1,
            )
        }
        fn acquire<'a>(
            &'a self,
            _: RateLimitRequest<'a>,
        ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
            Box::pin(async { Ok(RateLimitPermit::granted()) })
        }
        fn observe(&self, response: RateLimitResponse<'_>) {
            let expires = self.expires.lock().unwrap().unwrap();
            assert_eq!(
                tokio::time::Instant::now(),
                expires - Duration::from_millis(1)
            );
            assert!(
                tokio::time::Instant::now() < expires,
                "feedback started while live"
            );
            assert_eq!(response.status, StatusCode::OK);
            assert_eq!(response.server_delay, None);
            assert!(
                self.debug
                    .lock()
                    .unwrap()
                    .last()
                    .unwrap()
                    .starts_with("response-head "),
                "existing debug-before-feedback ordering"
            );
            self.trace.lock().unwrap().push("rate_begin");
            assert_eq!(self.feedback.fetch_add(1, Ordering::SeqCst), 0);
            if self.overrun {
                advance_paused_clock_synchronously(Duration::from_millis(2));
            }
            self.trace.lock().unwrap().push("rate_end");
        }
    }

    impl OperationObserver for CallbackBudgetFixture {
        fn observe(&self, event: OperationEvent<'_>) {
            match event {
                OperationEvent::ResponseHeadObserved(head) => {
                    assert_eq!(head.status, StatusCode::OK);
                    assert_eq!(head.server_delay, None);
                    assert_eq!(self.feedback.load(Ordering::SeqCst), 1);
                    assert_eq!(self.body_polls.load(Ordering::SeqCst), 0);
                    let expires = self.expires.lock().unwrap().unwrap();
                    assert_eq!(tokio::time::Instant::now() >= expires, self.overrun);
                    assert_eq!(self.heads.fetch_add(1, Ordering::SeqCst), 0);
                    self.trace.lock().unwrap().push("head");
                }
                OperationEvent::OperationFinished(finished) => {
                    self.finished
                        .lock()
                        .unwrap()
                        .push((finished.outcome, finished.final_status));
                    self.trace.lock().unwrap().push("finish");
                }
                _ => (),
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn entered_before_expiry_rate_callback_overrun_retains_one_head_with_positive_control() {
        for overrun in [false, true] {
            let executor = DeterministicExecutor::new();
            let gate = Gate::closed();
            let polls = Arc::new(AtomicUsize::new(0));
            let drops = Arc::new(AtomicUsize::new(0));
            let debug = Arc::new(Mutex::new(Vec::new()));
            let fixture = Arc::new(CallbackBudgetFixture {
                overrun,
                expires: Mutex::new(None),
                debug: debug.clone(),
                body_polls: polls.clone(),
                trace: Mutex::new(Vec::new()),
                feedback: AtomicUsize::new(0),
                heads: AtomicUsize::new(0),
                finished: Mutex::new(Vec::new()),
            });
            executor
                .application()
                .queue_response(
                    ScriptedResponse::success(Bytes::from_static(b"callback-body"))
                        .delivery_gate(gate.clone())
                        .body_poll_counter(polls.clone())
                        .body_drop_counter(drops.clone()),
                )
                .unwrap();
            let core = ClientCore::build_with_development(
                "https://boundary.example.test/",
                HttpConfigBuilder::new().http1_only(),
                RuntimeConfigBuilder::new()
                    .debug(DebugLevel::Basic)
                    .rate_limiter(fixture.clone())
                    .operation_observer(fixture.clone()),
                &executor,
            )
            .unwrap()
            .with_debug_output(debug.clone());
            let started = tokio::time::Instant::now();
            *fixture.expires.lock().unwrap() = Some(started + Duration::from_secs(1));
            let mut operation = Box::pin(
                Request::new(core, BoundaryEndpoint)
                    .response()
                    .into_future(),
            );
            assert!(poll_once(operation.as_mut()).await.is_pending());
            assert_eq!(gate.arrivals(), 1);
            assert_eq!(tokio::time::Instant::now(), started);
            tokio::time::advance(Duration::from_millis(999)).await;
            gate.open();
            let Poll::Ready(result) = poll_once(operation.as_mut()).await else {
                panic!("ready native response and synchronous callbacks must settle");
            };
            if overrun {
                let error = api_error(result);
                assert_eq!(error.category(), ErrorCategory::Timeout);
                assert_eq!(error.failure_stage(), FailureStage::ResponseAcquisition);
                assert_eq!(
                    error.dispatch_knowledge(),
                    DispatchKnowledge::ResponseObserved
                );
                assert_eq!(error.last_response_status(), Some(StatusCode::OK));
                assert_eq!(error.last_attempt().unwrap().status(), Some(StatusCode::OK));
                assert_eq!(
                    error.last_attempt().unwrap().dispatch_knowledge(),
                    DispatchKnowledge::ResponseObserved
                );
                let counts = error.operation_counts().unwrap();
                assert_eq!(counts.application_handoffs(), 1);
                assert_eq!(counts.attempts_started(), 1);
                assert_eq!(counts.quota_admissions(), 1);
                assert_eq!(counts.policy_retries(), 0);
                assert_eq!(counts.authentication_recoveries(), 0);
                assert_eq!(error.retry_after(), None);
                assert_eq!(error.provider_acquisition(), None);
                assert_eq!(polls.load(Ordering::SeqCst), 0);
                assert_eq!(
                    tokio::time::Instant::now() - started,
                    Duration::from_millis(1001)
                );
            } else {
                let response = result.unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.value(), &Bytes::from_static(b"callback-body"));
                assert!(polls.load(Ordering::SeqCst) > 0);
                assert_eq!(
                    tokio::time::Instant::now() - started,
                    Duration::from_millis(999)
                );
            }
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            assert_eq!(fixture.heads.load(Ordering::SeqCst), 1);
            assert_eq!(
                fixture.feedback.load(Ordering::SeqCst),
                1,
                "only callback entered before expiry"
            );
            assert_eq!(
                *fixture.finished.lock().unwrap(),
                vec![(
                    if overrun {
                        CompletionOutcome::Timeout
                    } else {
                        CompletionOutcome::Success
                    },
                    Some(StatusCode::OK),
                )]
            );
            assert_eq!(
                *fixture.trace.lock().unwrap(),
                vec!["rate_begin", "rate_end", "head", "finish"]
            );
            let captured = debug.lock().unwrap();
            assert_eq!(
                captured
                    .iter()
                    .filter(|line| line.starts_with("response-head "))
                    .count(),
                1
            );
            assert_eq!(
                captured
                    .iter()
                    .filter(|line| line.starts_with("completion "))
                    .count(),
                1
            );
            assert_eq!(executor.application().request_count().unwrap(), 1);
            assert_eq!(executor.application().remaining_scripts().unwrap(), 0);
            println!(
                "HEAD_CALLBACK_BUDGET overrun={overrun} elapsed_ms={} feedback=1 heads=1 finished=1 body_polls={} body_drops=1 ordering=debug,rate_begin,rate_end,head,finish",
                (tokio::time::Instant::now() - started).as_millis(),
                polls.load(Ordering::SeqCst)
            );
        }
    }
}

// Exercise late heads through the production retry, challenge and absence
// policies, with live controls that demonstrate each policy is enabled.
mod policy_followup {
    use super::*;
    use crate::endpoint::{AbsentStatus, Effect};
    use crate::response::{ExpectedAbsence, HeadAbsence};
    use crate::retry::{RetryDecision, RetryPolicy};
    use std::task::{Context, Waker};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Scenario {
        Retry503,
        Challenge401,
        Absence404,
    }
    impl Scenario {
        fn status(self) -> StatusCode {
            match self {
                Self::Retry503 => StatusCode::SERVICE_UNAVAILABLE,
                Self::Challenge401 => StatusCode::UNAUTHORIZED,
                Self::Absence404 => StatusCode::NOT_FOUND,
            }
        }
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Timing {
        ExactExpiry,
        FeedbackOverrun,
        LiveControl,
    }
    impl Timing {
        fn delivery(self) -> Duration {
            Duration::from_millis(match self {
                Self::ExactExpiry => 1000,
                Self::FeedbackOverrun => 999,
                Self::LiveControl => 100,
            })
        }
    }

    struct PolicyEndpoint {
        selector: Arc<SlotSelector<AccessToken>>,
        scenario: Scenario,
    }
    struct AbsenceEndpoint {
        selector: Arc<SlotSelector<AccessToken>>,
    }
    fn metadata() -> RequestMeta {
        RequestMeta::new_with_effect(
            "s1.head_deadline.policy",
            Method::GET,
            Some(Duration::from_secs(1)),
            Effect::Safe,
        )
    }
    fn prepare(
        mut builder: HeadBuilder,
        selector: &Arc<SlotSelector<AccessToken>>,
        challenge: ChallengePolicy,
    ) -> Result<PreparedHead, ApiError> {
        builder.push_static("head-policy")?;
        builder
            .push_auth(Auth::bearer(CredentialRef::new(selector.clone())).challenge(challenge))?;
        builder.push_rate_limit(
            RateLimitDeclaration::new("s1.head_deadline.policy_quota")
                .window(1, Duration::from_secs(1))
                .part("fixture", "policy"),
        )?;
        builder.finish()
    }
    impl EndpointSpec for PolicyEndpoint {
        type Input = ();
        type Body = EmptyBody;
        type Output = Bytes;
        type Response = Binary;
        fn request_meta(&self) -> RequestMeta {
            metadata()
        }
        fn prepare_head(&self, builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
            prepare(
                builder,
                &self.selector,
                if self.scenario == Scenario::Challenge401 {
                    ChallengePolicy::Unauthorized
                } else {
                    ChallengePolicy::Never
                },
            )
        }
        fn into_body(self) {}
    }
    impl EndpointSpec for AbsenceEndpoint {
        type Input = ();
        type Body = EmptyBody;
        type Output = Option<Bytes>;
        type Response = HeadAbsence<Binary>;
        fn request_meta(&self) -> RequestMeta {
            metadata()
        }
        fn prepare_head(&self, builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
            prepare(builder, &self.selector, ChallengePolicy::Never)
        }
        fn expected_absence(&self) -> Option<ExpectedAbsence<Self::Output, Self::Response>> {
            Some(ExpectedAbsence::new(
                AbsentStatus::codes([StatusCode::NOT_FOUND]).unwrap(),
            ))
        }
        fn into_body(self) {}
    }

    struct RefreshCounter {
        acquires: Arc<AtomicUsize>,
        refreshes: Arc<AtomicUsize>,
    }
    impl CredentialProvider for RefreshCounter {
        type Material = AccessToken;
        fn acquire<'a>(
            &'a self,
            _: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
            self.acquires.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(access_token(INITIAL)) })
        }
        fn refresh<'a>(
            &'a self,
            current: &'a AccessToken,
            context: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
            assert_eq!(current.expose_secret(), INITIAL);
            assert_eq!(context.reason, CredentialReason::Rejected);
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(access_token(REFRESHED)) })
        }
        fn recovery_mode(&self) -> RecoveryMode {
            RecoveryMode::Refresh
        }
    }

    type Finished = (CompletionOutcome, Option<StatusCode>, u32, u32, bool);
    struct Fixture {
        timing: Timing,
        expires: Mutex<Option<tokio::time::Instant>>,
        debug: Arc<Mutex<Vec<String>>>,
        admissions: AtomicUsize,
        feedback: AtomicUsize,
        heads: Mutex<Vec<(StatusCode, tokio::time::Instant)>>,
        finished: Mutex<Vec<Finished>>,
        trace: Mutex<Vec<&'static str>>,
    }
    // Tokio 1.53.1 performs the actual paused-clock advance before its first
    // yield. Verify that mutation directly; no TEST_CLOCK_OFFSET or model.
    fn advance_inside_callback(amount: Duration) {
        let before = tokio::time::Instant::now();
        let mut advance = Box::pin(tokio::time::advance(amount));
        let mut context = Context::from_waker(Waker::noop());
        let _ = advance.as_mut().poll(&mut context);
        assert_eq!(tokio::time::Instant::now(), before + amount);
    }
    impl RateLimiter for Fixture {
        fn capability(&self) -> crate::rate_limit::LimiterCapability {
            crate::rate_limit::LimiterCapability::new(
                crate::rate_limit::Enforcement::Local,
                "head-deadline-policy",
                1,
            )
        }
        fn acquire<'a>(
            &'a self,
            request: RateLimitRequest<'a>,
        ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
            assert!(
                !request.plan.is_empty(),
                "real effective conjunctive declaration"
            );
            self.admissions.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(RateLimitPermit::granted()) })
        }
        fn observe(&self, _: RateLimitResponse<'_>) {
            let expires = self.expires.lock().unwrap().unwrap();
            if self.timing == Timing::LiveControl {
                assert!(tokio::time::Instant::now() < expires);
            } else {
                assert_eq!(
                    tokio::time::Instant::now(),
                    expires - Duration::from_millis(1)
                );
            }
            assert!(
                self.debug
                    .lock()
                    .unwrap()
                    .last()
                    .unwrap()
                    .starts_with("response-head ")
            );
            self.feedback.fetch_add(1, Ordering::SeqCst);
            self.trace.lock().unwrap().push("feedback");
            if self.timing == Timing::FeedbackOverrun {
                advance_inside_callback(Duration::from_millis(2));
            }
        }
    }
    impl OperationObserver for Fixture {
        fn observe(&self, event: OperationEvent<'_>) {
            // Oracle assertions are outside this panic-contained callback.
            match event {
                OperationEvent::ResponseHeadObserved(head) => {
                    self.heads
                        .lock()
                        .unwrap()
                        .push((head.status, tokio::time::Instant::now()));
                    self.trace.lock().unwrap().push("head");
                }
                OperationEvent::OperationFinished(finished) => {
                    self.finished.lock().unwrap().push((
                        finished.outcome,
                        finished.final_status,
                        finished.attempts_started,
                        finished.policy_retries,
                        finished.recovered_authentication,
                    ));
                    self.trace.lock().unwrap().push("finish");
                }
                _ => (),
            }
        }
    }
    #[derive(Debug)]
    enum Value {
        Payload(Bytes),
        Absence(Option<Bytes>),
    }
    struct Terminal {
        value: Value,
        attempts: u32,
        policy_retries: u32,
        recovered: bool,
    }
    fn terminal<O>(
        response: crate::response::ApiResponse<O>,
        value: impl FnOnce(O) -> Value,
    ) -> Terminal {
        let attempts = response.metadata().attempts_started();
        let policy_retries = response.metadata().policy_retries();
        let recovered = response.metadata().recovered_authentication();
        Terminal {
            value: value(response.into_value()),
            attempts,
            policy_retries,
            recovered,
        }
    }

    async fn run(scenario: Scenario, timing: Timing) {
        let executor = DeterministicExecutor::new();
        let gate = Gate::closed();
        let polls = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let classified = Arc::new(AtomicUsize::new(0));
        let acquires = Arc::new(AtomicUsize::new(0));
        let refreshes = Arc::new(AtomicUsize::new(0));
        let selector = SlotSelector::provider(
            RefreshCounter {
                acquires: acquires.clone(),
                refreshes: refreshes.clone(),
            },
            Duration::ZERO,
        );
        let installed = selector.replace(access_token(INITIAL)).unwrap();
        let counters = selector.counters_for_test();
        let debug = Arc::new(Mutex::new(Vec::new()));
        let fixture = Arc::new(Fixture {
            timing,
            expires: Mutex::new(None),
            debug: debug.clone(),
            admissions: AtomicUsize::new(0),
            feedback: AtomicUsize::new(0),
            heads: Mutex::new(Vec::new()),
            finished: Mutex::new(Vec::new()),
            trace: Mutex::new(Vec::new()),
        });
        let classifier_counter = classified.clone();
        let policy = RetryPolicy::builder()
            .max_retries_per_operation(1)
            .without_budget()
            .classify_fn(move |_| {
                classifier_counter.fetch_add(1, Ordering::SeqCst);
                RetryDecision::Retry
            })
            .build()
            .unwrap();
        let core = ClientCore::build_with_development(
            "https://boundary.example.test/",
            HttpConfigBuilder::new().http1_only(),
            RuntimeConfigBuilder::new()
                .debug(DebugLevel::Basic)
                .retry_policy(policy)
                .rate_limiter(fixture.clone())
                .operation_observer(fixture.clone()),
            &executor,
        )
        .unwrap()
        .with_debug_output(debug.clone());
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new().header(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {INITIAL}")).unwrap(),
                ),
                ScriptedResponse::status(
                    scenario.status(),
                    Bytes::from_static(b"must-not-be-acquired"),
                )
                .delivery_gate(gate.clone())
                .body_poll_counter(polls.clone())
                .body_drop_counter(drops.clone()),
            )
            .unwrap();
        let second_token = if scenario == Scenario::Challenge401 {
            REFRESHED
        } else {
            INITIAL
        };
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new().header(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {second_token}")).unwrap(),
                ),
                ScriptedResponse::success(Bytes::from_static(b"policy-positive")),
            )
            .unwrap();
        let operation: BoxFuture<'static, Result<Terminal, ApiError>> =
            if scenario == Scenario::Absence404 {
                let request = Request::new(
                    core.clone(),
                    AbsenceEndpoint {
                        selector: selector.clone(),
                    },
                );
                Box::pin(async move {
                    request
                        .response()
                        .await
                        .map(|response| terminal(response, Value::Absence))
                })
            } else {
                let request = Request::new(
                    core.clone(),
                    PolicyEndpoint {
                        selector: selector.clone(),
                        scenario,
                    },
                );
                Box::pin(async move {
                    request
                        .response()
                        .await
                        .map(|response| terminal(response, Value::Payload))
                })
            };
        // Keep the borrowed Future sized for the existing poll_once<F> helper.
        let mut operation = Box::pin(operation);
        let started = tokio::time::Instant::now();
        *fixture.expires.lock().unwrap() = Some(started + Duration::from_secs(1));
        assert!(poll_once(operation.as_mut()).await.is_pending());
        assert_eq!(tokio::time::Instant::now(), started);
        assert_eq!(gate.arrivals(), 1);
        tokio::time::advance(timing.delivery()).await;
        gate.open();
        let result = match poll_once(operation.as_mut()).await {
            Poll::Ready(result) => result,
            Poll::Pending if timing == Timing::LiveControl => operation.await,
            Poll::Pending => panic!("ready head plus synchronous timeout path must settle"),
        };
        let heads = fixture.heads.lock().unwrap().clone();
        if timing != Timing::LiveControl {
            let error = result
                .err()
                .expect("expiry cannot manufacture replay, refresh, or Ok(None)");
            assert_eq!(error.category(), ErrorCategory::Timeout);
            assert_eq!(error.failure_stage(), FailureStage::RejectedResponse);
            assert_eq!(
                error.dispatch_knowledge(),
                DispatchKnowledge::ResponseObserved
            );
            assert_eq!(error.last_response_status(), Some(scenario.status()));
            assert_eq!(
                error.last_attempt().unwrap().status(),
                Some(scenario.status())
            );
            assert_eq!(
                error.last_attempt().unwrap().dispatch_knowledge(),
                DispatchKnowledge::ResponseObserved
            );
            let counts = error.operation_counts().unwrap();
            assert_eq!(counts.application_handoffs(), 1);
            assert_eq!(counts.attempts_started(), 1);
            assert_eq!(counts.quota_admissions(), 1);
            assert_eq!(counts.policy_retries(), 0);
            assert_eq!(counts.authentication_recoveries(), 0);
            assert_eq!(counts.freshness_restarts(), 0);
            assert_eq!(error.retry_after(), None);
            assert_eq!(error.provider_acquisition(), None);
            assert_eq!(classified.load(Ordering::SeqCst), 0);
            assert_eq!(refreshes.load(Ordering::SeqCst), 0);
            assert_eq!(selector.installed_version(), Some(installed));
            assert_eq!(selector.counters_for_test(), counters);
            assert!(selector.is_set(), "no post-deadline challenge invalidation");
            let still_ready = selector.preflight(core.provider_http(None)).await.unwrap();
            assert_eq!(still_ready.version(), installed);
            assert_eq!(still_ready.value().expose_secret(), INITIAL);
            assert_eq!(heads.len(), 1);
            assert_eq!(heads[0].0, scenario.status());
            assert!(heads[0].1 >= started + Duration::from_secs(1));
            assert_eq!(fixture.admissions.load(Ordering::SeqCst), 1);
            let expected_feedback = usize::from(timing == Timing::FeedbackOverrun);
            assert_eq!(fixture.feedback.load(Ordering::SeqCst), expected_feedback);
            assert_eq!(
                *fixture.finished.lock().unwrap(),
                vec![(
                    CompletionOutcome::Timeout,
                    Some(scenario.status()),
                    1,
                    0,
                    false
                )]
            );
            assert_eq!(
                *fixture.trace.lock().unwrap(),
                if expected_feedback == 0 {
                    vec!["head", "finish"]
                } else {
                    vec!["feedback", "head", "finish"]
                }
            );
            assert_eq!(executor.application().request_count().unwrap(), 1);
            assert_eq!(executor.application().remaining_scripts().unwrap(), 1);
            assert_eq!(
                tokio::time::Instant::now() - started,
                Duration::from_millis(if timing == Timing::ExactExpiry {
                    1000
                } else {
                    1001
                },)
            );
        } else {
            let outcome = result.unwrap();
            let replayed = scenario != Scenario::Absence404;
            match outcome.value {
                Value::Absence(value) => assert!(value.is_none(), "live absence positive control"),
                Value::Payload(value) => assert_eq!(value, Bytes::from_static(b"policy-positive")),
            }
            let expected_classified = usize::from(scenario == Scenario::Retry503);
            let expected_refreshes = usize::from(scenario == Scenario::Challenge401);
            let expected_attempts = if replayed { 2 } else { 1 };
            assert_eq!(classified.load(Ordering::SeqCst), expected_classified);
            assert_eq!(refreshes.load(Ordering::SeqCst), expected_refreshes);
            assert_eq!(outcome.attempts, expected_attempts);
            assert_eq!(outcome.policy_retries, expected_classified as u32);
            assert_eq!(outcome.recovered, expected_refreshes == 1);
            assert_eq!(
                fixture.feedback.load(Ordering::SeqCst),
                expected_attempts as usize
            );
            assert_eq!(
                fixture.admissions.load(Ordering::SeqCst),
                expected_attempts as usize
            );
            assert_eq!(
                heads.iter().map(|head| head.0).collect::<Vec<_>>(),
                if replayed {
                    vec![scenario.status(), StatusCode::OK]
                } else {
                    vec![scenario.status()]
                }
            );
            assert_eq!(
                *fixture.finished.lock().unwrap(),
                vec![(
                    CompletionOutcome::Success,
                    Some(if replayed {
                        StatusCode::OK
                    } else {
                        scenario.status()
                    }),
                    expected_attempts,
                    expected_classified as u32,
                    expected_refreshes == 1
                )]
            );
            assert_eq!(
                executor.application().request_count().unwrap(),
                expected_attempts as usize
            );
            assert_eq!(
                executor.application().remaining_scripts().unwrap(),
                usize::from(!replayed)
            );
            if expected_refreshes == 1 {
                assert_ne!(selector.installed_version(), Some(installed));
                assert_eq!(
                    selector.counters_for_test(),
                    (counters.0 + 1, counters.1 + 1)
                );
            } else {
                assert_eq!(selector.installed_version(), Some(installed));
                assert_eq!(selector.counters_for_test(), counters);
            }
        }
        assert_eq!(
            acquires.load(Ordering::SeqCst),
            0,
            "seeded material never reacquired"
        );
        assert_eq!(
            polls.load(Ordering::SeqCst),
            0,
            "first rejected/absent body always unread"
        );
        assert_eq!(
            drops.load(Ordering::SeqCst),
            1,
            "native response constructed and discarded once"
        );
        assert_eq!(fixture.finished.lock().unwrap().len(), 1);
        assert_eq!(
            debug
                .lock()
                .unwrap()
                .iter()
                .filter(|line| line.starts_with("response-head "))
                .count(),
            heads.len()
        );
        println!(
            "HEAD_POLICY scenario={scenario:?} timing={timing:?} classifiers={} refreshes={} feedback={} heads={} native_body_polls=0 native_body_drops=1 handoffs={} finals=1",
            classified.load(Ordering::SeqCst),
            refreshes.load(Ordering::SeqCst),
            fixture.feedback.load(Ordering::SeqCst),
            heads.len(),
            executor.application().request_count().unwrap()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn exact_expiry_and_feedback_overrun_do_not_activate_retry_refresh_or_absence() {
        for scenario in [
            Scenario::Retry503,
            Scenario::Challenge401,
            Scenario::Absence404,
        ] {
            for timing in [Timing::ExactExpiry, Timing::FeedbackOverrun] {
                run(scenario, timing).await;
            }
        }
    }
    #[tokio::test(start_paused = true)]
    async fn live_budget_positive_controls_activate_retry_refresh_and_absence() {
        for scenario in [
            Scenario::Retry503,
            Scenario::Challenge401,
            Scenario::Absence404,
        ] {
            run(scenario, Timing::LiveControl).await;
        }
    }
}
