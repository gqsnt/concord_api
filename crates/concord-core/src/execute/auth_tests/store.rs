//! Explicit endpoint-backed storage through the managed executor
//! (AUTH-03, AUTH-05, §11.3 ordering table, CONTROL-03 laziness).
//!
//! Interleavings are forced with deterministic-transport gates: a storage
//! operation "starts" when its first poll reserves the intent (before its
//! login request reaches the transport) and "succeeds" when its login
//! response is released.

use std::future::IntoFuture;

use crate::auth::{CredentialContext, StoreOutcome};
use crate::codec::CodecErrorKind;
use crate::controls::{
    CallOptions, CancellationToken, ControlError, MAX_CANCELLATION_SOURCES, RequestControls,
};
use crate::error::{DispatchKnowledge, FailureStage};
use crate::execute::StoreRequest;

use super::*;

const LOGIN: &str = "authentication_api.login";
const TOKEN_A: &str = "stored-session-a-5b1e44";
const TOKEN_B: &str = "stored-session-b-9c3d71";

struct SessionCodec;

impl DecodeBody for SessionCodec {
    type Output = AccessToken;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text =
            std::str::from_utf8(&body).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        AccessToken::try_new(text).map_err(|_| CodecError::new(CodecErrorKind::Decode))
    }
}

/// A credential source endpoint: its declared output is the material type.
struct LoginEndpoint;

impl EndpointSpec for LoginEndpoint {
    type Input = PreparedBody;
    type Body = PreparedBody;
    type Output = AccessToken;
    type Response = SessionCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(LOGIN, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("login")?;
        builder.finish()
    }

    fn into_body(self) -> Self::Body {
        PreparedBody::bytes(Bytes::new())
    }
}

fn store_as(core: &ClientCore, selector: &Arc<SlotSelector<AccessToken>>) -> StoreRequest {
    crate::execute::store_request(
        Request::new(core.clone(), LoginEndpoint),
        Arc::clone(selector),
    )
}

fn login_response(token: &'static str) -> ScriptedResponse {
    ScriptedResponse::success(Bytes::from_static(token.as_bytes()))
}

fn bearer(token: &'static str) -> ExpectedRequest {
    ExpectedRequest::new().header(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).expect("bearer header"),
    )
}

async fn protected_call(
    core: &ClientCore,
    selector: &Arc<SlotSelector<AccessToken>>,
) -> Result<Bytes, ApiError> {
    request(
        core.clone(),
        selector,
        ChallengePolicy::Unauthorized,
        PreparedBody::bytes(Bytes::new()),
    )
    .await
}

fn installed(outcome: StoreOutcome) -> crate::auth::CredentialVersion {
    match outcome {
        StoreOutcome::Installed { version } => version,
        StoreOutcome::Superseded => panic!("expected an installed outcome"),
    }
}

