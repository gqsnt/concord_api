//! Truthful execution journal, body/admission ordering, replay authorities,
//! caps, retry load budget, server delay and observer containment, evidenced
//! through downstream-generated requests against loopback servers
//! (§8, §13; V3-E01..E05, V3-E17..E21).
//!
//! Core-internal injection points (a deterministic transport with positive
//! no-submission proof, injected clocks and entropy, poisoned stores) are
//! exercised by the core executor tests; every assertion here uses only the
//! public facade, public extension roles and real sockets.

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

#[cfg(feature = "default-tls")]
use concord::auth::{AccessToken, AuthError, CredentialProvider, CredentialRequest};
use concord::body::{
    BodyError, BodyErrorKind, BodyFactory, PreparedBody, RequestBodyContext, RequestRepresentation,
    RequestStream,
};
use concord::error::{DispatchKnowledge, ErrorCategory, FailureStage};
use concord::http::{SafeProxy, StatusCode};
use concord::observation::{CompletionOutcome, OperationEvent, OperationObserver};
use concord::rate_limit::{
    BoxFuture, Enforcement, LimiterCapability, RateLimitError, RateLimitErrorKind, RateLimitPermit,
    RateLimitRequest, RateLimiter,
};
use concord::retry::{
    RetryBackoff, RetryDecision, RetryFailure, RetryPolicy, RetryPolicyBuildErrorKind,
};
use concord::{Binary, Bytes, CallOptions, CancellationToken, RequestControls, Text, api};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};

// ---------------------------------------------------------------------------
// Public extension probes.
// ---------------------------------------------------------------------------

/// Barrier used by the limiter probe.
struct Gate {
    reached: AtomicBool,
    reached_signal: tokio::sync::Notify,
    open: tokio::sync::Semaphore,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            reached: AtomicBool::new(false),
            reached_signal: tokio::sync::Notify::new(),
            open: tokio::sync::Semaphore::new(0),
        })
    }

    async fn wait_reached(&self) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let signal = self.reached_signal.notified();
                if self.reached.load(Ordering::SeqCst) {
                    return;
                }
                signal.await;
            }
        })
        .await
        .expect("the gated operation reached its barrier");
    }
}

/// Application limiter counting admissions; optionally gates or fails one
/// zero-based admission.
#[derive(Default)]
struct ProbeLimiter {
    acquisitions: AtomicUsize,
    fail_at: Option<usize>,
    gate_at: Mutex<VecDeque<(usize, Arc<Gate>)>>,
}

impl ProbeLimiter {
    fn failing_at(index: usize) -> Self {
        Self {
            fail_at: Some(index),
            ..Self::default()
        }
    }

    fn gate(&self, index: usize) -> Arc<Gate> {
        let gate = Gate::new();
        self.gate_at
            .lock()
            .expect("gates")
            .push_back((index, Arc::clone(&gate)));
        gate
    }

    fn acquisitions(&self) -> usize {
        self.acquisitions.load(Ordering::SeqCst)
    }
}

impl RateLimiter for ProbeLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.tests.journal", 1)
    }

    fn acquire<'a>(
        &'a self,
        _request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let index = self.acquisitions.fetch_add(1, Ordering::SeqCst);
        let gate = {
            let mut gates = self.gate_at.lock().expect("gates");
            gates
                .iter()
                .position(|(at, _)| *at == index)
                .and_then(|position| gates.remove(position))
                .map(|(_, gate)| gate)
        };
        let fail = self.fail_at == Some(index);
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.reached.store(true, Ordering::SeqCst);
                gate.reached_signal.notify_waiters();
                gate.open.acquire().await.expect("gate").forget();
            }
            if fail {
                Err(RateLimitError::new(RateLimitErrorKind::Acquire))
            } else {
                Ok(RateLimitPermit::granted())
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Completion {
    outcome: CompletionOutcome,
    attempts_started: u32,
    policy_retries: u32,
    final_status: Option<StatusCode>,
}

/// Observer recording completions, callback threads and formatted events;
/// optionally panics everywhere or sleeps in the final callback.
#[derive(Default)]
struct ProbeObserver {
    completions: Mutex<Vec<Completion>>,
    threads: Mutex<Vec<ThreadId>>,
    rendered: Mutex<Vec<String>>,
    head_seen: tokio::sync::Notify,
    panic_everywhere: bool,
    slow_final: Option<Duration>,
}

impl ProbeObserver {
    fn completions(&self) -> Vec<Completion> {
        self.completions.lock().expect("completions").clone()
    }
}

impl OperationObserver for ProbeObserver {
    fn observe(&self, event: OperationEvent<'_>) {
        self.threads
            .lock()
            .expect("threads")
            .push(std::thread::current().id());
        self.rendered
            .lock()
            .expect("rendered")
            .push(format!("{event:?}"));
        match &event {
            OperationEvent::ResponseHeadObserved(_) => self.head_seen.notify_one(),
            OperationEvent::OperationFinished(finished) => {
                self.completions
                    .lock()
                    .expect("completions")
                    .push(Completion {
                        outcome: finished.outcome,
                        attempts_started: finished.attempts_started,
                        policy_retries: finished.policy_retries,
                        final_status: finished.final_status,
                    });
                if let Some(delay) = self.slow_final {
                    std::thread::sleep(delay);
                }
            }
            _ => {}
        }
        assert!(
            !self.panic_everywhere,
            "observer panic sentinel must be contained"
        );
    }
}

/// Counting request representation whose preparation mode is selected by
/// the input.
pub struct Upload {
    mode: UploadMode,
    counts: Arc<BodyCounts>,
}

#[derive(Clone, Copy)]
enum UploadMode {
    /// Complete bytes (mechanically replayable).
    Bytes,
    /// A reconstructible factory stream.
    Factory,
    /// A one-shot lazy stream.
    OneShot,
    /// An invalid exact media type.
    InvalidMedia,
    /// A known length above the configured request-body limit.
    Oversized,
    /// A representation-level encoding failure.
    Producer,
}

#[derive(Default)]
struct BodyCounts {
    prepares: AtomicUsize,
    factory_creates: AtomicUsize,
}

const BODY: &[u8] = b"journal-body-bytes";

struct CountingFactory {
    counts: Arc<BodyCounts>,
}

impl BodyFactory for CountingFactory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        self.counts.factory_creates.fetch_add(1, Ordering::SeqCst);
        Ok(RequestStream::from_bytes(Bytes::from_static(BODY)))
    }
}

