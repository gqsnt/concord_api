use std::sync::Arc;

use concord_core::contracts::{DebugLevel, OperationObserver, RuntimeConfigBuilder};

struct Policy;
impl OperationObserver for Policy {}

#[test]
fn operation_observer_and_debug_setter_signatures_are_public() {
    let _observer: fn(RuntimeConfigBuilder, Arc<dyn OperationObserver>) -> RuntimeConfigBuilder =
        RuntimeConfigBuilder::operation_observer;
    let _debug: fn(RuntimeConfigBuilder, DebugLevel) -> RuntimeConfigBuilder =
        RuntimeConfigBuilder::debug;
    let policy = Arc::new(Policy);
    assert_eq!(Arc::strong_count(&policy), 1);
}
