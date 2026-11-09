//! The `api!` compiler for Concord. Use it through `concord::api!`, which passes the path of the
//! runtime crate as the facade reaches it (`$crate::__macro::concord_core`) as the first tokens,
//! followed by `;`.

use proc_macro::TokenStream;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};

mod derive_error;
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

/// Derives `Display`, `std::error::Error` and `From` for an SDK's error enum, whose variants stay
/// as written.
///
/// - `#[display("...")]` on each variant: a format string over its fields (`{0}`, `{name}`).
///   Exactly one field (`"{0}"`) formats as that field, with the caller's flags. Or
///   `#[display(with = path)]` on the enum: `fn(&Self, &mut Formatter<'_>) -> fmt::Result`.
/// - Each single-field tuple variant converts `From` its field (`From<T>` for `Box<T>`);
///   `#[from(skip)]` withholds one. `#[from(concord::Error<D>)]` makes the variant the provider's
///   answer: its field is built `From` the `Failure<D>`, any other failure goes through
///   `From<concord::Error>`.
/// - `#[source]` on one field of a variant is its `source()`; there is none otherwise.
#[proc_macro_derive(Error, attributes(display, from, source))]
pub fn derive_error(input: TokenStream) -> TokenStream {
    let item = syn::parse_macro_input!(input as syn::Item);
    derive_error::expand(&item)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}
