//! ```compile_fail
//! use concord_macros::api;
//!
//! api! {
//!   client C { scheme: https, host: "example.com", params { }, headers { } }
//!   path "x" {
//!     GET Bad ""
//!     headers { "x" => {debug?} }
//!     -> X<u8>;
//!   }
//! }
//! ```