struct OneChunk(Option<Bytes>);

impl futures_core::Stream for OneChunk {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.take().map(Ok))
    }
}

pub struct UploadRepresentation;

impl RequestRepresentation<Upload> for UploadRepresentation {
    fn prepare(input: Upload, context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        input.counts.prepares.fetch_add(1, Ordering::SeqCst);
        match input.mode {
            UploadMode::Bytes => {
                PreparedBody::bytes(Bytes::from_static(BODY)).content_type("application/x-journal")
            }
            UploadMode::Factory => PreparedBody::factory(CountingFactory {
                counts: Arc::clone(&input.counts),
            })
            .content_type("application/x-journal"),
            UploadMode::OneShot => Ok(PreparedBody::stream(
                RequestStream::from_stream(OneChunk(Some(Bytes::from_static(BODY))))
                    .exact_length(u64::try_from(BODY.len()).expect("small body")),
            )),
            UploadMode::InvalidMedia => {
                PreparedBody::bytes(Bytes::from_static(BODY)).content_type("not a media type")
            }
            UploadMode::Oversized => {
                let declared = u64::try_from(context.body_limit.get()).expect("limit fits") + 1;
                Ok(PreparedBody::stream(
                    RequestStream::from_stream(OneChunk(Some(Bytes::from_static(BODY))))
                        .exact_length(declared),
                ))
            }
            UploadMode::Producer => Err(BodyError::new(BodyErrorKind::Producer)),
        }
    }
}

// ---------------------------------------------------------------------------
// Declarations.
// ---------------------------------------------------------------------------

api! {
    client JournalApi {
        timeout 30s;
        base "http://journal.example.test/v1/";
        values {
            #[sensitive] header("X-Tenant-Key") tenant_key?: String;
        }
        rate_limits {
            windows open { allow 100000 per 1s; }
        }
        rate_limit "journal.all" key [origin] using open;

        GET read() -> Bytes via Binary {
            id "journal.read";
            path "read";
        }
        GET text() -> String via Text {
            id "journal.text";
            path "text";
        }
        POST write() -> Bytes via Binary {
            id "journal.write";
            path "write";
        }
        PUT put() -> Bytes via Binary {
            id "journal.put";
            path "put";
        }
        PUT upload(body body: Upload via UploadRepresentation) -> Bytes via Binary {
            id "journal.upload";
            path "upload";
        }
    }
}

api! {
    /// A second logical origin sharing retry policies with `JournalApi`.
    client OtherOriginApi {
        timeout 30s;
        base "http://other-origin.example.test/";

        GET read() -> Bytes via Binary {
            id "other.read";
            path "read";
        }
    }
}

struct Harness {
    server: ScriptedServer,
    limiter: Arc<ProbeLimiter>,
    observer: Arc<ProbeObserver>,
    client: JournalApi,
}

fn harness_with(
    scripts: impl IntoIterator<Item = ResponseScript>,
    limiter: ProbeLimiter,
    observer: ProbeObserver,
    policy: Option<RetryPolicy>,
) -> Harness {
    let server = ScriptedServer::start(scripts).expect("loopback server");
    let proxy = SafeProxy::parse(server.url("")).expect("credential-free proxy");
    let limiter = Arc::new(limiter);
    let observer = Arc::new(observer);
    let selected: Arc<dyn RateLimiter> = limiter.clone();
    let observation = Arc::clone(&observer);
    let mut builder = JournalApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| {
            runtime
                .rate_limiter(selected)
                .operation_observer(observation)
        });
    if let Some(policy) = policy {
        builder = builder.retry(policy);
    }
    Harness {
        server,
        limiter,
        observer,
        client: builder.build().expect("journal client"),
    }
}

fn harness(scripts: impl IntoIterator<Item = ResponseScript>) -> Harness {
    harness_with(
        scripts,
        ProbeLimiter::default(),
        ProbeObserver::default(),
        None,
    )
}

/// Policy recommending a retry for every classifiable failure and counting
/// consultations.
fn always_retry(classifications: &Arc<AtomicUsize>) -> RetryPolicy {
    let classifications = Arc::clone(classifications);
    RetryPolicy::builder()
        .max_retries_per_operation(16)
        .without_budget()
        .classify_fn(move |_| {
            classifications.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        })
        .build()
        .expect("always-retry policy")
}

fn retry_503() -> RetryPolicy {
    RetryPolicy::builder()
        .max_retries_per_operation(1)
        .without_budget()
        .classify_fn(|attempt| match attempt.failure() {
            RetryFailure::RejectedStatus { status, .. }
                if status == StatusCode::SERVICE_UNAVAILABLE =>
            {
                RetryDecision::Retry
            }
            _ => RetryDecision::Stop,
        })
        .build()
        .expect("503 retry policy")
}

fn upload(mode: UploadMode, counts: &Arc<BodyCounts>) -> Upload {
    Upload {
        mode,
        counts: Arc::clone(counts),
    }
}

fn assert_omits(rendered: &str, sentinels: &[&str]) {
    for sentinel in sentinels {
        assert!(
            !rendered.contains(sentinel),
            "automatic output exposed `{sentinel}`"
        );
    }
}