// CONTROL-03 + baseline storage: unpolled storage reserves nothing; a polled
// one installs a new version that protected calls then apply.
#[tokio::test]
async fn store_request_is_inert_until_first_poll_then_installs_and_is_applied() {
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let unpolled = store_as(&core, &selector);
    let _future = unpolled.into_future();
    drop(_future);
    assert_eq!(selector.counters_for_test(), (0, 0));
    assert_eq!(executor.application().request_count().expect("count"), 0);

    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let version = installed(store_as(&core, &selector).await.expect("storage"));
    assert_eq!(selector.installed_version(), Some(version));
    assert_eq!(selector.counters_for_test(), (1, 1));

    executor
        .application()
        .queue_expected_response(bearer(TOKEN_A), ScriptedResponse::success(Bytes::new()))
        .expect("protected response");
    protected_call(&core, &selector)
        .await
        .expect("stored material is applied");
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

// V3-E12 row 6: an unpolled storage request created before a clear and first
// polled after it is a new post-clear intent, not permanently banned.
#[tokio::test]
async fn unpolled_store_created_before_clear_is_a_new_post_clear_intent() {
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::endpoint(Duration::ZERO);
    selector
        .replace(access_token(INITIAL))
        .expect("existing session");
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let created_before_clear = store_as(&core, &selector);
    selector.clear().expect("logout");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    installed(created_before_clear.await.expect("post-clear storage"));
    assert!(selector.is_set());
}

// V3-E12 row 1: Start A; clear; A succeeds => A Superseded; store empty.
#[tokio::test]
async fn start_a_then_clear_then_a_succeeds_is_superseded_and_store_stays_empty() {
    let executor = DeterministicExecutor::new();
    let gate_a = Gate::closed();
    executor
        .application()
        .queue_gate(gate_a.clone())
        .expect("gate A");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login A");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let a = tokio::spawn(store_as(&core, &selector).into_future());
    gate_a.wait_until_reached().await;
    selector.clear().expect("clear while A is pending");
    gate_a.open();
    assert_eq!(
        a.await.expect("A task").expect("A completes"),
        StoreOutcome::Superseded
    );
    assert!(!selector.is_set());
    assert_eq!(selector.installed_version(), None);
    assert_eq!(
        api_error(protected_call(&core, &selector).await).category(),
        ErrorCategory::AuthenticationUnavailable
    );
    assert_eq!(executor.application().request_count().expect("count"), 1);
}

// V3-E12 row 2: Start A; manual replace B; A succeeds => A Superseded; B remains.
#[tokio::test]
async fn start_a_then_manual_replace_b_then_a_succeeds_keeps_b() {
    let executor = DeterministicExecutor::new();
    let gate_a = Gate::closed();
    executor
        .application()
        .queue_gate(gate_a.clone())
        .expect("gate A");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login A");
    executor
        .application()
        .queue_expected_response(bearer(TOKEN_B), ScriptedResponse::success(Bytes::new()))
        .expect("protected response with B");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let a = tokio::spawn(store_as(&core, &selector).into_future());
    gate_a.wait_until_reached().await;
    let b = selector
        .replace(access_token(TOKEN_B))
        .expect("manual replace B");
    gate_a.open();
    assert_eq!(
        a.await.expect("A task").expect("A completes"),
        StoreOutcome::Superseded
    );
    assert_eq!(selector.installed_version(), Some(b));
    protected_call(&core, &selector)
        .await
        .expect("B is applied");
}

// V3-E12 row 3: Start A; start B; A succeeds; B succeeds => A Superseded; B installs.
#[tokio::test]
async fn start_a_start_b_then_a_succeeds_then_b_succeeds_installs_b() {
    let executor = DeterministicExecutor::new();
    let gate_a = Gate::closed();
    let gate_b = Gate::closed();
    executor
        .application()
        .queue_gate(gate_a.clone())
        .expect("gate A");
    executor
        .application()
        .queue_gate(gate_b.clone())
        .expect("gate B");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login A");
    executor
        .application()
        .queue_response(login_response(TOKEN_B))
        .expect("login B");
    executor
        .application()
        .queue_expected_response(bearer(TOKEN_B), ScriptedResponse::success(Bytes::new()))
        .expect("protected response with B");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let a = tokio::spawn(store_as(&core, &selector).into_future());
    gate_a.wait_until_reached().await;
    let b = tokio::spawn(store_as(&core, &selector).into_future());
    gate_b.wait_until_reached().await;
    gate_a.open();
    assert_eq!(
        a.await.expect("A task").expect("A completes"),
        StoreOutcome::Superseded
    );
    assert!(
        !selector.is_set(),
        "A's material was dropped, not installed"
    );
    gate_b.open();
    let version = installed(b.await.expect("B task").expect("B completes"));
    assert_eq!(selector.installed_version(), Some(version));
    protected_call(&core, &selector)
        .await
        .expect("B is applied");
}

// V3-E12 row 4: Start A; start B; B fails; A succeeds => A Superseded;
// the pre-existing ready value remains; no revival of A.
#[tokio::test]
async fn start_a_start_b_then_b_fails_then_a_succeeds_keeps_existing_value() {
    let executor = DeterministicExecutor::new();
    let gate_a = Gate::closed();
    executor
        .application()
        .queue_gate(gate_a.clone())
        .expect("gate A");
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::BAD_REQUEST,
            Bytes::from_static(b"login rejected"),
        ))
        .expect("B fails");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login A");
    executor
        .application()
        .queue_expected_response(bearer(INITIAL), ScriptedResponse::success(Bytes::new()))
        .expect("protected response with the existing value");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let existing = selector
        .replace(access_token(INITIAL))
        .expect("pre-existing ready value");
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let a = tokio::spawn(store_as(&core, &selector).into_future());
    gate_a.wait_until_reached().await;
    let b_error = api_error(store_as(&core, &selector).await);
    assert_eq!(b_error.category(), ErrorCategory::HttpStatus);
    gate_a.open();
    assert_eq!(
        a.await.expect("A task").expect("A completes"),
        StoreOutcome::Superseded
    );
    assert_eq!(selector.installed_version(), Some(existing));
    protected_call(&core, &selector)
        .await
        .expect("the pre-existing value remains usable");
}

// V3-E13: an old challenge rejects only its applied version and cannot erase
// a newer installed version or a newer pending intent.
#[tokio::test]
async fn late_challenge_rejects_only_its_applied_version() {
    let executor = DeterministicExecutor::new();
    let old_gate = Gate::closed();
    // Old protected attempt (version 1) blocks at the transport; a newer
    // storage installs version 2; then the old attempt receives 401.
    executor
        .application()
        .queue_gate(old_gate.clone())
        .expect("old attempt gate");
    executor
        .application()
        .queue_response(login_response(TOKEN_B))
        .expect("newer login");
    executor
        .application()
        .queue_expected_response(
            bearer(INITIAL),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::new()),
        )
        .expect("late challenge for version 1");
    executor
        .application()
        .queue_expected_response(bearer(TOKEN_B), ScriptedResponse::success(Bytes::new()))
        .expect("newer version still applied");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    selector.replace(access_token(INITIAL)).expect("version 1");
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let old = {
        let core = core.clone();
        let selector = Arc::clone(&selector);
        tokio::spawn(async move { protected_call(&core, &selector).await })
    };
    old_gate.wait_until_reached().await;
    let newer = installed(store_as(&core, &selector).await.expect("newer storage"));
    old_gate.open();
    let challenge = api_error(old.await.expect("old attempt task"));
    assert_eq!(challenge.category(), ErrorCategory::AuthenticationRejected);
    assert_eq!(selector.installed_version(), Some(newer));
    protected_call(&core, &selector)
        .await
        .expect("the newer version survives the late challenge");
    assert_eq!(executor.application().request_count().expect("count"), 3);
}

#[tokio::test]
async fn challenge_on_current_version_keeps_a_newer_pending_intent_eligible() {
    let executor = DeterministicExecutor::new();
    let old_gate = Gate::closed();
    let login_gate = Gate::closed();
    executor
        .application()
        .queue_gate(old_gate.clone())
        .expect("old attempt gate");
    executor
        .application()
        .queue_gate(login_gate.clone())
        .expect("pending login gate");
    executor
        .application()
        .queue_expected_response(
            bearer(INITIAL),
            ScriptedResponse::status(StatusCode::UNAUTHORIZED, Bytes::new()),
        )
        .expect("challenge for version 1");
    executor
        .application()
        .queue_response(login_response(TOKEN_B))
        .expect("pending login");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    selector.replace(access_token(INITIAL)).expect("version 1");
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let old = {
        let core = core.clone();
        let selector = Arc::clone(&selector);
        tokio::spawn(async move { protected_call(&core, &selector).await })
    };
    old_gate.wait_until_reached().await;
    let pending = tokio::spawn(store_as(&core, &selector).into_future());
    login_gate.wait_until_reached().await;
    old_gate.open();
    assert_eq!(
        api_error(old.await.expect("old attempt task")).category(),
        ErrorCategory::AuthenticationRejected
    );
    assert!(!selector.is_set(), "exactly version 1 was rejected");
    login_gate.open();
    installed(
        pending
            .await
            .expect("pending task")
            .expect("pending storage"),
    );
    assert!(selector.is_set());
}

