//! The `api!` compiler for Concord. Use it through `concord::api!`, which passes the path of the
//! `concord` crate (`$crate`) as the first token, followed by `;`.

use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};

mod emit;
mod model;
mod syntax;
mod template;

/// Implementation of `concord::api!`; not meant to be invoked directly.
#[proc_macro]
pub fn api(input: TokenStream) -> TokenStream {
    let input = TokenStream2::from(input);
    let mut tokens = input.into_iter();
    let mut krate = TokenStream2::new();
    for tt in tokens.by_ref() {
        match &tt {
            TokenTree::Punct(p) if p.as_char() == ';' => break,
            _ => krate.extend([tt]),
        }
    }
    let rest: TokenStream2 = tokens.collect();
    let api = match syn::parse2::<syntax::Api>(rest) {
        Ok(api) => api,
        Err(error) => return error.to_compile_error().into(),
    };
    match model::Model::build(api) {
        Ok(model) => emit::emit(&model, krate).into(),
        Err(error) => error.to_compile_error().into(),
    }
}
