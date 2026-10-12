extern crate concord as sdk;
#[macro_export]
macro_rules! application_api {
    ($($t:tt)*) => { pub const CONFIG: &str = stringify!($($t)*); };
}
pub mod concord { pub use crate::application_api as api; }
::concord::api! {
    client Foreign2015 {
        base "http://foreign.example.test/";
        timeout 5s;
        GET read() -> () { path ["edition2015-foreign"]; }
    }
}
pub fn verify() {
    assert!(CONFIG.contains("[\"edition2015-foreign\"]"), "{CONFIG}");
    println!("FOREIGN_EDITION2015 {CONFIG}");
}
