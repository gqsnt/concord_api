//! EXEC-01 (§13.1, §13.7): a credential-provider acquisition is its own
//! operation identity with a bounded provider journal linked to the
//! application operation that led it.
//!
//! These tests drive real executor operations through the deterministic
//! application and provider channels and observe only public surfaces:
//! [`ApiError::provider_acquisition`], `ExecutionMeta::provider_acquisition`
//! and the `ProviderAcquisitionFinished` observation. They cover a successful
//! acquisition and a provider-caused failure, where the provider identity is
//! distinct from the application identity and linked to it; followers of one
//! flight that share the leader's provider identity; a cancelled leader that
//! settles its provider journal as cancelled; and an acquisition bounded by
//! its leader's deadline.

use std::future::IntoFuture;

use super::*;
use crate::controls::CancellationToken;
use crate::error::{
    DispatchKnowledge, FailureStage, ProviderAcquisitionFacts, ProviderAcquisitionOutcome,
};

const PROVIDER_URL: &str = "https://journal-provider.example.test/token";
const PROVIDER_HOST: &str = "journal-provider.example.test";
const PROVIDER_BODY_SENTINEL: &str = "provider-journal-body-sentinel-5b1e";

/// A provider that sends one helper request through the isolated provider
/// HTTP capability and succeeds only on a 200 response. An optional gate
/// holds the acquisition before its helper request.
struct JournaledProvider {
    origins: [ProviderOrigin; 1],
    gate: Option<Gate>,
    acquires: Arc<AtomicUsize>,
}

impl JournaledProvider {
    fn new(gate: Option<Gate>, acquires: &Arc<AtomicUsize>) -> Self {
        Self {
            origins: [
                ProviderOrigin::parse("https://journal-provider.example.test")
                    .expect("journal provider origin"),
            ],
            gate,
            acquires: Arc::clone(acquires),
        }
    }
}

impl CredentialProvider for JournaledProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.acquires.fetch_add(1, Ordering::SeqCst);
        let gate = self.gate.clone();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.wait().await;
            }
            let response = context
                .http
                .send(ProviderRequest::new(
                    Method::POST,
                    url::Url::parse(PROVIDER_URL).expect("provider URL"),
                ))
                .await?;
            if response.status() != StatusCode::OK {
                return Err(AuthError::new(AuthErrorKind::Acquisition));
            }
            Ok(access_token(INITIAL))
        })
    }

    fn http_origins(&self) -> &[ProviderOrigin] {
        &self.origins
    }
}

type ProviderRecord = (ProviderAcquisitionFacts, Option<AuthErrorKind>);

#[derive(Default)]
struct JournalObserver {
    providers: Mutex<Vec<ProviderRecord>>,
    completions: Mutex<Vec<(OperationId, CompletionOutcome)>>,
}

impl JournalObserver {
    fn providers(&self) -> Vec<ProviderRecord> {
        self.providers.lock().expect("provider records").clone()
    }

    fn completions(&self) -> Vec<(OperationId, CompletionOutcome)> {
        self.completions.lock().expect("completion records").clone()
    }

    fn completed_operations(&self) -> Vec<OperationId> {
        self.completions()
            .into_iter()
            .map(|(operation, _)| operation)
            .collect()
    }
}

impl OperationObserver for JournalObserver {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::ProviderAcquisitionFinished(finished) => {
                self.providers
                    .lock()
                    .expect("provider records")
                    .push((finished.facts, finished.auth_kind));
            }
            OperationEvent::OperationFinished(finished) => {
                self.completions
                    .lock()
                    .expect("completion records")
                    .push((finished.operation_id, finished.outcome));
            }
            _ => {}
        }
    }
}

fn journal_core(executor: &DeterministicExecutor, observer: &Arc<JournalObserver>) -> ClientCore {
    ClientCore::build_with_development(
        "https://protected.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new().operation_observer(Arc::clone(observer) as _),
        executor,
    )
    .expect("provider journal development core")
}

fn protected(
    core: &ClientCore,
    selector: &Arc<SlotSelector<AccessToken>>,
) -> Request<ProtectedEndpoint> {
    request(
        core.clone(),
        selector,
        ChallengePolicy::Never,
        PreparedBody::bytes(Bytes::new()),
    )
}

fn queue_application_successes(executor: &DeterministicExecutor, count: usize) {
    for _ in 0..count {
        executor
            .application()
            .queue_response(ScriptedResponse::success(Bytes::from_static(b"ok")))
            .expect("application response");
    }
}

/// Holds the gated acquisition until both operations have been polled, then
/// releases it.
async fn release_after_both_polled(gate: &Gate) {
    gate.wait_until_reached().await;
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    gate.open();
}

