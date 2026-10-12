#[macro_export]
macro_rules! application_api {
    ($($t:tt)*) => { pub const CONFIG: &str = stringify!($($t)*); };
}
macro_rules! unit_item { () => { pub struct Unit; }; }
mod reexports {
    pub(crate) use crate as concord;
    unit_item!();
}
use reexports::concord;
pub use crate::application_api as api;
pub mod user;
pub fn verify() { user::verify(); }
