//! ```compile_fail
//! use concord_macros::api;
//!
//! api! {
//!   client C { scheme: https, host: "example.com", params { }, headers { } }
//!   prefix "v1.api" {
//!     path "x" { GET Bad "" -> X<u8>; }
//!   }
//! }
//! ```
