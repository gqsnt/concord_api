//! ```compile_fail
//! use concord_macros::api;
//!
//! api! {
//!   client C { scheme: https, host: "example.com", params { }, headers { } }
//!   path "x" {
//!     GET Bad ""
//!     query { page: u32 = 1 }
//!     headers { "x-page" => {page: u32 = 2} }
//!     -> X<u8>;
//!   }
//! }
//! ```