#[tokio::test]
async fn exec_01_successful_acquisition_is_a_distinct_identity_linked_to_its_parent() {
    let executor = DeterministicExecutor::new();
    executor
        .provider()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"issued")))
        .expect("provider response");
    queue_application_successes(&executor, 2);
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(JournaledProvider::new(None, &acquires), Duration::ZERO);
    let observer = Arc::new(JournalObserver::default());
    let core = journal_core(&executor, &observer);

    let response = protected(&core, &selector)
        .response()
        .await
        .expect("provider-backed application request");

    let completions = observer.completions();
    assert_eq!(completions.len(), 1);
    let (application, outcome) = completions[0];
    assert_eq!(outcome, CompletionOutcome::Success);
    let providers = observer.providers();
    assert_eq!(providers.len(), 1, "exactly one provider observation");
    let (facts, auth_kind) = providers[0];
    assert_eq!(auth_kind, None);
    // Distinct identity, linked to the leading application operation.
    assert_ne!(facts.operation_id(), application);
    assert_eq!(facts.parent_operation_id(), Some(application));
    // The provider journal records its own helper traffic.
    assert_eq!(facts.reason(), CredentialReason::Missing);
    assert_eq!(facts.helper_sends(), 1);
    assert_eq!(facts.last_status(), Some(StatusCode::OK));
    assert_eq!(
        facts.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Installed);
    // The application result links the same provider journal.
    assert_eq!(response.metadata().provider_acquisition(), Some(facts));

    // A later operation using the installed material depends on no
    // acquisition and starts none.
    let later = protected(&core, &selector)
        .response()
        .await
        .expect("request with installed material");
    assert_eq!(later.metadata().provider_acquisition(), None);
    assert_eq!(observer.providers().len(), 1);
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.provider().request_count().expect("count"), 1);
    assert_eq!(executor.application().request_count().expect("count"), 2);
}

#[tokio::test]
async fn exec_01_provider_caused_failure_exposes_a_distinct_linked_provider_journal() {
    let executor = DeterministicExecutor::new();
    executor
        .provider()
        .queue_response(ScriptedResponse::status(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(PROVIDER_BODY_SENTINEL.as_bytes()),
        ))
        .expect("provider response");
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(JournaledProvider::new(None, &acquires), Duration::ZERO);
    let observer = Arc::new(JournalObserver::default());
    let core = journal_core(&executor, &observer);

    let error = api_error(protected(&core, &selector).await);
    assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
    assert_eq!(error.failure_stage(), FailureStage::CredentialPreflight);
    // The application request itself was never dispatched...
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    // ...while the separate provider journal records the auxiliary exchange
    // under its own identity, linked to the failed application operation.
    let application = error.operation_id().expect("application operation");
    let facts = error.provider_acquisition().expect("provider journal");
    assert_ne!(facts.operation_id(), application);
    assert_eq!(facts.parent_operation_id(), Some(application));
    assert_eq!(facts.helper_sends(), 1);
    assert_eq!(facts.last_status(), Some(StatusCode::SERVICE_UNAVAILABLE));
    assert_eq!(
        facts.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Failed);
    assert_eq!(executor.provider().request_count().expect("count"), 1);

    // The observation carries the same facts, emitted once by the leader.
    assert_eq!(
        observer.providers(),
        vec![(facts, Some(AuthErrorKind::Acquisition))]
    );
    assert_eq!(
        observer.completions(),
        vec![(application, CompletionOutcome::AuthenticationProvider)]
    );

    // Formatting stays value-free: identities are opaque and no provider
    // URL or body appears.
    let rendered = format!("{error} {error:?}");
    assert!(rendered.contains("provider_acquisition"));
    assert!(rendered.contains("OperationId(<opaque>)"));
    assert!(!rendered.contains(PROVIDER_BODY_SENTINEL));
    assert!(!rendered.contains(PROVIDER_HOST));
}

#[tokio::test]
async fn exec_01_followers_joining_one_acquisition_report_the_same_provider_identity() {
    let executor = DeterministicExecutor::new();
    executor
        .provider()
        .queue_response(ScriptedResponse::success(Bytes::from_static(b"issued")))
        .expect("provider response");
    queue_application_successes(&executor, 2);
    let gate = Gate::closed();
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        JournaledProvider::new(Some(gate.clone()), &acquires),
        Duration::ZERO,
    );
    let observer = Arc::new(JournalObserver::default());
    let core = journal_core(&executor, &observer);

    let (first, second, ()) = tokio::join!(
        protected(&core, &selector).response().into_future(),
        protected(&core, &selector).response().into_future(),
        release_after_both_polled(&gate),
    );
    let first = first.expect("first provider-backed request");
    let second = second.expect("second provider-backed request");

    // One flight: one provider call, one helper send, one observation.
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.provider().request_count().expect("count"), 1);
    let providers = observer.providers();
    assert_eq!(providers.len(), 1);
    let (facts, _) = providers[0];
    assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Installed);
    assert_eq!(facts.helper_sends(), 1);
    // Leader and follower report the same provider identity.
    assert_eq!(first.metadata().provider_acquisition(), Some(facts));
    assert_eq!(second.metadata().provider_acquisition(), Some(facts));
    // It is distinct from both application operations, and its parent is
    // exactly one of them: the leader.
    let operations = observer.completed_operations();
    assert_eq!(operations.len(), 2);
    assert_ne!(operations[0], operations[1]);
    assert!(!operations.contains(&facts.operation_id()));
    let parent = facts.parent_operation_id().expect("leading operation");
    assert!(operations.contains(&parent));
}