fn status(code: u16) -> ResponseScript {
    ResponseScript::status(code, Vec::new())
}

// ---------------------------------------------------------------------------
// V3-E01 / V3-E02: facts after an accepted head are never lost.
// ---------------------------------------------------------------------------

const BODY_SENTINEL: &str = "journal-body-sentinel-5d10c3";

#[tokio::test]
async fn v3_e01_truncated_accepted_body_is_response_observed_acquisition_with_actual_status() {
    let harness = harness([ResponseScript::Truncated {
        status: 200,
        declared_length: 64,
        prefix: BODY_SENTINEL.as_bytes().to_vec(),
    }]);
    let error = harness
        .client
        .read()
        .await
        .expect_err("a truncated accepted body must fail");

    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(error.failure_stage(), FailureStage::ResponseAcquisition);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert!(error.may_have_reached_server());
    assert_eq!(error.endpoint_id().as_str(), "journal.read");
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
        harness.observer.completions(),
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
    let classifications = Arc::new(AtomicUsize::new(0));
    let harness = harness_with(
        [
            ResponseScript::ok(b"not-utf8-\xff".to_vec()),
            ResponseScript::ok(b"never-sent".to_vec()),
        ],
        ProbeLimiter::default(),
        ProbeObserver::default(),
        Some(always_retry(&classifications)),
    );
    let error = harness.client.text().await.expect_err("decode failure");

    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    assert_eq!(error.failure_stage(), FailureStage::ResponseConversion);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert_eq!(
        classifications.load(Ordering::SeqCst),
        0,
        "accepted conversion is terminal: the classifier is never consulted"
    );
    assert_eq!(harness.server.request_count(), 1);
    assert_eq!(harness.limiter.acquisitions(), 1);
}

// ---------------------------------------------------------------------------
// V3-E03: aggregate knowledge versus last-attempt facts.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e03_observed_attempt_then_no_send_admission_failure_keeps_the_aggregate() {
    let harness = harness_with(
        [status(503), ResponseScript::ok(b"never".to_vec())],
        ProbeLimiter::failing_at(1),
        ProbeObserver::default(),
        Some(retry_503()),
    );
    let error = harness
        .client
        .read()
        .await
        .expect_err("second admission fails");

    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(error.failure_stage(), FailureStage::RateAdmission);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved,
        "an earlier observed response is never erased by a later no-send attempt"
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
    assert_eq!(counts.application_handoffs(), 1);
    assert_eq!(counts.quota_admissions(), 1);
    assert_eq!(harness.server.request_count(), 1);
    assert_eq!(harness.limiter.acquisitions(), 2);
}

// ---------------------------------------------------------------------------
// V3-E04: cancellation and timeout at pipeline boundaries.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e04_token_cancelled_before_first_poll_prevents_every_effect() {
    let harness = harness([ResponseScript::ok(b"unused".to_vec())]);
    let token = CancellationToken::new();
    token.cancel();
    let error = harness
        .client
        .read()
        .cancel_with(token)
        .expect("one source")
        .await
        .expect_err("cancelled before start");

    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(error.failure_stage(), FailureStage::BeforeDispatch);
    assert_eq!(error.operation_id(), None);
    assert_eq!(error.last_attempt(), None);
    assert_eq!(harness.limiter.acquisitions(), 0);
    assert_eq!(harness.server.request_count(), 0);
    assert!(harness.observer.threads.lock().expect("threads").is_empty());
}

#[tokio::test]
async fn v3_e04_cancellation_during_admission_is_not_dispatched_and_never_sends() {
    let harness = harness([ResponseScript::ok(b"unused".to_vec())]);
    let gate = harness.limiter.gate(0);
    let token = CancellationToken::new();
    let task = tokio::spawn(std::future::IntoFuture::into_future(
        harness
            .client
            .write()
            .cancel_with(token.clone())
            .expect("one source"),
    ));
    gate.wait_reached().await;
    token.cancel();
    let error = task
        .await
        .expect("join")
        .expect_err("cancelled during admission");

    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(error.failure_stage(), FailureStage::RateAdmission);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert!(error.operation_id().is_some());
    assert_eq!(harness.server.request_count(), 0);
    assert_eq!(
        harness.observer.completions()[0].outcome,
        CompletionOutcome::Cancelled
    );
}

#[tokio::test]
async fn v3_e04_timeout_before_the_response_head_is_possibly_dispatched() {
    let harness = harness([ResponseScript::DelayBeforeHeaders {
        delay: Duration::from_secs(2),
        status: 200,
        body: b"late".to_vec(),
    }]);
    let error = harness
        .client
        .write()
        .timeout(Duration::from_millis(400))
        .expect("valid timeout")
        .await
        .expect_err("no head before the deadline");

    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.failure_stage(), FailureStage::DuringDispatchOrUnknown);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched,
        "a timeout after native handoff never fabricates NotDispatched"
    );
    assert_eq!(error.last_response_status(), None);
    assert_eq!(
        error
            .operation_counts()
            .expect("counts")
            .application_handoffs(),
        1
    );
}

