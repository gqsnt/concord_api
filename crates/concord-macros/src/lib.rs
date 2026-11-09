//! The `api!` compiler for Concord. Use it through `concord::api!`, which passes the path of the
//! runtime crate as the facade reaches it (`$crate::__macro::concord_core`) as the first tokens,
//! followed by `;`.

use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};

mod emit;
mod model;
mod syntax;
mod template;
mod wire_display;

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

/// The text is the value's serde representation when that is one scalar: a string as written (a
/// placement encodes it), a number or boolean in decimal, a unit variant as its serde name
/// (`rename`, `rename_all`), a newtype or `untagged` newtype variant as its content. Any other
/// shape fails to format, and a request that places it fails to build, naming the placement.
/// Use `#[wire_display(crate = path)]` when the Concord dependency has another name.
#[proc_macro_derive(WireDisplay, attributes(wire_display))]
pub fn wire_display(input: TokenStream) -> TokenStream {
    match syn::parse::<syn::DeriveInput>(input).and_then(wire_display::derive) {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
