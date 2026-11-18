//! An attribute for the compiler fixtures of `paginate`. Written on an item inside a `paginate`
//! block, it replaces the item with a `return` of a closure that reads the page, which only the
//! inferred path of `paginate` accepts: the macro cannot see it.

use proc_macro::TokenStream;

/// Replaces the item with `return |call, page| …;`.
#[proc_macro_attribute]
pub fn returns_a_closure(_: TokenStream, _: TokenStream) -> TokenStream {
    "return |call, page| if page.value().next.is_some() { Some(call) } else { None };"
        .parse()
        .unwrap_or_default()
}
