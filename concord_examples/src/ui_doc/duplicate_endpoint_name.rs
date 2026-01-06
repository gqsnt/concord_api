//! ```compile_fail
//! use concord_macros::api;
//!
//! api! {
//!   client C { scheme: https, host: "example.com", params { }, headers { } }
//!   path "x" {
//!     GET Same "" -> X<u8>;
//!     GET Same "y" -> X<u8>;
//!   }
//! }
//! ```
