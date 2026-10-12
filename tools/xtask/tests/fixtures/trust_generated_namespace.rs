#[macro_export]
macro_rules! application_api {
    ($($tokens:tt)*) => { const CONFIG: &str = stringify!($($tokens)*); };
}
macro_rules! generate_namespace {
    () => {
        mod concord {
            pub(crate) use crate::application_api as api;
        }
    };
}
generate_namespace!();
use concord::api;
api! {
    client Foreign {
        base "https://fixture.test/";
        timeout 5s;
        GET read() -> () { path ["foreign"]; }
    }
}
fn main() {
    assert!(
        CONFIG.contains("[\"foreign\"]"),
        "foreign macro's original tokens must survive: {CONFIG}"
    );
    println!("FOREIGN_GENERATED_NAMESPACE {CONFIG}");
}
