#![forbid(unsafe_code)]

#[doc(hidden)]
pub use concord_actual::__concord_api as __fixture_api_impl;
pub use concord_actual::*;

#[macro_export]
macro_rules! api {
    ($($declaration:tt)*) => {
        $crate::__fixture_api_impl! {
            @concord_facade($crate)
            $($declaration)*
        }
    };
}

#[doc(hidden)]
pub mod __private {
    pub use concord_actual::__private::*;

    #[doc(hidden)]
    pub const GENERATED_CONTRACT_VERSION: u32 = 999;

    #[doc(hidden)]
    pub const fn assert_generated_contract(expected: u32) {
        assert!(
            expected == 35,
            "concord macro emitted an unexpected generated contract version"
        );
        assert!(
            GENERATED_CONTRACT_VERSION == expected,
            "concord macro/facade contract mismatch: facade 999, generated 35"
        );
    }
}