/// Asserts the single final observation of a storage operation that failed
/// locally after a successful exchange: it reports the failure (never
/// `Success`), for the same operation identity as the returned error, with the
/// retained response status and attempt count (EXEC-04, §11.4).
fn assert_install_failure_observed(hooks: &AuthHooks, error: &ApiError) {
    let completed = hooks.completion.lock().expect("completion records");
    assert_eq!(completed.len(), 1, "exactly one final observation");
    let (operation_id, _, final_status, parent, _, outcome, attempts, retries) = completed[0];
    assert_eq!(error.operation_id(), Some(operation_id));
    assert_eq!(parent, None);
    assert_eq!(
        outcome,
        CompletionOutcome::from_error_category(error.category())
    );
    assert_ne!(outcome, CompletionOutcome::Success);
    assert_eq!(final_status, Some(StatusCode::OK));
    assert_eq!(final_status, error.last_response_status());
    assert_eq!((attempts, retries), (1, 0));
}

// V3-E15 / AUTH-05: a successful remote response followed by a failed local
// installation retains contact, status and the validated endpoint ID, and its
// one final observation reports that failure (EXEC-04).
#[tokio::test]
async fn failed_local_install_after_success_retains_contact_status_and_endpoint() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .queue_gate(gate.clone())
        .expect("login gate");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core(&executor, Arc::clone(&hooks));

    let pending = tokio::spawn(store_as(&core, &selector).into_future());
    gate.wait_until_reached().await;
    selector.poison_for_test();
    gate.open();
    let error = api_error(pending.await.expect("storage task"));
    assert_eq!(error.stage(), FailureStage::CredentialInstall);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert_eq!(error.http_status(), None);
    assert_eq!(error.endpoint_id().as_str(), LOGIN);
    assert_eq!(error.method(), &Method::POST);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Internal));
    assert!(error.operation_id().is_some());
    assert_eq!(error.attempt(), Some(INITIAL_ATTEMPT));
    let rendered = format!("{error} {error:?}");
    assert!(!rendered.contains(TOKEN_A));
    assert_eq!(executor.application().request_count().expect("count"), 1);
    assert_install_failure_observed(&hooks, &error);
}

#[tokio::test]
async fn exhausted_version_counter_fails_install_with_correlated_error() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    selector.set_counters_for_test(0, u64::MAX);
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core(&executor, Arc::clone(&hooks));
    let error = api_error(store_as(&core, &selector).await);
    assert_eq!(error.stage(), FailureStage::CredentialInstall);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Internal));
    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (1, u64::MAX));
    assert_install_failure_observed(&hooks, &error);
}

struct AdvanceClockOnCompletion;

impl OperationObserver for AdvanceClockOnCompletion {
    fn observe(&self, event: OperationEvent<'_>) {
        if matches!(event, OperationEvent::OperationFinished(_)) {
            crate::operation::TestClockGuard::advance(Duration::from_secs(31));
        }
    }
}

// ---------------------------------------------------------------------------
// F01 (independent review) / §11.4 / EXEC-04: common caps apply through local
// installation, and the one final observation follows the fixed storage
// result. A genuine delay or cancellation before installation prevents it; a
// slow final observer cannot change Installed, Superseded or a fixed error.
// ---------------------------------------------------------------------------

/// One final observation of a storage operation, with the installed version
/// the credential store held at that moment.
#[derive(Clone, Copy, Debug)]
struct FinalRecord {
    operation_id: OperationId,
    outcome: CompletionOutcome,
    final_status: Option<StatusCode>,
    attempts_started: u32,
    installed_at_observation: Option<crate::auth::CredentialVersion>,
}

/// Final-observation probe: records every `OperationFinished` together with
/// the store state visible to the observer, then optionally simulates a slow
/// synchronous final callback by advancing the operation clock.
struct FinalProbe<M: CredentialMaterial> {
    selector: Arc<SlotSelector<M>>,
    slow_by: Option<Duration>,
    head_cancels: Option<CancellationToken>,
    finished: Mutex<Vec<FinalRecord>>,
}

impl<M: CredentialMaterial> FinalProbe<M> {
    fn new(selector: &Arc<SlotSelector<M>>) -> Self {
        Self {
            selector: Arc::clone(selector),
            slow_by: None,
            head_cancels: None,
            finished: Mutex::new(Vec::new()),
        }
    }

    fn slow(mut self, by: Duration) -> Self {
        self.slow_by = Some(by);
        self
    }

    fn cancel_on_head(mut self, token: CancellationToken) -> Self {
        self.head_cancels = Some(token);
        self
    }

    fn records(&self) -> Vec<FinalRecord> {
        self.finished.lock().expect("final records").clone()
    }

    fn only(&self) -> FinalRecord {
        let records = self.records();
        assert_eq!(records.len(), 1, "exactly one final observation");
        records[0]
    }
}

impl<M: CredentialMaterial> OperationObserver for FinalProbe<M> {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::ResponseHeadObserved(_) => {
                if let Some(token) = &self.head_cancels {
                    token.cancel();
                }
            }
            OperationEvent::OperationFinished(finished) => {
                self.finished
                    .lock()
                    .expect("final records")
                    .push(FinalRecord {
                        operation_id: finished.operation_id,
                        outcome: finished.outcome,
                        final_status: finished.final_status,
                        attempts_started: finished.attempts_started,
                        installed_at_observation: self.selector.installed_version(),
                    });
                if let Some(by) = self.slow_by {
                    crate::operation::TestClockGuard::advance(by);
                }
            }
            _ => {}
        }
    }
}

