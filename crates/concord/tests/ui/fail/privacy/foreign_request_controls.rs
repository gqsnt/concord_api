// V3-L15 / CALL-02: `RequestControls` is intentionally open, and this
// downstream crate's implementation (delegating to a real generated request)
// compiles and works with a generic control helper. It grants no authority:
// it cannot be bound as an intent, is not executable, and cannot reach the
// wrapped request's private core binding. Anti-example: a downstream crate
// cannot become a native response driver by implementing the core-sealed
// `ResponseDriver` trait; it fails on the unsatisfied private seal.
use std::future::IntoFuture;

use concord::endpoint::ResponseAcquisition;
use concord::response::ResponseDriver;
use concord::{
    Binary, Bytes, CallOptions, CancellationToken, ControlError, ControlSummary, RequestControls,
    api,
};

api! {
    client ControlsApi {
        base "https://controls.example.test";

        GET read() -> Bytes via Binary;
    }
}

pub struct Wrapped(controls_api::ReadRequest);

impl RequestControls for Wrapped {
    fn with_options(self, options: &CallOptions) -> Self {
        Self(self.0.with_options(options))
    }

    fn control_summary(&self) -> ControlSummary {
        self.0.control_summary()
    }

    fn cancel_with(self, token: CancellationToken) -> Result<Self, ControlError> {
        self.0.cancel_with(token).map(Self)
    }
}

fn narrow<R: RequestControls>(request: R) -> R {
    request.with_options(&CallOptions::new())
}

pub struct ForeignDriver;

impl ResponseDriver for ForeignDriver {
    type Body = Bytes;
    const ACQUISITION: ResponseAcquisition = ResponseAcquisition::Buffered;
}

fn main() {
    let client = ControlsApi::new().unwrap();
    let _ = narrow(client.read());
    let wrapped = narrow(Wrapped(client.read()));
    let _ = wrapped.control_summary();

    let _ = client.request(wrapped);
    let _ = narrow(Wrapped(client.read())).into_future();
    let _ = Wrapped(client.read()).0.__inner;
}
