#![forbid(unsafe_code)]

#[doc(hidden)]
pub mod __private {
    #[doc(hidden)]
    pub const GENERATED_CONTRACT_VERSION: u32 = 999;

    #[doc(hidden)]
    pub const fn assert_generated_contract(expected: u32) {
        assert!(
            GENERATED_CONTRACT_VERSION == expected,
            "concord macro/core contract mismatch"
        );
    }

    #[doc(hidden)]
    #[derive(Debug, Eq, PartialEq)]
    pub struct RequestHead {
        endpoint: &'static str,
        method: &'static str,
        logical_url: String,
        query: Vec<(String, String)>,
        headers: Vec<(String, String)>,
        scope_snapshot: Vec<(&'static str, String)>,
    }

    impl RequestHead {
        #[doc(hidden)]
        pub fn new(
            endpoint: &'static str,
            method: &'static str,
            logical_url: String,
            query: Vec<(String, String)>,
            headers: Vec<(String, String)>,
            scope_snapshot: Vec<(&'static str, String)>,
        ) -> Self {
            Self {
                endpoint,
                method,
                logical_url,
                query,
                headers,
                scope_snapshot,
            }
        }

        pub const fn method(&self) -> &'static str {
            self.method
        }
    }
}
