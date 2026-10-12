extern crate self as concord;
#[macro_export]
macro_rules! application_api { ($($t:tt)*) => { pub const CONFIG: &str = stringify!($($t)*); }; }
pub use crate::application_api as api;
pub mod prelude { pub use crate::application_api as api; }
pub mod user;
pub fn verify() { user::verify(); }