fn probed_core(executor: &DeterministicExecutor, probe: Arc<dyn OperationObserver>) -> ClientCore {
    ClientCore::build_with_development(
        "https://protected.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().operation_observer(probe),
        executor,
    )
    .expect("probed development core")
}

/// Test action run by the installation's own synchronous extension call (the
/// material's application-defined expiry read), i.e. strictly before local
/// installation and before the storage result is fixed.
enum InstallAction {
    /// A genuine delay: the operation clock passes the 30-second deadline.
    Delay(Duration),
    /// A caller cancellation signal fires.
    Cancel(CancellationToken),
}

std::thread_local! {
    static NEXT_INSTALL_ACTION: std::cell::RefCell<Option<InstallAction>> =
        const { std::cell::RefCell::new(None) };
}

/// Endpoint-backed credential material whose `expires_at` runs one
/// [`InstallAction`]. Decoding takes the action queued on the test thread.
struct ProbeToken {
    action: Mutex<Option<InstallAction>>,
    expiry_reads: Arc<AtomicUsize>,
}

impl CredentialMaterial for ProbeToken {
    fn expires_at(&self) -> Option<Instant> {
        self.expiry_reads.fetch_add(1, Ordering::SeqCst);
        match self.action.lock().expect("install action").take() {
            Some(InstallAction::Delay(by)) => crate::operation::TestClockGuard::advance(by),
            Some(InstallAction::Cancel(token)) => token.cancel(),
            None => {}
        }
        None
    }
}

std::thread_local! {
    static PROBE_EXPIRY_READS: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
}

struct ProbeCodec;

impl DecodeBody for ProbeCodec {
    type Output = ProbeToken;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        Ok(ProbeToken {
            action: Mutex::new(NEXT_INSTALL_ACTION.with(|next| next.borrow_mut().take())),
            expiry_reads: PROBE_EXPIRY_READS.with(Arc::clone),
        })
    }
}

/// The same credential source endpoint as [`LoginEndpoint`], producing
/// [`ProbeToken`] material.
struct ProbeLoginEndpoint;

impl EndpointSpec for ProbeLoginEndpoint {
    type Input = PreparedBody;
    type Body = PreparedBody;
    type Output = ProbeToken;
    type Response = ProbeCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(LOGIN, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("login")?;
        builder.finish()
    }

    fn into_body(self) -> Self::Body {
        PreparedBody::bytes(Bytes::new())
    }
}

fn store_probe(core: &ClientCore, selector: &Arc<SlotSelector<ProbeToken>>) -> StoreRequest {
    crate::execute::store_request(
        Request::new(core.clone(), ProbeLoginEndpoint),
        Arc::clone(selector),
    )
}

fn queue_install_action(action: InstallAction) {
    NEXT_INSTALL_ACTION.with(|next| *next.borrow_mut() = Some(action));
}

// Common caps apply through local installation: a genuine delay after the
// exchange and before installation (here inside the installation's own
// synchronous expiry read) exceeds the deadline, so nothing is installed, the
// reserved intent is not rolled back, and the failure keeps the exchange's
// contact facts. The one final observation reports that Timeout for the same
// operation. (The former oracle manufactured this failure with a slow *final*
// observer, which EXEC-04 forbids from changing the result; see
// `review_f01_final_observer_cannot_turn_store_success_into_timeout`.)
#[tokio::test]
async fn deadline_applies_through_local_installation() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::<ProbeToken>::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector));
    let core = probed_core(&executor, probe.clone());
    let expiry_reads = PROBE_EXPIRY_READS.with(Arc::clone);
    let reads_before = expiry_reads.load(Ordering::SeqCst);
    queue_install_action(InstallAction::Delay(Duration::from_secs(31)));

    let error = api_error(store_probe(&core, &selector).await);
    assert_eq!(expiry_reads.load(Ordering::SeqCst), reads_before + 1);
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.stage(), FailureStage::CredentialInstall);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert_eq!(error.endpoint_id().as_str(), LOGIN);
    assert_eq!(error.attempt(), Some(INITIAL_ATTEMPT));
    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (1, 0));

    let observed = probe.only();
    assert_eq!(error.operation_id(), Some(observed.operation_id));
    assert_eq!(observed.outcome, CompletionOutcome::Timeout);
    assert_eq!(observed.final_status, Some(StatusCode::OK));
    assert_eq!(observed.attempts_started, 1);
    assert_eq!(observed.installed_at_observation, None);
}

// A delay during the exchange (the clock passes the deadline inside the
// response-head observer, before result fixation) also prevents installation.
#[tokio::test]
async fn delay_during_the_exchange_prevents_installation() {
    struct AdvanceClockOnHead;

    impl OperationObserver for AdvanceClockOnHead {
        fn observe(&self, event: OperationEvent<'_>) {
            if matches!(event, OperationEvent::ResponseHeadObserved(_)) {
                crate::operation::TestClockGuard::advance(Duration::from_secs(31));
            }
        }
    }

    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let core = probed_core(&executor, Arc::new(AdvanceClockOnHead));
    let error = api_error(store_as(&core, &selector).await);
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (1, 0));
}

// The final observation follows installation: the observer already sees the
// installed version, which is exactly the returned one (EXEC-04 ordering).
#[tokio::test]
async fn final_observation_follows_installation_of_the_returned_version() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector));
    let core = probed_core(&executor, probe.clone());
    let version = installed(store_as(&core, &selector).await.expect("storage"));
    let observed = probe.only();
    assert_eq!(observed.outcome, CompletionOutcome::Success);
    assert_eq!(observed.installed_at_observation, Some(version));
    assert_eq!(observed.final_status, Some(StatusCode::OK));
    assert_eq!(observed.attempts_started, 1);
}

