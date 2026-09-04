use concord_core::contracts::{OperationEvent, OperationObserver};

struct Probe;

impl OperationObserver for Probe {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(request) => {
                let _ = request.observed_url;
                let _ = request.headers;
            }
            OperationEvent::ResponseHeadObserved(response) => {
                let _ = response.observed_url;
                let _ = response.headers;
            }
            OperationEvent::OperationFinished(completion) => {
                let _ = completion.observed_url;
            }
            _ => {}
        }
    }
}

fn main() {}
