//! ```compile_fail
//! use concord_macros::api;
//!
//! api! {
//!   client C { scheme: https, host: "example.com", params { }, headers { } }
//!   path "x{y}" {
//!     GET Bad "" -> X<u8>;
//!   }
//! }
//! ```