// A slow final observer cannot turn Installed into a failure: the storage
// result was fixed (and the material installed) before it ran.
#[tokio::test]
async fn slow_final_observer_cannot_change_installed() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector).slow(Duration::from_secs(31)));
    let core = probed_core(&executor, probe.clone());
    let version = installed(
        store_as(&core, &selector)
            .await
            .expect("a slow final observer cannot change the fixed result"),
    );
    assert_eq!(selector.installed_version(), Some(version));
    assert_eq!(selector.counters_for_test(), (1, 1));
    let observed = probe.only();
    assert_eq!(observed.outcome, CompletionOutcome::Success);
    assert_eq!(observed.installed_at_observation, Some(version));
}

// A slow final observer cannot turn Superseded into a failure. A concurrent
// manual clear during the exchange supersedes the intent; the storage result
// `Ok(Superseded)` is a successful storage operation whose final observation
// is `Success` (nothing is installed; the outcome says so).
#[tokio::test]
async fn slow_final_observer_cannot_change_superseded() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .queue_gate(gate.clone())
        .expect("login gate");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector).slow(Duration::from_secs(31)));
    let core = probed_core(&executor, probe.clone());

    let pending = tokio::spawn(store_as(&core, &selector).into_future());
    gate.wait_until_reached().await;
    selector.clear().expect("clear during the exchange");
    gate.open();
    assert_eq!(
        pending
            .await
            .expect("storage task")
            .expect("a slow final observer cannot change the fixed result"),
        StoreOutcome::Superseded
    );
    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (2, 0));
    let observed = probe.only();
    assert_eq!(observed.outcome, CompletionOutcome::Success);
    assert_eq!(observed.installed_at_observation, None);
    assert_eq!(observed.final_status, Some(StatusCode::OK));
}

// A concurrent manual replacement during the exchange supersedes the intent;
// the replacement stays installed and the final observation matches the
// fixed `Ok(Superseded)` result.
#[tokio::test]
async fn concurrent_replacement_during_the_exchange_is_superseded_and_observed_once() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .queue_gate(gate.clone())
        .expect("login gate");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector));
    let core = probed_core(&executor, probe.clone());

    let pending = tokio::spawn(store_as(&core, &selector).into_future());
    gate.wait_until_reached().await;
    let replacement = selector
        .replace(access_token(TOKEN_B))
        .expect("replacement during the exchange");
    gate.open();
    assert_eq!(
        pending.await.expect("storage task").expect("storage"),
        StoreOutcome::Superseded
    );
    assert_eq!(selector.installed_version(), Some(replacement));
    let observed = probe.only();
    assert_eq!(observed.outcome, CompletionOutcome::Success);
    assert_eq!(observed.installed_at_observation, Some(replacement));
}

// A slow final observer cannot change a fixed local installation error into a
// Timeout: the exhausted-version failure stays an authentication failure at
// CredentialInstall, and its one final observation reports that failure.
#[tokio::test]
async fn slow_final_observer_cannot_change_a_fixed_install_error() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    selector.set_counters_for_test(0, u64::MAX);
    let probe = Arc::new(FinalProbe::new(&selector).slow(Duration::from_secs(31)));
    let core = probed_core(&executor, probe.clone());
    let error = api_error(store_as(&core, &selector).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Internal));
    assert_eq!(error.stage(), FailureStage::CredentialInstall);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    let observed = probe.only();
    assert_eq!(error.operation_id(), Some(observed.operation_id));
    assert_eq!(observed.outcome, CompletionOutcome::AuthenticationProvider);
    assert_eq!(observed.final_status, Some(StatusCode::OK));
}

// A slow final observer cannot change a fixed exchange error either.
#[tokio::test]
async fn slow_final_observer_cannot_change_a_fixed_exchange_error() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::status(
            StatusCode::INTERNAL_SERVER_ERROR,
            Bytes::from_static(b"login failed"),
        ))
        .expect("failing login");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector).slow(Duration::from_secs(31)));
    let core = probed_core(&executor, probe.clone());
    let error = api_error(store_as(&core, &selector).await);
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.stage(), FailureStage::RejectedResponse);
    assert_eq!(error.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    let observed = probe.only();
    assert_eq!(error.operation_id(), Some(observed.operation_id));
    assert_eq!(observed.outcome, CompletionOutcome::HttpStatus);
    assert!(!selector.is_set());
}

// A cancellation signal that fires after the exchange and before installation
// (inside the installation's own synchronous expiry read) is observed before
// installing: nothing is installed and the operation reports Cancelled at
// CredentialInstall, with the exchange's contact facts retained.
#[tokio::test]
async fn cancellation_before_installation_prevents_it() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::<ProbeToken>::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector));
    let core = probed_core(&executor, probe.clone());
    let token = CancellationToken::new();
    queue_install_action(InstallAction::Cancel(token.clone()));
    let error = api_error(
        store_probe(&core, &selector)
            .cancel_with(token.clone())
            .expect("cancellation source")
            .await,
    );
    assert!(token.is_cancelled());
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(error.stage(), FailureStage::CredentialInstall);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (1, 0));
    let observed = probe.only();
    assert_eq!(error.operation_id(), Some(observed.operation_id));
    assert_eq!(observed.outcome, CompletionOutcome::Cancelled);
    assert_eq!(observed.final_status, Some(StatusCode::OK));
    assert_eq!(observed.installed_at_observation, None);
}