#[tokio::test]
async fn v3_e04_cancellation_after_head_keeps_response_observed_and_status() {
    let body_gate = BodyGate::closed();
    let harness = harness([ResponseScript::GatedBody {
        status: 200,
        body: BODY_SENTINEL.as_bytes().to_vec(),
        gate: body_gate.clone(),
    }]);
    let token = CancellationToken::new();
    let observer = Arc::clone(&harness.observer);
    let task = tokio::spawn(std::future::IntoFuture::into_future(
        harness
            .client
            .read()
            .cancel_with(token.clone())
            .expect("one source"),
    ));
    tokio::time::timeout(Duration::from_secs(10), observer.head_seen.notified())
        .await
        .expect("head observed");
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
    let harness = harness([ResponseScript::GatedBody {
        status: 200,
        body: b"late".to_vec(),
        gate: body_gate.clone(),
    }]);
    let error = harness
        .client
        .read()
        .timeout(Duration::from_millis(500))
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
async fn v3_e04_refused_connection_is_never_settled_as_not_dispatched() {
    let reserved =
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("reserve port");
    let address = reserved.local_addr().expect("address");
    drop(reserved);
    let proxy = SafeProxy::parse(format!("http://{address}")).expect("closed proxy origin");
    let client = JournalApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(ProbeLimiter::default())))
        .build()
        .expect("client");
    let error = client.write().await.expect_err("refused connection");
    assert!(matches!(
        error.category(),
        ErrorCategory::Connect | ErrorCategory::Transport
    ));
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched,
        "a Connect category alone never proves that nothing was submitted"
    );
    assert_eq!(error.failure_stage(), FailureStage::DuringDispatchOrUnknown);
}

#[tokio::test]
async fn v3_e04_dropping_a_started_request_records_one_cancelled_completion() {
    let harness = harness([ResponseScript::DelayBeforeHeaders {
        delay: Duration::from_secs(2),
        status: 200,
        body: b"late".to_vec(),
    }]);
    let task = tokio::spawn(std::future::IntoFuture::into_future(harness.client.write()));
    tokio::time::timeout(Duration::from_secs(10), async {
        while harness.server.request_count() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("request handed to the transport");
    task.abort();
    assert!(task.await.expect_err("aborted").is_cancelled());
    assert_eq!(
        harness.observer.completions(),
        [Completion {
            outcome: CompletionOutcome::Cancelled,
            attempts_started: 1,
            policy_retries: 0,
            final_status: None,
        }]
    );
}

/// A custom credential provider whose acquisition waits on a test gate.
#[cfg(feature = "default-tls")]
pub struct GatedProvider {
    gate: Arc<Gate>,
    acquires: Arc<AtomicUsize>,
}

#[cfg(feature = "default-tls")]
impl CredentialProvider for GatedProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.acquires.fetch_add(1, Ordering::SeqCst);
        let gate = Arc::clone(&self.gate);
        Box::pin(async move {
            gate.reached.store(true, Ordering::SeqCst);
            gate.reached_signal.notify_waiters();
            gate.open.acquire().await.expect("gate").forget();
            Ok(AccessToken::try_new("preflight-provider-token").expect("bounded token"))
        })
    }
}

#[cfg(feature = "default-tls")]
api! {
    /// A provider-backed HTTPS client with a rate policy (V3-E04 preflight).
    client PreflightJournalApi {
        timeout 30s;
        base "https://localhost/v1/";
        credentials {
            credential session = custom GatedProvider;
        }
        rate_limits {
            windows open { allow 100000 per 1s; }
        }
        rate_limit "journal.preflight" key [origin] using open;
        auth { bearer session; }

        GET read() -> Bytes via Binary {
            id "journal.preflight.read";
            path "read";
        }
    }
}

#[cfg(feature = "default-tls")]
struct PreflightHarness {
    server: concord_test_support::tls::TlsServer,
    _tunnel: concord_test_support::proxy::ConnectProxy,
    gate: Arc<Gate>,
    acquires: Arc<AtomicUsize>,
    limiter: Arc<ProbeLimiter>,
    observer: Arc<ProbeObserver>,
    client: PreflightJournalApi,
}

#[cfg(feature = "default-tls")]
fn preflight_harness() -> PreflightHarness {
    let server = concord_test_support::tls::TlsServer::start().expect("TLS loopback");
    let tunnel =
        concord_test_support::proxy::ConnectProxy::start(server.address()).expect("CONNECT tunnel");
    let proxy = SafeProxy::parse(tunnel.url()).expect("safe proxy");
    let root = server.certificate_pem();
    let gate = Gate::new();
    let acquires = Arc::new(AtomicUsize::new(0));
    let limiter = Arc::new(ProbeLimiter::default());
    let observer = Arc::new(ProbeObserver::default());
    let selected: Arc<dyn RateLimiter> = limiter.clone();
    let observation = Arc::clone(&observer);
    let client = PreflightJournalApi::builder()
        .session(GatedProvider {
            gate: Arc::clone(&gate),
            acquires: Arc::clone(&acquires),
        })
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(root)
        })
        .configure_runtime(move |runtime| {
            runtime
                .rate_limiter(selected)
                .operation_observer(observation)
        })
        .build()
        .expect("preflight client");
    PreflightHarness {
        server,
        _tunnel: tunnel,
        gate,
        acquires,
        limiter,
        observer,
        client,
    }
}

