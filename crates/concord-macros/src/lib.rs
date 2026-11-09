//! The `api!` compiler for Concord. Use it through `concord::api!`, which passes the path of the
//! runtime crate as the facade reaches it (`$crate::__macro::concord_core`) as the first tokens,
//! followed by `;`.

use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};

mod debug;
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
        Ok((model, unused)) => {
            let mut out = emit::emit(&model, krate);
            if let Some(error) = unused {
                out.extend(error.to_compile_error());
            }
            out.into()
        }
        Err(error) => error.to_compile_error().into(),
    }
}

/// Implementation of `#[derive(concord::Debug)]`; use it through `concord::Debug`.
#[proc_macro_derive(Debug, attributes(sensitive, public))]
pub fn derive_debug(input: TokenStream) -> TokenStream {
    debug::derive(input.into()).into()
}
