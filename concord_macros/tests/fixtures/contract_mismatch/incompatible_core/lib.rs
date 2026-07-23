#![forbid(unsafe_code)]

pub use concord_core_actual::*;

#[doc(hidden)]
pub mod __private {
    pub use concord_core_actual::__private::*;

    #[doc(hidden)]
    pub const GENERATED_CONTRACT_VERSION: u32 = 999;

    #[doc(hidden)]
    pub const fn assert_generated_contract(expected: u32) {
        assert!(
            expected == 19,
            "concord macro emitted an unexpected generated contract version"
        );
        assert!(
            GENERATED_CONTRACT_VERSION == expected,
            "concord macro/core contract mismatch: core 999, generated 19"
        );
    }
}
