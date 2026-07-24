#![allow(dead_code)]

const PROTECTED_INPUT: &str = "MSRV_PROTECTED_VALUE_SENTINEL";

mod case {
    use concord_core::contracts::{MultipartBody, MultipartFactory, MultipartPart};

    pub(super) fn compile_contract() {
        let _ = std::mem::size_of::<MultipartBody>();
        let _ = std::mem::size_of::<MultipartPart>();
        fn accepts_factory<T: MultipartFactory>() {}
        let _ = accepts_factory::<()>;
    }
}

fn main() {}