/// Facts shared by every failure fixed while provider preflight waits.
#[cfg(feature = "default-tls")]
fn assert_preflight_failure(error: &concord::ApiError, category: ErrorCategory) {
    assert_eq!(error.category(), category);
    assert_eq!(error.failure_stage(), FailureStage::CredentialPreflight);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert!(!error.may_have_reached_server());
    assert_eq!(
        error.auth_kind(),
        None,
        "not reported as a credential failure"
    );
    assert!(error.operation_id().is_some());
    let counts = error.operation_counts().expect("counts");
    assert_eq!(counts.quota_admissions(), 0);
    assert_eq!(counts.application_handoffs(), 0);
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e04_cancellation_while_generated_provider_preflight_waits_is_not_dispatched() {
    let harness = preflight_harness();
    let token = CancellationToken::new();
    let task = tokio::spawn(std::future::IntoFuture::into_future(
        harness
            .client
            .read()
            .cancel_with(token.clone())
            .expect("one source"),
    ));
    harness.gate.wait_reached().await;
    token.cancel();
    let error = task
        .await
        .expect("join")
        .expect_err("cancelled during provider preflight");
    harness.gate.open.add_permits(1);

    assert_preflight_failure(&error, ErrorCategory::Cancelled);
    assert_eq!(harness.acquires.load(Ordering::SeqCst), 1);
    assert_eq!(harness.limiter.acquisitions(), 0, "no quota admission");
    assert!(harness.server.requests().is_empty(), "nothing sent");
    assert_eq!(
        harness.observer.completions(),
        [Completion {
            outcome: CompletionOutcome::Cancelled,
            attempts_started: 1,
            policy_retries: 0,
            final_status: None,
        }]
    );
}

#[cfg(feature = "default-tls")]
#[tokio::test]
async fn v3_e04_deadline_while_generated_provider_preflight_waits_is_a_timeout() {
    let harness = preflight_harness();
    let started = Instant::now();
    let error = harness
        .client
        .read()
        .timeout(Duration::from_millis(300))
        .expect("valid timeout")
        .await
        .expect_err("operation deadline during provider preflight");
    let elapsed = started.elapsed();
    harness.gate.open.add_permits(1);

    assert_preflight_failure(&error, ErrorCategory::Timeout);
    assert!(harness.gate.reached.load(Ordering::SeqCst));
    // The 300 ms operation deadline fired, not the 30 s provider ceiling.
    assert!(elapsed >= Duration::from_millis(300), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(10), "{elapsed:?}");
    assert_eq!(harness.acquires.load(Ordering::SeqCst), 1);
    assert_eq!(harness.limiter.acquisitions(), 0, "no quota admission");
    assert!(harness.server.requests().is_empty(), "nothing sent");
    assert_eq!(
        harness.observer.completions(),
        [Completion {
            outcome: CompletionOutcome::Timeout,
            attempts_started: 1,
            policy_retries: 0,
            final_status: None,
        }]
    );
}

// ---------------------------------------------------------------------------
// V3-E05: body preparation precedes admission; replay reconstructs.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e05_invalid_body_media_or_length_fails_before_admission_and_dispatch() {
    let harness = harness([ResponseScript::ok(b"never".to_vec())]);
    for (mode, category) in [
        (UploadMode::InvalidMedia, ErrorCategory::RequestBody),
        (UploadMode::Oversized, ErrorCategory::RequestBody),
        (UploadMode::Producer, ErrorCategory::RequestEncoding),
    ] {
        let counts = Arc::new(BodyCounts::default());
        let error = harness
            .client
            .upload(upload(mode, &counts))
            .await
            .expect_err("invalid prepared body");
        assert_eq!(error.category(), category);
        assert_eq!(error.failure_stage(), FailureStage::Preparation);
        assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
        assert_eq!(counts.prepares.load(Ordering::SeqCst), 1);
    }
    assert_eq!(
        harness.limiter.acquisitions(),
        0,
        "no admission for an already invalid body"
    );
    assert_eq!(harness.server.request_count(), 0);
}

#[tokio::test]
async fn v3_e05_a_repeated_attempt_prepares_once_and_reconstructs_by_replay_capability() {
    let harness = harness_with(
        [
            status(503),
            ResponseScript::ok(b"bytes-ok".to_vec()),
            status(503),
            ResponseScript::ok(b"factory-ok".to_vec()),
            status(503),
            ResponseScript::ok(b"must-remain".to_vec()),
        ],
        ProbeLimiter::default(),
        ProbeObserver::default(),
        Some(retry_503()),
    );

    let replayable = Arc::new(BodyCounts::default());
    assert_eq!(
        harness
            .client
            .upload(upload(UploadMode::Bytes, &replayable))
            .await
            .expect("replayable bytes retry"),
        Bytes::from_static(b"bytes-ok")
    );
    assert_eq!(replayable.prepares.load(Ordering::SeqCst), 1);

    let reconstructible = Arc::new(BodyCounts::default());
    assert_eq!(
        harness
            .client
            .upload(upload(UploadMode::Factory, &reconstructible))
            .await
            .expect("reconstructible factory retry"),
        Bytes::from_static(b"factory-ok")
    );
    assert_eq!(reconstructible.prepares.load(Ordering::SeqCst), 1);
    assert_eq!(
        reconstructible.factory_creates.load(Ordering::SeqCst),
        2,
        "one fresh stream per admitted attempt"
    );

    // A one-shot stream is never automatically replayed.
    let one_shot = Arc::new(BodyCounts::default());
    let error = harness
        .client
        .upload(upload(UploadMode::OneShot, &one_shot))
        .await
        .expect_err("one-shot body is not replayed");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(one_shot.prepares.load(Ordering::SeqCst), 1);

    let requests = harness.server.requests();
    assert_eq!(requests.len(), 5);
    for request in &requests {
        assert_eq!(request.body(), BODY);
    }
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/x-journal")
    );
    assert_eq!(
        requests[1].header("content-type"),
        Some("application/x-journal")
    );
}

// ---------------------------------------------------------------------------
// V3-E17: policy replay authority.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e17_non_idempotent_observed_or_ambiguous_failure_is_never_replayed() {
    let classifications = Arc::new(AtomicUsize::new(0));
    let harness = harness_with(
        [
            status(503),
            ResponseScript::CloseWithoutResponse,
            ResponseScript::ok(b"must-remain".to_vec()),
        ],
        ProbeLimiter::default(),
        ProbeObserver::default(),
        Some(always_retry(&classifications)),
    );

    let observed = harness
        .client
        .write()
        .await
        .expect_err("observed POST failure");
    assert_eq!(observed.category(), ErrorCategory::HttpStatus);
    assert_eq!(
        observed.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(
        observed
            .operation_counts()
            .expect("counts")
            .policy_retries(),
        0
    );

    let ambiguous = harness
        .client
        .write()
        .await
        .expect_err("ambiguous POST failure");
    assert!(matches!(
        ambiguous.category(),
        ErrorCategory::Transport | ErrorCategory::Connect
    ));
    assert_eq!(
        ambiguous.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched
    );
    assert_eq!(
        ambiguous
            .operation_counts()
            .expect("counts")
            .policy_retries(),
        0
    );
    assert_eq!(harness.server.request_count(), 2);
}

