#![allow(dead_code)]

const PROTECTED_INPUT: &str = "MSRV_PROTECTED_VALUE_SENTINEL";

mod case {
    use concord::endpoint::EndpointId;

    pub(super) fn compile_contract() {
        let _: EndpointId = "unsafe.infallible".into();
    }
}

fn main() {}
