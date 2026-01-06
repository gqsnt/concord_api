//! ```compile_fail
//! use concord_macros::api;
//!
//! api! {
//!   client C {
//!     scheme: https,
//!     host: "example.com",
//!     params { id: u32; }
//!     headers { }
//!   }
//!   path "x" {
//!     GET Bad {id: u32}
//!     headers { id }
//!     -> X<u8>;
//!   }
//! }
//! ```
