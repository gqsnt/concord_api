//! Declaration compiler for Concord APIs.
//!
//! This implementation crate supports `concord::api!`. Applications should
//! depend on and import from `concord`. This package parses and validates
//! façade declarations and emits against the façade's versioned generated-code
//! bridge.

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
const MAX_RATE_LIMIT_ID_BYTES: usize = 256;
const MAX_RATE_LIMIT_WINDOWS_PER_RULE: usize = 16;
const MAX_RATE_LIMIT_RULES_PER_PLAN: usize = 64;

/// Internal façade entry point carrying façade-selected capability tokens.
#[doc(hidden)]
#[proc_macro]
pub fn __api(input: TokenStream) -> TokenStream {
    syn::parse::<FacadeInput>(input)
        .and_then(|input| {
            expand_with_runtime(input.declaration, input.capabilities, &input.runtime_path)
        })
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_with_runtime(
    input: TokenStream2,
    capabilities: emit::Capabilities,
    runtime_path: &TokenStream2,
) -> syn::Result<TokenStream2> {
    syntax::parse(input)
        .and_then(semantic::resolve)
        .map(|model| emit::generate(&model, runtime_path, capabilities))
}

struct FacadeInput {
    capabilities: emit::Capabilities,
    runtime_path: TokenStream2,
    declaration: TokenStream2,
}

impl Parse for FacadeInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<Token![@]>()?;
        let prefix: Ident = input.parse()?;
        if prefix != "concord_capabilities" {
            return Err(syn::Error::new(
                prefix.span(),
                "invalid internal Concord macro capability prefix",
            ));
        }
        let content;
        parenthesized!(content in input);
        let mut capabilities = emit::Capabilities::default();
        while !content.is_empty() {
            let capability: Ident = content.parse()?;
            match capability.to_string().as_str() {
                "raw_response" if !capabilities.raw_response => {
                    capabilities.raw_response = true;
                }
                "raw_response" => {
                    return Err(syn::Error::new(
                        capability.span(),
                        "duplicate internal Concord macro capability",
                    ));
                }
                _ => {
                    return Err(syn::Error::new(
                        capability.span(),
                        "unknown internal Concord macro capability",
                    ));
                }
            }
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
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
            capabilities,
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