#[tokio::test]
async fn exec_01_followers_of_a_failed_acquisition_report_the_same_provider_identity() {
    let executor = DeterministicExecutor::new();
    executor
        .provider()
        .queue_response(ScriptedResponse::status(
            StatusCode::BAD_GATEWAY,
            Bytes::from_static(PROVIDER_BODY_SENTINEL.as_bytes()),
        ))
        .expect("provider response");
    let gate = Gate::closed();
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        JournaledProvider::new(Some(gate.clone()), &acquires),
        Duration::ZERO,
    );
    let observer = Arc::new(JournalObserver::default());
    let core = journal_core(&executor, &observer);

    let (first, second, ()) = tokio::join!(
        protected(&core, &selector).into_future(),
        protected(&core, &selector).into_future(),
        release_after_both_polled(&gate),
    );
    let first = api_error(first);
    let second = api_error(second);

    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.provider().request_count().expect("count"), 1);
    assert_eq!(executor.application().request_count().expect("count"), 0);
    let first_facts = first.provider_acquisition().expect("leader journal");
    let second_facts = second.provider_acquisition().expect("follower journal");
    assert_eq!(first_facts, second_facts);
    assert_eq!(first_facts.outcome(), ProviderAcquisitionOutcome::Failed);
    assert_eq!(first_facts.last_status(), Some(StatusCode::BAD_GATEWAY));
    assert_eq!(
        observer.providers(),
        vec![(first_facts, Some(AuthErrorKind::Acquisition))]
    );

    let first_operation = first.operation_id().expect("first application operation");
    let second_operation = second.operation_id().expect("second application operation");
    assert_ne!(first_operation, second_operation);
    for operation in [first_operation, second_operation] {
        assert_ne!(first_facts.operation_id(), operation);
    }
    // The provider acquisition is parented by the leader only; the follower
    // links to it without becoming its parent.
    let parent = first_facts.parent_operation_id().expect("leader");
    assert!(parent == first_operation || parent == second_operation);
    for error in [&first, &second] {
        assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
        assert_eq!(error.category(), ErrorCategory::AuthenticationProvider);
    }
}

#[tokio::test]
async fn exec_01_leader_cancelled_while_its_provider_runs_settles_the_journal_as_cancelled() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        JournaledProvider::new(Some(gate.clone()), &acquires),
        Duration::ZERO,
    );
    let observer = Arc::new(JournalObserver::default());
    let core = journal_core(&executor, &observer);
    let token = CancellationToken::new();

    let (result, ()) = tokio::join!(
        protected(&core, &selector)
            .cancel_with(token.clone())
            .expect("one cancellation source")
            .into_future(),
        async {
            gate.wait_until_reached().await;
            token.cancel();
        },
    );
    let error = api_error(result);
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    let application = error.operation_id().expect("application operation");
    let facts = error
        .provider_acquisition()
        .expect("cancelled provider journal linked into the application journal");
    assert_ne!(facts.operation_id(), application);
    assert_eq!(facts.parent_operation_id(), Some(application));
    assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Cancelled);
    assert_eq!(facts.helper_sends(), 0);
    assert_eq!(facts.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    // The leader's synchronous drop guard emitted the single observation.
    assert_eq!(observer.providers(), vec![(facts, None)]);
    assert_eq!(
        observer.completions(),
        vec![(application, CompletionOutcome::Cancelled)]
    );
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.provider().request_count().expect("count"), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
}

#[tokio::test(start_paused = true)]
async fn exec_01_provider_bounded_by_its_leaders_deadline_fails_as_a_linked_acquisition() {
    let executor = DeterministicExecutor::new();
    let gate = Gate::closed();
    let acquires = Arc::new(AtomicUsize::new(0));
    let selector = SlotSelector::provider(
        JournaledProvider::new(Some(gate.clone()), &acquires),
        Duration::ZERO,
    );
    let observer = Arc::new(JournalObserver::default());
    let core = journal_core(&executor, &observer);

    let error = api_error(
        protected(&core, &selector)
            .timeout(Duration::from_secs(2))
            .expect("finite timeout")
            .await,
    );
    // The application operation reports its own deadline...
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    // ...while the acquisition, bounded by min(remaining operation deadline,
    // provider timeout), failed at its own helper time limit.
    let application = error.operation_id().expect("application operation");
    let facts = error
        .provider_acquisition()
        .expect("linked provider journal");
    assert_ne!(facts.operation_id(), application);
    assert_eq!(facts.parent_operation_id(), Some(application));
    assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Failed);
    assert_eq!(facts.helper_sends(), 0);
    assert_eq!(
        observer.providers(),
        vec![(facts, Some(AuthErrorKind::Acquisition))]
    );
    assert_eq!(acquires.load(Ordering::SeqCst), 1);
    assert_eq!(executor.provider().request_count().expect("count"), 0);
    assert_eq!(executor.application().request_count().expect("count"), 0);
}
