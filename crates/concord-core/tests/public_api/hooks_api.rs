use concord_core::contracts::{
    AttemptInfo, BeforeSend, BoxFuture, CompletionEvent, HookError, PaginationCompletionEvent,
    ResponseHeadEvent, RuntimeHooks,
};

struct PublicHooks;

impl RuntimeHooks for PublicHooks {
    fn before_send<'a>(&'a self, context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        inspect_attempt(context.attempt);
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(
        &'a self,
        context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        inspect_attempt(context.attempt);
        Box::pin(async { Ok(()) })
    }

    fn on_complete<'a>(&'a self, context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        let _: (u32, u32, bool) = (
            context.attempts_started,
            context.policy_retries,
            context.recovered_authentication,
        );
        Box::pin(async {})
    }

    fn on_pagination_complete<'a>(
        &'a self,
        context: PaginationCompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        let _: (u32, u32, bool) = (
            context.attempts_started,
            context.policy_retries,
            context.recovered_authentication,
        );
        Box::pin(async {})
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
fn exact_public_hook_trait_is_implementable_without_private_imports() {
    assert_send_sync_static::<PublicHooks>();
    let hooks: std::sync::Arc<dyn RuntimeHooks> = std::sync::Arc::new(PublicHooks);
    assert_eq!(std::sync::Arc::strong_count(&hooks), 1);
}
