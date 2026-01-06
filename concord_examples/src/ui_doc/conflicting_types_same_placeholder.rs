//! ```compile_fail
//! use concord_macros::api;
//!
//! api! {
//!   client C { scheme: https, host: "example.com", params { }, headers { } }
//!   path "x" {
//!     GET Bad {id: u32}
//!     query { id: String }
//!     -> X<u8>;
//!   }
//! }
//! ```
