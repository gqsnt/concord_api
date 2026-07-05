//! Proc-macro entry point for the Concord DSL.
//!
//! The implementation is intentionally staged:
//!
//! 1. `parse` accepts current syntax.
//! 2. `sema` normalizes and resolves the API tree into `ResolvedApi` and
//!    `ResolvedEndpoint`.
//! 3. `codegen` emits clients and endpoint `plan()` implementations from the
//!    resolved model only.

use proc_macro::TokenStream;

mod ast;
mod codegen;
mod emit_helpers;
mod kw;
mod limits;
mod model;
mod parse;
mod sema;

#[proc_macro]
pub fn api(input: TokenStream) -> TokenStream {
    let input2: proc_macro2::TokenStream = input.into();
    let ast = match syn::parse2::<ast::RawApi>(input2) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let resolved_api = match sema::analyze(ast) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    codegen::emit(resolved_api).into()
}
