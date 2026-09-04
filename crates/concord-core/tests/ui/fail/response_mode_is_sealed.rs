use concord_core::contracts::{ResponseAcquisition, ResponseDriver};

struct NativeOrUnboundedMode;

impl ResponseDriver for NativeOrUnboundedMode {
    type Body = ();
    const ACQUISITION: ResponseAcquisition = ResponseAcquisition::Buffered;
}

fn main() {}
