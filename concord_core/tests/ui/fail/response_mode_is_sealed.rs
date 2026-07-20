use concord_core::advanced::ResponseMode;

struct NativeOrUnboundedMode;

impl ResponseMode for NativeOrUnboundedMode {
    type Body = ();
}

fn main() {}
