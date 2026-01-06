//! ```compile_fail
//! use concord_macros::api;
//!
//! api! {
//!   client C {
//!     scheme: https,
//!     host: "example.com",
//!     params { }
//!     headers { missing_var }
//!   }
//!   path "x" { GET Ok "" -> X<u8>; }
//! }
//! ```
