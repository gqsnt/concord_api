extern crate self as concord;

#[macro_export]
macro_rules! application_api {
    ($($tokens:tt)*) => { const CONFIG: &str = stringify!($($tokens)*); };
}
pub use application_api as api;

concord::api! {
    client ForeignBinary {
        base "https://foreign.example.test/";
        timeout 5s;
        GET read() -> () { path ["foreign-self"]; }
    }
}

fn main() {
    assert!(CONFIG.contains("[\"foreign-self\"]"), "{CONFIG}");
    println!("FOREIGN_TARGET_SELF {CONFIG}");
}
