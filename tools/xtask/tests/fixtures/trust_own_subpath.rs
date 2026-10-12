extern crate self as concord;
pub use ::concord_macros::__api as api;
#[macro_export]
macro_rules! application_api { ($($t:tt)*) => { pub const CONFIG: &str = stringify!($($t)*); }; }
pub mod m { pub use crate::application_api as api; }
concord::m::api! { client ForeignSubpath { base "http://foreign.example.test/"; timeout 5s;
  GET read() -> () { path ["subpath-foreign"]; } } }
pub fn verify() { assert!(CONFIG.contains("[\"subpath-foreign\"]"), "{CONFIG}"); println!("FOREIGN_SUBPATH {CONFIG}"); }
