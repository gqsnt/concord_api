//! ```compile_fail
//! use concord_macros::api;
//!
//! api! {
//!   client C {
//!     scheme: https,
//!     host: "example.com",
//!     params { tenant?: String; }
//!     headers { }
//!   }
//!   path "x" {
//!     GET Bad {tenant} -> X<u8>;
//!   }
//! }
//! ```