// A cancellation signal fired during the exchange (by the response-head
// observer, before result fixation) prevents installation even when the rest
// of the exchange completes without yielding (with the deterministic
// transport the buffered body is ready, so the pre-installation decision is
// what observes the signal).
#[tokio::test]
async fn cancellation_during_the_exchange_prevents_installation() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let token = CancellationToken::new();
    let probe = Arc::new(FinalProbe::new(&selector).cancel_on_head(token.clone()));
    let core = probed_core(&executor, probe.clone());
    let error = api_error(
        store_as(&core, &selector)
            .cancel_with(token)
            .expect("cancellation source")
            .await,
    );
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(error.stage(), FailureStage::CredentialInstall);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (1, 0));
    let observed = probe.only();
    assert_eq!(error.operation_id(), Some(observed.operation_id));
    assert_eq!(observed.outcome, CompletionOutcome::Cancelled);
    assert_eq!(observed.installed_at_observation, None);
}

// A cancellation while the exchange is still in flight stops the operation
// before any result: nothing is installed and the intent is not rolled back.
#[tokio::test]
async fn cancellation_while_the_exchange_is_in_flight_installs_nothing() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .queue_gate(gate.clone())
        .expect("login gate");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector));
    let core = probed_core(&executor, probe.clone());
    let token = CancellationToken::new();
    let pending = tokio::spawn(
        store_as(&core, &selector)
            .cancel_with(token.clone())
            .expect("cancellation source")
            .into_future(),
    );
    gate.wait_until_reached().await;
    token.cancel();
    let error = api_error(pending.await.expect("storage task"));
    gate.open();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched
    );
    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (1, 0));
    let observed = probe.only();
    assert_eq!(error.operation_id(), Some(observed.operation_id));
    assert_eq!(observed.outcome, CompletionOutcome::Cancelled);
}

#[tokio::test]
async fn store_controls_are_carried_validated_and_value_free() {
    let executor = DeterministicExecutor::new();
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let store = store_as(&core, &selector);
    assert_eq!(
        store_as(&core, &selector)
            .timeout(Duration::ZERO)
            .expect_err("zero timeout"),
        ControlError::ZeroDuration
    );
    let store = store
        .timeout(Duration::from_secs(60))
        .expect("selected timeout")
        .with_options(
            &CallOptions::new()
                .cap_timeout(Duration::from_secs(5))
                .expect("cap")
                .cap_replays(0),
        );
    let summary = store.control_summary();
    assert_eq!(summary.declared_timeout(), Some(Duration::from_secs(30)));
    assert_eq!(summary.selected_timeout(), Some(Duration::from_secs(60)));
    assert_eq!(summary.effective_timeout(), Some(Duration::from_secs(5)));
    assert_eq!(summary.replay_cap(), Some(0));
    let store = store.reset_timeout();
    let summary = RequestControls::control_summary(&store);
    assert_eq!(summary.selected_timeout(), Some(Duration::from_secs(30)));
    assert_eq!(summary.effective_timeout(), Some(Duration::from_secs(5)));

    let mut store = store;
    for _ in 0..MAX_CANCELLATION_SOURCES {
        store = store
            .cancel_with(CancellationToken::new())
            .expect("bounded cancellation source");
    }
    assert_eq!(
        RequestControls::cancel_with(store, CancellationToken::new())
            .expect_err("ninth cancellation source"),
        ControlError::TooManyCancellationSources
    );
    let rendered = format!("{:?}", store_as(&core, &selector));
    assert!(rendered.starts_with("StoreRequest"));
    assert!(!rendered.contains(LOGIN));
    assert_eq!(
        selector.counters_for_test(),
        (0, 0),
        "controls reserve nothing"
    );
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

/// The generic application helper from §23.2 accepts the storage terminal.
async fn execute_with<R, T>(request: R, options: &CallOptions) -> Result<T, ApiError>
where
    R: RequestControls + IntoFuture<Output = Result<T, ApiError>>,
{
    request.with_options(options).await
}

#[tokio::test]
async fn generic_request_controls_helper_executes_a_store_request() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let outcome = execute_with(
        store_as(&core, &selector),
        &CallOptions::new()
            .cap_timeout(Duration::from_secs(10))
            .expect("cap"),
    )
    .await
    .expect("generic storage");
    installed(outcome);
}

#[tokio::test]
async fn storage_is_refused_before_network_for_non_endpoint_or_foreign_stores() {
    let executor = DeterministicExecutor::new();
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));

    let static_store = SlotSelector::static_value(access_token(INITIAL), Duration::ZERO);
    let error = api_error(store_as(&core, &static_store).await);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::InvalidConfiguration));
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);

    let foreign = SlotSelector::endpoint_in_context(Duration::ZERO, CredentialContext::new());
    let error = api_error(
        crate::execute::store_request(
            Request::new(core.clone(), LoginEndpoint),
            Arc::clone(&foreign),
        )
        .await,
    );
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(foreign.counters_for_test(), (0, 0));
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test]
async fn decode_failure_installs_nothing_and_does_not_revive_older_intents() {
    let executor = DeterministicExecutor::new();
    let gate_a = Gate::closed();
    executor
        .application()
        .queue_gate(gate_a.clone())
        .expect("gate A");
    executor
        .application()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"\xff\xfe")))
        .expect("undecodable B");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login A");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let (core, _) = core(&executor, Arc::new(AuthHooks::default()));
    let a = tokio::spawn(store_as(&core, &selector).into_future());
    gate_a.wait_until_reached().await;
    let error = api_error(store_as(&core, &selector).await);
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    gate_a.open();
    assert_eq!(
        a.await.expect("A task").expect("A completes"),
        StoreOutcome::Superseded
    );
    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (2, 0));
}

// Independent review F01. Regression addition: NOT compiled by the reviewer.
// A final observer is post-fixation even for the credential-storage terminal.
#[tokio::test]
async fn review_f01_final_observer_cannot_turn_store_success_into_timeout() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    let core = ClientCore::build_with_development(
        "https://protected.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().operation_observer(Arc::new(AdvanceClockOnCompletion)),
        &executor,
    )
    .expect("development core");
    installed(
        store_as(&core, &selector)
            .await
            .expect("a slow final observer cannot change the fixed store result"),
    );
    assert!(selector.is_set());
}

