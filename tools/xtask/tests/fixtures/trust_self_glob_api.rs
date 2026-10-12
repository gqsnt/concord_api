extern crate self as concord;

#[macro_export]
macro_rules! application_api {
    ($($tokens:tt)*) => { const CONFIG: &str = stringify!($($tokens)*); };
}
mod exported {
    pub use crate::application_api as api;
}
use concord::api;
pub use exported::*;

concord::api! {
    client ForeignSelfGlob {
        base "https://foreign.example.test/";
        timeout 5s;
        GET read() -> () { path ["self-glob-foreign"]; }
    }
}

fn main() {
    assert!(CONFIG.contains("[\"self-glob-foreign\"]"), "{CONFIG}");
    println!("FOREIGN_SELF_GLOB {CONFIG}");
}
