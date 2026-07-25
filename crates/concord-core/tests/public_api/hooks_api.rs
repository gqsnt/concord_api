use concord_core::contracts::{
    BeforeSend, BoxFuture, CompletionEvent, HookError, PaginationCompletionEvent,
    ResponseHeadEvent, RuntimeHooks,
};

struct PublicHooks;

impl RuntimeHooks for PublicHooks {
    fn before_send<'a>(&'a self, _context: BeforeSend<'a>) -> BoxFuture<'a, Result<(), HookError>> {
        Box::pin(async { Ok(()) })
    }

    fn on_response_head<'a>(
        &'a self,
        _context: ResponseHeadEvent<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        Box::pin(async { Ok(()) })
    }

    fn on_complete<'a>(&'a self, _context: CompletionEvent<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    fn on_pagination_complete<'a>(
        &'a self,
        _context: PaginationCompletionEvent<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

#[test]
fn exact_public_hook_trait_is_implementable_without_private_imports() {
    assert_send_sync_static::<PublicHooks>();
    let hooks: std::sync::Arc<dyn RuntimeHooks> = std::sync::Arc::new(PublicHooks);
    assert_eq!(std::sync::Arc::strong_count(&hooks), 1);
}
