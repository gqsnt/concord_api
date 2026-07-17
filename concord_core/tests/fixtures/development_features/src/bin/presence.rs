use concord_core::__development::{
    ApplicationChannel, BodyCategory, CapturedHeader, CapturedHeaderValue, CapturedRequest, Channel,
    DeterministicExecutor, ExecutionFailure, ExecutionFailureKind, ExpectedRequest, Gate,
    ProviderChannel, ScriptedResponse, TransportFailure,
};

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

fn main() {
    assert_send_sync_static::<ApplicationChannel>();
    assert_send_sync_static::<ProviderChannel>();
    assert_send_sync_static::<CapturedRequest>();
    assert_send_sync_static::<ExecutionFailure>();
    let executor = DeterministicExecutor::new();
    executor
        .application()
        .queue_response(ScriptedResponse::success("application"))
        .expect("application response should queue");
    executor
        .provider()
        .queue_failure(TransportFailure::Connect)
        .expect("provider failure should queue");
    let _ = (
        BodyCategory::Empty,
        CapturedHeaderValue::Redacted,
        Channel::Application,
        ExecutionFailureKind::EmptyQueue,
        ExpectedRequest::new(),
        Gate::closed(),
    );
    let _: Option<CapturedHeader> = None;
}
