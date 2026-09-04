use concord_core::contracts::{AttemptInfo, OperationEvent, OperationObserver};

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

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

#[test]
fn exact_public_observer_trait_is_implementable_without_private_imports() {
    assert_send_sync_static::<PublicPolicy>();
    let policy = std::sync::Arc::new(PublicPolicy);
    assert_eq!(std::sync::Arc::strong_count(&policy), 1);
}
