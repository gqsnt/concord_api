//! Declaration compiler for Concord APIs.
//!
//! This implementation crate supports `concord::api!`. Applications should
//! depend on and import from `concord`. This package parses and validates
//! façade declarations and emits against the façade's versioned generated-code
//! bridge. Its resolved surface model owns the exact `set_*`, optional
//! `clear_*`, and defaulted `reset_*` value-control names.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod emit;
mod semantic;
mod syntax;

use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use syn::parse::{Parse, ParseStream};
use syn::{Token, parenthesized};

// Core owns these public structural limits. Compiler boundary tests exercise
// the same exact first-excess behavior without coupling this crate to core.
const MAX_AUTH_PLACEMENTS: usize = 64;
const MAX_AUTH_PLACEMENT_NAME_BYTES: usize = 256;
const MAX_REQUEST_HEAD_COMPONENTS: usize = 256;
const MAX_REQUEST_URL_BYTES: usize = 131_072;
const MAX_REQUEST_HEADER_COUNT: usize = 256;
const MAX_REQUEST_HEADER_BYTES: usize = 65_536;
const MAX_RATE_LIMIT_ID_BYTES: usize = 256;
const MAX_RATE_LIMIT_KEY_PARTS: usize = 32;
const MAX_RATE_LIMIT_WINDOWS_PER_RULE: usize = 16;
const MAX_RATE_LIMIT_RULES_PER_PLAN: usize = 64;

/// Internal façade entry point carrying façade-selected capability tokens.
#[doc(hidden)]
#[proc_macro]
pub fn __api(input: TokenStream) -> TokenStream {
    syn::parse::<FacadeInput>(input)
        .and_then(|input| expand_with_runtime(input.declaration, &input.runtime_path))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_with_runtime(
    input: TokenStream2,
    runtime_path: &TokenStream2,
) -> syn::Result<TokenStream2> {
    syntax::parse(input)
        .and_then(semantic::resolve)
        .map(|model| emit::generate(&model, runtime_path))
}

struct FacadeInput {
    runtime_path: TokenStream2,
    declaration: TokenStream2,
}

impl Parse for FacadeInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Token![@]>()?;
        let prefix: Ident = input.parse()?;
        if prefix != "concord_facade" {
            return Err(syn::Error::new(
                prefix.span(),
                "invalid internal Concord macro façade prefix",
            ));
        }
        let content;
        parenthesized!(content in input);
        let runtime_path = content.parse()?;
        Ok(Self {
            runtime_path,
            declaration: input.parse()?,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn implementation_macro_has_no_public_api_entry() {
        let source =
            syn::parse_file(include_str!("lib.rs")).expect("macro crate source must parse");
        let proc_macros = source
            .items
            .iter()
            .filter_map(|item| {
                let syn::Item::Fn(function) = item else {
                    return None;
                };
                function
                    .attrs
                    .iter()
                    .any(|attribute| attribute.path().is_ident("proc_macro"))
                    .then(|| function.sig.ident.to_string())
            })
            .collect::<Vec<_>>();

        assert_eq!(proc_macros, ["__api"]);
    }
}