// Failed installation must not leave a successful final operation observation.
#[tokio::test]
async fn review_f01_failed_install_is_not_observed_as_final_success() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::endpoint(Duration::ZERO);
    selector.set_counters_for_test(0, u64::MAX);
    let hooks = Arc::new(AuthHooks::default());
    let (core, _) = core(&executor, Arc::clone(&hooks));
    let error = api_error(store_as(&core, &selector).await);
    assert_eq!(error.stage(), FailureStage::CredentialInstall);
    let completed = hooks.completion.lock().expect("completion records");
    assert_eq!(
        completed.len(),
        1,
        "one final observation for this storage operation"
    );
    assert!(!matches!(completed[0].5, CompletionOutcome::Success));
    assert_eq!(error.operation_id(), Some(completed[0].0));
}

// ---------------------------------------------------------------------------
// F01 post-fix review follow-up (EXEC-04, §11.3, §11.4): credential material
// the storage operation no longer holds (the value an installation replaced,
// or decoded material that is not installed) is dropped only after the
// storage result is fixed and finally observed. An application `Drop` that
// panics therefore cannot turn a fixed result into a `Cancelled` observation;
// its panic still propagates to the caller, after the truthful observation.
// ---------------------------------------------------------------------------

const DROP_PANIC: &str = "deliberate credential material Drop panic";

/// Credential material whose application-defined `Drop` panics when armed.
/// With `expiry_delay`, its expiry read (the installation's own synchronous
/// extension call) advances the operation clock: a genuine delay immediately
/// before installation.
struct DropPanicToken {
    armed: bool,
    expiry_delay: Option<Duration>,
}

impl DropPanicToken {
    const fn plain() -> Self {
        Self {
            armed: false,
            expiry_delay: None,
        }
    }

    const fn armed() -> Self {
        Self {
            armed: true,
            expiry_delay: None,
        }
    }
}

impl CredentialMaterial for DropPanicToken {
    fn expires_at(&self) -> Option<Instant> {
        if let Some(by) = self.expiry_delay {
            crate::operation::TestClockGuard::advance(by);
        }
        None
    }
}

impl Drop for DropPanicToken {
    fn drop(&mut self) {
        if self.armed && !std::thread::panicking() {
            std::panic::panic_any(DROP_PANIC);
        }
    }
}

/// Decodes `plain`, `armed` or `armed-slow` (armed, with a 31-second delay in
/// its expiry read) into [`DropPanicToken`] material.
struct DropPanicCodec;

impl DecodeBody for DropPanicCodec {
    type Output = DropPanicToken;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        match &body[..] {
            b"plain" => Ok(DropPanicToken::plain()),
            b"armed" => Ok(DropPanicToken::armed()),
            b"armed-slow" => Ok(DropPanicToken {
                armed: true,
                expiry_delay: Some(Duration::from_secs(31)),
            }),
            _ => Err(CodecError::new(CodecErrorKind::Decode)),
        }
    }
}

/// The same credential source endpoint as [`LoginEndpoint`], producing
/// [`DropPanicToken`] material.
struct DropPanicLoginEndpoint;

impl EndpointSpec for DropPanicLoginEndpoint {
    type Input = PreparedBody;
    type Body = PreparedBody;
    type Output = DropPanicToken;
    type Response = DropPanicCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(LOGIN, Method::POST, Some(Duration::from_secs(30)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("login")?;
        builder.finish()
    }

    fn into_body(self) -> Self::Body {
        PreparedBody::bytes(Bytes::new())
    }
}

fn store_drop_panic(
    core: &ClientCore,
    selector: &Arc<SlotSelector<DropPanicToken>>,
) -> StoreRequest {
    crate::execute::store_request(
        Request::new(core.clone(), DropPanicLoginEndpoint),
        Arc::clone(selector),
    )
}

/// Asserts that a spawned storage task ended with the material's `Drop`
/// panic: the panic propagates to the caller instead of being swallowed.
fn assert_drop_panic(joined: Result<Result<StoreOutcome, ApiError>, tokio::task::JoinError>) {
    let error = match joined {
        Ok(result) => panic!(
            "expected the material's Drop panic to propagate, got {:?}",
            result.map_err(|error| error.category())
        ),
        Err(error) => error,
    };
    assert!(error.is_panic(), "the storage task panicked");
    let payload = error.into_panic();
    assert_eq!(payload.downcast_ref::<&str>(), Some(&DROP_PANIC));
}

// The post-fix review note: the `Drop` of the credential material replaced by
// an installation panics. The new material is installed, so the one final
// observation is `Success` and sees the new version; the panic then
// propagates to the caller.
#[tokio::test]
async fn retired_material_drop_panic_is_observed_after_the_installed_result() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response("plain"))
        .expect("login response");
    let selector = SlotSelector::<DropPanicToken>::endpoint(Duration::ZERO);
    let existing = selector
        .replace(DropPanicToken::armed())
        .expect("existing credential whose Drop panics");
    let probe = Arc::new(FinalProbe::new(&selector));
    let core = probed_core(&executor, probe.clone());

    assert_drop_panic(tokio::spawn(store_drop_panic(&core, &selector).into_future()).await);

    let installed_now = selector
        .installed_version()
        .expect("the new material is installed");
    assert_ne!(installed_now, existing);
    assert_eq!(selector.counters_for_test(), (2, 2));
    let observed = probe.only();
    assert_eq!(observed.outcome, CompletionOutcome::Success);
    assert_eq!(observed.installed_at_observation, Some(installed_now));
    assert_eq!(observed.final_status, Some(StatusCode::OK));
    assert_eq!(observed.attempts_started, 1);
}

