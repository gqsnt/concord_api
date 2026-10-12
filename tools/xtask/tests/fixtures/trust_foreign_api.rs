#![forbid(unsafe_code)]
#[macro_export]
macro_rules! application_api {
    ($($token:tt)*) => {
        fn declaration_text() -> &'static str { stringify!($($token)*) }
    };
}
mod concord {
    pub(crate) use crate::application_api as api;
    pub struct Binary;
    pub struct Bytes;
    pub mod prelude {
        pub use super::{Binary, Bytes};
    }
}
use concord::api;
api! { client ApplicationTokens {
    base "http://application.example.test/";
    timeout 5s;
    GET get() -> Bytes via Binary { path ["foreign"]; }
} }
fn main() {
    println!("APPLICATION_MACRO_TEXT={}", declaration_text());
    assert!(
        declaration_text().contains("path [\"foreign\"]"),
        "application macro token meaning must be preserved"
    );
}