#[tokio::test]
async fn v3_e17_safe_or_idempotent_operations_may_use_permitted_policy_replay() {
    let classifications = Arc::new(AtomicUsize::new(0));
    let harness = harness_with(
        [
            status(503),
            ResponseScript::ok(b"put-ok".to_vec()),
            ResponseScript::CloseWithoutResponse,
            ResponseScript::ok(b"read-ok".to_vec()),
        ],
        ProbeLimiter::default(),
        ProbeObserver::default(),
        Some(always_retry(&classifications)),
    );
    let put = harness
        .client
        .put()
        .response()
        .await
        .expect("idempotent replay");
    assert_eq!(put.value(), &Bytes::from_static(b"put-ok"));
    assert_eq!(put.metadata().policy_retries(), 1);
    let read = harness
        .client
        .read()
        .response()
        .await
        .expect("safe replay after an ambiguous failure");
    assert_eq!(read.value(), &Bytes::from_static(b"read-ok"));
    assert_eq!(read.metadata().policy_retries(), 1);
    assert_eq!(classifications.load(Ordering::SeqCst), 2);
    assert_eq!(harness.server.request_count(), 4);
}

// ---------------------------------------------------------------------------
// V3-E18: separate counts; caps intersect and survive terminal wrappers.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e18_caps_intersect_and_never_reset_through_terminal_wrappers() {
    let harness = harness([status(503), ResponseScript::ok(b"must-remain".to_vec())]);
    let request = harness
        .client
        .read()
        .with_options(
            &CallOptions::new()
                .cap_sends(3)
                .cap_timeout(Duration::from_secs(2))
                .expect("finite cap"),
        )
        .response()
        .with_options(&CallOptions::new().cap_sends(5).cap_replays(4))
        .timeout(Duration::from_secs(60))
        .expect("selected timeout");
    let summary = RequestControls::control_summary(&request);
    assert_eq!(
        summary.send_cap(),
        Some(3),
        "a wider later cap never widens"
    );
    assert_eq!(summary.replay_cap(), Some(4));
    assert_eq!(summary.timeout_cap(), Some(Duration::from_secs(2)));
    assert_eq!(summary.selected_timeout(), Some(Duration::from_secs(60)));
    assert_eq!(
        summary.effective_timeout(),
        Some(Duration::from_secs(2)),
        "a cap is a constraint, a timeout a preference"
    );
    let reset = request.reset_timeout();
    let summary = reset.control_summary();
    assert_eq!(summary.declared_timeout(), Some(Duration::from_secs(30)));
    assert_eq!(summary.effective_timeout(), Some(Duration::from_secs(2)));
    assert_eq!(summary.cancellation_sources(), 0);
    drop(reset);
    assert_eq!(harness.server.request_count(), 0, "controls are inert");
}

#[tokio::test]
async fn v3_e18_replay_and_send_caps_bound_policy_retries_without_preparing_work() {
    let harness = harness_with(
        [
            status(503),
            ResponseScript::ok(b"must-remain".to_vec()),
            status(503),
            ResponseScript::ok(b"after-retry".to_vec()),
        ],
        ProbeLimiter::default(),
        ProbeObserver::default(),
        Some(retry_503()),
    );

    // A zero replay cap forbids the policy replay the classifier wants.
    let capped = harness
        .client
        .read()
        .with_options(&CallOptions::new().cap_replays(0))
        .await
        .expect_err("replay cap forbids the retry");
    assert_eq!(capped.category(), ErrorCategory::HttpStatus);
    let counts = capped.operation_counts().expect("counts");
    assert_eq!(counts.attempts_started(), 1);
    assert_eq!(counts.policy_retries(), 0);
    assert_eq!(counts.automatic_replays(), 0);
    assert_eq!(harness.server.request_count(), 1);

    // A zero send cap prevents dispatch before body, admission or transport.
    let counts_before = harness.limiter.acquisitions();
    let body = Arc::new(BodyCounts::default());
    let error = harness
        .client
        .upload(upload(UploadMode::Bytes, &body))
        .with_options(&CallOptions::new().cap_sends(0))
        .await
        .expect_err("no send allowed");
    assert_eq!(error.category(), ErrorCategory::SendCapExhausted);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(body.prepares.load(Ordering::SeqCst), 0);
    assert_eq!(harness.limiter.acquisitions(), counts_before);
    // The first script ("must-remain") is consumed by the next request.
    harness
        .client
        .read()
        .await
        .expect("the skipped script is served next");

    // Policy retries and replays are counted separately from attempts.
    let response = harness
        .client
        .read()
        .response()
        .await
        .expect("policy retry");
    assert_eq!(response.metadata().attempts_started(), 2);
    assert_eq!(response.metadata().policy_retries(), 1);
    assert!(!response.metadata().recovered_authentication());
    assert_eq!(harness.server.request_count(), 4);
}

// ---------------------------------------------------------------------------
// V3-E19: retry load budget.
// ---------------------------------------------------------------------------

