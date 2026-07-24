use concord_core::contracts::{ResponseAcquisition, ResponseMode};

struct NativeOrUnboundedMode;

impl ResponseMode for NativeOrUnboundedMode {
    type Body = ();
    const ACQUISITION: ResponseAcquisition = ResponseAcquisition::Buffered;
}

fn main() {}
