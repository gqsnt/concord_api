//! Semantic runtime kernel and extension contracts for Concord.
//!
//! This implementation package owns bounded managed HTTP, lifecycle ordering,
//! redaction, transport isolation, and the semantic contracts used by
//! extensions. Applications should depend on the `concord` façade rather than
//! importing this crate directly.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "dangerous-dev-tools")]
pub mod __development;
pub mod advanced;
#[cfg(any(feature = "dangerous-custom-retry", feature = "dangerous-raw-response"))]
pub mod dangerous;
pub mod prelude;
#[cfg(feature = "records")]
pub mod records;

mod auth;
mod body;
mod client;
pub mod codec;
mod error;
mod generated_contract;
mod hooks;
mod http;
mod operation;
pub mod pagination;
pub mod rate_limit;
mod redaction;
mod request;
mod response;
mod runtime;

pub use pagination::{PageLimit, PaginatedRequest};
pub use request::{Request, ResponseRequest};

mod execute;

#[doc(hidden)]
pub mod __private;

#[cfg(feature = "dangerous-raw-response")]
#[doc(hidden)]
#[macro_export]
macro_rules! __concord_emit_generated_raw_response_method {
    ($endpoint:ty, $endpoint_trait:path, $field:ident) => {
        /// Selects the complete bounded successful response without decoding it.
        ///
        /// This method exists only when the `dangerous-raw-response` capability is
        /// enabled and the declared sealed response mode supports bounded raw
        /// acquisition.
        #[must_use]
        pub fn raw_response<Mode>(self) -> $crate::dangerous::RawResponseRequest<$endpoint>
        where
            <$endpoint as $endpoint_trait>::Response:
                $crate::advanced::ResponseBodySpec<Mode = Mode>,
            Mode: $crate::dangerous::RawResponseMode,
        {
            $crate::__private::raw_response_request(self.$field)
        }
    };
}

#[cfg(not(feature = "dangerous-raw-response"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __concord_emit_generated_raw_response_method {
    ($endpoint:ty, $endpoint_trait:path, $field:ident) => {};
}

#[cfg(feature = "dangerous-dev-tools")]
#[doc(hidden)]
#[macro_export]
macro_rules! __concord_emit_generated_development_method {
    ($endpoint:ty, $field:ident) => {
        #[doc(hidden)]
        pub fn __concord_into_development_request(self) -> $crate::Request<$endpoint> {
            self.$field
        }
    };
}

#[cfg(not(feature = "dangerous-dev-tools"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __concord_emit_generated_development_method {
    ($endpoint:ty, $field:ident) => {};
}

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_test_target_runs() {}
}