#[test]
fn v3_e19_invalid_extra_load_ratios_are_rejected_at_build() {
    for ratio in [f64::NAN, f64::INFINITY, -0.1, 1000.5] {
        let error = RetryPolicy::builder()
            .max_extra_load(ratio)
            .classify_fn(|_| RetryDecision::Retry)
            .build()
            .expect_err("invalid ratio");
        assert_eq!(error.kind(), RetryPolicyBuildErrorKind::InvalidMaxExtraLoad);
    }
    for ratio in [0.0, 0.2, 1000.0] {
        RetryPolicy::builder()
            .max_extra_load(ratio)
            .classify_fn(|_| RetryDecision::Retry)
            .build()
            .expect("valid ratio");
    }
    let error = RetryPolicy::builder()
        .classify_fn(|_| RetryDecision::Retry)
        .budget_scope_capacity(0)
        .build()
        .expect_err("zero capacity");
    assert_eq!(
        error.kind(),
        RetryPolicyBuildErrorKind::InvalidBudgetScopeCapacity
    );
}

fn budget_policy(ratio: f64, capacity: usize, backoff: RetryBackoff) -> RetryPolicy {
    RetryPolicy::builder()
        .max_extra_load(ratio)
        .budget_scope_capacity(capacity)
        .backoff(backoff)
        .max_retries_per_operation(1)
        .classify_fn(|attempt| match attempt.failure() {
            RetryFailure::RejectedStatus { .. } => RetryDecision::Retry,
            _ => RetryDecision::Stop,
        })
        .build()
        .expect("budget policy")
}

#[tokio::test]
async fn v3_e19_zero_ratio_denies_every_extra_attempt_but_never_initial_requests() {
    let harness = harness_with(
        [
            status(503),
            status(503),
            ResponseScript::ok(b"must-remain".to_vec()),
        ],
        ProbeLimiter::default(),
        ProbeObserver::default(),
        Some(budget_policy(
            0.0,
            4_096,
            RetryBackoff::fixed(Duration::ZERO),
        )),
    );
    for _ in 0..2 {
        let error = harness
            .client
            .read()
            .await
            .expect_err("no extra load allowed");
        assert_eq!(error.category(), ErrorCategory::HttpStatus);
        assert_eq!(
            error.operation_counts().expect("counts").policy_retries(),
            0
        );
    }
    assert_eq!(harness.server.request_count(), 2);
}

#[tokio::test]
async fn v3_e19_exhausted_origin_capacity_denies_extras_for_a_new_origin_only() {
    let policy = budget_policy(1000.0, 1, RetryBackoff::fixed(Duration::ZERO));
    let first = harness_with(
        [status(503), ResponseScript::ok(b"first-recovered".to_vec())],
        ProbeLimiter::default(),
        ProbeObserver::default(),
        Some(policy.clone()),
    );
    first.client.read().await.expect("first origin retries");
    assert_eq!(first.server.request_count(), 2);

    let second_server =
        ScriptedServer::start([status(503), ResponseScript::ok(b"must-remain".to_vec())])
            .expect("second origin server");
    let proxy = SafeProxy::parse(second_server.url("")).expect("proxy");
    let other = OtherOriginApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .retry(policy)
        .build()
        .expect("other-origin client");
    let error = other
        .read()
        .await
        .expect_err("no scope capacity remains for a second origin");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(
        second_server.request_count(),
        1,
        "the initial request is never denied by the budget"
    );
}

