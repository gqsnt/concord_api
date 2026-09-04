use concord_core::contracts::{OperationEvent, OperationObserver};

struct ReadOnlyObserver;

impl OperationObserver for ReadOnlyObserver {
    fn observe(&self, _event: OperationEvent<'_>) {}
}

#[test]
fn observation_contract_exposes_no_result_or_async_decision_channel() {
    fn assert_observer<T: OperationObserver>() {}
    assert_observer::<ReadOnlyObserver>();
}
