use concord_core::contracts::{OperationEvent, OperationObserver};

struct Probe;

impl OperationObserver for Probe {
    fn observe(&self, event: OperationEvent<'_>) {
        if let OperationEvent::ResponseHeadObserved(response) = event {
            let _ = response.headers;
        }
    }
}

fn main() {}
