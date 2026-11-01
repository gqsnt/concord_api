//! The `api!` compiler for Concord. Use it through the `concord` crate.

use proc_macro::TokenStream;

mod emit;
mod model;
mod syntax;
mod template;

/// Declares an HTTP API as a client → scopes → endpoints tree and generates its typed client.
///
/// See the `concord` crate documentation for the language.
#[proc_macro]
pub fn api(input: TokenStream) -> TokenStream {
    let api = syn::parse_macro_input!(input as syntax::Api);
    match model::Model::build(api) {
        Ok(model) => emit::emit(&model).into(),
        Err(error) => error.to_compile_error().into(),
    }
}
