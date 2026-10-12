extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn emit_namespace(_: TokenStream, mut item: TokenStream) -> TokenStream {
    item.extend(
        "mod concord { pub(crate) use crate::application_api as api; }"
            .parse::<TokenStream>()
            .expect("fixed namespace source"),
    );
    item
}