// Superseded decoded material is dropped, never installed; a panic in its
// `Drop` follows the `Success` observation of the fixed `Ok(Superseded)`.
#[tokio::test]
async fn superseded_material_drop_panic_is_observed_after_the_superseded_result() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .queue_gate(gate.clone())
        .expect("login gate");
    executor
        .application()
        .queue_response(login_response("armed"))
        .expect("login response");
    let selector = SlotSelector::<DropPanicToken>::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector));
    let core = probed_core(&executor, probe.clone());

    let pending = tokio::spawn(store_drop_panic(&core, &selector).into_future());
    gate.wait_until_reached().await;
    let replacement = selector
        .replace(DropPanicToken::plain())
        .expect("replacement during the exchange");
    gate.open();
    assert_drop_panic(pending.await);

    assert_eq!(selector.installed_version(), Some(replacement));
    assert_eq!(selector.counters_for_test(), (2, 1));
    let observed = probe.only();
    assert_eq!(observed.outcome, CompletionOutcome::Success);
    assert_eq!(observed.installed_at_observation, Some(replacement));
    assert_eq!(observed.final_status, Some(StatusCode::OK));
}

// A genuine delay before installation fixes a Timeout; the decoded material
// is then dropped, and its panicking `Drop` cannot turn that Timeout
// observation into `Cancelled`.
#[tokio::test]
async fn uninstalled_material_drop_panic_is_observed_after_a_timeout_before_installation() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response("armed-slow"))
        .expect("login response");
    let selector = SlotSelector::<DropPanicToken>::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector));
    let core = probed_core(&executor, probe.clone());

    assert_drop_panic(tokio::spawn(store_drop_panic(&core, &selector).into_future()).await);

    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (1, 0));
    let observed = probe.only();
    assert_eq!(observed.outcome, CompletionOutcome::Timeout);
    assert_eq!(observed.installed_at_observation, None);
    assert_eq!(observed.final_status, Some(StatusCode::OK));
}

// A fail-closed installation (exhausted installed-version counter) fixes an
// authentication failure; the decoded material is dropped only after that
// failure is observed.
#[tokio::test]
async fn uninstalled_material_drop_panic_is_observed_after_a_failed_installation() {
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response("armed"))
        .expect("login response");
    let selector = SlotSelector::<DropPanicToken>::endpoint(Duration::ZERO);
    selector.set_counters_for_test(0, u64::MAX);
    let probe = Arc::new(FinalProbe::new(&selector));
    let core = probed_core(&executor, probe.clone());

    assert_drop_panic(tokio::spawn(store_drop_panic(&core, &selector).into_future()).await);

    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (1, u64::MAX));
    let observed = probe.only();
    assert_eq!(observed.outcome, CompletionOutcome::AuthenticationProvider);
    assert_eq!(observed.installed_at_observation, None);
    assert_eq!(observed.final_status, Some(StatusCode::OK));
}

// The storage operation can hold the last reference to its credential store
// (the application dropped its client while the storage was running). The
// store, with the material just installed in it, is then released only after
// the installed result is fixed and observed; a panicking `Drop` of that
// material follows the `Success` observation.
#[tokio::test]
async fn last_store_reference_is_released_after_the_installed_result_is_observed() {
    #[derive(Default)]
    struct FinalOutcomes(Mutex<Vec<(CompletionOutcome, Option<StatusCode>)>>);

    impl OperationObserver for FinalOutcomes {
        fn observe(&self, event: OperationEvent<'_>) {
            if let OperationEvent::OperationFinished(finished) = event {
                self.0
                    .lock()
                    .expect("final outcomes")
                    .push((finished.outcome, finished.final_status));
            }
        }
    }

    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(login_response("armed"))
        .expect("login response");
    let selector = SlotSelector::<DropPanicToken>::endpoint(Duration::ZERO);
    let outcomes = Arc::new(FinalOutcomes::default());
    let core = probed_core(&executor, outcomes.clone());
    let storage = store_drop_panic(&core, &selector);
    let store = Arc::downgrade(&selector);
    drop(selector);

    assert_drop_panic(tokio::spawn(storage.into_future()).await);

    assert!(
        store.upgrade().is_none(),
        "the storage operation held the last reference to its store"
    );
    assert_eq!(
        *outcomes.0.lock().expect("final outcomes"),
        [(CompletionOutcome::Success, Some(StatusCode::OK))]
    );
}

// Kept ordering (§11.3–§11.4): the common caps are decided before the
// lock-guarded intent check, so an operation whose deadline passed before
// installation fails with Timeout even when its intent was also superseded.
// Neither outcome installs anything or rolls the intent back.
#[tokio::test]
async fn deadline_before_installation_is_reported_over_a_superseded_intent() {
    let _clock = crate::operation::TestClockGuard::reset();
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    executor
        .application()
        .queue_gate(gate.clone())
        .expect("login gate");
    executor
        .application()
        .queue_response(login_response(TOKEN_A))
        .expect("login response");
    let selector = SlotSelector::<ProbeToken>::endpoint(Duration::ZERO);
    let probe = Arc::new(FinalProbe::new(&selector));
    let core = probed_core(&executor, probe.clone());
    queue_install_action(InstallAction::Delay(Duration::from_secs(31)));

    let pending = tokio::spawn(store_probe(&core, &selector).into_future());
    gate.wait_until_reached().await;
    selector
        .clear()
        .expect("clear supersedes the pending intent");
    gate.open();
    let error = api_error(pending.await.expect("storage task"));
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.stage(), FailureStage::CredentialInstall);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert!(!selector.is_set());
    assert_eq!(selector.counters_for_test(), (2, 0));
    let observed = probe.only();
    assert_eq!(error.operation_id(), Some(observed.operation_id));
    assert_eq!(observed.outcome, CompletionOutcome::Timeout);
    assert_eq!(observed.installed_at_observation, None);
}
