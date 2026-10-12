extern crate self as concord;
#[macro_export]
macro_rules! application_api {
    ($($t:tt)*) => { pub const CONFIG: &str = stringify!($($t)*); };
}
mod exported { pub use crate::application_api as api; }
pub use exported::*;
concord::api! {
    client ForeignRootGlob {
        base "http://foreign.example.test/";
        timeout 5s;
        GET read() -> () { path ["root-glob-foreign"]; }
    }
}
pub fn verify() {
    assert!(CONFIG.contains("[\"root-glob-foreign\"]"), "{CONFIG}");
    println!("FOREIGN_ROOT_GLOB {CONFIG}");
}
