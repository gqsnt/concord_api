use concord_core::contracts::{
    ApiError, AttemptInfo, AuthErrorKind, CredentialReason, DispatchKnowledge, ExecutionMeta,
    OperationEvent, OperationId, OperationObserver, ProviderAcquisitionFacts,
    ProviderAcquisitionFinished, ProviderAcquisitionOutcome, StatusCode,
};

struct PublicPolicy;

impl OperationObserver for PublicPolicy {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(context) => inspect_attempt(context.attempt),
            OperationEvent::ResponseHeadObserved(context) => inspect_attempt(context.attempt),
            OperationEvent::OperationFinished(context) => {
                let _: (u32, u32, bool) = (
                    context.attempts_started,
                    context.policy_retries,
                    context.recovered_authentication,
                );
            }
            OperationEvent::PaginationFinished(context) => {
                let _: (u32, u32, bool) = (
                    context.attempts_started,
                    context.policy_retries,
                    context.recovered_authentication,
                );
            }
            OperationEvent::ProviderAcquisitionFinished(finished) => inspect_provider(&finished),
            _ => {}
        }
    }
}

fn inspect_attempt(attempt: AttemptInfo) {
    let _: (u32, u32, bool) = (
        attempt.index,
        attempt.policy_retry_index,
        attempt.authentication_recovery,
    );
}

/// EXEC-01: the provider journal surface is nameable and value-free.
fn inspect_provider(finished: &ProviderAcquisitionFinished) {
    let _: Option<AuthErrorKind> = finished.auth_kind;
    let _: std::time::Duration = finished.elapsed;
    inspect_facts(finished.facts);
}

fn inspect_facts(facts: ProviderAcquisitionFacts) {
    let _: OperationId = facts.operation_id();
    let _: Option<OperationId> = facts.parent_operation_id();
    let _: CredentialReason = facts.reason();
    let _: u32 = facts.helper_sends();
    let _: Option<StatusCode> = facts.last_status();
    let _: DispatchKnowledge = facts.dispatch_knowledge();
    match facts.outcome() {
        ProviderAcquisitionOutcome::Installed
        | ProviderAcquisitionOutcome::Failed
        | ProviderAcquisitionOutcome::Superseded
        | ProviderAcquisitionOutcome::Cancelled => {}
        _ => {}
    }
}

fn provider_links(error: &ApiError, metadata: &ExecutionMeta) {
    let _: Option<ProviderAcquisitionFacts> = error.provider_acquisition();
    let _: Option<ProviderAcquisitionFacts> = metadata.provider_acquisition();
}

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

fn assert_value_facts<T: Clone + Copy + std::fmt::Debug + Eq + Send + Sync + 'static>() {}

#[test]
fn exact_public_observer_trait_is_implementable_without_private_imports() {
    assert_send_sync_static::<PublicPolicy>();
    assert_value_facts::<ProviderAcquisitionFacts>();
    assert_value_facts::<ProviderAcquisitionOutcome>();
    assert_send_sync_static::<ProviderAcquisitionFinished>();
    let _ = provider_links;
    let policy = std::sync::Arc::new(PublicPolicy);
    assert_eq!(std::sync::Arc::strong_count(&policy), 1);
}
