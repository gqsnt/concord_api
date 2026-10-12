extern crate proc_macro;
use proc_macro::{Literal, TokenStream};

#[proc_macro_attribute]
pub fn capture_item(_: TokenStream, input: TokenStream) -> TokenStream {
    let captured = Literal::string(&input.to_string());
    format!("const CONFIG: &str = {captured};").parse().expect("fixed const source")
}
