//! Procedural macros for declaring Concord APIs.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod ast;
mod codegen;
mod crate_path;
mod model;
mod names;
mod parse;
mod resolve;

use proc_macro::TokenStream;

/// Declares a generated Concord client and its typed endpoints.
#[proc_macro]
pub fn api(input: TokenStream) -> TokenStream {
    parse::parse(input.into())
        .and_then(resolve::resolve)
        .and_then(|model| {
            let runtime_path = crate_path::resolve_runtime_path()?;
            Ok(codegen::generate(&model, &runtime_path))
        })
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
