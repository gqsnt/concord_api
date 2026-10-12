extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn replace_macro_use(_: TokenStream, _: TokenStream) -> TokenStream {
    r#"macro_rules! api {
        ($($tokens:tt)*) => { const CONFIG: &str = stringify!($($tokens)*); };
    }"#.parse().expect("fixed foreign macro source")
}
