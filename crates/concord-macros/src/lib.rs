//! Declaration compiler for Concord APIs.
//!
//! Applications use the [`api`] reexport from the `concord` façade. This
//! implementation package parses and validates declarations and emits against
//! the façade's versioned generated-code bridge.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod crate_path;
mod emit;
mod semantic;
mod syntax;

use proc_macro::TokenStream;

// Core owns these public structural limits. The documentation xtask
// mechanically checks these parser/resolver mirrors against core literals.
const MAX_RATE_LIMIT_ID_BYTES: usize = 256;
const MAX_RATE_LIMIT_WINDOWS_PER_RULE: usize = 16;
const MAX_RATE_LIMIT_RULES_PER_PLAN: usize = 64;

/// Declares a generated Concord client and its typed endpoints.
#[proc_macro]
pub fn api(input: TokenStream) -> TokenStream {
    syntax::parse(input.into())
        .and_then(semantic::resolve)
        .and_then(|model| {
            let runtime_path = crate_path::resolve_runtime_path()?;
            Ok(emit::generate(&model, &runtime_path))
        })
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