#[tokio::test]
async fn v3_e19_cancellation_during_backoff_consumes_no_retry_reservation() {
    let observer = ProbeObserver::default();
    let harness = harness_with(
        [
            status(503),
            status(503),
            ResponseScript::ok(b"second-recovered".to_vec()),
        ],
        ProbeLimiter::default(),
        observer,
        Some(budget_policy(
            0.2,
            4_096,
            RetryBackoff::fixed(Duration::from_millis(300)),
        )),
    );
    let token = CancellationToken::new();
    let observer = Arc::clone(&harness.observer);
    let task = tokio::spawn(std::future::IntoFuture::into_future(
        harness
            .client
            .read()
            .cancel_with(token.clone())
            .expect("one source"),
    ));
    tokio::time::timeout(Duration::from_secs(10), observer.head_seen.notified())
        .await
        .expect("first head observed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    token.cancel();
    let cancelled = task.await.expect("join").expect_err("cancelled in backoff");
    assert_eq!(cancelled.category(), ErrorCategory::Cancelled);
    assert_eq!(
        cancelled.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(harness.server.request_count(), 1);

    // ceil(0.2 * 2 initial dispatches) = 1 approved retry remains available,
    // because the cancelled sleep reserved none.
    let recovered = harness
        .client
        .read()
        .response()
        .await
        .expect("the next operation may still retry");
    assert_eq!(recovered.metadata().policy_retries(), 1);
    assert_eq!(harness.server.request_count(), 3);
}

// ---------------------------------------------------------------------------
// V3-E20: server delay.
// ---------------------------------------------------------------------------

fn delay_policy(backoff: RetryBackoff, willingness: Duration) -> RetryPolicy {
    RetryPolicy::builder()
        .max_retries_per_operation(1)
        .without_budget()
        .backoff(backoff)
        .honor_server_delay_up_to(willingness)
        .classify_fn(|attempt| match attempt.failure() {
            RetryFailure::RejectedStatus { .. } => RetryDecision::Retry,
            _ => RetryDecision::Stop,
        })
        .build()
        .expect("server-delay policy")
}

fn retry_after(value: &str) -> ResponseScript {
    ResponseScript::status(503, Vec::new()).with_header("Retry-After", value)
}

#[tokio::test]
async fn v3_e20_retry_waits_the_maximum_of_backoff_and_server_minimum_not_their_sum() {
    let harness = harness_with(
        [
            retry_after("2"),
            ResponseScript::ok(b"after-delay".to_vec()),
        ],
        ProbeLimiter::default(),
        ProbeObserver::default(),
        Some(delay_policy(
            RetryBackoff::fixed(Duration::from_millis(1_500)),
            Duration::from_secs(5),
        )),
    );
    let started = Instant::now();
    let response = harness
        .client
        .read()
        .response()
        .await
        .expect("retry after the server minimum");
    let elapsed = started.elapsed();
    assert_eq!(response.value(), &Bytes::from_static(b"after-delay"));
    assert!(
        elapsed >= Duration::from_secs(2),
        "never earlier than the server minimum: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(3_300),
        "max(backoff, server minimum), not their 3.5 s sum: {elapsed:?}"
    );
}

#[tokio::test]
async fn v3_e20_too_long_or_overflowing_server_minimum_refuses_the_retry() {
    let harness = harness_with(
        [
            retry_after("5"),
            retry_after("18446744073709551616000"),
            ResponseScript::ok(b"must-remain".to_vec()),
        ],
        ProbeLimiter::default(),
        ProbeObserver::default(),
        Some(delay_policy(
            RetryBackoff::fixed(Duration::ZERO),
            Duration::from_secs(1),
        )),
    );
    let started = Instant::now();
    let too_long = harness
        .client
        .read()
        .await
        .expect_err("server minimum exceeds willingness");
    assert_eq!(too_long.category(), ErrorCategory::HttpStatus);
    assert_eq!(too_long.retry_after(), Some(Duration::from_secs(5)));
    let overflow = harness
        .client
        .read()
        .await
        .expect_err("overlarge hint forbids retry rather than wrapping");
    assert_eq!(overflow.category(), ErrorCategory::HttpStatus);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "no early retry sleep"
    );
    assert_eq!(harness.server.request_count(), 2);
}

#[tokio::test]
async fn v3_e20_malformed_duplicate_past_and_zero_hints_do_not_block_a_retry() {
    let duplicate = ResponseScript::status(503, Vec::new())
        .with_header("Retry-After", "1")
        .with_header("Retry-After", "1");
    let harness = harness_with(
        [
            retry_after("Wed, 21 Oct 2015 07:28:00 GMT"),
            ResponseScript::ok(b"past".to_vec()),
            duplicate,
            ResponseScript::ok(b"duplicate".to_vec()),
            retry_after("0"),
            ResponseScript::ok(b"zero".to_vec()),
            retry_after("-3"),
            ResponseScript::ok(b"negative".to_vec()),
        ],
        ProbeLimiter::default(),
        ProbeObserver::default(),
        // Zero willingness: any valid positive minimum would refuse.
        Some(delay_policy(
            RetryBackoff::fixed(Duration::ZERO),
            Duration::ZERO,
        )),
    );
    for expected in ["past", "duplicate", "zero", "negative"] {
        let response = harness
            .client
            .read()
            .response()
            .await
            .expect("no usable positive hint: immediate retry");
        assert_eq!(response.value().as_ref(), expected.as_bytes());
        assert_eq!(response.metadata().policy_retries(), 1);
    }
    assert_eq!(harness.server.request_count(), 8);
}

// ---------------------------------------------------------------------------
// V3-E21: observer containment and fixed results.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v3_e21_observer_panics_are_contained_and_never_change_the_result() {
    const TENANT_SENTINEL: &str = "observer-tenant-key-sentinel-66b2";
    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"second".to_vec()),
    ])
    .expect("server");
    let proxy = SafeProxy::parse(server.url("")).expect("proxy");
    let observer = Arc::new(ProbeObserver {
        panic_everywhere: true,
        ..ProbeObserver::default()
    });
    let observation = Arc::clone(&observer);
    let client = JournalApi::builder()
        .set_tenant_key(TENANT_SENTINEL)
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| {
            runtime
                .rate_limiter(Arc::new(ProbeLimiter::default()))
                .operation_observer(observation)
        })
        .build()
        .expect("client");

    assert_eq!(
        client.read().await.expect("result despite panics"),
        Bytes::from_static(b"first")
    );
    assert_eq!(
        client.read().await.expect("still healthy afterwards"),
        Bytes::from_static(b"second")
    );
    // Callbacks ran synchronously on the polling thread; core started no
    // detached observer or exporter task.
    let test_thread = std::thread::current().id();
    let threads = observer.threads.lock().expect("threads");
    assert!(threads.len() >= 6);
    assert!(threads.iter().all(|thread| *thread == test_thread));
    // The sensitive public header reached the server but never the observer.
    assert_eq!(
        server.requests()[0].header("x-tenant-key"),
        Some(TENANT_SENTINEL)
    );
    assert_omits(
        &observer.rendered.lock().expect("rendered").join("\n"),
        &[TENANT_SENTINEL],
    );
}

#[tokio::test]
async fn v3_e21_slow_final_callback_cannot_change_the_fixed_result() {
    let harness = harness_with(
        [ResponseScript::ok(b"fixed".to_vec())],
        ProbeLimiter::default(),
        ProbeObserver {
            slow_final: Some(Duration::from_millis(900)),
            ..ProbeObserver::default()
        },
        None,
    );
    let started = Instant::now();
    let value = harness
        .client
        .read()
        .timeout(Duration::from_millis(600))
        .expect("valid timeout")
        .await
        .expect("the result is fixed before final observation");
    assert_eq!(value, Bytes::from_static(b"fixed"));
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "the documented cost of a slow final callback is delayed delivery"
    );
    assert_eq!(
        harness.observer.completions()[0].outcome,
        CompletionOutcome::Success
    );
}

#[tokio::test]
async fn v3_e18_response_byte_cap_narrows_and_is_reported() {
    let harness = harness([ResponseScript::ok(vec![b'x'; 64])]);
    let request = harness.client.read().with_options(
        &CallOptions::new().cap_response_bytes(NonZeroUsize::new(16).expect("nonzero")),
    );
    assert_eq!(
        request.control_summary().response_byte_cap(),
        NonZeroUsize::new(16)
    );
    let error = request.await.expect_err("response exceeds the caller cap");
    assert_eq!(error.category(), ErrorCategory::ResponseBody);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
}
