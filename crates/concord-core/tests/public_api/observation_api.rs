use concord_core::contracts::{
    AttemptInfo, BeforeSend, BoxFuture, CompletionEvent, CompletionObserver, GateError,
    PaginationCompletionEvent, RequestGate, ResponseGate, ResponseHeadEvent,
};

struct PublicPolicy;

impl RequestGate for PublicPolicy {
    fn before_send<'a>(&'a self, context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), GateError>> {
        inspect_attempt(context.attempt);
        Box::pin(async { Ok(()) })
    }
}

impl ResponseGate for PublicPolicy {
    fn before_response_handling<'a>(
        &'a self,
        context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), GateError>> {
        inspect_attempt(context.attempt);
        Box::pin(async { Ok(()) })
    }
}

impl CompletionObserver for PublicPolicy {
    fn on_complete(&self, context: CompletionEvent<'_>) {
        let _: (u32, u32, bool) = (
            context.attempts_started,
            context.policy_retries,
            context.recovered_authentication,
        );
    }

    fn on_pagination_complete(&self, context: PaginationCompletionEvent<'_>) {
        let _: (u32, u32, bool) = (
            context.attempts_started,
            context.policy_retries,
            context.recovered_authentication,
        );
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
fn exact_public_gate_and_observer_traits_are_implementable_without_private_imports() {
    assert_send_sync_static::<PublicPolicy>();
    let policy = std::sync::Arc::new(PublicPolicy);
    assert_eq!(std::sync::Arc::strong_count(&policy), 1);
}
