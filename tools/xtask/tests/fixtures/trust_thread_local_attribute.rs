extern crate namespace_attribute;
use namespace_attribute::emit_namespace;

#[macro_export]
macro_rules! application_api {
    ($($tokens:tt)*) => { const CONFIG: &str = stringify!($($tokens)*); };
}

std::thread_local! {
    #[emit_namespace]
    static SOURCE_COUNTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

use concord::api;
api! {
    client ForeignThreadLocal {
        base "https://foreign.example.test/";
        timeout 5s;
        GET read() -> () { path ["thread-local-foreign"]; }
    }
}

fn main() {
    assert!(CONFIG.contains("[\"thread-local-foreign\"]"), "{CONFIG}");
    println!("FOREIGN_THREAD_LOCAL {CONFIG}");
}
