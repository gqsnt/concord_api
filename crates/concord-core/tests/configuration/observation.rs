use std::sync::Arc;

use concord_core::contracts::{
    CompletionObserver, DebugLevel, RequestGate, ResponseGate, RuntimeConfigBuilder,
};

struct Policy;
impl RequestGate for Policy {}
impl ResponseGate for Policy {}
impl CompletionObserver for Policy {}

#[test]
fn final_gate_observer_and_debug_setter_signatures_are_public() {
    let _request: fn(RuntimeConfigBuilder, Arc<dyn RequestGate>) -> RuntimeConfigBuilder =
        RuntimeConfigBuilder::request_gate;
    let _response: fn(RuntimeConfigBuilder, Arc<dyn ResponseGate>) -> RuntimeConfigBuilder =
        RuntimeConfigBuilder::response_gate;
    let _observer: fn(RuntimeConfigBuilder, Arc<dyn CompletionObserver>) -> RuntimeConfigBuilder =
        RuntimeConfigBuilder::completion_observer;
    let _debug: fn(RuntimeConfigBuilder, DebugLevel) -> RuntimeConfigBuilder =
        RuntimeConfigBuilder::debug;
    let policy = Arc::new(Policy);
    assert_eq!(Arc::strong_count(&policy), 1);
}
