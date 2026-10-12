#[macro_export]
macro_rules! application_api {
    ($($t:tt)*) => { pub const CONFIG: &str = stringify!($($t)*); };
}
pub mod concord { pub use crate::application_api as api; }
::concord::api! {
    client Genuine2024 {
        base "http://genuine.example.test/";
        timeout 5s;
        GET read() -> () via ::concord::Empty { path "edition2024-genuine"; }
    }
}
pub fn verify() {
    let _ = std::mem::size_of::<Genuine2024>();
    println!("GENUINE_EDITION2024_EXTERNAL");
}
