#![allow(dead_code)]

const PROTECTED_INPUT: &str = "MSRV_PROTECTED_VALUE_SENTINEL";

mod case {
    use concord_core::dangerous::RawResponse;

    pub(super) fn compile_contract() {
        let _: Option<RawResponse> = None;
    }
}

fn main() {}
