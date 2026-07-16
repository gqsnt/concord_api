//! Procedural macros for declaring Concord APIs.

#![forbid(unsafe_code)]

mod codegen;
mod crate_path;
mod model;
mod parse;

use proc_macro::TokenStream;

/// Declares the minimal Phase 0 Concord client, scope, and endpoint surface.
#[proc_macro]
pub fn api(input: TokenStream) -> TokenStream {
    parse::parse(input.into())
        .and_then(|model| {
            let core_path = crate_path::resolve_core_path()?;
            Ok(codegen::generate(&model, &core_path))
        })
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
